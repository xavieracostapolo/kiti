pub mod fleet;

pub use fleet::{SimFleet, SimSpec, TICK_MS};

use crate::ports::{AgentKiller, KillError, KillOutcome, KillTarget};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct SimKiller;

impl AgentKiller for SimKiller {
    async fn terminate(
        &self,
        _target: KillTarget,
        _grace: Duration,
    ) -> Result<KillOutcome, KillError> {
        Ok(KillOutcome::Killed)
    }
}
