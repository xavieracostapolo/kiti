//! Dominio de Monitoring (ADR 0002 §1–§3). Puro y determinista: sin I/O, sin reloj
//! (el tiempo llega como argumento) y sin aleatoriedad. No depende de `kiti-protocol`.

mod agent;
mod events;
mod ids;
mod registry;
mod signal;
mod stale;
mod status;

pub use agent::{Agent, AgentInput, DetectedAgent, Events};
pub use events::DomainEvent;
pub use ids::{
    AgentId, Pid, ProcessIdentity, ProcessStartTime, Provider, Timestamp, ToolName, ToolUseKey,
    WorkingDir,
};
pub use registry::{AgentRegistry, RegistryDiff};
pub use signal::{Signal, ToolOutcome};
pub use stale::StalePolicy;
pub use status::{Status, StatusKind, StatusReason, TrafficLight};
