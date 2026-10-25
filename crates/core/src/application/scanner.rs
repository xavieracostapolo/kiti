//! Bucle del escáner de procesos (ADR 0002 §4): fuera del actor, en `spawn_blocking`.

use std::time::{Duration, Instant};

use tracing::{debug, warn};

use super::runtime::RuntimeHandle;
use crate::domain::{DetectedAgent, dedupe_tree};
use crate::ports::{Clock, ProcessScanner};

/// Escanea cada `every`, clasifica y envía el resultado al runtime con `Reconcile`.
/// Termina si el runtime se cierra.
pub async fn run_scanner<S: ProcessScanner, C: Clock>(
    mut scanner: S,
    runtime: RuntimeHandle,
    clock: C,
    every: Duration,
) {
    let mut ticker = tokio::time::interval(every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let started = Instant::now();
        let Ok((back, result)) = tokio::task::spawn_blocking(move || {
            let result = scanner.scan();
            (scanner, result)
        })
        .await
        else {
            warn!(event = "scan", "la tarea de escaneo terminó con pánico");
            return;
        };
        scanner = back;
        match result {
            Ok(snapshots) => {
                let detected: Vec<DetectedAgent> = dedupe_tree(&snapshots)
                    .into_iter()
                    .map(|(s, provider)| s.to_detected(provider))
                    .collect();
                debug!(
                    event = "scan",
                    duration_ms = started.elapsed().as_millis() as u64,
                    processes = snapshots.len(),
                    agents = detected.len()
                );
                runtime.reconcile(detected, clock.now()).await;
            }
            Err(e) => warn!(event = "scan", error = %e),
        }
        if runtime.is_closed() {
            return;
        }
    }
}
