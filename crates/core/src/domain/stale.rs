//! Umbrales de caducidad del semáforo (ADR 0002 §3).

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StalePolicy {
    /// `Working` sin señales y sin herramientas abiertas pasa a `Idle`.
    pub idle_after: Duration,
    /// `Working` sin señales con herramientas abiertas pasa a `Idle` (una herramienta puede tardar).
    pub pending_max: Duration,
    /// Un `Dead` se retira del registro tras este tiempo.
    pub dead_ttl: Duration,
}

impl Default for StalePolicy {
    fn default() -> Self {
        Self {
            idle_after: Duration::from_secs(30),
            pending_max: Duration::from_secs(10 * 60),
            dead_ttl: Duration::from_secs(60),
        }
    }
}
