//! Runtime actor y coalescedor de diffs (ADR 0002 §4, ADR 0004 §1).
//!
//! Una única tarea es dueña del `AgentRegistry` (sin contención de `Mutex`).
//! Coalesce los cambios a ~10 Hz (máx. 1 `Diff` cada 100 ms si hay cambios).
//! La hora siempre sale del port `Clock`; los kills corren fuera del actor.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval};

use crate::application::mail;
use crate::application::projection::{project_diff, project_provider, project_snapshot};
use crate::application::scanner::TranscriptUpdate;
use crate::application::summary::Summary;
use crate::application::telemetry::Telemetry;
use crate::domain::{
    AgentId, AgentInput, AgentRegistry, AgentStats, AlertPolicy, DetectedAgent, DomainEvent,
    Events, GitStatus, Mailbox, MessageId, Pid, Provider, QuotaPolicy, QuotaWindow, SessionId,
    StalePolicy, StatusKind, ThreadId, Timestamp,
};
use crate::ingestion::HookEvent;
use crate::ingestion::claude::SessionFile;
use crate::ports::{
    AgentKiller, AlertNotice, AlertSink, Clock, CwdProjects, DiffPublisher, KillError, KillOutcome,
    KillTarget, MailCourier, ProjectResolver, SelfMonitor,
};
use kiti_protocol::mcp;
use kiti_protocol::{Hello, KillResult, PROTOCOL_VERSION, ServerMsg, Source};
use tracing::{debug, info};

pub const FLUSH_INTERVAL: Duration = Duration::from_millis(100);
pub const METRICS_INTERVAL: Duration = Duration::from_secs(1);
pub const STALE_TICK_INTERVAL: Duration = Duration::from_secs(2);
/// Gracia entre SIGTERM y SIGKILL (ADR 0002 §5).
pub const KILL_GRACE: Duration = Duration::from_secs(5);
/// Tras escribir en la terminal de un agente, no se le escribe otra vez hasta verlo trabajar,
/// o hasta pasado este tiempo si nunca llega a verse (ADR 0017).
pub const MAIL_RETRY: Duration = Duration::from_secs(60);

/// Lo que la UI puede hacer con los mensajes (ADR 0017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailAction {
    Mode(kiti_protocol::MailMode),
    Pause(bool),
    Approve(u64),
    Discard(u64),
    Continue(u64),
    Close(u64),
}

type McpReply<T> = oneshot::Sender<Result<T, mcp::ErrorReply>>;

pub enum RuntimeCommand {
    RegisterAgent {
        detected: DetectedAgent,
        now: Timestamp,
    },
    ApplyInput {
        id: AgentId,
        input: AgentInput,
        now: Timestamp,
    },
    /// Resultado de un escaneo de procesos ya clasificado, más los archivos de sesión de Claude.
    Reconcile {
        detected: Vec<DetectedAgent>,
        sessions: Vec<SessionFile>,
        /// Lo nuevo de cada transcript.
        transcripts: Vec<TranscriptUpdate>,
        now: Timestamp,
    },
    /// Proyectos (raíz git o carpeta) con algún agente vivo, para consultar su git (ADR 0016).
    LiveProjects(oneshot::Sender<Vec<String>>),
    /// Estado de git de esos proyectos (`None`: no es un repositorio).
    Git(Vec<(String, Option<GitStatus>)>),
    /// Modelo, contexto y coste de una sesión (ADR 0016).
    SessionStats {
        session: SessionId,
        stats: AgentStats,
        now: Timestamp,
    },
    /// Cuota de uso de un proveedor (ADR 0015).
    Quota {
        provider: Provider,
        windows: Vec<QuotaWindow>,
    },
    /// Antigravity (ADR 0014): título y espera de permiso leídos de su base de datos.
    AntigravityLive {
        updates: Vec<(SessionId, Option<String>, bool)>,
        now: Timestamp,
    },
    /// Un hook ya traducido por la ACL de su CLI.
    Hook {
        provider: Provider,
        event: HookEvent,
        /// PID del forwarder y sus ancestros (vacío para Claude, que llega por http directo).
        ancestry: Vec<Pid>,
        now: Timestamp,
        received: Instant,
    },
    /// `cwd` y título del agente: diálogo de confirmación e "Ir a su terminal".
    Describe {
        id: AgentId,
        reply: oneshot::Sender<Option<AgentBrief>>,
    },
    Subscribe {
        publisher: Box<dyn DiffPublisher>,
        reply: oneshot::Sender<Hello>,
    },
    SetDebugStream(bool),
    /// Enfocar una oficina en la UI; si no hay ventana, al conectarse la siguiente.
    Focus(AgentId),
    /// Cuenta de agentes por color, para la barra de menú.
    Summary(oneshot::Sender<Summary>),
    KillAgent {
        id: AgentId,
        reply: oneshot::Sender<KillResult>,
    },
    /// `kiti_agents` (ADR 0017): `ancestry` es el de `kiti-mcp`, para saber quién pregunta.
    McpAgents {
        ancestry: Vec<Pid>,
        reply: McpReply<mcp::AgentsReply>,
    },
    McpSend {
        ancestry: Vec<Pid>,
        request: mcp::SendRequest,
        reply: McpReply<mcp::SendReply>,
    },
    McpInbox {
        ancestry: Vec<Pid>,
        reply: McpReply<mcp::InboxReply>,
    },
    /// PIDs de los agentes lanzados desde Kiti (sesiones tmux): a ellos se les puede escribir.
    MailReachable(Vec<Pid>),
    /// La escritura en la terminal falló: esos mensajes vuelven a la cola.
    MailUndeliver(Vec<u64>),
    Mail(MailAction),
    /// Interno: la tarea de kill terminó y devuelve el resultado al actor.
    #[doc(hidden)]
    KillFinished {
        id: AgentId,
        outcome: Result<KillOutcome, KillError>,
        reply: oneshot::Sender<KillResult>,
    },
}

#[derive(Clone)]
pub struct RuntimeHandle {
    tx: mpsc::Sender<RuntimeCommand>,
    telemetry: Arc<Telemetry>,
}

impl RuntimeHandle {
    pub async fn register_agent(&self, detected: DetectedAgent, now: Timestamp) {
        let _ = self
            .tx
            .send(RuntimeCommand::RegisterAgent { detected, now })
            .await;
    }

    pub async fn apply_input(&self, id: AgentId, input: AgentInput, now: Timestamp) {
        let _ = self
            .tx
            .send(RuntimeCommand::ApplyInput { id, input, now })
            .await;
    }

    pub async fn reconcile(
        &self,
        detected: Vec<DetectedAgent>,
        sessions: Vec<SessionFile>,
        transcripts: Vec<TranscriptUpdate>,
        now: Timestamp,
    ) {
        let _ = self
            .tx
            .send(RuntimeCommand::Reconcile {
                detected,
                sessions,
                transcripts,
                now,
            })
            .await;
    }

    /// Proyectos con algún agente vivo (ADR 0016).
    pub async fn live_projects(&self) -> Vec<String> {
        let (reply, rx) = oneshot::channel();
        if self
            .tx
            .send(RuntimeCommand::LiveProjects(reply))
            .await
            .is_err()
        {
            return Vec::new();
        }
        rx.await.unwrap_or_default()
    }

    /// Nuevo estado de git de cada proyecto (ADR 0016).
    pub async fn git(&self, statuses: Vec<(String, Option<GitStatus>)>) {
        let _ = self.tx.send(RuntimeCommand::Git(statuses)).await;
    }

    /// Modelo, contexto y coste de una sesión de Claude (ADR 0016).
    pub async fn session_stats(&self, session: SessionId, stats: AgentStats, now: Timestamp) {
        let _ = self
            .tx
            .send(RuntimeCommand::SessionStats {
                session,
                stats,
                now,
            })
            .await;
    }

    /// Nueva lectura de la cuota de `provider` (ADR 0015).
    pub async fn quota(&self, provider: Provider, windows: Vec<QuotaWindow>) {
        let _ = self
            .tx
            .send(RuntimeCommand::Quota { provider, windows })
            .await;
    }

    /// `(sesión, título, esperando permiso)` de las conversaciones recientes de Antigravity.
    pub async fn antigravity_live(
        &self,
        updates: Vec<(SessionId, Option<String>, bool)>,
        now: Timestamp,
    ) {
        let _ = self
            .tx
            .send(RuntimeCommand::AntigravityLive { updates, now })
            .await;
    }

    pub async fn hook(&self, provider: Provider, event: HookEvent, now: Timestamp) {
        self.hook_from(provider, event, Vec::new(), now).await;
    }

    pub async fn hook_from(
        &self,
        provider: Provider,
        event: HookEvent,
        ancestry: Vec<Pid>,
        now: Timestamp,
    ) {
        let _ = self
            .tx
            .send(RuntimeCommand::Hook {
                provider,
                event,
                ancestry,
                now,
                received: Instant::now(),
            })
            .await;
    }

    pub async fn describe(&self, id: AgentId) -> Option<AgentBrief> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::Describe { id, reply })
            .await
            .ok()?;
        rx.await.ok().flatten()
    }

    pub async fn subscribe<P: DiffPublisher>(&self, publisher: P) -> Result<Hello, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::Subscribe {
                publisher: Box::new(publisher),
                reply: reply_tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        reply_rx.await.map_err(|e| e.to_string())
    }

    pub async fn summary(&self) -> Summary {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(RuntimeCommand::Summary(reply)).await.is_err() {
            return Summary::default();
        }
        rx.await.unwrap_or_default()
    }

    /// Pide a la UI que enfoque y seleccione la oficina `id` (clic en una notificación).
    pub async fn focus(&self, id: AgentId) {
        let _ = self.tx.send(RuntimeCommand::Focus(id)).await;
    }

    pub async fn set_debug_stream(&self, on: bool) {
        let _ = self.tx.send(RuntimeCommand::SetDebugStream(on)).await;
    }

    pub async fn kill_agent(&self, id: AgentId) -> Result<KillResult, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(RuntimeCommand::KillAgent {
                id,
                reply: reply_tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        reply_rx.await.map_err(|e| e.to_string())
    }

    async fn mcp<T>(
        &self,
        make: impl FnOnce(McpReply<T>) -> RuntimeCommand,
    ) -> Result<T, mcp::ErrorReply> {
        let (reply, rx) = oneshot::channel();
        let closed = || mcp::ErrorReply {
            code: "closed".into(),
            message: "Kiti se está cerrando".into(),
        };
        self.tx.send(make(reply)).await.map_err(|_| closed())?;
        rx.await.map_err(|_| closed())?
    }

    /// `kiti_agents`: los demás agentes vivos (ADR 0017).
    pub async fn mcp_agents(
        &self,
        ancestry: Vec<Pid>,
    ) -> Result<mcp::AgentsReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpAgents { ancestry, reply })
            .await
    }

    pub async fn mcp_send(
        &self,
        ancestry: Vec<Pid>,
        request: mcp::SendRequest,
    ) -> Result<mcp::SendReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpSend {
            ancestry,
            request,
            reply,
        })
        .await
    }

    pub async fn mcp_inbox(&self, ancestry: Vec<Pid>) -> Result<mcp::InboxReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpInbox { ancestry, reply })
            .await
    }

    /// Agentes a los que Kiti puede escribir (sesiones tmux).
    pub async fn mail_reachable(&self, pids: Vec<Pid>) {
        let _ = self.tx.send(RuntimeCommand::MailReachable(pids)).await;
    }

    pub async fn mail_undeliver(&self, ids: Vec<u64>) {
        let _ = self.tx.send(RuntimeCommand::MailUndeliver(ids)).await;
    }

    pub async fn mail(&self, action: MailAction) {
        let _ = self.tx.send(RuntimeCommand::Mail(action)).await;
    }

    /// El actor terminó (se soltaron todos los handles o el runtime se apagó).
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
    }
}

fn git_msg(project: &str, status: Option<&GitStatus>) -> ServerMsg {
    ServerMsg::Git {
        project: project.to_owned(),
        git: status.map(|s| kiti_protocol::GitInfo {
            branch: s.branch.clone(),
            ahead: s.ahead,
            behind: s.behind,
            changed: s.changed,
        }),
    }
}

fn quota_msg(provider: Provider, windows: &[QuotaWindow]) -> ServerMsg {
    ServerMsg::Quota {
        provider: project_provider(provider),
        windows: windows
            .iter()
            .map(|w| kiti_protocol::QuotaWindow {
                label: w.label.clone(),
                weekly: w.weekly,
                remaining_pct: (w.remaining * 100.0).round().clamp(0.0, 100.0) as u8,
                resets_at: w.resets_at,
            })
            .collect(),
    }
}

/// Lo justo de un agente para el shell: dónde trabaja y cómo se llama su conversación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBrief {
    pub cwd: String,
    pub title: Option<String>,
}

fn log_status_changes(events: &[DomainEvent]) {
    for ev in events {
        if let DomainEvent::StatusChanged {
            id,
            from,
            to,
            reason,
            ..
        } = ev
        {
            info!(event = "status.changed", agent_id = %id, ?from, ?to, ?reason);
        }
    }
}

pub struct Runtime<K: AgentKiller + Clone, C: Clock> {
    registry: AgentRegistry,
    rx: mpsc::Receiver<RuntimeCommand>,
    /// Para que las tareas de kill devuelvan su resultado al actor.
    self_tx: mpsc::Sender<RuntimeCommand>,
    publisher: Option<Box<dyn DiffPublisher>>,
    monitor: Option<Box<dyn SelfMonitor>>,
    killer: K,
    clock: C,
    telemetry: Arc<Telemetry>,
    seq: u64,
    debug_stream: bool,
    /// Hook más antiguo aún no publicado: mide hook → Diff.
    oldest_unpublished_hook: Option<Instant>,
    source: Source,
    app_version: String,
    stale_policy: StalePolicy,
    projects: Box<dyn ProjectResolver>,
    /// Qué cambios avisan al usuario (ADR 0010) y quién los muestra. Sin sink, no hay avisos.
    alert_policy: AlertPolicy,
    alerts: Option<Box<dyn AlertSink>>,
    /// Foco pedido sin ventana conectada: se manda tras el siguiente Snapshot.
    pending_focus: Option<AgentId>,
    /// Última cuota de cada proveedor (ADR 0015): se reenvía tras cada Snapshot.
    quotas: std::collections::BTreeMap<Provider, Vec<QuotaWindow>>,
    /// Último estado de git de cada proyecto (ADR 0016): se reenvía tras cada Snapshot.
    git: std::collections::BTreeMap<String, Option<GitStatus>>,
    quota_policy: QuotaPolicy,
    /// Mensajes entre agentes (ADR 0017).
    mailbox: Mailbox,
    mail_dirty: bool,
    courier: Option<Box<dyn MailCourier>>,
    reachable: std::collections::HashSet<Pid>,
    /// A quién se escribió y cuándo: no se le vuelve a escribir hasta verlo trabajar.
    typed: std::collections::HashMap<AgentId, Timestamp>,
}

impl<K: AgentKiller + Clone, C: Clock + 'static> Runtime<K, C> {
    /// Crea el actor sin arrancarlo: quien llama decide en qué runtime de tokio corre `run()`
    /// (en la app, el de Tauri).
    pub fn new(
        killer: K,
        clock: C,
        telemetry: Arc<Telemetry>,
        source: Source,
        app_version: impl Into<String>,
    ) -> (Self, RuntimeHandle) {
        let (tx, rx) = mpsc::channel(1024);
        let runtime = Self {
            registry: AgentRegistry::new(),
            rx,
            self_tx: tx.clone(),
            publisher: None,
            monitor: None,
            killer,
            clock,
            telemetry: Arc::clone(&telemetry),
            seq: 0,
            debug_stream: false,
            oldest_unpublished_hook: None,
            source,
            app_version: app_version.into(),
            stale_policy: StalePolicy::default(),
            projects: Box::new(CwdProjects),
            alert_policy: AlertPolicy::default(),
            alerts: None,
            pending_focus: None,
            quotas: std::collections::BTreeMap::new(),
            git: std::collections::BTreeMap::new(),
            quota_policy: QuotaPolicy::default(),
            mailbox: Mailbox::default(),
            mail_dirty: false,
            courier: None,
            reachable: std::collections::HashSet::new(),
            typed: std::collections::HashMap::new(),
        };
        (runtime, RuntimeHandle { tx, telemetry })
    }

    /// Muestreo de RSS y CPU del proceso para el panel de debug.
    pub fn with_monitor(mut self, monitor: impl SelfMonitor) -> Self {
        self.monitor = Some(Box::new(monitor));
        self
    }

    /// Cómo agrupar oficinas por proyecto (ADR 0009). Por defecto, cada carpeta es uno.
    pub fn with_projects(mut self, projects: impl ProjectResolver) -> Self {
        self.projects = Box::new(projects);
        self
    }

    /// Avisos al usuario (ADR 0010). En el simulador no se instala: serían ruido.
    pub fn with_alerts(mut self, sink: impl AlertSink) -> Self {
        self.alerts = Some(Box::new(sink));
        self
    }

    /// Quién escribe los mensajes en las terminales (ADR 0017). Sin él, solo hay `kiti_inbox`.
    pub fn with_courier(mut self, courier: impl MailCourier) -> Self {
        self.courier = Some(Box::new(courier));
        self
    }

    /// Pasa los eventos por la política de avisos y entrega los que tocan.
    fn notify(&mut self, events: &[DomainEvent], now: Timestamp) {
        let Some(sink) = self.alerts.as_mut() else {
            return;
        };
        for alert in self.alert_policy.observe(events, now) {
            let Some(agent) = self.registry.get(&alert.id) else {
                continue;
            };
            let cwd = agent.cwd().as_str().to_owned();
            let notice = AlertNotice {
                kind: alert.kind,
                agent: alert.id,
                provider: alert.id.provider,
                project: self.projects.project_of(&cwd),
                cwd,
                title: agent.title().map(str::to_owned),
            };
            info!(event = "alert.raised", agent_id = %alert.id, kind = ?alert.kind);
            sink.alert(notice);
        }
    }

    /// Atajo para tests y binarios que ya están dentro de un runtime de tokio.
    pub fn spawn(
        killer: K,
        clock: C,
        telemetry: Arc<Telemetry>,
        source: Source,
        app_version: impl Into<String>,
    ) -> RuntimeHandle {
        let (runtime, handle) = Self::new(killer, clock, telemetry, source, app_version);
        tokio::spawn(runtime.run());
        handle
    }

    pub async fn run(mut self) {
        let mut flush_timer = interval(FLUSH_INTERVAL);
        let mut metrics_timer = interval(METRICS_INTERVAL);
        let mut stale_timer = interval(STALE_TICK_INTERVAL);

        loop {
            tokio::select! {
                cmd = self.rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    self.handle(cmd);
                }
                _ = flush_timer.tick() => self.flush_diff(),
                _ = metrics_timer.tick() => {
                    if self.debug_stream {
                        self.send_metrics();
                    }
                }
                _ = stale_timer.tick() => {
                    let now = self.clock.now();
                    let events = self.registry.tick(&self.stale_policy, now);
                    self.notify(&events, now);
                }
            }
        }
    }

    fn handle(&mut self, cmd: RuntimeCommand) {
        match cmd {
            RuntimeCommand::RegisterAgent { detected, now } => {
                self.registry.register(detected, now);
                self.telemetry.record_event();
            }
            RuntimeCommand::ApplyInput { id, input, now } => {
                self.registry.apply(&id, &input, now);
                self.telemetry.record_event();
            }
            RuntimeCommand::Reconcile {
                detected,
                sessions,
                transcripts,
                now,
            } => {
                let mut events = self.registry.reconcile(&detected, now);
                for sf in &sessions {
                    events.extend(self.registry.session_file(
                        Provider::Claude,
                        sf.pid,
                        &sf.session,
                        sf.activity,
                        now,
                    ));
                }
                for t in &transcripts {
                    if let Some(tool) = &t.tool {
                        self.registry.transcript(&t.session, tool, now);
                    }
                    if !t.info.is_empty() {
                        self.registry.transcript_info(&t.session, &t.info, now);
                    }
                }
                log_status_changes(&events);
                self.notify(&events, now);
                for ev in events {
                    match ev {
                        DomainEvent::AgentAppeared { id } => {
                            info!(event = "agent.appeared", agent_id = %id);
                        }
                        DomainEvent::AgentExited { id, expected } => {
                            info!(event = "agent.exited", agent_id = %id, expected);
                        }
                        _ => {}
                    }
                }
            }
            RuntimeCommand::Hook {
                provider,
                event,
                ancestry,
                now,
                received,
            } => {
                debug!(event = "hook.received", provider = %provider, name = %event.name);
                let events = self.registry.hook_info(
                    provider,
                    &event.session,
                    &event.cwd,
                    &ancestry,
                    &event.signal,
                    &event.info,
                    now,
                );
                log_status_changes(&events);
                self.notify(&events, now);
                self.telemetry.record_event();
                self.oldest_unpublished_hook.get_or_insert(received);
            }
            RuntimeCommand::LiveProjects(reply) => {
                let mut projects: Vec<String> = self
                    .registry
                    .agents()
                    .filter(|a| !a.is_dead())
                    .map(|a| a.cwd().as_str().to_owned())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|cwd| self.projects.project_of(&cwd))
                    .collect();
                projects.sort();
                projects.dedup();
                let _ = reply.send(projects);
            }
            RuntimeCommand::Git(statuses) => {
                for (project, status) in statuses {
                    if self.git.get(&project) == Some(&status) {
                        continue;
                    }
                    let msg = git_msg(&project, status.as_ref());
                    self.git.insert(project, status);
                    if let Some(p) = self.publisher.as_mut()
                        && !p.publish(msg)
                    {
                        self.drop_publisher();
                    }
                }
            }
            RuntimeCommand::SessionStats {
                session,
                stats,
                now,
            } => {
                self.registry.session_stats(&session, &stats, now);
            }
            RuntimeCommand::Quota { provider, windows } => {
                debug!(event = "quota.updated", provider = %provider, windows = windows.len());
                let alerts = self.quota_policy.observe(provider, &windows);
                if let Some(sink) = self.alerts.as_mut() {
                    for alert in alerts {
                        info!(event = "alert.raised", provider = %provider, kind = "QuotaLow");
                        sink.quota_low(alert);
                    }
                }
                let msg = quota_msg(provider, &windows);
                self.quotas.insert(provider, windows);
                if let Some(p) = self.publisher.as_mut()
                    && !p.publish(msg)
                {
                    self.drop_publisher();
                }
            }
            RuntimeCommand::AntigravityLive { updates, now } => {
                let mut events = Events::new();
                for (session, title, waiting) in &updates {
                    events.extend(self.registry.antigravity_live(
                        session,
                        title.as_deref(),
                        *waiting,
                        now,
                    ));
                }
                log_status_changes(&events);
                self.notify(&events, now);
            }
            RuntimeCommand::Describe { id, reply } => {
                let _ = reply.send(self.registry.get(&id).map(|a| AgentBrief {
                    cwd: a.cwd().as_str().to_owned(),
                    title: a.title().map(str::to_owned),
                }));
            }
            RuntimeCommand::Subscribe {
                mut publisher,
                reply,
            } => {
                self.seq = 0;
                let snapshot_agents = self.registry.snapshot();
                if publisher.publish(project_snapshot(
                    self.seq,
                    &snapshot_agents,
                    self.projects.as_mut(),
                )) {
                    for (provider, windows) in &self.quotas {
                        publisher.publish(quota_msg(*provider, windows));
                    }
                    for (project, status) in &self.git {
                        publisher.publish(git_msg(project, status.as_ref()));
                    }
                    self.sync_mail();
                    publisher.publish(ServerMsg::Mail(mail::mail_state(&self.mailbox)));
                    self.mail_dirty = false;
                    if let Some(id) = self.pending_focus.take() {
                        publisher.publish(ServerMsg::Focus {
                            id: kiti_protocol::AgentId(id.to_string()),
                        });
                    }
                    self.publisher = Some(publisher);
                } else {
                    self.drop_publisher();
                }
                let _ = reply.send(Hello {
                    protocol_version: PROTOCOL_VERSION,
                    app_version: self.app_version.clone(),
                    source: self.source,
                });
            }
            RuntimeCommand::SetDebugStream(on) => self.debug_stream = on,
            RuntimeCommand::Focus(id) => {
                let msg = ServerMsg::Focus {
                    id: kiti_protocol::AgentId(id.to_string()),
                };
                match self.publisher.as_mut() {
                    Some(p) => {
                        if !p.publish(msg) {
                            self.drop_publisher();
                        }
                    }
                    None => self.pending_focus = Some(id),
                }
            }
            RuntimeCommand::Summary(reply) => {
                let mut summary = Summary::default();
                for agent in self.registry.agents() {
                    summary.add(agent.status().kind);
                }
                let _ = reply.send(summary);
            }
            RuntimeCommand::KillAgent { id, reply } => self.start_kill(id, reply),
            RuntimeCommand::McpAgents { ancestry, reply } => {
                let _ = reply.send(self.mcp_agents(&ancestry));
            }
            RuntimeCommand::McpSend {
                ancestry,
                request,
                reply,
            } => {
                let _ = reply.send(self.mcp_send(&ancestry, request));
            }
            RuntimeCommand::McpInbox { ancestry, reply } => {
                let _ = reply.send(self.mcp_inbox(&ancestry));
            }
            RuntimeCommand::MailReachable(pids) => {
                self.reachable = pids.into_iter().collect();
            }
            RuntimeCommand::MailUndeliver(ids) => {
                let ids: Vec<MessageId> = ids.into_iter().map(MessageId).collect();
                for m in self.mailbox.messages().filter(|m| ids.contains(&m.id)) {
                    self.typed.remove(&m.to);
                }
                self.mailbox.undeliver(&ids);
                info!(event = "mail.undelivered", count = ids.len());
                self.mail_dirty = true;
            }
            RuntimeCommand::Mail(action) => {
                self.sync_mail();
                let changed = match action {
                    MailAction::Mode(m) => self.mailbox.set_mode(mail::domain_mail_mode(m)),
                    MailAction::Pause(p) => self.mailbox.set_paused(p),
                    MailAction::Approve(id) => self.mailbox.approve(MessageId(id)),
                    MailAction::Discard(id) => self.mailbox.discard(MessageId(id)),
                    MailAction::Continue(id) => self.mailbox.continue_thread(ThreadId(id)),
                    MailAction::Close(id) => self.mailbox.close_thread(ThreadId(id)),
                };
                info!(event = "mail.action", action = ?action, changed);
                self.mail_dirty |= changed;
            }
            RuntimeCommand::KillFinished { id, outcome, reply } => {
                let _ = reply.send(self.finish_kill(id, outcome));
            }
        }
    }

    /// Alias para los agentes vivos; los que murieron sueltan el suyo y lo que se les debía.
    fn sync_mail(&mut self) {
        let live: Vec<AgentId> = self
            .registry
            .agents()
            .filter(|a| !a.is_dead())
            .map(|a| a.id())
            .collect();
        for id in &live {
            if self.mailbox.alias(id).is_none() {
                self.mailbox.join(*id);
                self.mail_dirty = true;
            }
        }
        let gone: Vec<AgentId> = self
            .mailbox
            .members()
            .map(|(id, _)| *id)
            .filter(|id| !live.contains(id))
            .collect();
        for id in gone {
            self.mailbox.leave(&id);
            self.typed.remove(&id);
            self.mail_dirty = true;
        }
    }

    /// Escribe lo pendiente en la terminal de los destinatarios libres: en gris, lanzados desde
    /// Kiti y sin una entrega reciente que aún no hayan empezado a atender.
    fn deliver_mail(&mut self) {
        let now = self.clock.now();
        let registry = &self.registry;
        self.typed.retain(|id, at| {
            registry
                .get(id)
                .is_some_and(|a| a.status().kind == StatusKind::Idle)
                && now.since(*at) < MAIL_RETRY
        });
        let Some(courier) = self.courier.as_mut() else {
            return;
        };
        let (reachable, typed) = (&self.reachable, &self.typed);
        let deliveries = self.mailbox.take_deliveries(|id| {
            reachable.contains(&id.pid)
                && !typed.contains_key(id)
                && registry
                    .get(id)
                    .is_some_and(|a| a.status().kind == StatusKind::Idle)
        });
        for d in deliveries {
            let projects = &mut self.projects;
            let text = mail::delivery_prompt(&d, |m| {
                registry.get(&m.from).map_or_else(
                    || "?".to_owned(),
                    |a| mail::project_name(&projects.project_of(a.cwd().as_str())),
                )
            });
            let ids: Vec<u64> = d.messages.iter().map(|m| m.id.0).collect();
            let wait_ms = d
                .messages
                .first()
                .map_or(0, |m| now.0.saturating_sub(m.sent_at.0));
            info!(event = "mail.delivered", to = %d.to, count = ids.len(), wait_ms, via = "terminal");
            self.typed.insert(d.to, now);
            courier.deliver(d.to.pid, text, ids);
            self.mail_dirty = true;
        }
    }

    fn caller(&mut self, ancestry: &[Pid]) -> Result<AgentId, mcp::ErrorReply> {
        self.sync_mail();
        self.registry.agent_in_ancestry(ancestry).ok_or_else(|| {
            info!(event = "mail.rejected", reason = "unknown_sender");
            mail::unknown_caller()
        })
    }

    fn mcp_agents(&mut self, ancestry: &[Pid]) -> Result<mcp::AgentsReply, mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        let mut peers: Vec<mcp::Peer> = Vec::new();
        for agent in self
            .registry
            .agents()
            .filter(|a| !a.is_dead() && a.id() != me)
        {
            let Some(alias) = self.mailbox.alias(&agent.id()) else {
                continue;
            };
            let project = self.projects.project_of(agent.cwd().as_str());
            peers.push(mail::peer(
                agent,
                alias,
                &project,
                self.reachable.contains(&agent.id().pid),
            ));
        }
        peers.sort_by(|a, b| a.alias.cmp(&b.alias));
        Ok(mcp::AgentsReply {
            you: self.mailbox.alias(&me).unwrap_or_default().to_owned(),
            peers,
        })
    }

    fn mcp_send(
        &mut self,
        ancestry: &[Pid],
        request: mcp::SendRequest,
    ) -> Result<mcp::SendReply, mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        let now = self.clock.now();
        match self.mailbox.send(
            me,
            &request.to,
            &request.text,
            request.reply_to.map(MessageId),
            now,
        ) {
            Ok(sent) => {
                info!(
                    event = "mail.sent",
                    from = self.mailbox.alias(&me).unwrap_or_default(),
                    to = request.to.trim(),
                    len = request.text.chars().count(),
                    thread = sent.thread.0,
                    state = mail::state_code(sent.state),
                );
                self.telemetry.record_mail();
                self.mail_dirty = true;
                if sent.paused_thread {
                    self.alert_mail_paused(sent.thread);
                }
                Ok(mcp::SendReply {
                    id: sent.id.0,
                    thread: sent.thread.0,
                    state: mail::state_code(sent.state).to_owned(),
                })
            }
            Err(e) => {
                info!(event = "mail.rejected", reason = e.code());
                Err(mail::send_error(&e))
            }
        }
    }

    fn alert_mail_paused(&mut self, thread: ThreadId) {
        let Some(t) = self.mailbox.thread(thread) else {
            return;
        };
        let mut who: Vec<&str> = Vec::new();
        for m in self.mailbox.messages().filter(|m| m.thread == thread) {
            for a in [m.from_alias.as_str(), m.to_alias.as_str()] {
                if !who.contains(&a) {
                    who.push(a);
                }
            }
        }
        info!(
            event = "alert.raised",
            kind = "MailPaused",
            thread = thread.0
        );
        if let Some(sink) = self.alerts.as_mut() {
            sink.mail_paused(&t.subject, &who.join(" ⇄ "));
        }
    }

    fn mcp_inbox(&mut self, ancestry: &[Pid]) -> Result<mcp::InboxReply, mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        let messages = self.mailbox.inbox(&me);
        if !messages.is_empty() {
            info!(event = "mail.delivered", to = %me, count = messages.len(), via = "inbox");
            self.mail_dirty = true;
        }
        let out = messages
            .into_iter()
            .map(|m| {
                let project = self.registry.get(&m.from).map_or_else(
                    || "?".to_owned(),
                    |a| mail::project_name(&self.projects.project_of(a.cwd().as_str())),
                );
                mcp::InboxMessage {
                    id: m.id.0,
                    from: m.from_alias,
                    project,
                    thread: m.thread.0,
                    subject: self
                        .mailbox
                        .thread(m.thread)
                        .map(|t| t.subject.clone())
                        .unwrap_or_default(),
                    text: m.text,
                    sent_at: m.sent_at.0,
                }
            })
            .collect();
        Ok(mcp::InboxReply { messages: out })
    }

    fn flush_diff(&mut self) {
        self.sync_mail();
        self.deliver_mail();
        if self.mail_dirty
            && let Some(publisher) = self.publisher.as_mut()
        {
            self.mail_dirty = false;
            if !publisher.publish(ServerMsg::Mail(mail::mail_state(&self.mailbox))) {
                self.drop_publisher();
            }
        }
        let Some(publisher) = self.publisher.as_mut() else {
            return;
        };
        let diff = self.registry.flush_diff();
        if diff.is_empty() {
            return;
        }
        self.seq += 1;
        if !publisher.publish(project_diff(
            self.seq,
            self.clock.now().ms(),
            diff,
            self.projects.as_mut(),
        )) {
            self.drop_publisher();
            return;
        }
        self.telemetry.record_flush();
        if let Some(received) = self.oldest_unpublished_hook.take() {
            // ADR 0006, ficha del Hito 4: hook recibido → Diff publicado.
            self.telemetry
                .record_publish_latency_ms(received.elapsed().as_millis() as u64);
        }
    }

    fn send_metrics(&mut self) {
        let Some(publisher) = self.publisher.as_mut() else {
            return;
        };
        let (rss, cpu) = self.monitor.as_mut().map_or((0, 0.0), |m| m.sample());
        let metrics = self.telemetry.snapshot_core_metrics(
            self.registry.len() as u32,
            rss,
            cpu,
            self.mailbox.pending() as u32,
        );
        if !publisher.publish(ServerMsg::Metrics(metrics)) {
            self.drop_publisher();
        }
    }

    /// La ventana se cerró: se deja de publicar hasta que se vuelva a suscribir.
    fn drop_publisher(&mut self) {
        if self.publisher.take().is_some() {
            info!(event = "ipc.subscriber_dropped");
        }
        self.debug_stream = false;
    }

    /// Marca la salida como esperada y termina el proceso en otra tarea: la gracia de 5 s
    /// no puede congelar el actor (ni los diffs que ve la UI).
    fn start_kill(&mut self, id: AgentId, reply: oneshot::Sender<KillResult>) {
        let Some(agent) = self.registry.get(&id) else {
            let _ = reply.send(KillResult::NotFound);
            return;
        };
        let target = KillTarget {
            id,
            identity: *agent.identity(),
        };
        self.registry
            .apply(&id, &AgentInput::KillRequested, self.clock.now());
        let killer = self.killer.clone();
        let back = self.self_tx.clone();
        tokio::spawn(async move {
            let outcome = killer.terminate(target, KILL_GRACE).await;
            let _ = back
                .send(RuntimeCommand::KillFinished { id, outcome, reply })
                .await;
        });
    }

    fn finish_kill(&mut self, id: AgentId, outcome: Result<KillOutcome, KillError>) -> KillResult {
        match outcome {
            Ok(KillOutcome::Killed) => {
                self.registry
                    .apply(&id, &AgentInput::ProcessGone, self.clock.now());
                KillResult::Killed
            }
            Ok(KillOutcome::Cancelled) => KillResult::Cancelled,
            Ok(KillOutcome::StaleProcess) => KillResult::StaleProcess,
            Ok(KillOutcome::NotFound) => KillResult::NotFound,
            Err(e) => KillResult::Failed {
                message: e.to_string(),
            },
        }
    }
}
