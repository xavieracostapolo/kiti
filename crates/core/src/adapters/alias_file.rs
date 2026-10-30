//! Alias de los mensajes entre agentes en `aliases.json` (ADR 0017). Escritura atómica
//! (archivo temporal + rename): un cierre a mitad no deja el libro a medias.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::domain::{AgentId, AliasEntry, AliasKey, Timestamp};
use crate::ports::AliasStore;

#[derive(Serialize, Deserialize)]
struct Row {
    /// `tmux:kiti-3` o `process:<AgentId>`.
    key: String,
    alias: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    released_at: Option<u64>,
}

fn key_to_string(key: &AliasKey) -> String {
    match key {
        AliasKey::Tmux(s) => format!("tmux:{s}"),
        AliasKey::Process(id) => format!("process:{id}"),
    }
}

fn key_from_str(s: &str) -> Option<AliasKey> {
    if let Some(name) = s.strip_prefix("tmux:") {
        return Some(AliasKey::Tmux(name.to_owned()));
    }
    s.strip_prefix("process:")?
        .parse::<AgentId>()
        .ok()
        .map(AliasKey::Process)
}

pub struct FileAliasStore {
    path: PathBuf,
}

impl FileAliasStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Lo guardado; archivo ausente o ilegible = libro vacío.
    pub fn load(&self) -> Vec<AliasEntry> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let rows: Vec<Row> = serde_json::from_str(&text).unwrap_or_default();
        rows.into_iter()
            .filter_map(|r| {
                Some(AliasEntry {
                    key: key_from_str(&r.key)?,
                    alias: r.alias,
                    released_at: r.released_at.map(Timestamp),
                })
            })
            .collect()
    }
}

impl AliasStore for FileAliasStore {
    fn save(&mut self, entries: &[AliasEntry]) {
        let rows: Vec<Row> = entries
            .iter()
            .map(|e| Row {
                key: key_to_string(&e.key),
                alias: e.alias.clone(),
                released_at: e.released_at.map(|t| t.0),
            })
            .collect();
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("json.tmp");
        let result = serde_json::to_vec_pretty(&rows)
            .map_err(std::io::Error::other)
            .and_then(|json| std::fs::write(&tmp, json))
            .and_then(|()| std::fs::rename(&tmp, &self.path));
        if let Err(e) = result {
            warn!(event = "mail.aliases_save", error = %e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Pid, ProcessStartTime, Provider};

    #[test]
    fn guarda_y_carga_el_libro_sin_dejar_temporales() {
        let dir = std::env::temp_dir().join(format!("kiti-alias-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("aliases.json");
        let id = AgentId {
            provider: Provider::Antigravity,
            pid: Pid(42),
            start_time: ProcessStartTime(7),
        };
        let entries = vec![
            AliasEntry {
                key: AliasKey::Tmux("kiti-3".into()),
                alias: "claude-1".into(),
                released_at: None,
            },
            AliasEntry {
                key: AliasKey::Process(id),
                alias: "agy-1".into(),
                released_at: Some(Timestamp(5)),
            },
        ];
        let mut store = FileAliasStore::new(&path);
        assert!(store.load().is_empty());
        store.save(&entries);
        assert_eq!(store.load(), entries);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::write(&path, "no es json").unwrap();
        assert!(store.load().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}
