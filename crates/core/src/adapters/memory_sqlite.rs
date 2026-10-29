//! Almacén persistente de recuerdos en SQLite con `WAL` y `FTS5` (`bm25`) (ADR 0018).
//!
//! Usa `/usr/bin/sqlite3` (incluido en macOS con `FTS5` compilado) para no añadir dependencias C
//! ni consumo de RAM en reposo.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use crate::domain::{KEEP_MEMORIES, MemoryEntry, MemoryId, MemoryKind, Timestamp};
use crate::ports::MemoryStore;

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

fn sql_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Convierte una consulta libre en una expresión `FTS5` segura (`"term1"* OR "term2"*`),
/// inmune a comillas desbalanceadas o caracteres especiales de la sintaxis FTS5.
pub fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|t| t.trim_matches('-').to_lowercase())
        .filter(|t| !t.is_empty())
        .take(12)
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

fn parse_rows(out: &str) -> Vec<Vec<String>> {
    out.split(RECORD)
        .filter(|r| !r.trim().is_empty())
        .map(|r| {
            r.split(FIELD)
                .map(|f| f.trim_matches('\n').to_owned())
                .collect()
        })
        .collect()
}

fn row_to_entry(row: &[String]) -> Option<MemoryEntry> {
    let [id, project, kind, title, detail, files, author, created_at] = row else {
        return None;
    };
    let id = id.parse::<u64>().ok()?;
    let kind = MemoryKind::from_str(kind).ok()?;
    let files: Vec<String> = serde_json::from_str(files).unwrap_or_default();
    let created_at = created_at.parse::<u64>().ok()?;
    Some(MemoryEntry {
        id: MemoryId(id),
        project: project.clone(),
        kind,
        title: title.clone(),
        detail: detail.clone(),
        files,
        author: author.clone(),
        created_at: Timestamp(created_at),
    })
}

pub struct SqliteMemoryStore {
    db: PathBuf,
}

impl SqliteMemoryStore {
    pub fn new(db: PathBuf) -> Self {
        let store = Self { db };
        store.init();
        store
    }

    pub fn path(&self) -> &Path {
        &self.db
    }

    fn exec(&self, sql: &str) -> Option<String> {
        if let Some(parent) = self.db.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out = Command::new("/usr/bin/sqlite3")
            .args(["-ascii", &self.db.display().to_string(), sql])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn init(&self) {
        let schema = "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY,
                project TEXT NOT NULL,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL,
                files TEXT NOT NULL,
                author TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_project ON memories(project, created_at DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                title,
                detail,
                files,
                tokenize='porter unicode61'
            );
        ";
        let _ = self.exec(schema);
    }

    /// Lee los recuerdos guardados (en orden cronológico ascendente) para hidratar `MemoryBook`.
    pub fn load_all(&self) -> Vec<MemoryEntry> {
        if !self.db.is_file() {
            return Vec::new();
        }
        let sql = format!(
            "SELECT id, project, kind, title, detail, files, author, created_at \
             FROM (SELECT * FROM memories ORDER BY id DESC LIMIT {KEEP_MEMORIES}) \
             ORDER BY id ASC;"
        );
        self.exec(&sql)
            .map(|out| {
                parse_rows(&out)
                    .iter()
                    .filter_map(|r| row_to_entry(r))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl MemoryStore for SqliteMemoryStore {
    fn save(&mut self, entry: &MemoryEntry) {
        let files_json = serde_json::to_string(&entry.files).unwrap_or_else(|_| "[]".into());
        let files_text = entry.files.join(" ");
        let sql = format!(
            "BEGIN IMMEDIATE; \
             INSERT OR REPLACE INTO memories (id, project, kind, title, detail, files, author, created_at) \
             VALUES ({id}, {project}, {kind}, {title}, {detail}, {files_json}, {author}, {created_at}); \
             DELETE FROM memories_fts WHERE rowid = {id}; \
             INSERT INTO memories_fts (rowid, title, detail, files) \
             VALUES ({id}, {title}, {detail}, {files_text}); \
             COMMIT;",
            id = entry.id.0,
            project = sql_quote(&entry.project),
            kind = sql_quote(entry.kind.as_str()),
            title = sql_quote(&entry.title),
            detail = sql_quote(&entry.detail),
            files_json = sql_quote(&files_json),
            files_text = sql_quote(&files_text),
            author = sql_quote(&entry.author),
            created_at = entry.created_at.0,
        );
        let _ = self.exec(&sql);
    }

    fn search(&mut self, project: &str, query: &str, limit: usize) -> Option<Vec<MemoryEntry>> {
        let fts = fts_query(query)?;
        let sql = format!(
            "SELECT m.id, m.project, m.kind, m.title, m.detail, m.files, m.author, m.created_at \
             FROM memories_fts f \
             JOIN memories m ON m.id = f.rowid \
             WHERE memories_fts MATCH {fts} AND m.project = {project} \
             ORDER BY bm25(memories_fts, 5.0, 1.0, 2.0), m.created_at DESC \
             LIMIT {limit};",
            fts = sql_quote(&fts),
            project = sql_quote(project),
            limit = limit.max(1),
        );
        let out = self.exec(&sql)?;
        Some(
            parse_rows(&out)
                .iter()
                .filter_map(|r| row_to_entry(r))
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MemoryBook;

    #[test]
    fn persiste_hidrata_y_busca_con_fts5_bm25() {
        let dir = std::env::temp_dir().join(format!("kiti-mem-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = dir.join("memory.db");

        let mut store = SqliteMemoryStore::new(db.clone());
        let mut book = MemoryBook::default();

        let e1 = book
            .remember(
                "/Users/dev/kiti",
                "agy-2",
                "gotcha",
                "Permisos de mise en sandbox",
                "En el sandbox mise exec falla con os error 1; usar node de NVM.",
                &["ui/package.json".into()],
                Timestamp(1_000),
            )
            .unwrap();
        store.save(&e1);

        let e2 = book
            .remember(
                "/Users/dev/kiti",
                "claude-2",
                "decision",
                "Motor SQLite FTS5 con BM25",
                "Para recuperar recuerdos en menos de 1 ms sin gastar ventana de contexto.",
                &["crates/core/src/adapters/memory_sqlite.rs".into()],
                Timestamp(2_000),
            )
            .unwrap();
        store.save(&e2);

        let e3 = book
            .remember(
                "/Users/dev/otro",
                "claude-1",
                "note",
                "SQLite en otro proyecto",
                "No debe mezclarse con kiti.",
                &[],
                Timestamp(3_000),
            )
            .unwrap();
        store.save(&e3);

        // Hidratar un libro nuevo desde el disco.
        let loaded = store.load_all();
        assert_eq!(loaded.len(), 3);
        let mut restored = MemoryBook::default();
        restored.load(loaded);
        assert_eq!(restored.for_project("/Users/dev/kiti").len(), 2);

        // Búsqueda FTS5 BM25: incluso con símbolos raros en la consulta.
        let hits = store
            .search("/Users/dev/kiti", "sandbox: 'mise' (permiso)?", 5)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Permisos de mise en sandbox");
        assert_eq!(hits[0].files, vec!["ui/package.json"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
