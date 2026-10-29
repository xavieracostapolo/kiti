//! Cuota de uso de cada proveedor (ADR 0015) y cuándo avisar de que queda poca. Puro.

use std::collections::HashSet;
use std::time::Duration;

use super::ids::{Provider, Timestamp};

/// Una ventana de límite: "Claude · 5 h", "Claude y GPT · semana"…
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaWindow {
    /// Texto visible (sin el proveedor), p. ej. `5 h` o `Gemini · semana`.
    pub label: String,
    /// Semanal (las de 5 h se reinician cada poco y no avisan).
    pub weekly: bool,
    /// Lo que queda, de 0 a 1.
    pub remaining: f64,
    /// Cuándo se reinicia, en ms desde epoch (0 si no se sabe).
    pub resets_at: u64,
}

/// A partir de aquí (incluido) se avisa.
pub const LOW: f64 = 0.20;

#[derive(Debug, Clone, PartialEq)]
pub struct QuotaAlert {
    pub provider: Provider,
    pub window: QuotaWindow,
}

/// Avisa una vez por ventana semanal y periodo (`resets_at`) al bajar de `LOW`.
#[derive(Debug, Default)]
pub struct QuotaPolicy {
    alerted: HashSet<(Provider, String, u64)>,
}

impl QuotaPolicy {
    pub fn observe(&mut self, provider: Provider, windows: &[QuotaWindow]) -> Vec<QuotaAlert> {
        windows
            .iter()
            .filter(|w| w.weekly && w.remaining <= LOW)
            .filter(|w| {
                self.alerted
                    .insert((provider, w.label.clone(), w.resets_at))
            })
            .map(|w| QuotaAlert {
                provider,
                window: w.clone(),
            })
            .collect()
    }
}

/// Cuándo volver a consultar una cuota que hay que pedir (la de Antigravity): al terminar un turno
/// del agente, que es cuando cambia, sin bajar de `min_gap` entre consultas; y, sin turnos, cada
/// `idle` por si se gastó desde fuera de Kiti.
#[derive(Debug)]
pub struct QuotaRefresh {
    min_gap: Duration,
    idle: Duration,
    last: Option<Timestamp>,
    pending: bool,
}

impl QuotaRefresh {
    pub fn new(min_gap: Duration, idle: Duration) -> Self {
        Self {
            min_gap,
            idle,
            last: None,
            pending: false,
        }
    }

    /// Terminó un turno: la cuota pudo cambiar.
    pub fn kick(&mut self) {
        self.pending = true;
    }

    /// Cuánto falta para la próxima consulta (cero: ya toca).
    pub fn wait(&self, now: Timestamp) -> Duration {
        let Some(last) = self.last else {
            return Duration::ZERO;
        };
        let gap = if self.pending {
            self.min_gap
        } else {
            self.idle
        };
        gap.saturating_sub(now.since(last))
    }

    /// Se acaba de consultar.
    pub fn polled(&mut self, now: Timestamp) {
        self.last = Some(now);
        self.pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1000;

    fn refresh() -> QuotaRefresh {
        QuotaRefresh::new(Duration::from_secs(30), Duration::from_secs(300))
    }

    #[test]
    fn la_primera_consulta_es_inmediata_y_sin_turnos_espera_el_intervalo_largo() {
        let mut r = refresh();
        assert_eq!(r.wait(Timestamp(0)), Duration::ZERO);
        r.polled(Timestamp(0));
        assert_eq!(r.wait(Timestamp(100 * S)), Duration::from_secs(200));
        assert_eq!(r.wait(Timestamp(300 * S)), Duration::ZERO);
    }

    #[test]
    fn un_turno_terminado_adelanta_la_consulta_sin_bajar_del_minimo() {
        let mut r = refresh();
        r.polled(Timestamp(0));
        r.kick();
        // Dentro del mínimo: espera lo que falta, no consulta en cada turno.
        assert_eq!(r.wait(Timestamp(10 * S)), Duration::from_secs(20));
        // Pasado el mínimo: ya toca.
        assert_eq!(r.wait(Timestamp(45 * S)), Duration::ZERO);
        r.polled(Timestamp(45 * S));
        // Consultada, vuelve al intervalo largo.
        assert_eq!(r.wait(Timestamp(50 * S)), Duration::from_secs(295));
    }

    fn w(label: &str, weekly: bool, remaining: f64, resets_at: u64) -> QuotaWindow {
        QuotaWindow {
            label: label.into(),
            weekly,
            remaining,
            resets_at,
        }
    }

    #[test]
    fn avisa_una_vez_por_periodo_semanal_al_bajar_del_20() {
        let mut p = QuotaPolicy::default();
        assert!(
            p.observe(Provider::Claude, &[w("semana", true, 0.5, 100)])
                .is_empty()
        );
        let low = p.observe(Provider::Claude, &[w("semana", true, 0.18, 100)]);
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].window.remaining, 0.18);
        // Sigue bajando en el mismo periodo: no se repite.
        assert!(
            p.observe(Provider::Claude, &[w("semana", true, 0.05, 100)])
                .is_empty()
        );
        // Nuevo periodo (otro resets_at): vuelve a avisar si baja.
        assert_eq!(
            p.observe(Provider::Claude, &[w("semana", true, 0.1, 200)])
                .len(),
            1
        );
    }

    #[test]
    fn las_de_5_h_no_avisan_y_cada_proveedor_va_por_su_cuenta() {
        let mut p = QuotaPolicy::default();
        assert!(
            p.observe(Provider::Claude, &[w("5 h", false, 0.01, 1)])
                .is_empty()
        );
        assert_eq!(
            p.observe(Provider::Claude, &[w("semana", true, 0.2, 1)])
                .len(),
            1
        );
        assert_eq!(
            p.observe(Provider::Antigravity, &[w("semana", true, 0.2, 1)])
                .len(),
            1
        );
    }
}
