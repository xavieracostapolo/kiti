//! Mensajes entre agentes a través del runtime (ADR 0017): `kiti_agents`, `kiti_send`,
//! `kiti_inbox`, entrega por terminal y estado para la UI.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use kiti_core::application::runtime::MailAction;
use kiti_core::application::{Runtime, RuntimeHandle, Telemetry};
use kiti_core::domain::{AliasEntry, AliasKey};
use kiti_core::domain::{
    DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider, Timestamp, WorkingDir,
};
use kiti_core::ports::{
    AgentKiller, AliasStore, DiffPublisher, KillOutcome, KillTarget, MailCourier,
};
use kiti_core::testing::ManualClock;
use kiti_protocol::mcp::{RecallRequest, RememberRequest, SendRequest};
use kiti_protocol::{MailMessageState, MailMode, ServerMsg, Source};

#[derive(Clone)]
struct NoKill;

impl AgentKiller for NoKill {
    async fn terminate(
        &self,
        _target: KillTarget,
        _grace: Duration,
    ) -> Result<KillOutcome, kiti_core::ports::KillError> {
        Ok(KillOutcome::Killed)
    }
}

type Typed = Arc<Mutex<Vec<(u32, String, Vec<u64>)>>>;

#[derive(Clone, Default)]
struct FakeCourier(Typed);

impl MailCourier for FakeCourier {
    fn deliver(&mut self, pid: Pid, text: String, ids: Vec<u64>) {
        self.0.lock().unwrap().push((pid.0, text, ids));
    }
}

#[derive(Clone, Default)]
struct FakeAliasStore(Arc<Mutex<Vec<Vec<AliasEntry>>>>);

impl AliasStore for FakeAliasStore {
    fn save(&mut self, entries: &[AliasEntry]) {
        self.0.lock().unwrap().push(entries.to_vec());
    }
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<ServerMsg>>>);

impl DiffPublisher for Sink {
    fn publish(&mut self, msg: ServerMsg) -> bool {
        self.0.lock().unwrap().push(msg);
        true
    }
}

fn agent(provider: Provider, pid: u32, cwd: &str) -> DetectedAgent {
    DetectedAgent {
        provider,
        identity: ProcessIdentity {
            pid: Pid(pid),
            start_time: ProcessStartTime(100),
            uid: 501,
            cmd_fingerprint: 1,
        },
        cwd: WorkingDir::new(cwd),
    }
}

async fn setup() -> (RuntimeHandle, FakeCourier, Sink, ManualClock) {
    let clock = ManualClock::new(1_000);
    let courier = FakeCourier::default();
    let (runtime, handle) = Runtime::new(
        NoKill,
        clock.clone(),
        Arc::new(Telemetry::new()),
        Source::Sim,
        "test",
    );
    tokio::spawn(runtime.with_courier(courier.clone()).run());
    let sink = Sink::default();
    handle.subscribe(sink.clone()).await.unwrap();
    // Sin sesiones tmux todavía: desbloquea el reparto de alias.
    handle.mail_reachable(Vec::new()).await;
    for a in [
        agent(Provider::Claude, 10, "/Users/dev/tienda-web"),
        agent(Provider::Codex, 20, "/Users/dev/api"),
    ] {
        handle.register_agent(a, Timestamp(1_000)).await;
    }
    (handle, courier, sink, clock)
}

/// Deja correr el actor unos cuantos flushes.
async fn settle() {
    for _ in 0..5 {
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
    }
}

fn last_mail(sink: &Sink) -> kiti_protocol::MailState {
    sink.0
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|m| match m {
            ServerMsg::Mail(s) => Some(s.clone()),
            _ => None,
        })
        .expect("ningún ServerMsg::Mail")
}

/// `kiti-mcp` (900) es hijo del agente.
fn from(pid: u32) -> Vec<Pid> {
    vec![Pid(900), Pid(pid), Pid(1)]
}

fn send(to: &str, text: &str) -> SendRequest {
    SendRequest {
        to: to.into(),
        text: text.into(),
        reply_to: None,
    }
}

#[tokio::test(start_paused = true)]
async fn kiti_agents_dice_quien_soy_y_quien_mas_hay() {
    let (h, _, _, _) = setup().await;
    h.mail_reachable(vec![(Pid(20), "kiti-20".into())]).await;
    let r = h.mcp_agents(from(10)).await.unwrap();
    assert_eq!(r.you, "claude-1");
    assert_eq!(r.peers.len(), 1);
    assert_eq!(r.peers[0].alias, "codex-1");
    assert_eq!(r.peers[0].provider, "codex");
    assert_eq!(r.peers[0].project, "api");
    assert_eq!(r.peers[0].status, "idle");
    assert!(r.peers[0].reachable);
    let err = h.mcp_agents(vec![Pid(900), Pid(1)]).await.unwrap_err();
    assert_eq!(err.code, "unknown_sender");
}

#[tokio::test(start_paused = true)]
async fn un_mensaje_se_escribe_en_la_terminal_del_destinatario_libre() {
    let (h, courier, sink, _) = setup().await;
    let sent = h
        .mcp_send(from(10), send("codex-1", "¿revisas mi diff?"))
        .await
        .unwrap();
    assert_eq!(sent.state, "queued");
    settle().await;
    // Aún no se sabe que se le pueda escribir: espera.
    assert!(courier.0.lock().unwrap().is_empty());
    h.mail_reachable(vec![(Pid(20), "kiti-20".into())]).await;
    settle().await;
    let typed = courier.0.lock().unwrap().clone();
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0].0, 20);
    assert!(
        typed[0]
            .1
            .starts_with("[Kiti] Mensaje 1 de claude-1 (tienda-web):\n¿revisas mi diff?")
    );
    assert_eq!(typed[0].2, vec![sent.id]);
    let mail = last_mail(&sink);
    assert_eq!(
        mail.threads[0].messages[0].state,
        MailMessageState::Delivered
    );
    assert_eq!(mail.aliases.len(), 2);
}

#[tokio::test(start_paused = true)]
async fn no_se_le_vuelve_a_escribir_hasta_que_atiende_o_pasa_un_minuto() {
    let (h, courier, _, clock) = setup().await;
    h.mail_reachable(vec![(Pid(20), "kiti-20".into())]).await;
    h.mcp_send(from(10), send("codex-1", "uno")).await.unwrap();
    settle().await;
    h.mcp_send(from(10), send("codex-1", "dos")).await.unwrap();
    settle().await;
    assert_eq!(courier.0.lock().unwrap().len(), 1);
    clock.advance(61_000);
    settle().await;
    let typed = courier.0.lock().unwrap().clone();
    assert_eq!(typed.len(), 2);
    assert!(typed[1].1.contains("\ndos\n"));
}

#[tokio::test(start_paused = true)]
async fn si_falla_la_escritura_se_reintenta() {
    let (h, courier, _, _) = setup().await;
    h.mail_reachable(vec![(Pid(20), "kiti-20".into())]).await;
    let sent = h.mcp_send(from(10), send("codex-1", "uno")).await.unwrap();
    settle().await;
    h.mail_undeliver(vec![sent.id]).await;
    settle().await;
    assert_eq!(courier.0.lock().unwrap().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn sin_terminal_el_destinatario_lo_lee_con_inbox() {
    let (h, courier, _, _) = setup().await;
    let sent = h
        .mcp_send(from(20), send("claude-1", "el total va sin impuestos"))
        .await
        .unwrap();
    settle().await;
    assert!(courier.0.lock().unwrap().is_empty());
    let inbox = h.mcp_inbox(from(10)).await.unwrap();
    assert_eq!(inbox.messages.len(), 1);
    let m = &inbox.messages[0];
    assert_eq!(m.id, sent.id);
    assert_eq!(m.from, "codex-1");
    assert_eq!(m.project, "api");
    assert_eq!(m.subject, "el total va sin impuestos");
    assert!(h.mcp_inbox(from(10)).await.unwrap().messages.is_empty());
}

#[tokio::test(start_paused = true)]
async fn en_manual_espera_al_usuario_y_los_errores_tienen_codigo() {
    let (h, courier, sink, _) = setup().await;
    h.mail_reachable(vec![(Pid(20), "kiti-20".into())]).await;
    h.mail(MailAction::Mode {
        mode: MailMode::Manual,
    })
    .await;
    let sent = h.mcp_send(from(10), send("codex-1", "uno")).await.unwrap();
    assert_eq!(sent.state, "pending");
    settle().await;
    assert!(courier.0.lock().unwrap().is_empty());
    let mail = last_mail(&sink);
    assert_eq!(mail.mode, MailMode::Manual);
    assert_eq!(mail.attention, 1);
    h.mail(MailAction::Approve { id: sent.id }).await;
    settle().await;
    assert_eq!(courier.0.lock().unwrap().len(), 1);
    assert_eq!(last_mail(&sink).attention, 0);

    let err = h
        .mcp_send(from(10), send("claude-1", "a mí"))
        .await
        .unwrap_err();
    assert_eq!(err.code, "self");
    let err = h.mcp_send(from(10), send("agy-3", "x")).await.unwrap_err();
    assert_eq!(err.code, "unknown_recipient");
}

#[tokio::test(start_paused = true)]
async fn recuerda_y_recupera_pulso_indice_y_detalle_del_proyecto() {
    let (h, _, sink, _) = setup().await;
    // El agente claude-1 (pid 10, /Users/dev/tienda-web) tiene un prompt inicial, un archivo y una tarea.
    let mut info = kiti_core::domain::AgentInfo {
        prompt: Some("Refactorizar pasarela de pagos".into()),
        touched_file: Some("src/payments.rs".into()),
        tasks: Some(vec![
            kiti_core::domain::TaskItem {
                text: "Configurar webhooks".into(),
                status: kiti_core::domain::TaskStatus::Completed,
            },
            kiti_core::domain::TaskItem {
                text: "Validar firma HMAC".into(),
                status: kiti_core::domain::TaskStatus::InProgress,
            },
        ]),
        ..Default::default()
    };
    h.hook_from(
        Provider::Claude,
        kiti_core::ingestion::HookEvent {
            session: kiti_core::domain::SessionId::new("s-1"),
            cwd: "/Users/dev/tienda-web".into(),
            name: "UserPromptSubmit".into(),
            signal: kiti_core::domain::Signal::PromptSubmit,
            info: std::mem::take(&mut info),
        },
        from(10),
        Timestamp(1_100),
    )
    .await;

    let rem = h
        .mcp_remember(
            from(10),
            RememberRequest {
                kind: "gotcha".into(),
                title: "Webhooks de Stripe requieren raw body".into(),
                detail: "No parsear JSON antes de verificar la firma HMAC.".into(),
                files: vec!["src/payments.rs".into()],
            },
        )
        .await
        .unwrap();
    assert_eq!(rem.id, 1);
    assert_eq!(rem.project, "tienda-web");
    settle().await;

    // 1. Recall sin query devuelve el pulso de sesiones y el índice ligero (detail = None)
    let pulse = h
        .mcp_recall(from(10), RecallRequest { query: None })
        .await
        .unwrap();
    assert_eq!(pulse.sessions.len(), 1);
    assert_eq!(
        pulse.sessions[0].initial_prompt.as_deref(),
        Some("Refactorizar pasarela de pagos")
    );
    assert_eq!(pulse.sessions[0].touched_files, vec!["src/payments.rs"]);
    assert_eq!(
        pulse.sessions[0].pending_tasks,
        vec!["[en curso] Validar firma HMAC"]
    );
    assert_eq!(pulse.memories.len(), 1);
    assert_eq!(
        pulse.memories[0].title,
        "Webhooks de Stripe requieren raw body"
    );
    assert!(pulse.memories[0].detail.is_none());

    // 2. Recall con query devuelve el detalle completo
    let search = h
        .mcp_recall(
            from(10),
            RecallRequest {
                query: Some("HMAC".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(search.memories.len(), 1);
    assert_eq!(
        search.memories[0].detail.as_deref(),
        Some("No parsear JSON antes de verificar la firma HMAC.")
    );

    // 3. El estado se publica a la UI por ServerMsg::Memory
    let mem_msg = sink
        .0
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|m| match m {
            ServerMsg::Memory { memories } => Some(memories.clone()),
            _ => None,
        })
        .expect("ningún ServerMsg::Memory");
    assert_eq!(mem_msg.len(), 1);
    assert_eq!(mem_msg[0].title, "Webhooks de Stripe requieren raw body");
}

fn process_key(provider: Provider, pid: u32) -> AliasKey {
    AliasKey::Process(kiti_core::domain::AgentId {
        provider,
        pid: Pid(pid),
        start_time: ProcessStartTime(100),
    })
}

/// Runtime con alias guardados de un arranque anterior; los agentes llegan en otro orden.
async fn setup_with(
    initial: Vec<AliasEntry>,
) -> (RuntimeHandle, FakeAliasStore, Sink, ManualClock) {
    let clock = ManualClock::new(1_000);
    let store = FakeAliasStore::default();
    let (runtime, handle) = Runtime::new(
        NoKill,
        clock.clone(),
        Arc::new(Telemetry::new()),
        Source::Sim,
        "test",
    );
    tokio::spawn(
        runtime
            .with_courier(FakeCourier::default())
            .with_aliases(initial, store.clone())
            .run(),
    );
    let sink = Sink::default();
    handle.subscribe(sink.clone()).await.unwrap();
    (handle, store, sink, clock)
}

#[tokio::test(start_paused = true)]
async fn los_alias_guardados_sobreviven_al_reinicio_aunque_el_escaner_llegue_en_otro_orden() {
    let initial = vec![
        AliasEntry {
            key: process_key(Provider::Claude, 10),
            alias: "claude-1".into(),
            released_at: None,
        },
        AliasEntry {
            key: AliasKey::Tmux("kiti-5".into()),
            alias: "claude-2".into(),
            released_at: None,
        },
        AliasEntry {
            key: process_key(Provider::Claude, 99),
            alias: "claude-3".into(),
            released_at: None,
        },
    ];
    let (h, store, _, _) = setup_with(initial).await;
    // Llegan en orden inverso al guardado; el 99 ya no existe.
    h.register_agent(agent(Provider::Claude, 20, "/w/b"), Timestamp(1_000))
        .await;
    h.register_agent(agent(Provider::Claude, 10, "/w/a"), Timestamp(1_000))
        .await;
    // Antes del primer MailReachable no hay alias.
    assert_eq!(h.mcp_agents(from(10)).await.unwrap().you, "");
    h.mail_reachable(vec![(Pid(20), "kiti-5".into())]).await;
    assert_eq!(h.mcp_agents(from(10)).await.unwrap().you, "claude-1");
    assert_eq!(h.mcp_agents(from(20)).await.unwrap().you, "claude-2");
    // El 99 quedó liberado y reservado; un recién llegado no hereda claude-3.
    h.register_agent(agent(Provider::Claude, 30, "/w/c"), Timestamp(1_000))
        .await;
    assert_eq!(h.mcp_agents(from(30)).await.unwrap().you, "claude-4");
    let saved = store.0.lock().unwrap().last().cloned().unwrap();
    let released = saved.iter().find(|e| e.alias == "claude-3").unwrap();
    assert!(released.released_at.is_some());
}

#[tokio::test(start_paused = true)]
async fn un_agente_relanzado_en_la_misma_sesion_tmux_recupera_su_alias() {
    let (h, _, _, _) = setup_with(Vec::new()).await;
    h.mail_reachable(Vec::new()).await;
    h.register_agent(agent(Provider::Claude, 10, "/w/a"), Timestamp(1_000))
        .await;
    assert_eq!(h.mcp_agents(from(10)).await.unwrap().you, "claude-1");
    h.register_agent(agent(Provider::Claude, 11, "/w/b"), Timestamp(1_000))
        .await;
    assert_eq!(h.mcp_agents(from(11)).await.unwrap().you, "claude-2");
    h.mail_reachable(vec![(Pid(10), "kiti-10".into())]).await;
    assert_eq!(h.mcp_agents(from(10)).await.unwrap().you, "claude-1");
    // `claude --continue` en kiti-10: el pid 10 muere y nace el 12 en la misma sesión.
    h.reconcile(
        vec![agent(Provider::Claude, 11, "/w/b")],
        Vec::new(),
        Vec::new(),
        Timestamp(2_000),
    )
    .await;
    h.register_agent(agent(Provider::Claude, 12, "/w/a"), Timestamp(1_000))
        .await;
    h.mail_reachable(vec![(Pid(12), "kiti-10".into())]).await;
    assert_eq!(h.mcp_agents(from(12)).await.unwrap().you, "claude-1");
    assert_eq!(h.mcp_agents(from(11)).await.unwrap().you, "claude-2");
}

#[tokio::test(start_paused = true)]
async fn un_agente_lanzado_a_mitad_conserva_su_alias_al_aparecer_en_tmux() {
    let (h, _, _, _) = setup_with(Vec::new()).await;
    h.mail_reachable(Vec::new()).await;
    h.register_agent(agent(Provider::Claude, 10, "/w/a"), Timestamp(1_000))
        .await;
    assert_eq!(h.mcp_agents(from(10)).await.unwrap().you, "claude-1");
    h.register_agent(agent(Provider::Claude, 11, "/w/b"), Timestamp(1_000))
        .await;
    assert_eq!(h.mcp_agents(from(11)).await.unwrap().you, "claude-2");
    h.mail_reachable(vec![(Pid(11), "kiti-7".into())]).await;
    assert_eq!(h.mcp_agents(from(11)).await.unwrap().you, "claude-2");
    assert_eq!(h.mcp_agents(from(10)).await.unwrap().you, "claude-1");
}
