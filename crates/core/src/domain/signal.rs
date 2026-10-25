//! Los 8 eventos normalizados de Kiti (ADR 0005 §5), ya traducidos por la ACL de cada CLI.
//!
//! Se llaman `Signal::*` para no chocar con los `DomainEvent` de salida. Las variantes no
//! llevan todavía la causa (`source`, `cause`, `kind`) que menciona el ADR 0002: ninguna
//! transición la usa. La ACL del Hito 4 la añadirá si la telemetría la necesita.

use super::ids::{ToolName, ToolUseKey};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutcome {
    Ok,
    Failed,
    Denied,
}
