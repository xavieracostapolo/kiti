use std::time::{Duration, Instant};

use sysinfo::{ProcessesToUpdate, Signal, System};
use tracing::info;

use super::process_read::{read_one, refresh_kind};
use crate::domain::{Pid, ProcessIdentity};
use crate::ports::{AgentKiller, KillError, KillOutcome, KillTarget};

/// Cierra un agente en macOS/Linux (ADR 0002 §5, ADR 0004 §2): verifica que el PID sigue siendo
/// el mismo proceso antes de **cada** señal, envía SIGTERM y, tras la gracia, SIGKILL.
#[derive(Debug, Clone)]
pub struct UnixKiller {
    poll: Duration,
}

impl Default for UnixKiller {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(50),
        }
    }
}

/// `true` si el PID existe y es exactamente el proceso de `identity`.
fn same_process(identity: &ProcessIdentity) -> bool {
    read_one(identity.pid).is_some_and(|s| s.identity() == *identity)
}

fn signal(pid: Pid, sig: Signal) -> Result<(), KillError> {
    let mut system = System::new();
    let spid = sysinfo::Pid::from_u32(pid.0);
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[spid]), true, refresh_kind());
    match system.process(spid).and_then(|p| p.kill_with(sig)) {
        Some(true) => Ok(()),
        _ => Err(KillError::Failed(format!(
            "no se pudo enviar {sig:?} a {}",
            pid.0
        ))),
    }
}

impl UnixKiller {
    async fn wait_gone(&self, identity: &ProcessIdentity, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            if !same_process(identity) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(self.poll).await;
        }
    }
}

impl AgentKiller for UnixKiller {
    async fn terminate(
        &self,
        target: KillTarget,
        grace: Duration,
    ) -> Result<KillOutcome, KillError> {
        let id = target.identity;
        let started = Instant::now();
        if id.pid.0 <= 1 || id.pid.0 == std::process::id() {
            info!(event = "kill.audit", pid = id.pid.0, step = "refused");
            return Ok(KillOutcome::StaleProcess);
        }
        if !same_process(&id) {
            let outcome = if read_one(id.pid).is_some() {
                KillOutcome::StaleProcess
            } else {
                KillOutcome::NotFound
            };
            info!(
                event = "kill.audit",
                pid = id.pid.0,
                step = "stale",
                ?outcome
            );
            return Ok(outcome);
        }
        info!(event = "kill.audit", pid = id.pid.0, step = "term");
        signal(id.pid, Signal::Term)?;
        if !self.wait_gone(&id, grace).await {
            // Se vuelve a verificar: en la gracia el PID pudo morir y reutilizarse.
            if same_process(&id) {
                info!(event = "kill.audit", pid = id.pid.0, step = "kill");
                signal(id.pid, Signal::Kill)?;
                if !self.wait_gone(&id, Duration::from_secs(1)).await {
                    return Err(KillError::Failed(format!(
                        "el proceso {} sigue vivo tras SIGKILL",
                        id.pid.0
                    )));
                }
            }
        }
        info!(
            event = "kill.audit",
            pid = id.pid.0,
            step = "done",
            elapsed_ms = started.elapsed().as_millis() as u64
        );
        Ok(KillOutcome::Killed)
    }
}
