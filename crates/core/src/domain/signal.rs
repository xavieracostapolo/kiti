//! Los 8 eventos normalizados de Kiti (ADR 0005 §5), ya traducidos por la ACL de cada CLI.
//!
//! Se llaman `Signal::*` para no chocar con los `DomainEvent` de salida. Las variantes no
//! llevan todavía la causa (`source`, `cause`, `kind`) que menciona el ADR 0002: ninguna
//! transición la usa. La ACL del Hito 4 la añadirá si la telemetría la necesita.

use super::ids::{ToolName, ToolUseKey};
use super::memory::TaskItem;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    SessionStart,
    PromptSubmit,
    ToolStart {
        key: ToolUseKey,
        tool: ToolName,
    },
    ToolEnd {
        key: ToolUseKey,
        outcome: ToolOutcome,
    },
    /// `PermissionRequest`, `Notification` de permiso o elicitación.
    AwaitingInput,
    /// `Stop` (incluida la interrupción) o `idle_prompt`.
    Stop,
    /// `StopFailure`: error del agente, no de una herramienta.
    Failure,
    SessionEnd,
    /// Evento que la ACL no reconoce. Solo cuenta una métrica.
    Unknown {
        name: String,
    },
}

/// Texto legible para el usuario (ADR 0009): de qué va la conversación, qué hace ahora y qué
/// pidió por última vez. `None` = sin cambios. Nunca se escribe en los logs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentInfo {
    pub title: Option<String>,
    pub activity: Option<String>,
    pub prompt: Option<String>,
    /// Archivo modificado en este paso (`Edit`, `Write`…), para la memoria de sesión (ADR 0018).
    pub touched_file: Option<String>,
    /// Lista de tareas (`TodoWrite`), para la memoria de sesión (ADR 0018).
    pub tasks: Option<Vec<TaskItem>>,
}

impl AgentInfo {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.activity.is_none()
            && self.prompt.is_none()
            && self.touched_file.is_none()
            && self.tasks.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutcome {
    Ok,
    Failed,
    Denied,
}
