//! Antigravity CLI en disco (ADR 0014): `~/.gemini/antigravity-cli/`.
//!
//! - `conversation_summaries.db`: título, carpeta, fecha y estado de cada conversación.
//! - `conversations/<id>.db`: los pasos; uno con `status = 9` espera el permiso del usuario.
//! - `brain/<id>/.system_generated/logs/transcript_full.jsonl`: de ahí sale el último pedido.
//!
//! Se lee con `/usr/bin/sqlite3` (viene con macOS) sin escribir nunca: en solo lectura si la base
//! tiene `-wal` (agy abierto) y como `immutable` si no (no hay cambios pendientes que perder).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::tail::Tail;
use crate::domain::SessionId;

/// Estado de un paso que espera la aprobación del usuario (observado en la 1.4.7).
const STEP_WAITING: &str = "9";
const RUNNING: &str = "CASCADE_RUN_STATUS_RUNNING";
/// Separadores del modo `-ascii` de sqlite3: no aparecen en títulos ni rutas.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

/// Filas de la salida `-ascii` de sqlite3.
pub fn parse_ascii(out: &str) -> Vec<Vec<String>> {
    out.split(RECORD)
        .filter(|r| !r.trim().is_empty())
        .map(|r| {
            r.split(FIELD)
                .map(|f| f.trim_matches('\n').to_owned())
                .collect()
        })
        .collect()
}

/// Primera carpeta de `workspace_uris` (`["file:///Users/x/My%20Proj"]`) como ruta.
pub fn workspace_path(uris: &str) -> Option<String> {
    let list: Vec<String> = serde_json::from_str(uris).ok()?;
    let uri = list.into_iter().next()?;
    let path = uri.strip_prefix("file://")?;
    percent_decode(path)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Último pedido del usuario en las líneas de un transcript: el `USER_INPUT` más reciente, sin
/// las etiquetas que añade agy.
pub fn last_user_request(lines: &[Vec<u8>]) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Step {
        #[serde(rename = "type")]
        kind: Option<String>,
        content: Option<String>,
    }
    lines.iter().rev().find_map(|line| {
        if !line.windows(12).any(|w| w == b"\"USER_INPUT\"") {
            return None;
        }
        let step: Step = serde_json::from_slice(line).ok()?;
        if step.kind.as_deref() != Some("USER_INPUT") {
            return None;
        }
        let content = step.content?;
        let start = content
            .find("<USER_REQUEST>")
            .map_or(0, |i| i + "<USER_REQUEST>".len());
        let end = content.find("</USER_REQUEST>").unwrap_or(content.len());
        let text = content.get(start..end)?.trim();
        (!text.is_empty()).then(|| text.to_owned())
    })
}

/// Una conversación reciente, tal como la guarda agy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgyConversation {
    pub session: SessionId,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub running: bool,
    /// Última modificación, en ms desde epoch.
    pub updated_at: u64,
}

/// Lo que se sabe de una conversación en curso en cada escaneo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgyLive {
    pub session: SessionId,
    pub title: Option<String>,
    /// Algún paso espera el permiso del usuario.
    pub waiting: bool,
}

pub struct AntigravityStore {
    root: PathBuf,
}

impl AntigravityStore {
    /// `root`: `~/.gemini/antigravity-cli`.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn query(&self, db: &Path, sql: &str) -> Option<Vec<Vec<String>>> {
        if !db.is_file() {
            return None;
        }
        let wal = PathBuf::from(format!("{}-wal", db.display()));
        let target = if wal.exists() {
            db.display().to_string()
        } else {
            format!("file:{}?immutable=1", db.display())
        };
        let mut cmd = Command::new("/usr/bin/sqlite3");
        if wal.exists() {
            cmd.arg("-readonly");
        }
        let out = cmd.args(["-ascii", &target, sql]).output().ok()?;
        out.status
            .success()
            .then(|| parse_ascii(&String::from_utf8_lossy(&out.stdout)))
    }

    /// Conversaciones modificadas en los últimos `days` días, de la más reciente a la más vieja.
    pub fn conversations(&self, days: u32) -> Vec<AgyConversation> {
        let sql = format!(
            "select conversation_id, title, workspace_uris, status, \
             cast((julianday(last_modified_time) - 2440587.5) * 86400000 as integer) \
             from conversation_summaries \
             where last_modified_time > datetime('now', '-{days} days') \
             order by last_modified_time desc"
        );
        let db = self.root.join("conversation_summaries.db");
        self.query(&db, &sql)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| {
                let [id, title, uris, status, updated] = row.as_slice() else {
                    return None;
                };
                crate::ingestion::claude::is_session_id(id).then(|| AgyConversation {
                    session: SessionId::new(id.clone()),
                    title: Some(title.clone()).filter(|t| !t.is_empty()),
                    cwd: workspace_path(uris),
                    running: status == RUNNING,
                    updated_at: updated.parse().unwrap_or(0),
                })
            })
            .collect()
    }

    /// Título y espera de permiso de las conversaciones recientes. Solo se miran los pasos de
    /// las que están en curso.
    pub fn live(&self) -> Vec<AgyLive> {
        self.conversations(2)
            .into_iter()
            .map(|c| {
                let waiting = c.running && self.has_waiting_step(&c.session);
                AgyLive {
                    session: c.session,
                    title: c.title,
                    waiting,
                }
            })
            .collect()
    }

    fn has_waiting_step(&self, session: &SessionId) -> bool {
        let db = self
            .root
            .join("conversations")
            .join(format!("{}.db", session.as_str()));
        let sql = format!("select count(*) from steps where status = {STEP_WAITING}");
        self.query(&db, &sql)
            .and_then(|rows| rows.first()?.first()?.parse::<u32>().ok())
            .is_some_and(|n| n > 0)
    }

    /// Último pedido de una conversación, de los últimos 256 KiB de su transcript.
    pub fn last_prompt(&self, session: &SessionId) -> Option<String> {
        let path = self
            .root
            .join("brain")
            .join(session.as_str())
            .join(".system_generated/logs/transcript_full.jsonl");
        let lines = Tail::open_at_end(&path, 256 * 1024).ok()?.read_new().ok()?;
        last_user_request(&lines)
    }
}

/// Dónde está `agy`: una app abierta desde el Finder no tiene `/opt/homebrew/bin` en el `PATH`.
pub fn find_agy() -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("agy"));
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let known = ["/opt/homebrew/bin/agy", "/usr/local/bin/agy"]
        .into_iter()
        .map(PathBuf::from)
        .chain(home.map(|h| h.join(".local/bin/agy")));
    from_path.chain(known).find(|p| p.is_file())
}

/// `agy -p /usage --output-format json` (ADR 0015): no gasta tokens. Se corta a los `timeout`.
pub fn usage_json(agy: &Path, timeout: std::time::Duration) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut child = Command::new(agy)
        .args(["-p", "/usage", "--output-format", "json"])
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return None,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = Vec::new();
    child.stdout.take()?.read_to_end(&mut out).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_salida_ascii_se_lee_por_filas_y_campos() {
        let out = "c-1\u{1f}Migrar datos\u{1f}CASCADE_RUN_STATUS_IDLE\u{1e}c-2\u{1f}\u{1f}X\u{1e}";
        assert_eq!(
            parse_ascii(out),
            vec![
                vec![
                    "c-1".to_owned(),
                    "Migrar datos".into(),
                    "CASCADE_RUN_STATUS_IDLE".into()
                ],
                vec!["c-2".to_owned(), String::new(), "X".into()],
            ]
        );
        assert!(parse_ascii("").is_empty());
    }

    #[test]
    fn la_carpeta_sale_de_la_primera_uri() {
        assert_eq!(
            workspace_path(r#"["file:///Users/dev/My%20Proj","file:///otra"]"#).as_deref(),
            Some("/Users/dev/My Proj")
        );
        assert_eq!(workspace_path("[]"), None);
        assert_eq!(workspace_path("no json"), None);
        assert_eq!(workspace_path(r#"["https://x"]"#), None);
    }

    #[test]
    fn el_ultimo_pedido_sin_las_etiquetas_de_agy() {
        let lines: Vec<Vec<u8>> = [
            r#"{"step_index":0,"type":"USER_INPUT","content":"<USER_REQUEST>\nprimero\n</USER_REQUEST>\n<ADDITIONAL_METADATA>x</ADDITIONAL_METADATA>"}"#,
            r#"{"step_index":1,"type":"PLANNER_RESPONSE","content":"ok"}"#,
            r#"{"step_index":2,"type":"USER_INPUT","content":"<USER_REQUEST>\nmigra la tabla\n</USER_REQUEST>"}"#,
            r#"{"step_index":3,"type":"PLANNER_RESPONSE","content":"USER_INPUT en el texto no cuenta"}"#,
        ]
        .iter()
        .map(|l| l.as_bytes().to_vec())
        .collect();
        assert_eq!(last_user_request(&lines).as_deref(), Some("migra la tabla"));
        assert_eq!(last_user_request(&[]), None);
    }
}
