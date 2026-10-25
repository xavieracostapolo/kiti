use std::sync::{Arc, Mutex};
use std::time::Duration;

use kiti_core::application::{Runtime, RuntimeHandle, Telemetry};
use kiti_core::domain::{
    AgentId, AgentInput, DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider, Signal,
    Timestamp, WorkingDir,
};
use kiti_core::ports::{AgentKiller, DiffPublisher, KillOutcome, KillTarget};
use kiti_core::testing::ManualClock;
use kiti_protocol::{KillResult, PROTOCOL_VERSION, ServerMsg, Source};

#[derive(Clone)]
struct TestKiller;

impl AgentKiller for TestKiller {
    async fn terminate(
        &self,
        _target: KillTarget,
        _grace: Duration,
    ) -> Result<KillOutcome, kiti_core::ports::KillError> {
        Ok(KillOutcome::Killed)
    }
}

#[derive(Clone)]
struct TestPublisher {
    messages: Arc<Mutex<Vec<ServerMsg>>>,
}

impl TestPublisher {
    fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl DiffPublisher for TestPublisher {
    fn publish(&mut self, msg: ServerMsg) {
        self.messages.lock().unwrap().push(msg);
    }
}

fn spawn<K: AgentKiller + Clone>(killer: K, clock: ManualClock) -> RuntimeHandle {
    Runtime::spawn(
        killer,
        clock,
        Arc::new(Telemetry::new()),
        Source::Sim,
        "0.1.0-test",
    )
}

fn id_of(d: &DetectedAgent) -> AgentId {
    AgentId {
        provider: d.provider,
        pid: d.identity.pid,
        start_time: d.identity.start_time,
    }
}

fn sample_agent(pid: u32) -> DetectedAgent {
    DetectedAgent {
        provider: Provider::Claude,
        identity: ProcessIdentity {
            pid: Pid(pid),
            start_time: ProcessStartTime(100),
            uid: 501,
            cmd_fingerprint: 1,
        },
        cwd: WorkingDir::new("/Users/dev/app"),
    }
}

#[tokio::test(start_paused = true)]
async fn snapshot_primero_y_coalescencia_de_diffs() {
    let handle = spawn(TestKiller, ManualClock::new(1000));

    // Registrar un agente
    let detected = sample_agent(1234);
    let agent_id = AgentId {
        provider: detected.provider,
        pid: detected.identity.pid,
        start_time: detected.identity.start_time,
    };
    handle.register_agent(detected, Timestamp(1000)).await;

    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();

    let hello = handle.subscribe(publisher).await.expect("subscribe ok");
    assert_eq!(hello.protocol_version, PROTOCOL_VERSION);

    // Verificamos que llegó el Snapshot inmediatamente con seq = 0
    {
        let list = msgs.lock().unwrap();
        assert_eq!(list.len(), 1);
        match &list[0] {
            ServerMsg::Snapshot { seq, agents } => {
                assert_eq!(*seq, 0);
                assert_eq!(agents.len(), 1);
                assert_eq!(agents[0].id.0, agent_id.to_string());
            }
            _ => panic!("primer mensaje debe ser Snapshot"),
        }
    }

    // Enviamos múltiples señales en ráfaga dentro del mismo intervalo de 100 ms
    handle
        .apply_input(
            agent_id,
            AgentInput::Signal(Signal::PromptSubmit),
            Timestamp(1010),
        )
        .await;
    handle
        .apply_input(
            agent_id,
            AgentInput::Signal(Signal::AwaitingInput),
            Timestamp(1020),
        )
        .await;

    // Antes de los 100 ms no hay Diff todavía
    tokio::time::advance(Duration::from_millis(50)).await;
    tokio::task::yield_now().await;
    assert_eq!(msgs.lock().unwrap().len(), 1);

    // Avanzamos hasta completar los 100 ms: debe salir exactamente 1 Diff
    tokio::time::advance(Duration::from_millis(60)).await;
    tokio::task::yield_now().await;

    {
        let list = msgs.lock().unwrap();
        assert_eq!(list.len(), 2);
        match &list[1] {
            ServerMsg::Diff {
                seq,
                upserts,
                removed,
                ..
            } => {
                assert_eq!(*seq, 1);
                assert_eq!(upserts.len(), 1);
                assert!(removed.is_empty());
                assert_eq!(upserts[0].status.kind, kiti_protocol::StatusKind::Waiting);
            }
            _ => panic!("segundo mensaje debe ser Diff"),
        }
    }

    // Si pasan otros 200 ms sin cambios, no se emite ningún Diff vacío
    tokio::time::advance(Duration::from_millis(200)).await;
    tokio::task::yield_now().await;
    assert_eq!(msgs.lock().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn stream_de_debug_emite_metricas_a_1hz_solo_cuando_esta_activo() {
    let handle = spawn(TestKiller, ManualClock::new(1000));
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();

    handle.subscribe(publisher).await.unwrap();
    assert_eq!(msgs.lock().unwrap().len(), 1); // solo Snapshot

    // Sin debug stream activo, tras 3 s no hay mensajes de métricas
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert_eq!(msgs.lock().unwrap().len(), 1);

    // Activamos debug stream y avanzamos 1 s
    handle.set_debug_stream(true).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;

    {
        let list = msgs.lock().unwrap();
        assert!(list.iter().any(|m| matches!(m, ServerMsg::Metrics(_))));
    }
}

#[tokio::test(start_paused = true)]
async fn kill_agent_elimina_la_oficina() {
    let handle = spawn(TestKiller, ManualClock::new(1000));

    let detected = sample_agent(4321);
    let agent_id = AgentId {
        provider: detected.provider,
        pid: detected.identity.pid,
        start_time: detected.identity.start_time,
    };
    handle.register_agent(detected, Timestamp(1000)).await;

    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.unwrap();

    let res = handle.kill_agent(agent_id).await.unwrap();
    assert_eq!(res, KillResult::Killed);

    // Al llegar el siguiente flush se publica el removed
    tokio::time::advance(Duration::from_millis(150)).await;
    tokio::task::yield_now().await;

    {
        let list = msgs.lock().unwrap();
        let last = list.last().unwrap();
        match last {
            ServerMsg::Diff { removed, .. } => {
                assert_eq!(removed.len(), 1);
                assert_eq!(removed[0].0, agent_id.to_string());
            }
            _ => panic!("esperaba Diff con removed"),
        }
    }
}

/// Killer que tarda en terminar, como el periodo de gracia real de SIGTERM → SIGKILL.
#[derive(Clone)]
struct SlowKiller;

impl AgentKiller for SlowKiller {
    async fn terminate(
        &self,
        _target: KillTarget,
        grace: Duration,
    ) -> Result<KillOutcome, kiti_core::ports::KillError> {
        tokio::time::sleep(grace).await;
        Ok(KillOutcome::Killed)
    }
}

#[tokio::test(start_paused = true)]
async fn un_kill_lento_no_bloquea_el_actor() {
    let handle = spawn(SlowKiller, ManualClock::new(1000));
    let victim = sample_agent(1);
    let other = sample_agent(2);
    let (victim_id, other_id) = (id_of(&victim), id_of(&other));
    handle.register_agent(victim, Timestamp(1000)).await;
    handle.register_agent(other, Timestamp(1000)).await;
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.unwrap();

    let killing = tokio::spawn({
        let handle = handle.clone();
        async move { handle.kill_agent(victim_id).await }
    });
    tokio::task::yield_now().await;

    // Mientras dura la gracia de 5 s, el actor sigue procesando y publicando.
    handle
        .apply_input(
            other_id,
            AgentInput::Signal(Signal::PromptSubmit),
            Timestamp(1100),
        )
        .await;
    tokio::time::advance(Duration::from_millis(150)).await;
    tokio::task::yield_now().await;
    assert!(!killing.is_finished());
    assert!(msgs.lock().unwrap().iter().any(|m| matches!(
        m,
        ServerMsg::Diff { upserts, .. } if upserts.iter().any(|a| a.id.0 == other_id.to_string())
    )));

    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(killing.await.unwrap().unwrap(), KillResult::Killed);
    tokio::time::advance(Duration::from_millis(150)).await;
    tokio::task::yield_now().await;
    assert!(msgs.lock().unwrap().iter().any(|m| matches!(
        m,
        ServerMsg::Diff { removed, .. } if removed.iter().any(|r| r.0 == victim_id.to_string())
    )));
}

#[tokio::test(start_paused = true)]
async fn la_caducidad_usa_el_reloj_del_port_clock() {
    let clock = ManualClock::new(1_000);
    let handle = spawn(TestKiller, clock.clone());
    let agent = sample_agent(9);
    let id = id_of(&agent);
    handle.register_agent(agent, Timestamp(1_000)).await;
    handle
        .apply_input(
            id,
            AgentInput::Signal(Signal::PromptSubmit),
            Timestamp(1_000),
        )
        .await;
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.unwrap();

    // 29 s de reloj del dominio: sigue verde aunque pasen ticks de caducidad.
    clock.set(30_000);
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    let kind_now = |msgs: &Arc<Mutex<Vec<ServerMsg>>>| {
        msgs.lock().unwrap().iter().rev().find_map(|m| match m {
            ServerMsg::Snapshot { agents, .. }
            | ServerMsg::Diff {
                upserts: agents, ..
            } => agents.first().map(|a| a.status.kind),
            _ => None,
        })
    };
    assert_eq!(kind_now(&msgs), Some(kiti_protocol::StatusKind::Working));

    // A los 30 s pasa a gris por caducidad.
    clock.set(31_000);
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert_eq!(kind_now(&msgs), Some(kiti_protocol::StatusKind::Idle));
}
