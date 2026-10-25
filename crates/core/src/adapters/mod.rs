//! Adapters de los ports (ADR 0002 §5). Los que traen dependencias pesadas (axum, sysinfo,
//! notify) irán detrás de *features* en los Hitos 3 y 4.

mod clock;

pub use clock::SystemClock;
