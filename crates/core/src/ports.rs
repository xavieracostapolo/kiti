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

/// Persistencia de los alias de los mensajes entre agentes (ADR 0017): el runtime guarda el
/// libro entero cada vez que cambia.
pub trait AliasStore: Send + 'static {
    fn save(&mut self, entries: &[crate::domain::AliasEntry]);
}

/// Persistencia de los mensajes en espera y los hilos abiertos (ADR 0017): el runtime guarda
/// la instantánea cada vez que cambia.
pub trait MailStore: Send + 'static {
    fn save(&mut self, snapshot: &crate::domain::MailSnapshot);
}

/// Persistencia y búsqueda `FTS5 BM25` de los recuerdos del proyecto (ADR 0018).
pub trait MemoryStore: Send + 'static {
    /// Persiste un recuerdo recién creado.
    fn save(&mut self, entry: &crate::domain::MemoryEntry);
    /// Busca recuerdos de `project` por relevancia (`FTS5 bm25`). `None` usa la búsqueda en
    /// memoria de `MemoryBook`.
    fn search(
        &mut self,
        _project: &str,
        _query: &str,
        _limit: usize,
    ) -> Option<Vec<crate::domain::MemoryEntry>> {
        None
    }
}

/// Lo que hace falta para lanzar el agente de una tarea (ADR 0019).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub task: crate::domain::TaskId,
    /// Repositorio principal donde se crea el worktree.
    pub repo: std::path::PathBuf,
    pub branch: String,
    pub provider: Provider,
    pub model: String,
    pub role: crate::domain::TaskRole,
    /// El texto del encargo: el lanzador lo guarda en un archivo; nunca va a los logs.
    pub brief: String,
}

/// Dónde quedó el agente lanzado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub worktree: std::path::PathBuf,
    pub session: String,
    /// Pid del proceso de la sesión tmux: el del agente cuando el escáner lo registra.
    pub pid: crate::domain::Pid,
    pub brief_path: std::path::PathBuf,
}

/// Lo que hizo `TaskLauncher::close`, para el log y el tablero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloseOutcome {
    pub session_killed: bool,
    pub conversation_archived: bool,
    pub worktree_removed: bool,
    /// Por qué se dejó el worktree (cambios sin guardar, rama sin subir…).
    pub kept_reason: Option<String>,
}

/// Lanza y cierra los agentes efímeros (ADR 0019). Bloqueante (git, tmux, disco): el runtime lo
/// llama fuera del actor, como el kill.
pub trait TaskLauncher: Send + Sync + 'static {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, String>;
    /// Cierra la sesión, archiva la conversación y borra el worktree si es seguro.
    fn close(&self, task: &crate::domain::Task) -> CloseOutcome;
}

/// Proyecto (raíz git o carpeta) de un `cwd` (ADR 0009). Puede consultar el disco y guardar
/// en caché: se llama al proyectar cada agente.
pub trait ProjectResolver: Send + 'static {
    fn project_of(&mut self, cwd: &str) -> String;

    /// Repositorio principal del proyecto: igual que `project_of`, salvo que los worktrees
    /// de git apuntan a la raíz de su repositorio (clave de la memoria, ADR 0018).
    fn repo_of(&mut self, cwd: &str) -> String {
        self.project_of(cwd)
    }
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
