//! Conversaciones de Claude en disco (ADR 0009): `~/.claude/projects/<carpeta>/<id>.jsonl`.
//! Se leen solo los últimos bytes de cada transcript y se recuerdan por fecha y tamaño.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::tail::Tail;
use crate::domain::SessionId;
use crate::ingestion::claude::{conversation_meta, is_session_id};

/// Lo bastante para encontrar el último título, pedido y `cwd` sin leer transcripts de ~60 MB.
const BACK: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    pub session: SessionId,
    pub cwd: String,
    pub title: Option<String>,
    pub prompt: Option<String>,
    /// Última escritura del transcript, en ms desde epoch.
    pub updated_at: u64,
}

#[derive(Default)]
pub struct ConversationIndex {
    /// Ruta → (fecha y tamaño cuando se leyó, resultado). `None`: sin `cwd` o sin nada que mostrar.
    cache: HashMap<PathBuf, (SystemTime, u64, Option<Conversation>)>,
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl ConversationIndex {
    /// Conversaciones con actividad desde `since`, de la más reciente a la más vieja. Las que no
    /// tienen ni título ni pedido (abiertas y cerradas sin escribir) no cuentan.
    pub fn list(&mut self, projects: &Path, since: SystemTime) -> Vec<Conversation> {
        let mut seen = Vec::new();
        let dirs = std::fs::read_dir(projects).into_iter().flatten().flatten();
        for file in dirs
            .filter_map(|d| std::fs::read_dir(d.path()).ok())
            .flatten()
            .flatten()
        {
            let path = file.path();
            let Some(id) = path
                .extension()
                .filter(|x| *x == "jsonl")
                .and(path.file_stem())
                .and_then(|s| s.to_str())
                .filter(|s| is_session_id(s))
                .map(str::to_owned)
            else {
                continue;
            };
            let Ok(meta) = file.metadata() else { continue };
            let Ok(modified) = meta.modified() else {
                continue;
            };
            if modified < since {
                continue;
            }
            seen.push(path.clone());
            if self
                .cache
                .get(&path)
                .is_some_and(|(m, len, _)| *m == modified && *len == meta.len())
            {
                continue;
            }
            let conversation = read_conversation(&path, &id, ms(modified));
            self.cache
                .insert(path, (modified, meta.len(), conversation));
        }
        self.cache.retain(|path, _| seen.contains(path));
        let mut all: Vec<Conversation> = self
            .cache
            .values()
            .filter_map(|(_, _, c)| c.clone())
            .collect();
        all.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        all
    }

    /// Conversación ya indexada con ese id.
    pub fn find(&self, session: &str) -> Option<&Conversation> {
        self.cache
            .values()
            .filter_map(|(_, _, c)| c.as_ref())
            .find(|c| c.session.as_str() == session)
    }
}

/// Conversaciones que el usuario archivó: no vuelven a salir como oficinas apagadas (ADR 0009).
/// Solo se guarda su id en un JSON propio de Kiti; el transcript de Claude no se toca.
pub struct HiddenConversations {
    path: PathBuf,
    /// En orden de llegada: al pasar de `HIDDEN_MAX` se olvidan las más viejas.
    ids: Vec<String>,
}

const HIDDEN_MAX: usize = 2000;

#[derive(serde::Serialize, serde::Deserialize)]
struct HiddenFile {
    hidden: Vec<String>,
}

impl HiddenConversations {
    /// Un archivo ausente o ilegible es una lista vacía: archivar nunca impide arrancar.
    pub fn load(path: PathBuf) -> Self {
        let ids = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<HiddenFile>(&b).ok())
            .map(|f| {
                f.hidden
                    .into_iter()
                    .filter(|id| is_session_id(id))
                    .collect()
            })
            .unwrap_or_default();
        Self { path, ids }
    }

    pub fn contains(&self, session: &str) -> bool {
        self.ids.iter().any(|id| id == session)
    }

    /// Archiva y guarda en disco (escritura atómica: temporal y `rename`).
    pub fn hide(&mut self, session: &str) -> std::io::Result<()> {
        if !is_session_id(session) {
            return Err(std::io::Error::other("id de conversación inválido"));
        }
        if self.contains(session) {
            return Ok(());
        }
        self.ids.push(session.to_owned());
        if self.ids.len() > HIDDEN_MAX {
            self.ids.drain(..self.ids.len() - HIDDEN_MAX);
        }
        let body = serde_json::to_vec_pretty(&HiddenFile {
            hidden: self.ids.clone(),
        })
        .map_err(std::io::Error::other)?;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(tmp, &self.path)
    }
}

fn read_conversation(path: &Path, id: &str, updated_at: u64) -> Option<Conversation> {
    let lines = Tail::open_at_end(path, BACK).ok()?.read_new().ok()?;
    let meta = conversation_meta(&lines);
    if meta.title.is_none() && meta.prompt.is_none() {
        return None;
    }
    Some(Conversation {
        session: SessionId::new(id),
        cwd: meta.cwd?,
        title: meta.title,
        prompt: meta.prompt,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn lista_las_recientes_con_titulo_y_las_recuerda() {
        let root = std::env::temp_dir().join(format!("kiti-conversations-{}", std::process::id()));
        let dir = root.join("-w-thoth");
        std::fs::create_dir_all(dir.join("aaaaaaaa-1111/subagents")).unwrap();
        let line = |t: &str| format!("{t}\n");
        std::fs::write(
            dir.join("aaaaaaaa-1111.jsonl"),
            line(r#"{"type":"user","cwd":"/w/thoth","message":{"content":"x"}}"#)
                + &line(r#"{"type":"ai-title","aiTitle":"Vulnerabilidades","sessionId":"aaaaaaaa-1111"}"#),
        )
        .unwrap();
        // Sin título ni pedido: no cuenta. Tampoco lo que no es un transcript.
        std::fs::write(
            dir.join("bbbbbbbb-2222.jsonl"),
            line(r#"{"type":"user","cwd":"/w/thoth"}"#),
        )
        .unwrap();
        std::fs::write(dir.join("notas.txt"), "x").unwrap();

        let mut index = ConversationIndex::default();
        let week_ago = SystemTime::now() - Duration::from_secs(7 * 24 * 3600);
        let found = index.list(&root, week_ago);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].session.as_str(), "aaaaaaaa-1111");
        assert_eq!(found[0].cwd, "/w/thoth");
        assert_eq!(found[0].title.as_deref(), Some("Vulnerabilidades"));
        assert!(index.find("aaaaaaaa-1111").is_some());

        // Nada es reciente si el corte es el futuro.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(index.list(&root, later).is_empty());
        assert!(index.find("aaaaaaaa-1111").is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn las_archivadas_se_recuerdan_entre_arranques() {
        let dir = std::env::temp_dir().join(format!("kiti-hidden-{}", std::process::id()));
        let path = dir.join("hidden-conversations.json");
        let mut hidden = HiddenConversations::load(path.clone());
        assert!(!hidden.contains("aaaaaaaa-1111"));
        hidden.hide("aaaaaaaa-1111").unwrap();
        hidden.hide("aaaaaaaa-1111").unwrap();
        assert!(hidden.hide("x'; rm -rf ~").is_err());
        let again = HiddenConversations::load(path.clone());
        assert!(again.contains("aaaaaaaa-1111"));
        assert_eq!(again.ids.len(), 1);
        // Un archivo roto no impide arrancar.
        std::fs::write(&path, "no es json").unwrap();
        assert!(!HiddenConversations::load(path).contains("aaaaaaaa-1111"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
