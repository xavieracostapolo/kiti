//! Ports (traits) del backend (ADR 0002 §4).
//!
//! Despacho estático con genéricos y `async fn` nativo. Sin `async-trait`.

use crate::domain::{AgentId, ProcessIdentity, ProcessSnapshot, Timestamp};
use kiti_protocol::ServerMsg;
use std::time::Duration;

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("fallo al escanear procesos: {0}")]
    Failed(String),
}

/// Síncrono: sysinfo es bloqueante. El escáner corre en `spawn_blocking` cada 2 s.
pub trait ProcessScanner: Send + 'static {
    /// Procesos del usuario actual.
    fn scan(&mut self) -> Result<Vec<ProcessSnapshot>, ScanError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillTarget {
    pub id: AgentId,
    pub identity: ProcessIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KillOutcome {
    Killed,
    Cancelled,
    StaleProcess,
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum KillError {
    #[error("fallo al terminar el proceso: {0}")]
    Failed(String),
}

pub trait AgentKiller: Send + Sync + 'static {
    fn terminate(
        &self,
        target: KillTarget,
        grace: Duration,
    ) -> impl std::future::Future<Output = Result<KillOutcome, KillError>> + Send;
}

/// RSS (bytes) y CPU (%) del propio proceso, para `CoreMetrics` (ADR 0004 §3).
/// Solo se muestrea con el stream de debug activo.
pub trait SelfMonitor: Send + 'static {
    fn sample(&mut self) -> (u64, f64);
}

pub trait DiffPublisher: Send + 'static {
    fn publish(&mut self, msg: ServerMsg);
}

impl<F> DiffPublisher for F
where
    F: FnMut(ServerMsg) + Send + 'static,
{
    fn publish(&mut self, msg: ServerMsg) {
        (self)(msg);
    }
}
