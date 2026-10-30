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
use crate::application::memory;
use crate::application::projection::{project_diff, project_provider, project_snapshot};
use crate::application::scanner::TranscriptUpdate;
use crate::application::summary::Summary;
use crate::application::tasks;
use crate::application::telemetry::Telemetry;
use crate::domain::{
    AgentId, AgentInput, AgentRegistry, AgentStats, AlertPolicy, AliasEntry, AliasKey,
    DetectedAgent, DomainEvent, Events, GitStatus, MailSnapshot, Mailbox, MemoryBook, MemoryEntry,
    MessageId, NewTask, Pid, Placement, Provider, QuotaPolicy, QuotaWindow, SEARCH_LIMIT,
    SessionId, StalePolicy, StatusKind, TaskBook, TaskId, ThreadId, Timestamp,
};
use crate::ingestion::HookEvent;
use crate::ingestion::claude::SessionFile;
use crate::ports::{
    AgentKiller, AlertNotice, AlertSink, AliasStore, Clock, CwdProjects, DiffPublisher, KillError,
    KillOutcome, KillTarget, LaunchSpec, Launched, MailCourier, MailStore, MemoryStore,
    ProjectResolver, SelfMonitor, TaskLauncher,
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
/// Entre `kiti_done` y el cierre de la sesión del agente: tiempo para que termine su turno.
pub const TASK_CLOSE_DELAY: Duration = Duration::from_secs(10);

/// Lo que la UI puede hacer con los mensajes (ADR 0017).
pub type MailAction = kiti_protocol::MailCommand;

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
    McpRemember {
        ancestry: Vec<Pid>,
        request: mcp::RememberRequest,
        reply: McpReply<mcp::RememberReply>,
    },
    McpRecall {
        ancestry: Vec<Pid>,
        request: mcp::RecallRequest,
        reply: McpReply<mcp::RecallReply>,
    },
    /// `kiti_spawn` (ADR 0019): responde cuando el lanzador termina, fuera del actor.
    McpSpawn {
        ancestry: Vec<Pid>,
        request: mcp::SpawnRequest,
        reply: McpReply<mcp::SpawnReply>,
    },
    McpDone {
        ancestry: Vec<Pid>,
        request: mcp::DoneRequest,
        reply: McpReply<mcp::DoneReply>,
    },
    /// Interno: el lanzador terminó y devuelve el resultado al actor.
    #[doc(hidden)]
    SpawnFinished {
        id: TaskId,
        result: Result<Launched, String>,
        reply: McpReply<mcp::SpawnReply>,
    },
    /// PIDs de los agentes lanzados desde Kiti (sesiones tmux): a ellos se les puede escribir.
    /// Pid del agente y nombre de su sesión tmux de Kiti.
    MailReachable(Vec<(Pid, String)>),
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
    /// Un agente terminó un turno y su cuota hay que pedirla (Antigravity): quien la consulta
    /// lo hace antes de su intervalo.
    quota_kick: Arc<tokio::sync::Notify>,
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

    /// Un agente de Antigravity terminó un turno: su cuota pudo cambiar.
    pub fn kick_quota(&self) {
        self.quota_kick.notify_one();
    }

    /// Se completa cuando conviene volver a consultar la cuota de Antigravity. Un aviso sin nadie
    /// esperando se guarda para la siguiente espera, y varios seguidos cuentan como uno.
    pub async fn quota_kicked(&self) {
        self.quota_kick.notified().await;
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

    pub async fn mcp_remember(
        &self,
        ancestry: Vec<Pid>,
        request: mcp::RememberRequest,
    ) -> Result<mcp::RememberReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpRemember {
            ancestry,
            request,
            reply,
        })
        .await
    }

    pub async fn mcp_recall(
        &self,
        ancestry: Vec<Pid>,
        request: mcp::RecallRequest,
    ) -> Result<mcp::RecallReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpRecall {
            ancestry,
            request,
            reply,
        })
        .await
    }

    pub async fn mcp_spawn(
        &self,
        ancestry: Vec<Pid>,
        request: mcp::SpawnRequest,
    ) -> Result<mcp::SpawnReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpSpawn {
            ancestry,
            request,
            reply,
        })
        .await
    }

    pub async fn mcp_done(
        &self,
        ancestry: Vec<Pid>,
        request: mcp::DoneRequest,
    ) -> Result<mcp::DoneReply, mcp::ErrorReply> {
        self.mcp(|reply| RuntimeCommand::McpDone {
            ancestry,
            request,
            reply,
        })
        .await
    }

    /// Agentes a los que Kiti puede escribir (sesiones tmux).
    pub async fn mail_reachable(&self, sessions: Vec<(Pid, String)>) {
        let _ = self.tx.send(RuntimeCommand::MailReachable(sessions)).await;
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
    /// Agentes en sesiones tmux de Kiti (pid → sesión). Hasta el primer `MailReachable` no se
    /// reparten alias: la clave depende de él.
    reachable: std::collections::HashMap<Pid, String>,
    reachable_known: bool,
    alias_store: Option<Box<dyn AliasStore>>,
    alias_saved: Vec<AliasEntry>,
    mail_store: Option<Box<dyn MailStore>>,
    mail_saved: MailSnapshot,
    /// A quién se escribió y cuándo: no se le vuelve a escribir hasta verlo trabajar.
    typed: std::collections::HashMap<AgentId, Timestamp>,
    /// Memoria de los proyectos (ADR 0018).
    memories: MemoryBook,
    memory_store: Option<Box<dyn MemoryStore>>,
    memory_dirty: bool,
    /// Agentes efímeros (ADR 0019).
    tasks: TaskBook,
    launcher: Option<Arc<dyn TaskLauncher>>,
    /// Tareas cuyo agente ya recibió su mensaje de rol.
    greeted: std::collections::HashSet<TaskId>,
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
            reachable: std::collections::HashMap::new(),
            reachable_known: false,
            alias_store: None,
            mail_store: None,
            mail_saved: MailSnapshot::default(),
            alias_saved: Vec::new(),
            typed: std::collections::HashMap::new(),
            memories: MemoryBook::default(),
            memory_store: None,
            memory_dirty: false,
            tasks: TaskBook::default(),
            launcher: None,
            greeted: std::collections::HashSet::new(),
        };
        (
            runtime,
            RuntimeHandle {
                tx,
                telemetry,
                quota_kick: Arc::default(),
            },
        )
    }

    /// Muestreo de RSS y CPU del proceso para el panel de debug.
    pub fn with_monitor(mut self, monitor: impl SelfMonitor) -> Self {
        self.monitor = Some(Box::new(monitor));
        self
    }

    /// Cómo agrupar oficinas por proyecto (ADR 0009). Por defecto, cada carpeta es uno.
    pub fn with_projects(mut self, projects: impl ProjectResolver) -> Self {
        self.projects = Box::new(projects);
        self.rekey_memories();
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

    /// Quién lanza y cierra los agentes efímeros (ADR 0019). Sin él, `kiti_spawn` se rechaza.
    pub fn with_launcher(mut self, launcher: impl TaskLauncher) -> Self {
        self.launcher = Some(Arc::new(launcher));
        self
    }

    /// Alias guardados de un arranque anterior y dónde persistirlos (ADR 0017).
    pub fn with_aliases(mut self, initial: Vec<AliasEntry>, store: impl AliasStore) -> Self {
        self.mailbox.restore_aliases(initial);
        self.alias_saved = self.mailbox.alias_entries();
        self.alias_store = Some(Box::new(store));
        self
    }

    /// Mensajes en espera e hilos abiertos de un arranque anterior y dónde persistirlos
    /// (ADR 0017). Se reasignan a los agentes vivos por alias tras el primer reparto.
    pub fn with_mail_store(mut self, initial: MailSnapshot, store: impl MailStore) -> Self {
        self.mailbox.restore(initial);
        self.mail_saved = self.mailbox.snapshot();
        self.mail_store = Some(Box::new(store));
        self
    }

    /// Almacén persistente de recuerdos (`SqliteMemoryStore`, ADR 0018) e hidratación inicial.
    pub fn with_memories(mut self, initial: Vec<MemoryEntry>, store: impl MemoryStore) -> Self {
        self.memories.load(initial);
        self.rekey_memories();
        self.telemetry.set_memories_total(self.memories.total());
        self.memory_store = Some(Box::new(store));
        self
    }

    /// Los recuerdos guardados con la ruta de un worktree pasan a su repositorio principal.
    /// Solo en memoria: `memory.db` no cambia. Idempotente, así que el orden de
    /// `with_projects` y `with_memories` da igual.
    fn rekey_memories(&mut self) {
        let projects = &mut self.projects;
        self.memories.rekey(|p| projects.repo_of(p));
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
                    self.tick_tasks(now);
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
                // La status line de Claude se repite mucho y casi siempre igual: solo los cambios
                // que la UI llega a ver (porcentaje redondeado) viajan.
                let same = self
                    .quotas
                    .insert(provider, windows)
                    .is_some_and(|old| quota_msg(provider, &old) == msg);
                if same {
                    return;
                }
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
                    if self.memories.total() > 0 {
                        publisher.publish(memory::memory_msg(&self.memories));
                    }
                    self.memory_dirty = false;
                    let infos = self.task_infos();
                    if !infos.is_empty() {
                        publisher.publish(tasks::tasks_msg(infos));
                    }
                    self.tasks.take_dirty();
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
            RuntimeCommand::McpRemember {
                ancestry,
                request,
                reply,
            } => {
                let _ = reply.send(self.mcp_remember(&ancestry, request));
            }
            RuntimeCommand::McpRecall {
                ancestry,
                request,
                reply,
            } => {
                let _ = reply.send(self.mcp_recall(&ancestry, request));
            }
            RuntimeCommand::McpSpawn {
                ancestry,
                request,
                reply,
            } => self.start_spawn(&ancestry, request, reply),
            RuntimeCommand::McpDone {
                ancestry,
                request,
                reply,
            } => {
                let _ = reply.send(self.mcp_done(&ancestry, request));
            }
            RuntimeCommand::SpawnFinished { id, result, reply } => {
                let _ = reply.send(self.finish_spawn(id, result));
            }
            RuntimeCommand::MailReachable(sessions) => {
                self.reachable = sessions.into_iter().collect();
                self.reachable_known = true;
                self.mail_dirty = true;
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
                    MailAction::Mode { mode } => {
                        self.mailbox.set_mode(mail::domain_mail_mode(mode))
                    }
                    MailAction::Pause { paused } => self.mailbox.set_paused(paused),
                    MailAction::Approve { id } => self.mailbox.approve(MessageId(id)),
                    MailAction::Discard { id } => self.mailbox.discard(MessageId(id)),
                    MailAction::Continue { id } => self.mailbox.continue_thread(ThreadId(id)),
                    MailAction::Close { id } => self.mailbox.close_thread(ThreadId(id)),
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
        // La clave de un agente depende de si vive en tmux: hasta saberlo no hay alias.
        if !self.reachable_known {
            return;
        }
        let now = self.clock.now();
        let live: Vec<(AgentId, AliasKey)> = self
            .registry
            .agents()
            .filter(|a| !a.is_dead())
            .map(|a| a.id())
            .map(|id| {
                let key = match self.reachable.get(&id.pid) {
                    Some(session) => AliasKey::Tmux(session.clone()),
                    None => AliasKey::Process(id),
                };
                (id, key)
            })
            .collect();
        // Primero salen los que ya no viven: si un agente se relanzó en su sesión tmux, el
        // viejo suelta la clave antes de que el nuevo la tome.
        let gone: Vec<AgentId> = self
            .mailbox
            .members()
            .map(|(id, _)| *id)
            .filter(|id| !live.iter().any(|(l, _)| l == id))
            .collect();
        for id in gone {
            self.mailbox.leave(&id, now);
            self.typed.remove(&id);
            self.mail_dirty = true;
        }
        for (id, key) in &live {
            if self.mailbox.alias(id).is_none() {
                self.mailbox.join(*id, key.clone(), now);
                self.mail_dirty = true;
            } else if self.mailbox.key_of(id) != Some(key)
                && self.mailbox.rekey(id, key.clone(), now)
            {
                self.mail_dirty = true;
            }
        }
        if self.mailbox.has_restored() {
            let r = self.mailbox.rebind_restored();
            info!(event = "mail.restored", pending = r.kept);
            if r.discarded > 0 {
                info!(event = "mail.restored_discarded", count = r.discarded);
            }
            self.mail_dirty = true;
        }
        let wanted: std::collections::HashSet<AliasKey> =
            live.into_iter().map(|(_, k)| k).collect();
        if self.mailbox.release_unbound(&wanted, now) | self.mailbox.prune_aliases(now) {
            self.mail_dirty = true;
        }
        self.save_aliases();
    }

    /// Persiste los mensajes en espera y los hilos abiertos si cambiaron.
    fn save_mail(&mut self) {
        let Some(store) = self.mail_store.as_mut() else {
            return;
        };
        let snapshot = self.mailbox.snapshot();
        if snapshot != self.mail_saved {
            store.save(&snapshot);
            self.mail_saved = snapshot;
        }
    }

    /// Persiste el libro de alias si cambió desde el último guardado.
    fn save_aliases(&mut self) {
        let Some(store) = self.alias_store.as_mut() else {
            return;
        };
        let entries = self.mailbox.alias_entries();
        if entries != self.alias_saved {
            store.save(&entries);
            self.alias_saved = entries;
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
            reachable.contains_key(&id.pid)
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
                self.reachable.get(&agent.id().pid).map(String::as_str),
            ));
        }
        peers.sort_by(|a, b| a.alias.cmp(&b.alias));
        Ok(mcp::AgentsReply {
            you: self.mailbox.alias(&me).unwrap_or_default().to_owned(),
            you_session: self.reachable.get(&me.pid).cloned(),
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

    fn mcp_remember(
        &mut self,
        ancestry: &[Pid],
        request: mcp::RememberRequest,
    ) -> Result<mcp::RememberReply, mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        let author = self.mailbox.alias(&me).unwrap_or("agent").to_owned();
        let cwd = self
            .registry
            .get(&me)
            .map(|a| a.cwd().as_str().to_owned())
            .unwrap_or_default();
        let project = self.projects.repo_of(&cwd);
        let now = self.clock.now();
        match self.memories.remember(
            &project,
            &author,
            &request.kind,
            &request.title,
            &request.detail,
            &request.files,
            now,
        ) {
            Ok(entry) => {
                if let Some(store) = self.memory_store.as_mut() {
                    store.save(&entry);
                }
                info!(
                    event = "memory.stored",
                    author = %author,
                    kind = entry.kind.as_str(),
                    files = entry.files.len()
                );
                self.telemetry.record_memory();
                self.memory_dirty = true;
                Ok(memory::remember_reply(&entry))
            }
            Err(e) => {
                info!(event = "memory.rejected", reason = e.code());
                Err(memory::remember_error(&e))
            }
        }
    }

    fn mcp_recall(
        &mut self,
        ancestry: &[Pid],
        request: mcp::RecallRequest,
    ) -> Result<mcp::RecallReply, mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        let cwd = self
            .registry
            .get(&me)
            .map(|a| a.cwd().as_str().to_owned())
            .unwrap_or_default();
        let project = self.projects.repo_of(&cwd);
        self.telemetry.record_recall();
        let query = request
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_owned);

        let mut sessions = Vec::new();
        for agent in self.registry.agents().filter(|a| !a.is_dead()) {
            if self.projects.repo_of(agent.cwd().as_str()) == project {
                let alias = self.mailbox.alias(&agent.id()).unwrap_or("?");
                sessions.push(memory::recall_session(agent, alias));
            }
        }
        sessions.sort_by(|a, b| a.alias.cmp(&b.alias));

        let memories: Vec<mcp::RecallEntry> = match &query {
            Some(q) => {
                // Siempre en memoria: el FTS de SQLite no conoce las claves por repositorio ni
                // ignora tildes; `memory.db` solo persiste.
                let hits = self.memories.search(&project, q, SEARCH_LIMIT);
                hits.iter().map(|e| memory::recall_entry(e, true)).collect()
            }
            None => self
                .memories
                .for_project(&project)
                .into_iter()
                .take(20)
                .map(|e| memory::recall_entry(e, false))
                .collect(),
        };
        info!(
            event = "memory.recalled",
            has_query = query.is_some(),
            results = memories.len()
        );
        Ok(mcp::RecallReply {
            project: mail::project_name(&project),
            query,
            sessions,
            memories,
        })
    }

    /// Las tareas para el tablero, con el alias de su agente si ya está en el mapa.
    fn task_infos(&self) -> Vec<kiti_protocol::TaskInfo> {
        self.tasks
            .tasks()
            .map(|t| {
                let alias = t.pid.and_then(|pid| {
                    let agent = self
                        .registry
                        .agents()
                        .find(|a| !a.is_dead() && a.id().pid == pid)?;
                    self.mailbox.alias(&agent.id()).map(str::to_owned)
                });
                tasks::task_info(t, alias)
            })
            .collect()
    }

    /// `kiti_spawn`: valida, reserva el hueco y lanza fuera del actor (ADR 0019).
    fn start_spawn(
        &mut self,
        ancestry: &[Pid],
        request: mcp::SpawnRequest,
        reply: McpReply<mcp::SpawnReply>,
    ) {
        match self.try_spawn(ancestry, &request) {
            Err(e) => {
                let _ = reply.send(Err(e));
            }
            Ok((id, spec, launcher)) => {
                let back = self.self_tx.clone();
                tokio::task::spawn_blocking(move || {
                    let result = launcher.launch(spec);
                    let _ = back.blocking_send(RuntimeCommand::SpawnFinished { id, result, reply });
                });
            }
        }
    }

    fn try_spawn(
        &mut self,
        ancestry: &[Pid],
        request: &mcp::SpawnRequest,
    ) -> Result<(TaskId, LaunchSpec, Arc<dyn TaskLauncher>), mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        // Lanzar un agente gasta cuota: en modo manual hace falta el visto bueno del usuario, que
        // no puede viajar como mensaje de otro agente (ADR 0019).
        if self.mailbox.mode() == crate::domain::MailMode::Manual {
            info!(event = "task.rejected", reason = "needs_approval");
            return Err(tasks::error(
                "NEEDS_APPROVAL",
                "Kiti está en modo manual: lanzar un agente gasta cuota y lo tiene que aprobar \
                 el usuario. Pídele que cambie a modo automático o que lance el agente él.",
            ));
        }
        let plan = tasks::plan(request)?;
        let Some(launcher) = self.launcher.clone() else {
            return Err(tasks::error(
                "NO_LAUNCHER",
                "Esta instancia de Kiti no puede lanzar agentes.",
            ));
        };
        let now = self.clock.now();
        let cwd = self
            .registry
            .get(&me)
            .map(|a| a.cwd().as_str().to_owned())
            .unwrap_or_default();
        let repo = self.projects.repo_of(&cwd);
        let id = self
            .tasks
            .begin(
                NewTask {
                    orchestrator: me,
                    orchestrator_alias: self.mailbox.alias(&me).unwrap_or("?").to_owned(),
                    role: plan.role,
                    provider: plan.provider,
                    model: plan.model.clone(),
                    branch: plan.branch.clone(),
                },
                now,
            )
            .map_err(|reason| {
                tracing::warn!(event = "task.limited", reason = %reason);
                tasks::error("LIMIT", reason.to_string())
            })?;
        let spec = LaunchSpec {
            task: id,
            repo: repo.into(),
            branch: plan.branch,
            provider: plan.provider,
            model: plan.model,
            role: plan.role,
            brief: plan.brief,
        };
        Ok((id, spec, launcher))
    }

    fn finish_spawn(
        &mut self,
        id: TaskId,
        result: Result<Launched, String>,
    ) -> Result<mcp::SpawnReply, mcp::ErrorReply> {
        let now = self.clock.now();
        match result {
            Ok(l) => {
                let _ = self.tasks.launched(
                    id,
                    Placement {
                        worktree: l.worktree.clone(),
                        session: l.session.clone(),
                        pid: l.pid,
                        brief_path: l.brief_path,
                    },
                    now,
                );
                let task = self.tasks.get(id);
                let branch = task.map(|t| t.branch.clone()).unwrap_or_default();
                if let Some(t) = task {
                    info!(
                        event = "task.spawned",
                        task_id = id.0,
                        provider = %t.provider,
                        model = %t.model,
                        role = t.role.as_str(),
                        repo = %mail::project_name(&l.worktree.to_string_lossy()),
                    );
                }
                Ok(mcp::SpawnReply {
                    task_id: id.0,
                    branch,
                    session: Some(l.session),
                    worktree: Some(l.worktree.to_string_lossy().into_owned()),
                })
            }
            Err(reason) => {
                let _ = self.tasks.launch_failed(id, reason.clone(), now);
                tracing::warn!(event = "task.launch_failed", task_id = id.0);
                Err(tasks::error("LAUNCH_FAILED", reason))
            }
        }
    }

    /// Cuando el escáner registra al agente de una tarea, le llega su mensaje de rol en nombre
    /// del orquestador.
    fn greet_tasks(&mut self) {
        if !self.reachable_known {
            return;
        }
        let now = self.clock.now();
        let pending: Vec<(TaskId, Pid)> = self
            .tasks
            .tasks()
            .filter(|t| t.state == crate::domain::TaskState::Running)
            .filter(|t| !self.greeted.contains(&t.id))
            .filter_map(|t| t.pid.map(|pid| (t.id, pid)))
            .collect();
        for (id, pid) in pending {
            let Some(agent) = self
                .registry
                .agents()
                .find(|a| !a.is_dead() && a.id().pid == pid)
                .map(|a| a.id())
            else {
                continue;
            };
            let Some(alias) = self.mailbox.alias(&agent).map(str::to_owned) else {
                continue;
            };
            let Some(task) = self.tasks.get(id).cloned() else {
                continue;
            };
            self.greeted.insert(id);
            self.tasks.touch();
            let path = task
                .brief_path
                .as_deref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = tasks::role_message(task.role, &path);
            match self
                .mailbox
                .send(task.orchestrator, &alias, &text, None, now)
            {
                Ok(_) => self.mail_dirty = true,
                Err(e) => {
                    tracing::warn!(
                        event = "task.greet_failed",
                        task_id = id.0,
                        reason = e.code()
                    );
                }
            }
        }
    }

    /// `kiti_done`: solo el agente de una tarea abierta.
    fn mcp_done(
        &mut self,
        ancestry: &[Pid],
        request: mcp::DoneRequest,
    ) -> Result<mcp::DoneReply, mcp::ErrorReply> {
        let me = self.caller(ancestry)?;
        let not_a_task = || {
            tasks::error(
                "NOT_A_TASK",
                "Tú no eres el agente de ninguna tarea abierta: kiti_done es solo para agentes \
                 lanzados con kiti_spawn.",
            )
        };
        let id = self
            .tasks
            .open_for_pid(me.pid)
            .map(|t| t.id)
            .ok_or_else(not_a_task)?;
        let now = self.clock.now();
        let outcome = match request
            .failed
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
        {
            Some(reason) => Err(reason.to_owned()),
            None => Ok(request
                .pr
                .as_deref()
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_owned)),
        };
        self.tasks
            .finish(id, outcome.clone(), now)
            .map_err(|_| not_a_task())?;
        let task = self.tasks.get(id).cloned().ok_or_else(not_a_task)?;
        info!(
            event = "task.done",
            task_id = id.0,
            result = if outcome.is_ok() { "done" } else { "failed" },
            duration_ms = now.0.saturating_sub(task.created_at.0),
        );
        let text = tasks::done_message(&task, &outcome, &request.summary);
        if let Some(alias) = self.mailbox.alias(&task.orchestrator).map(str::to_owned) {
            match self.mailbox.send(me, &alias, &text, None, now) {
                Ok(_) => self.mail_dirty = true,
                Err(e) => {
                    tracing::warn!(
                        event = "task.notify_failed",
                        task_id = id.0,
                        reason = e.code()
                    );
                }
            }
        }
        self.schedule_close(task);
        Ok(mcp::DoneReply {
            task_id: id.0,
            state: if outcome.is_ok() { "done" } else { "failed" }.to_owned(),
        })
    }

    /// Pasados unos segundos cierra la sesión, archiva la conversación y limpia el worktree,
    /// fuera del actor.
    fn schedule_close(&self, task: crate::domain::Task) {
        let Some(launcher) = self.launcher.clone() else {
            return;
        };
        tokio::spawn(async move {
            tokio::time::sleep(TASK_CLOSE_DELAY).await;
            let id = task.id.0;
            let outcome = tokio::task::spawn_blocking(move || launcher.close(&task))
                .await
                .unwrap_or_default();
            info!(
                event = "task.closed",
                task_id = id,
                worktree_removed = outcome.worktree_removed,
                kept_reason = outcome.kept_reason.as_deref().unwrap_or(""),
            );
        });
    }

    fn tick_tasks(&mut self, now: Timestamp) {
        for id in self.tasks.tick(now) {
            tracing::warn!(event = "task.stuck", task_id = id.0);
        }
    }

    fn flush_diff(&mut self) {
        self.sync_mail();
        self.greet_tasks();
        self.deliver_mail();
        self.save_mail();
        if self.mail_dirty
            && let Some(publisher) = self.publisher.as_mut()
        {
            self.mail_dirty = false;
            if !publisher.publish(ServerMsg::Mail(mail::mail_state(&self.mailbox))) {
                self.drop_publisher();
            }
        }
        if self.memory_dirty
            && let Some(publisher) = self.publisher.as_mut()
        {
            self.memory_dirty = false;
            if !publisher.publish(memory::memory_msg(&self.memories)) {
                self.drop_publisher();
            }
        }
        if self.publisher.is_some() && self.tasks.take_dirty() {
            let msg = tasks::tasks_msg(self.task_infos());
            if let Some(publisher) = self.publisher.as_mut()
                && !publisher.publish(msg)
            {
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
