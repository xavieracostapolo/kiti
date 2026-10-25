//! Capa de aplicación (ADR 0002 §6).
//!
//! Traduce el modelo de dominio a DTOs de `kiti-protocol`, gestiona el actor Runtime,
//! coalescencia de diffs y métricas de rendimiento.

pub mod projection;
pub mod runtime;
pub mod scanner;
pub mod telemetry;

pub use runtime::{Runtime, RuntimeHandle};
pub use scanner::run_scanner;
pub use telemetry::Telemetry;
