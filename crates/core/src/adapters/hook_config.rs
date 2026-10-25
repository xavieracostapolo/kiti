//! Operaciones comunes de los instaladores de hooks (Claude, Codex y Gemini): quitar y añadir
//! grupos propios sin tocar lo ajeno, y editar el archivo de forma atómica.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Quita los grupos propios de cada evento; si un evento o `hooks` quedan vacíos, los quita.
pub(crate) fn remove_groups(settings: &mut Value, events: &[&str], is_ours: fn(&Value) -> bool) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    for event in events {
        if let Some(groups) = hooks.get_mut(*event).and_then(Value::as_array_mut) {
            groups.retain(|g| !is_ours(g));
            if groups.is_empty() {
                hooks.shift_remove(*event);
            }
        }
    }
    if hooks.is_empty()
        && let Some(obj) = settings.as_object_mut()
    {
        obj.shift_remove("hooks");
    }
}

/// Añade al final de cada evento el grupo que devuelve `group_for(evento)`.
pub(crate) fn add_groups(settings: &mut Value, events: &[&str], group_for: impl Fn(&str) -> Value) {
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let obj = settings.as_object_mut().expect("objeto");
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(hooks) = hooks.as_object_mut() else {
        return;
    };
    for event in events {
        let groups = hooks
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(groups) = groups.as_array_mut() {
            groups.push(group_for(event));
        }
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Lee, transforma y escribe de forma atómica: backup único (`O_EXCL`), archivo temporal en el
/// mismo directorio con los mismos permisos, `fsync` y `rename`. Si el original cambió mientras
/// tanto (Claude Code también lo escribe), reintenta. Si no es JSON válido, no escribe nada.
pub(crate) fn edit_file(path: &Path, change: impl Fn(&mut Value)) -> std::io::Result<()> {
    for _ in 0..3 {
        let original = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let mut value: Value = match &original {
            Some(bytes) => serde_json::from_slice(bytes).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} no es JSON válido, no se toca: {e}", path.display()),
                )
            })?,
            None => json!({}),
        };
        change(&mut value);
        let mode = std::fs::metadata(path).map_or(0o600, |m| m.permissions().mode() & 0o777);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Some(bytes) = &original {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(sibling(path, ".kiti.bak"))
            {
                Ok(mut backup) => {
                    backup.write_all(bytes)?;
                    backup.sync_all()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        let tmp = sibling(path, ".kiti-tmp");
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(mode)
                .open(&tmp)?;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
            let mut text = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
            text.push('\n');
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
        }
        let now = std::fs::read(path).ok();
        if now == original {
            std::fs::rename(&tmp, path)?;
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
    }
    Err(std::io::Error::other(format!(
        "{} cambió mientras se editaba; se reintentará",
        path.display()
    )))
}
