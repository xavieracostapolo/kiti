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
    /// Ignora mayúsculas y tildes, y un término de 4 o más letras también coincide por prefijo
    /// (singular y plural). Es la búsqueda real: SQLite solo persiste.
    pub fn search(&self, project: &str, query: &str, limit: usize) -> Vec<MemoryEntry> {
        let terms = words(query);
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
                let title_w = words(&e.title);
                let detail_w = words(&e.detail);
                let files_w = words(&e.files.join(" "));
                let mut score = 0u32;
                for t in &terms {
                    if matches_any(t, &title_w) {
                        score += 5;
                    }
                    if matches_any(t, &files_w) {
                        score += 3;
                    }
                    if matches_any(t, &detail_w) {
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

/// Minúsculas y sin diacríticos del castellano (á é í ó ú ü ñ).
fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            c => c,
        })
        .collect()
}

/// Palabras normalizadas de un texto.
fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Longitud mínima para coincidir por prefijo.
const PREFIX_MIN: usize = 4;

fn matches_any(term: &str, words: &[String]) -> bool {
    let long = term.chars().count() >= PREFIX_MIN;
    words.iter().any(|w| {
        w == term
            || (long
                && w.chars().count() >= PREFIX_MIN
                && (w.starts_with(term) || term.starts_with(w.as_str())))
    })
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

    fn book_with(entries: &[(&str, &str, &str, u64)]) -> MemoryBook {
        let mut book = MemoryBook::default();
        for (title, detail, file, at) in entries {
            let files: Vec<String> = if file.is_empty() {
                vec![]
            } else {
                vec![(*file).to_owned()]
            };
            book.remember("/w/p", "a", "note", title, detail, &files, Timestamp(*at))
                .unwrap();
        }
        book
    }

    #[test]
    fn la_busqueda_ignora_tildes_y_mayusculas() {
        let book = book_with(&[("Aprobación manual", "Detalle", "", 1)]);
        assert_eq!(book.search("/w/p", "aprobacion", 5).len(), 1);
        assert_eq!(book.search("/w/p", "APROBACIÓN", 5).len(), 1);
        let book = book_with(&[("Diseno del nino", "x", "", 1)]);
        assert_eq!(book.search("/w/p", "diseño niño", 5).len(), 1);
    }

    #[test]
    fn singular_y_plural_se_encuentran_entre_si() {
        let book = book_with(&[("Aprobaciones de permisos", "x", "", 1)]);
        assert_eq!(book.search("/w/p", "aprobacion", 5).len(), 1);
        let book = book_with(&[("Aprobacion de permiso", "x", "", 1)]);
        assert_eq!(book.search("/w/p", "aprobaciones", 5).len(), 1);
    }

    #[test]
    fn un_termino_corto_no_coincide_por_prefijo() {
        let book = book_with(&[("Mutex compartido", "x", "", 1)]);
        assert!(book.search("/w/p", "mut", 5).is_empty());
        assert_eq!(book.search("/w/p", "mutex", 5).len(), 1);
        // La palabra corta del texto tampoco es prefijo de un término largo.
        let book = book_with(&[("Uso de la API", "x", "", 1)]);
        assert!(book.search("/w/p", "descubrir", 5).is_empty());
    }

    #[test]
    fn el_orden_es_por_puntuacion_y_luego_por_fecha() {
        let book = book_with(&[
            ("Otra cosa", "habla de aprobaciones", "", 10),
            ("Aprobación", "x", "", 5),
            ("Sin relación", "aprobacion también", "", 20),
        ]);
        let titles: Vec<String> = book
            .search("/w/p", "aprobación", 5)
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(titles, ["Aprobación", "Sin relación", "Otra cosa"]);
    }
}
