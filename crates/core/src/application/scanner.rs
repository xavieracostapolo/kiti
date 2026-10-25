//! Bucle del escáner de procesos (ADR 0002 §4): fuera del actor, en `spawn_blocking`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::{debug, warn};

use super::runtime::RuntimeHandle;
use std::collections::HashMap;

use crate::adapters::tail::Tail;
use crate::domain::{DetectedAgent, SessionId, ToolName, dedupe_tree};
use crate::ingestion::claude::{SessionFile, parse_session_file, transcript_tool};

/// Al abrir un transcript solo se leen sus últimos bytes: los de sesiones largas pesan ~10 MB.
const TRANSCRIPT_BACK: u64 = 64 * 1024;

/// Sigue los transcripts de las sesiones vivas de Claude (respaldo de `last_tool` sin hooks).
#[derive(Default)]
pub struct TranscriptTracker {
    tails: HashMap<SessionId, Tail>,
}

impl TranscriptTracker {
    /// `projects`: `~/.claude/projects`. Devuelve la última herramienta nueva de cada sesión.
    pub fn poll(
        &mut self,
        projects: &Path,
        sessions: &[SessionFile],
    ) -> Vec<(SessionId, ToolName)> {
        self.tails
            .retain(|sid, _| sessions.iter().any(|s| &s.session == sid));
        let mut updates = Vec::new();
        for s in sessions {
            if !self.tails.contains_key(&s.session) {
                // `projects/<cwd codificado>/<sessionId>.jsonl`: se busca por nombre en lugar de
                // reproducir la codificación del cwd, que no está documentada.
                let name = format!("{}.jsonl", s.session.as_str());
                let found = std::fs::read_dir(projects).ok().and_then(|dirs| {
                    dirs.filter_map(Result::ok)
                        .map(|d| d.path().join(&name))
                        .find(|p| p.is_file())
                });
                let Some(tail) = found.and_then(|p| Tail::open_at_end(&p, TRANSCRIPT_BACK).ok())
                else {
                    continue;
                };
                self.tails.insert(s.session.clone(), tail);
            }
            let Some(tail) = self.tails.get_mut(&s.session) else {
                continue;
            };
            match tail.read_new() {
                Ok(lines) => {
                    if let Some(tool) = lines.iter().rev().find_map(|l| transcript_tool(l)) {
                        updates.push((s.session.clone(), tool));
                    }
                }
                Err(_) => {
                    self.tails.remove(&s.session);
                }
            }
        }
        updates
    }
}
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
    let mut transcripts = TranscriptTracker::default();
    let mut ticker = tokio::time::interval(every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let started = Instant::now();
        let dir = sessions_dir.clone();
        let mut tracker = std::mem::take(&mut transcripts);
        let Ok((back, result, sessions, tracker, tools)) = tokio::task::spawn_blocking(move || {
            let result = scanner.scan();
            let sessions = dir.as_deref().map(read_session_dir).unwrap_or_default();
            let tools = match dir.as_deref().and_then(Path::parent) {
                Some(claude) => tracker.poll(&claude.join("projects"), &sessions),
                None => Vec::new(),
            };
            (scanner, result, sessions, tracker, tools)
        })
        .await
        else {
            warn!(event = "scan", "la tarea de escaneo terminó con pánico");
            return;
        };
        scanner = back;
        transcripts = tracker;
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
                runtime
                    .reconcile(detected, sessions, tools, clock.now())
                    .await;
            }
            Err(e) => warn!(event = "scan", error = %e),
        }
        if runtime.is_closed() {
            return;
        }
    }
}
