//! Dominio de Monitoring (ADR 0002 §1–§3). Puro y determinista: sin I/O, sin reloj
//! (el tiempo llega como argumento) y sin aleatoriedad. No depende de `kiti-protocol`.

mod agent;
mod alerts;
mod events;
mod ids;
mod process;
mod project;
mod quota;
mod registry;
mod signal;
mod stale;
mod status;

pub use agent::{
    Agent, AgentInput, AgentStats, DetectedAgent, Events, HISTORY_MAX, SessionActivity,
};
pub use alerts::{Alert, AlertKind, AlertPolicy};
pub use events::DomainEvent;
pub use ids::{
    AgentId, ParseAgentIdError, Pid, ProcessIdentity, ProcessStartTime, Provider, SessionId,
    Timestamp, ToolName, ToolUseKey, WorkingDir,
};
pub use process::{ProcessSnapshot, Role, classify, dedupe_tree, is_candidate_exe};
pub use project::{GitStatus, project_root};
pub use quota::{LOW as QUOTA_LOW, QuotaAlert, QuotaPolicy, QuotaWindow};
pub use registry::{AgentRegistry, RegistryDiff};
pub use signal::{AgentInfo, Signal, ToolOutcome};
pub use stale::StalePolicy;
pub use status::{Status, StatusKind, StatusReason, TrafficLight};
