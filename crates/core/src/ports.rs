//! Ports (traits) del backend (ADR 0002 §4).
//!
//! Despacho estático con genéricos y `async fn` nativo. Sin `async-trait`.

use crate::domain::{AgentId, AlertKind, ProcessIdentity, ProcessSnapshot, Provider, Timestamp};
use kiti_protocol::ServerMsg;
use std::time::Duration;

/// Un aviso listo para mostrar (ADR 0010). Lleva el título de la conversación, nunca el pedido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertNotice {
    pub kind: AlertKind,
    pub agent: AgentId,
    pub provider: Provider,
    pub project: String,
    pub cwd: String,
    pub title: Option<String>,
}

/// Muestra los avisos (notificaciones del sistema en la app). Decide si molestar o no, por
/// ejemplo si la ventana ya está al frente.
pub trait AlertSink: Send + 'static {
    fn alert(&mut self, notice: AlertNotice);
    /// Queda poca cuota semanal de un proveedor (ADR 0015).
    fn quota_low(&mut self, _alert: crate::domain::QuotaAlert) {}
    /// Un hilo de mensajes llegó al límite de saltos y espera al usuario (ADR 0017).
    fn mail_paused(&mut self, _subject: &str, _between: &str) {}
}

/// Escribe mensajes en la terminal de un agente lanzado desde Kiti (ADR 0017). No bloquea: la
/// escritura va en otra tarea, y si falla se devuelven los ids con
/// `RuntimeHandle::mail_undeliver`.
pub trait MailCourier: Send + 'static {
    fn deliver(&mut self, pid: crate::domain::Pid, text: String, ids: Vec<u64>);
}

/// Proyecto (raíz git o carpeta) de un `cwd` (ADR 0009). Puede consultar el disco y guardar
/// en caché: se llama al proyectar cada agente.
pub trait ProjectResolver: Send + 'static {
    fn project_of(&mut self, cwd: &str) -> String;
}

/// Sin disco: cada carpeta es su propio proyecto (simulador y tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct CwdProjects;

impl ProjectResolver for CwdProjects {
    fn project_of(&mut self, cwd: &str) -> String {
        cwd.to_owned()
    }
}

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

/// `publish` devuelve `false` si el destino ya no existe (ventana cerrada): el runtime deja de
/// publicarle hasta la siguiente suscripción.
pub trait DiffPublisher: Send + 'static {
    fn publish(&mut self, msg: ServerMsg) -> bool;
}

impl<F> DiffPublisher for F
where
    F: FnMut(ServerMsg) + Send + 'static,
{
    fn publish(&mut self, msg: ServerMsg) -> bool {
        (self)(msg);
        true
    }
}
