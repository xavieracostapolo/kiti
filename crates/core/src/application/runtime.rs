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
use crate::application::telemetry::Telemetry;
use crate::domain::{AgentId, AgentInput, AgentRegistry, DetectedAgent, StalePolicy, Timestamp};
use crate::ports::{AgentKiller, Clock, DiffPublisher, KillError, KillOutcome, KillTarget};
use kiti_protocol::{Hello, KillResult, PROTOCOL_VERSION, ServerMsg, Source};

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
    Subscribe {
        publisher: Box<dyn DiffPublisher>,
        reply: oneshot::Sender<Hello>,
    },
    SetDebugStream(bool),
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

    pub fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
    }
}

pub struct Runtime<K: AgentKiller + Clone, C: Clock> {
    registry: AgentRegistry,
    rx: mpsc::Receiver<RuntimeCommand>,
    /// Para que las tareas de kill devuelvan su resultado al actor.
    self_tx: mpsc::Sender<RuntimeCommand>,
    publisher: Option<Box<dyn DiffPublisher>>,
    killer: K,
    clock: C,
    telemetry: Arc<Telemetry>,
    seq: u64,
    debug_stream: bool,
    source: Source,
    app_version: String,
    stale_policy: StalePolicy,
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
            killer,
            clock,
            telemetry: Arc::clone(&telemetry),
            seq: 0,
            debug_stream: false,
            source,
            app_version: app_version.into(),
            stale_policy: StalePolicy::default(),
        };
        (runtime, RuntimeHandle { tx, telemetry })
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
                    self.registry.tick(&self.stale_policy, now);
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
            RuntimeCommand::Subscribe {
                mut publisher,
                reply,
            } => {
                self.seq = 0;
                let snapshot_agents = self.registry.snapshot();
                publisher.publish(project_snapshot(self.seq, &snapshot_agents));
                self.publisher = Some(publisher);
                let _ = reply.send(Hello {
                    protocol_version: PROTOCOL_VERSION,
                    app_version: self.app_version.clone(),
                    source: self.source,
                });
            }
            RuntimeCommand::SetDebugStream(on) => self.debug_stream = on,
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
        let start_send = Instant::now();
        publisher.publish(project_diff(self.seq, self.clock.now().ms(), diff));
        self.telemetry.record_flush();
        self.telemetry
            .record_publish_latency_ms(start_send.elapsed().as_millis() as u64);
    }

    fn send_metrics(&mut self) {
        let Some(publisher) = self.publisher.as_mut() else {
            return;
        };
        let metrics = self
            .telemetry
            .snapshot_core_metrics(self.registry.len() as u32, 0, 0.0);
        publisher.publish(ServerMsg::Metrics(metrics));
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
