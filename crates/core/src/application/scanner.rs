//! Bucle del escáner de procesos (ADR 0002 §4): fuera del actor, en `spawn_blocking`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use super::runtime::RuntimeHandle;
use crate::domain::{DetectedAgent, dedupe_tree};
use crate::ingestion::claude::{SessionFile, parse_session_file};
use crate::ports::{Clock, ProcessScanner};

/// Lee `~/.claude/sessions/*.json`. Un directorio ausente o un archivo ilegible no son errores:
/// Claude puede no estar instalado o estar escribiendo el archivo en ese momento.
pub fn read_session_dir(dir: &Path) -> Vec<SessionFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|body| parse_session_file(&body))
        .collect()
}

/// Escanea cada `every`, clasifica y envía el resultado al runtime con `Reconcile`, junto con
/// los archivos de sesión de Claude si se pasa su directorio. Termina si el runtime se cierra.
pub async fn run_scanner<S: ProcessScanner, C: Clock>(
    mut scanner: S,
    sessions_dir: Option<PathBuf>,
    runtime: RuntimeHandle,
    clock: C,
    every: Duration,
) {
    let mut ticker = tokio::time::interval(every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let started = Instant::now();
        let dir = sessions_dir.clone();
        let Ok((back, result, sessions)) = tokio::task::spawn_blocking(move || {
            let result = scanner.scan();
            let sessions = dir.as_deref().map(read_session_dir).unwrap_or_default();
            (scanner, result, sessions)
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
                runtime.reconcile(detected, sessions, clock.now()).await;
            }
            Err(e) => warn!(event = "scan", error = %e),
        }
        if runtime.is_closed() {
            return;
        }
    }
}
