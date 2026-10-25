//! Runtime actor y coalescedor de diffs (ADR 0002 §4, ADR 0004 §1).
//!
//! Una única tarea es dueña del `AgentRegistry` (sin contención de `Mutex`).
//! Coalesce los cambios a ~10 Hz (máx. 1 `Diff` cada 100 ms si hay cambios).

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, interval};

use crate::application::projection::{project_diff, project_snapshot};
use crate::application::telemetry::Telemetry;
use crate::domain::{AgentId, AgentInput, AgentRegistry, DetectedAgent, StalePolicy, Timestamp};
use crate::ports::{AgentKiller, DiffPublisher, KillOutcome, KillTarget};
use kiti_protocol::{Hello, KillResult, PROTOCOL_VERSION, ServerMsg, Source};

pub const FLUSH_INTERVAL: Duration = Duration::from_millis(100);
pub const METRICS_INTERVAL: Duration = Duration::from_secs(1);
pub const STALE_TICK_INTERVAL: Duration = Duration::from_secs(2);

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

pub struct Runtime<K: AgentKiller> {
    registry: AgentRegistry,
    rx: mpsc::Receiver<RuntimeCommand>,
    publisher: Option<Box<dyn DiffPublisher>>,
    killer: K,
    telemetry: Arc<Telemetry>,
    seq: u64,
    debug_stream: bool,
    source: Source,
    app_version: String,
    stale_policy: StalePolicy,
}

impl<K: AgentKiller> Runtime<K> {
    pub fn spawn(
        killer: K,
        telemetry: Arc<Telemetry>,
        source: Source,
        app_version: impl Into<String>,
    ) -> RuntimeHandle {
        let (tx, rx) = mpsc::channel(1024);
        let runtime = Self {
            registry: AgentRegistry::new(),
            rx,
            publisher: None,
            killer,
            telemetry: Arc::clone(&telemetry),
            seq: 0,
            debug_stream: false,
            source,
            app_version: app_version.into(),
            stale_policy: StalePolicy::default(),
        };

        tokio::spawn(runtime.run());

        RuntimeHandle { tx, telemetry }
    }

    async fn run(mut self) {
        let mut flush_timer = interval(FLUSH_INTERVAL);
        let mut metrics_timer = interval(METRICS_INTERVAL);
        let mut stale_timer = interval(STALE_TICK_INTERVAL);

        loop {
            tokio::select! {
                cmd = self.rx.recv() => {
                    match cmd {
                        Some(RuntimeCommand::RegisterAgent { detected, now }) => {
                            self.registry.register(detected, now);
                            self.telemetry.record_event();
                        }
                        Some(RuntimeCommand::ApplyInput { id, input, now }) => {
                            self.registry.apply(&id, &input, now);
                            self.telemetry.record_event();
                        }
                        Some(RuntimeCommand::Subscribe { mut publisher, reply }) => {
                            // Al suscribirse enviamos el Snapshot inicial con seq = 0
                            self.seq = 0;
                            let snapshot_agents = self.registry.snapshot();
                            let msg = project_snapshot(self.seq, &snapshot_agents);
                            publisher.publish(msg);
                            self.publisher = Some(publisher);
                            let _ = reply.send(Hello {
                                protocol_version: PROTOCOL_VERSION,
                                app_version: self.app_version.clone(),
                                source: self.source,
                            });
                        }
                        Some(RuntimeCommand::SetDebugStream(on)) => {
                            self.debug_stream = on;
                        }
                        Some(RuntimeCommand::KillAgent { id, reply }) => {
                            let result = self.handle_kill(id).await;
                            let _ = reply.send(result);
                        }
                        None => break,
                    }
                }
                _ = flush_timer.tick() => {
                    self.flush_diff();
                }
                _ = metrics_timer.tick() => {
                    if self.debug_stream {
                        self.send_metrics();
                    }
                }
                _ = stale_timer.tick() => {
                    let now = Timestamp(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    );
                    self.registry.tick(&self.stale_policy, now);
                }
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
        let ingest_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let start_send = Instant::now();
        let msg = project_diff(self.seq, ingest_ms, diff);
        publisher.publish(msg);
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

    async fn handle_kill(&mut self, id: AgentId) -> KillResult {
        let agent = match self.registry.get(&id) {
            Some(a) => a,
            None => return KillResult::NotFound,
        };
        let target = KillTarget {
            id,
            identity: *agent.identity(),
        };

        // Marcamos KillRequested en el agente
        self.registry
            .apply(&id, &AgentInput::KillRequested, Timestamp(0));

        match self.killer.terminate(target, Duration::from_secs(5)).await {
            Ok(KillOutcome::Killed) => {
                // Notificamos que el proceso se fue
                self.registry
                    .apply(&id, &AgentInput::ProcessGone, Timestamp(0));
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
