use std::sync::{Arc, Mutex};
use std::time::Duration;

use kiti_core::application::run_scanner;
use kiti_core::application::{Runtime, RuntimeHandle, Telemetry};
use kiti_core::domain::ProcessSnapshot;
use kiti_core::domain::{
    AgentId, AgentInput, DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider, Signal,
    Timestamp, WorkingDir,
};
use kiti_core::ports::Clock;
use kiti_core::ports::{
    AgentKiller, DiffPublisher, KillOutcome, KillTarget, ProcessScanner, ScanError,
};
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
    fn publish(&mut self, msg: ServerMsg) -> bool {
        // Los mensajes entre agentes tienen su propio test (mail_runtime.rs).
        if !matches!(msg, ServerMsg::Mail(_)) {
            self.messages.lock().unwrap().push(msg);
        }
        true
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

/// Escáner falso: cada `scan()` devuelve el siguiente lote preparado (el último se repite).
struct ScriptedScanner(Vec<Vec<ProcessSnapshot>>);

impl ProcessScanner for ScriptedScanner {
    fn scan(&mut self) -> Result<Vec<ProcessSnapshot>, ScanError> {
        Ok(if self.0.len() > 1 {
            self.0.remove(0)
        } else {
            self.0[0].clone()
        })
    }
}

fn claude_process(pid: u32) -> ProcessSnapshot {
    ProcessSnapshot {
        pid: Pid(pid),
        ppid: Some(Pid(1)),
        start_time: ProcessStartTime(5_000),
        uid: 501,
        exe: Some("/Users/dev/.local/share/claude/versions/2.1.310".into()),
        argv: vec!["claude".into()],
        cwd: Some("/Users/dev/kiti".into()),
    }
}

#[tokio::test(start_paused = true)]
async fn el_escaner_alimenta_el_registro_y_detecta_salidas() {
    let clock = ManualClock::new(10_000);
    let handle = spawn(TestKiller, clock.clone());
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.unwrap();

    let zsh = ProcessSnapshot {
        exe: Some("/bin/zsh".into()),
        argv: vec!["-zsh".into()],
        ..claude_process(50)
    };
    let scanner = ScriptedScanner(vec![vec![claude_process(77), zsh.clone()], vec![zsh]]);
    tokio::spawn(run_scanner(
        scanner,
        None,
        handle.clone(),
        clock.clone(),
        Duration::from_secs(2),
    ));

    let ids_in = |m: &ServerMsg| match m {
        ServerMsg::Diff { upserts, .. } => upserts.iter().map(|a| (a.pid, a.status.kind)).collect(),
        _ => vec![],
    };
    let seen = || -> Vec<(u32, kiti_protocol::StatusKind)> {
        msgs.lock().unwrap().iter().flat_map(ids_in).collect()
    };

    // Primer escaneo: aparece solo el agente, no el shell.
    eventually(|| !seen().is_empty()).await;
    assert_eq!(seen(), vec![(77, kiti_protocol::StatusKind::Idle)]);

    // Segundo escaneo, 2 s después: el proceso ya no está y no hubo SessionEnd → dead.
    eventually(|| seen().len() > 1).await;
    assert_eq!(seen().last(), Some(&(77, kiti_protocol::StatusKind::Dead)));
}

/// Con el reloj pausado, el escaneo corre en un hilo real (`spawn_blocking`): se avanza el
/// tiempo virtual de 100 en 100 ms dejando que ese hilo termine, hasta que se cumpla `cond`.
async fn eventually(cond: impl Fn() -> bool) {
    for _ in 0..100 {
        if cond() {
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
    }
    panic!("la condición no se cumplió");
}

#[tokio::test(start_paused = true)]
async fn un_hook_de_claude_pone_verde_la_oficina_enlazada_por_el_archivo_de_sesion() {
    let clock = ManualClock::new(10_000);
    let handle = spawn(TestKiller, clock.clone());
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.unwrap();

    // Dos agentes en la misma carpeta: solo el archivo de sesión dice cuál es cuál.
    let dir = std::env::temp_dir().join(format!("kiti-sessions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("78.json"),
        r#"{"pid": 78, "sessionId": "s-78", "status": "idle", "updatedAt": 9000}"#,
    )
    .unwrap();
    let scanner = ScriptedScanner(vec![vec![claude_process(77), claude_process(78)]]);
    tokio::spawn(run_scanner(
        scanner,
        Some(dir.clone()),
        handle.clone(),
        clock.clone(),
        Duration::from_secs(2),
    ));
    let pids_with = |kind: kiti_protocol::StatusKind| {
        let mut last = std::collections::HashMap::new();
        for m in msgs.lock().unwrap().iter() {
            if let ServerMsg::Diff { upserts, .. } = m {
                for a in upserts {
                    last.insert(a.pid, a.status.kind);
                }
            }
        }
        let mut pids: Vec<u32> = last
            .into_iter()
            .filter(|(_, k)| *k == kind)
            .map(|(p, _)| p)
            .collect();
        pids.sort();
        pids
    };
    eventually(|| pids_with(kiti_protocol::StatusKind::Idle).len() == 2).await;

    let body = br#"{"session_id":"s-78","cwd":"/Users/dev/kiti","hook_event_name":"UserPromptSubmit","prompt":"x"}"#;
    let event = kiti_core::ingestion::claude::parse_hook(body).unwrap();
    handle.hook(Provider::Claude, event, clock.now()).await;
    eventually(|| pids_with(kiti_protocol::StatusKind::Working) == vec![78]).await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(start_paused = true)]
async fn el_transcript_da_la_ultima_herramienta_sin_hooks() {
    let clock = ManualClock::new(10_000);
    let handle = spawn(TestKiller, clock.clone());
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.unwrap();

    let claude_dir = std::env::temp_dir().join(format!("kiti-claude-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&claude_dir);
    std::fs::create_dir_all(claude_dir.join("sessions")).unwrap();
    std::fs::create_dir_all(claude_dir.join("projects/-Users-dev-kiti")).unwrap();
    std::fs::write(
        claude_dir.join("sessions/78.json"),
        r#"{"pid": 78, "sessionId": "s-78", "status": "busy", "updatedAt": 9000}"#,
    )
    .unwrap();
    let transcript = claude_dir.join("projects/-Users-dev-kiti/s-78.jsonl");
    std::fs::write(
        &transcript,
        concat!(
            r#"{"type":"user","message":{"content":"secreto"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t","name":"Grep","input":{"pattern":"secreto"}}]}}"#,
            "\n"
        ),
    )
    .unwrap();

    tokio::spawn(run_scanner(
        ScriptedScanner(vec![vec![claude_process(78)]]),
        Some(claude_dir.join("sessions")),
        handle.clone(),
        clock.clone(),
        Duration::from_secs(2),
    ));
    let last_tool = || {
        msgs.lock().unwrap().iter().rev().find_map(|m| match m {
            ServerMsg::Diff { upserts, .. } => upserts
                .iter()
                .find(|a| a.pid == 78)
                .and_then(|a| a.last_tool.clone()),
            _ => None,
        })
    };
    eventually(|| last_tool().as_deref() == Some("Grep")).await;

    // Lo que se añade después se lee en el siguiente escaneo.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap();
    writeln!(
        f,
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"u","name":"Edit","input":{{}}}}]}}}}"#
    )
    .unwrap();
    eventually(|| last_tool().as_deref() == Some("Edit")).await;
    let all = format!("{:?}", msgs.lock().unwrap());
    assert!(
        !all.contains("secreto"),
        "del transcript solo sale el nombre de la herramienta"
    );
    std::fs::remove_dir_all(claude_dir).unwrap();
}

#[tokio::test(start_paused = true)]
async fn summary_cuenta_los_agentes_por_color() {
    let clock = ManualClock::new(1_000);
    let handle = spawn(TestKiller, clock.clone());
    let (a, b) = (sample_agent(1), sample_agent(2));
    let a_id = id_of(&a);
    handle.register_agent(a, Timestamp(1_000)).await;
    handle.register_agent(b, Timestamp(1_000)).await;
    handle
        .apply_input(
            a_id,
            AgentInput::Signal(Signal::PromptSubmit),
            Timestamp(1_001),
        )
        .await;
    let s = handle.summary().await;
    assert_eq!((s.working, s.idle, s.waiting), (1, 1, 0));
}

/// Publicador cuya ventana se cerró: falla siempre y cuenta los intentos.
struct DeadPublisher(Arc<Mutex<u32>>);

impl DiffPublisher for DeadPublisher {
    fn publish(&mut self, _msg: ServerMsg) -> bool {
        *self.0.lock().unwrap() += 1;
        false
    }
}

#[tokio::test(start_paused = true)]
async fn un_publicador_muerto_se_descarta_tras_el_primer_fallo() {
    let clock = ManualClock::new(1_000);
    let handle = spawn(TestKiller, clock.clone());
    let agent = sample_agent(1);
    let id = id_of(&agent);
    handle.register_agent(agent, Timestamp(1_000)).await;
    let attempts = Arc::new(Mutex::new(0));
    handle
        .subscribe(DeadPublisher(attempts.clone()))
        .await
        .unwrap();
    for i in 0..5 {
        let sig = if i % 2 == 0 {
            Signal::PromptSubmit
        } else {
            Signal::Stop
        };
        handle
            .apply_input(id, AgentInput::Signal(sig), Timestamp(1_001 + i))
            .await;
        tokio::time::advance(Duration::from_millis(150)).await;
        tokio::task::yield_now().await;
    }
    // El Snapshot falla y ya no se intenta nada más hasta una nueva suscripción.
    assert_eq!(*attempts.lock().unwrap(), 1);
}

#[tokio::test(start_paused = true)]
async fn el_foco_pedido_sin_ventana_llega_tras_el_snapshot() {
    // Clic en una notificación con la ventana cerrada: la oficina se enfoca al reabrirla.
    let handle = spawn(TestKiller, ManualClock::new(1000));
    let detected = sample_agent(4321);
    let id = id_of(&detected);
    handle.register_agent(detected, Timestamp(1000)).await;
    handle.focus(id).await;

    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.expect("subscribe ok");
    {
        let list = msgs.lock().unwrap();
        assert!(matches!(list[0], ServerMsg::Snapshot { .. }));
        assert_eq!(
            list[1],
            ServerMsg::Focus {
                id: kiti_protocol::AgentId(id.to_string())
            }
        );
    }
    // Con la ventana ya abierta, llega en el momento; y no se repite en otra suscripción.
    handle.focus(id).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(msgs.lock().unwrap().len(), 3);
    let again = TestPublisher::new();
    let again_msgs = again.messages.clone();
    handle.subscribe(again).await.expect("subscribe ok");
    assert_eq!(again_msgs.lock().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn la_cuota_se_publica_y_se_reenvia_tras_el_snapshot() {
    let handle = spawn(TestKiller, ManualClock::new(1000));
    let first = TestPublisher::new();
    let first_msgs = first.messages.clone();
    handle.subscribe(first).await.expect("subscribe ok");
    let window = kiti_core::domain::QuotaWindow {
        label: "semana".into(),
        weekly: true,
        remaining: 0.395,
        resets_at: 1_793_572_705_000,
    };
    handle.quota(Provider::Claude, vec![window]).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let expected = ServerMsg::Quota {
        provider: kiti_protocol::Provider::Claude,
        windows: vec![kiti_protocol::QuotaWindow {
            label: "semana".into(),
            weekly: true,
            remaining_pct: 40,
            resets_at: 1_793_572_705_000,
        }],
    };
    assert!(first_msgs.lock().unwrap().contains(&expected));
    // Una ventana que se conecta después la recibe justo tras el Snapshot.
    let later = TestPublisher::new();
    let later_msgs = later.messages.clone();
    handle.subscribe(later).await.expect("subscribe ok");
    let list = later_msgs.lock().unwrap();
    assert!(matches!(list[0], ServerMsg::Snapshot { .. }));
    assert_eq!(list[1], expected);
}

#[tokio::test(start_paused = true)]
async fn la_cuota_repetida_no_se_vuelve_a_publicar() {
    let handle = spawn(TestKiller, ManualClock::new(1000));
    let publisher = TestPublisher::new();
    let msgs = publisher.messages.clone();
    handle.subscribe(publisher).await.expect("subscribe ok");
    let window = |remaining| kiti_core::domain::QuotaWindow {
        label: "semana".into(),
        weekly: true,
        remaining,
        resets_at: 1_793_572_705_000,
    };
    let quotas = || {
        msgs.lock()
            .unwrap()
            .iter()
            .filter(|m| matches!(m, ServerMsg::Quota { .. }))
            .count()
    };
    // La status line de Claude llega varias veces por respuesta, casi siempre igual.
    handle.quota(Provider::Claude, vec![window(0.40)]).await;
    handle.quota(Provider::Claude, vec![window(0.40)]).await;
    // Mismo porcentaje redondeado: tampoco cambia lo que ve la UI.
    handle.quota(Provider::Claude, vec![window(0.401)]).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(quotas(), 1);
    // Cambia: se publica en el momento, sin esperar al siguiente diff.
    handle.quota(Provider::Claude, vec![window(0.38)]).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(quotas(), 2);
}

#[tokio::test(start_paused = true)]
async fn git_solo_se_publica_si_cambia_y_se_reenvia_tras_el_snapshot() {
    let handle = spawn(TestKiller, ManualClock::new(1000));
    handle
        .register_agent(sample_agent(77), Timestamp(1000))
        .await;
    assert_eq!(
        handle.live_projects().await,
        vec!["/Users/dev/app".to_owned()]
    );
    let first = TestPublisher::new();
    let msgs = first.messages.clone();
    handle.subscribe(first).await.expect("subscribe ok");
    let status = kiti_core::domain::GitStatus {
        branch: Some("main".into()),
        ahead: 1,
        behind: 0,
        changed: 3,
    };
    let update = vec![("/Users/dev/app".to_owned(), Some(status))];
    handle.git(update.clone()).await;
    handle.git(update).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let git_msgs = |list: &[ServerMsg]| {
        list.iter()
            .filter(|m| matches!(m, ServerMsg::Git { .. }))
            .count()
    };
    assert_eq!(
        git_msgs(&msgs.lock().unwrap()),
        1,
        "el mismo estado no se repite"
    );
    let later = TestPublisher::new();
    let later_msgs = later.messages.clone();
    handle.subscribe(later).await.expect("subscribe ok");
    let list = later_msgs.lock().unwrap();
    assert_eq!(
        list[1],
        ServerMsg::Git {
            project: "/Users/dev/app".into(),
            git: Some(kiti_protocol::GitInfo {
                branch: Some("main".into()),
                ahead: 1,
                behind: 0,
                changed: 3,
            }),
        }
    );
}
