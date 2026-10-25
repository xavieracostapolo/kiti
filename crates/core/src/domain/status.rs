//! Estado del semáforo. El color ([`TrafficLight`]) nunca se guarda: se deriva de `StatusKind`.

use super::ids::Timestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusKind {
    Working,
    Waiting,
    Idle,
    Error,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusReason {
    Hook,
    SessionFile,
    Stale,
    Failure,
    ProcessGone,
    Startup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub kind: StatusKind,
    pub reason: StatusReason,
    /// Desde cuándo está en este `kind`. No cambia si solo cambia el motivo.
    pub since: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLight {
    Green,
    Yellow,
    Red,
    Gray,
}

impl From<StatusKind> for TrafficLight {
    fn from(kind: StatusKind) -> Self {
        match kind {
            StatusKind::Working => TrafficLight::Green,
            StatusKind::Waiting => TrafficLight::Yellow,
            StatusKind::Error | StatusKind::Dead => TrafficLight::Red,
            StatusKind::Idle => TrafficLight::Gray,
        }
    }
}
