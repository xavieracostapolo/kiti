//! Capa de aplicación (ADR 0002 §6).
//!
//! Traduce el modelo de dominio a DTOs de `kiti-protocol`, gestiona el actor Runtime,
//! coalescencia de diffs y métricas de rendimiento.

pub mod alerts;
pub mod projection;
pub mod runtime;
pub mod scanner;
pub mod summary;
pub mod telemetry;

pub use alerts::alert_text;
pub use runtime::{AgentBrief, Runtime, RuntimeHandle};
pub use scanner::run_scanner;
pub use summary::Summary;
pub use telemetry::Telemetry;
