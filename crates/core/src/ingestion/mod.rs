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

/// `2026-11-01T22:38:25Z` (o con fracción de segundo) → ms desde epoch. Solo UTC (`Z`).
pub(crate) fn rfc3339_ms(text: &str) -> Option<u64> {
    let t = text.strip_suffix('Z')?;
    let (date, time) = t.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut h = hms.splitn(3, ':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (h.next()??, h.next()??, h.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Días desde 1970-01-01 (algoritmo de Howard Hinnant, días civiles).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let ms: i64 = frac
        .chars()
        .chain(std::iter::repeat('0'))
        .take(3)
        .collect::<String>()
        .parse()
        .ok()?;
    let total = ((days * 24 + hh) * 60 + mm) * 60 + ss;
    u64::try_from(total * 1000 + ms).ok()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AclError {
    #[error("payload no es JSON válido: {0}")]
    Json(String),
    #[error("falta el campo {0}")]
    Missing(&'static str),
}

#[cfg(test)]
mod tests {
    use super::rfc3339_ms;

    #[test]
    fn fechas_rfc3339_a_ms() {
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        // `date -u -j -f %Y-%m-%dT%H:%M:%SZ 2026-11-01T22:38:25Z +%s` = 1793572705
        assert_eq!(rfc3339_ms("2026-11-01T22:38:25Z"), Some(1_793_572_705_000));
        assert_eq!(
            rfc3339_ms("2024-02-29T12:00:00.5Z"),
            Some(1_709_208_000_500)
        );
        assert_eq!(rfc3339_ms("2026-11-01T22:38:25+02:00"), None);
        assert_eq!(rfc3339_ms("ayer"), None);
    }
}
