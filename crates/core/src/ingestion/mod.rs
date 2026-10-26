//! Capa anticorrupción (ADR 0002 §1, Ingestion): traduce los payloads de cada CLI a `Signal`.
//! Lee de forma tolerante solo los campos que usa; el resto (prompts, salidas) se descarta.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod gemini;

use crate::domain::{AgentInfo, SessionId, Signal};

/// Un hook ya normalizado, listo para el `Correlator`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEvent {
    pub session: SessionId,
    pub cwd: String,
    /// Nombre nativo del evento, para logs y métricas.
    pub name: String,
    pub signal: Signal,
    /// Título, actividad y último pedido, si el evento los trae (ADR 0009).
    pub info: AgentInfo,
}

/// Texto en una línea y de como mucho `max` caracteres, con `…` si se recorta.
pub(crate) fn clip(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AclError {
    #[error("payload no es JSON válido: {0}")]
    Json(String),
    #[error("falta el campo {0}")]
    Missing(&'static str),
}
