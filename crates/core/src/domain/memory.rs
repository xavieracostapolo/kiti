//! Memoria de sesión y de proyecto (ADR 0018). Pura y sin I/O.
//!
//! - De cada sesión se recuerdan los archivos tocados (`touched_files`), el checklist (`tasks`) y
//!   el objetivo inicial (`initial_prompt`), todo extraído de los hooks sin coste de tokens.
//! - De cada proyecto (`MemoryBook`) se guardan recuerdos estructurados (`decision`, `gotcha`,
//!   `handoff`, `note`) que los agentes escriben con `kiti_remember` y consultan en dos pasos con
//!   `kiti_recall` (índice ligero sin consulta; detalle por relevancia al pasar `query`).

use std::collections::VecDeque;
use std::str::FromStr;

use super::ids::Timestamp;

/// Máximo de archivos tocados que conserva una sesión.
pub const TOUCHED_MAX: usize = 16;
/// Máximo de tareas del checklist (`TodoWrite`) que conserva una sesión.
pub const TASKS_MAX: usize = 16;
/// Máximo de caracteres del título de un recuerdo.
pub const MAX_TITLE: usize = 120;
/// Máximo de caracteres del detalle de un recuerdo.
pub const MAX_DETAIL: usize = 600;
/// Máximo de archivos asociados a un recuerdo.
pub const MAX_MEMORY_FILES: usize = 8;
/// Máximo de recuerdos en memoria por instancia de `MemoryBook`.
pub const KEEP_MEMORIES: usize = 500;
/// Máximo de resultados devueltos en una búsqueda `kiti_recall(query)`.
pub const SEARCH_LIMIT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskItem {
    pub text: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemoryId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Decision,
    Gotcha,
    Handoff,
    Note,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Gotcha => "gotcha",
            Self::Handoff => "handoff",
            Self::Note => "note",
        }
    }
}

impl FromStr for MemoryKind {
    type Err = RememberError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "decision" => Ok(Self::Decision),
            "gotcha" => Ok(Self::Gotcha),
            "handoff" => Ok(Self::Handoff),
            "note" => Ok(Self::Note),
            other => Err(RememberError::InvalidKind(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEntry {
    pub id: MemoryId,
    pub project: String,
    pub kind: MemoryKind,
    pub title: String,
    pub detail: String,
    pub files: Vec<String>,
    pub author: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RememberError {
    #[error("el remitente no es un agente conocido")]
    UnknownCaller,
    #[error("tipo de recuerdo inválido «{0}» (usa decision, gotcha, handoff o note)")]
    InvalidKind(String),
    #[error("el título del recuerdo está vacío")]
    EmptyTitle,
    #[error("el detalle del recuerdo está vacío")]
    EmptyDetail,
}

impl RememberError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownCaller => "unknown_sender",
            Self::InvalidKind(_) => "invalid_kind",
            Self::EmptyTitle => "empty_title",
            Self::EmptyDetail => "empty_detail",
        }
    }
}

fn clean_line(text: &str, max: usize) -> String {
    let flat = text
        .chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let mut out: String = flat.chars().take(max - 1).collect();
    out.push('…');
    out
}

fn clean_text(text: &str, max: usize) -> String {
    let filtered = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .trim()
        .to_owned();
    if filtered.chars().count() <= max {
        return filtered;
    }
    let mut out: String = filtered.chars().take(max - 1).collect();
    out.push('…');
    out
}

#[derive(Debug, Default)]
pub struct MemoryBook {
    next_id: u64,
    total_stored: u64,
    entries: VecDeque<MemoryEntry>,
}

impl MemoryBook {
    /// Carga recuerdos iniciales (p. ej. leídos de `memory.db` al arrancar).
    pub fn load(&mut self, entries: Vec<MemoryEntry>) {
        for e in entries {
            self.next_id = self.next_id.max(e.id.0);
            self.entries.push_back(e);
            while self.entries.len() > KEEP_MEMORIES {
                self.entries.pop_front();
            }
        }
        self.total_stored = self.entries.len() as u64;
    }

    pub fn total(&self) -> u64 {
        self.total_stored
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remember(
        &mut self,
        project: &str,
        author: &str,
        kind: &str,
        title: &str,
        detail: &str,
        files: &[String],
        now: Timestamp,
    ) -> Result<MemoryEntry, RememberError> {
        let kind = MemoryKind::from_str(kind)?;
        let title = clean_line(title, MAX_TITLE);
        if title.is_empty() {
            return Err(RememberError::EmptyTitle);
        }
        let detail = clean_text(detail, MAX_DETAIL);
        if detail.is_empty() {
            return Err(RememberError::EmptyDetail);
        }
        let mut clean_files = Vec::new();
        for f in files {
            let name = clean_line(f, 80);
            if !name.is_empty() && !clean_files.contains(&name) {
                clean_files.push(name);
                if clean_files.len() >= MAX_MEMORY_FILES {
                    break;
                }
            }
        }
        self.next_id += 1;
        self.total_stored += 1;
        let entry = MemoryEntry {
            id: MemoryId(self.next_id),
            project: project.to_owned(),
            kind,
            title,
            detail,
            files: clean_files,
            author: author.to_owned(),
            created_at: now,
        };
        self.entries.push_back(entry.clone());
        while self.entries.len() > KEEP_MEMORIES {
            self.entries.pop_front();
        }
        Ok(entry)
    }

    /// Recuerdos de `project`, del más reciente al más antiguo.
    pub fn for_project(&self, project: &str) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|e| e.project == project)
            .collect()
    }

    /// Todos los recuerdos en memoria, del más reciente al más antiguo.
    pub fn all(&self) -> impl Iterator<Item = &MemoryEntry> {
        self.entries.iter().rev()
    }

    /// Búsqueda en memoria por palabras clave con pesos (título × 5, archivos × 3, detalle × 1).
    /// Se usa en el simulador, en tests de dominio y como respaldo si no hay SQLite FTS5.
    pub fn search(&self, project: &str, query: &str, limit: usize) -> Vec<MemoryEntry> {
        let terms: Vec<String> = query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect();
        if terms.is_empty() {
            return self
                .for_project(project)
                .into_iter()
                .take(limit)
                .cloned()
                .collect();
        }
        let mut scored: Vec<(u32, u64, &MemoryEntry)> = self
            .entries
            .iter()
            .filter(|e| e.project == project)
            .filter_map(|e| {
                let title_l = e.title.to_lowercase();
                let detail_l = e.detail.to_lowercase();
                let files_l = e.files.join(" ").to_lowercase();
                let mut score = 0u32;
                for t in &terms {
                    if title_l.contains(t) {
                        score += 5;
                    }
                    if files_l.contains(t) {
                        score += 3;
                    }
                    if detail_l.contains(t) {
                        score += 1;
                    }
                }
                (score > 0).then_some((score, e.created_at.0, e))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, e)| e.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guarda_y_lista_recuerdos_por_proyecto_del_mas_nuevo_al_mas_viejo() {
        let mut book = MemoryBook::default();
        let e1 = book
            .remember(
                "/w/kiti",
                "agy-2",
                "gotcha",
                "Permisos de mise en sandbox",
                "En el sandbox mise exec falla; usar el node de NVM.",
                &["ui/package.json".into()],
                Timestamp(100),
            )
            .unwrap();
        let _e2 = book
            .remember(
                "/w/otro",
                "claude-1",
                "note",
                "Otro proyecto",
                "Detalle ajeno",
                &[],
                Timestamp(200),
            )
            .unwrap();
        let e3 = book
            .remember(
                "/w/kiti",
                "claude-2",
                "decision",
                "SQLite FTS5 para memoria",
                "Elegido por latencia < 1 ms y 0 MB de RAM extra.",
                &["memory.rs".into(), "memory.rs".into()],
                Timestamp(300),
            )
            .unwrap();
        assert_eq!(e1.id, MemoryId(1));
        assert_eq!(e3.id, MemoryId(3));
        assert_eq!(e3.files, vec!["memory.rs"]);
        assert_eq!(book.total(), 3);

        let kiti = book.for_project("/w/kiti");
        assert_eq!(kiti.len(), 2);
        assert_eq!(kiti[0].title, "SQLite FTS5 para memoria");
        assert_eq!(kiti[1].title, "Permisos de mise en sandbox");
    }

    #[test]
    fn valida_tipo_titulo_y_detalle() {
        let mut book = MemoryBook::default();
        assert_eq!(
            book.remember("/w/kiti", "a", "inventado", "t", "d", &[], Timestamp(1)),
            Err(RememberError::InvalidKind("inventado".into()))
        );
        assert_eq!(
            book.remember("/w/kiti", "a", "note", "   ", "d", &[], Timestamp(1)),
            Err(RememberError::EmptyTitle)
        );
        assert_eq!(
            book.remember("/w/kiti", "a", "note", "t", " \n ", &[], Timestamp(1)),
            Err(RememberError::EmptyDetail)
        );
    }

    #[test]
    fn busca_por_relevancia_ponderando_titulo_archivos_y_detalle() {
        let mut book = MemoryBook::default();
        book.remember(
            "/w/kiti",
            "claude-1",
            "note",
            "Ajuste menor de estilos",
            "Se menciona sqlite de pasada en el texto.",
            &[],
            Timestamp(10),
        )
        .unwrap();
        book.remember(
            "/w/kiti",
            "agy-2",
            "decision",
            "Motor SQLite FTS5 con BM25",
            "Búsqueda local instantánea.",
            &["memory_sqlite.rs".into()],
            Timestamp(20),
        )
        .unwrap();
        let hits = book.search("/w/kiti", "sqlite fts5", 5);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Motor SQLite FTS5 con BM25");
        assert_eq!(hits[1].title, "Ajuste menor de estilos");
    }
}
