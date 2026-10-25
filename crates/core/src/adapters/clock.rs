use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::Timestamp;
use crate::ports::Clock;

/// Reloj de pared en epoch ms. El único sitio del core que lee la hora del sistema.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Timestamp(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        )
    }
}
