//! Utilidades de test (feature `testing`): fakes de los ports.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::domain::Timestamp;
use crate::ports::Clock;

/// Reloj controlado por el test. Los clones comparten la misma hora.
#[derive(Debug, Clone, Default)]
pub struct ManualClock(Arc<AtomicU64>);

impl ManualClock {
    pub fn new(ms: u64) -> Self {
        Self(Arc::new(AtomicU64::new(ms)))
    }

    pub fn set(&self, ms: u64) {
        self.0.store(ms, Ordering::SeqCst);
    }

    pub fn advance(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.0.load(Ordering::SeqCst))
    }
}
