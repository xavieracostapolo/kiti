//! Núcleo de Kiti (ADR 0002). Rust puro y **sin Tauri**.
//!
//! - [`domain`]: agregado `Agent`, reducer puro del semáforo y registro. Sin I/O ni reloj.
//! - [`application`]: runtime, coalescedor, proyección a `kiti-protocol` y telemetría.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ingestion;
pub mod ports;

#[cfg(feature = "testing")]
pub mod testing;

#[cfg(feature = "sim")]
pub mod sim;
