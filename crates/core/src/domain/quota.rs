//! Cuota de uso de cada proveedor (ADR 0015) y cuándo avisar de que queda poca. Puro.

use std::collections::HashSet;

use super::ids::Provider;

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

#[cfg(test)]
mod tests {
    use super::*;

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
