//! Runtime actor y coalescedor de diffs (ADR 0002 §4, ADR 0004 §1).
//!
//! Una única tarea es dueña del `AgentRegistry` (sin contención de `Mutex`).
//! Coalesce los cambios a ~10 Hz (máx. 1 `Diff` cada 100 ms si hay cambios).
//! La hora siempre sale del port `Clock`; los kills corren fuera del actor.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval};

use crate::application::projection::{project_diff, project_snapshot};
use crate::application::scanner::TranscriptUpdate;
use crate::application::summary::Summary;
use crate::application::telemetry::Telemetry;
use crate::domain::{
    AgentId, AgentInput, AgentRegistry, AlertPolicy, DetectedAgent, DomainEvent, Pid, Provider,
    StalePolicy, Timestamp,
};
use crate::ingestion::HookEvent;
use crate::ingestion::claude::SessionFile;
use crate::ports::{
    AgentKiller, AlertNotice, AlertSink, Clock, CwdProjects, DiffPublisher, KillError, KillOutcome,
    KillTarget, ProjectResolver, SelfMonitor,
};
use kiti_protocol::{Hello, KillResult, PROTOCOL_VERSION, ServerMsg, Source};
use tracing::{debug, info};

pub const FLUSH_INTERVAL: Duration = Duration::from_millis(100);
pub const METRICS_INTERVAL: Duration = Duration::from_secs(1);
pub const STALE_TICK_INTERVAL: Duration = Duration::from_secs(2);
/// Gracia entre SIGTERM y SIGKILL (ADR 0002 §5).
pub const KILL_GRACE: Duration = Duration::from_secs(5);

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

    /// El actor terminó (se soltaron todos los handles o el runtime se apagó).
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    pub fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
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
            RuntimeCommand::KillFinished { id, outcome, reply } => {
                let _ = reply.send(self.finish_kill(id, outcome));
            }
        }
    }

    fn flush_diff(&mut self) {
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
        let metrics = self
            .telemetry
            .snapshot_core_metrics(self.registry.len() as u32, rss, cpu);
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
