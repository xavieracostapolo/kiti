//! Hechos de salida del reducer. Los consumen el registro, la telemetría y los logs.

use super::ids::{AgentId, Timestamp, ToolName};
use super::signal::ToolOutcome;
use super::status::{StatusKind, StatusReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainEvent {
    AgentAppeared {
        id: AgentId,
    },
    SessionStarted {
        id: AgentId,
    },
    StatusChanged {
        id: AgentId,
        from: StatusKind,
        to: StatusKind,
        reason: StatusReason,
        at: Timestamp,
    },
    ToolStarted {
        id: AgentId,
        tool: ToolName,
    },
    ToolFinished {
        id: AgentId,
        tool: ToolName,
        outcome: ToolOutcome,
    },
    /// El proceso terminó. `expected` si hubo `SessionEnd` o un kill pedido por el usuario.
    AgentExited {
        id: AgentId,
        expected: bool,
    },
    /// El agente sale del registro y de la UI.
    AgentRemoved {
        id: AgentId,
    },
}
