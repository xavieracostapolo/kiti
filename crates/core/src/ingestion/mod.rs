//! Capa anticorrupción (ADR 0002 §1, Ingestion): traduce los payloads de cada CLI a `Signal`.
//! Lee de forma tolerante solo los campos que usa; el resto (prompts, salidas) se descarta.

pub mod claude;

use crate::domain::{SessionId, Signal};

/// Un hook ya normalizado, listo para el `Correlator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEvent {
    pub session: SessionId,
    pub cwd: String,
    /// Nombre nativo del evento, para logs y métricas.
    pub name: String,
    pub signal: Signal,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AclError {
    #[error("payload no es JSON válido: {0}")]
    Json(String),
    #[error("falta el campo {0}")]
    Missing(&'static str),
}
