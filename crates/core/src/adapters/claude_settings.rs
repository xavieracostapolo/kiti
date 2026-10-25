//! Instala los hooks de Kiti en `~/.claude/settings.json` sin pisar los del usuario
//! (ADR 0005 §1, algoritmo portado de Pixel Agents).

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use tracing::info;

/// Eventos de Claude a los que se suscribe Kiti (ADR 0005 §5).
pub const EVENTS: [&str; 11] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "PermissionDenied",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Firma de lo propio: un hook http a 127.0.0.1 en la ruta de Kiti. Lo ajeno no se toca.
pub fn is_kiti_group(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks.iter().any(|h| {
            h["type"] == "http"
                && h["url"]
                    .as_str()
                    .is_some_and(|u| u.starts_with("http://127.0.0.1:") && u.contains("/v1/hooks/"))
        })
    })
}

fn kiti_group(url: &str, token: &str) -> Value {
    // timeout 2 s (el de Claude en http es 600 s); sin onFailure: por defecto "continue".
    json!({ "hooks": [ {
        "type": "http",
        "url": url,
        "timeout": 2,
        "headers": { "Authorization": format!("Bearer {token}") }
    } ] })
}

pub fn uninstall(settings: &mut Value) {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    for event in EVENTS {
        if let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) {
            groups.retain(|g| !is_kiti_group(g));
            if groups.is_empty() {
                hooks.shift_remove(event);
            }
        }
    }
    if hooks.is_empty()
        && let Some(obj) = settings.as_object_mut()
    {
        obj.shift_remove("hooks");
    }
}

pub fn install(settings: &mut Value, url: &str, token: &str) {
    uninstall(settings);
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let obj = settings.as_object_mut().expect("objeto");
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks.as_object_mut().expect("hooks es un objeto");
    for event in EVENTS {
        let groups = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(groups) = groups.as_array_mut() {
            groups.push(kiti_group(url, token));
        }
    }
}

pub fn is_installed(settings: &Value, url: &str) -> bool {
    EVENTS.iter().all(|event| {
        settings["hooks"][event].as_array().is_some_and(|groups| {
            groups
                .iter()
                .any(|g| is_kiti_group(g) && g["hooks"][0]["url"] == url)
        })
    })
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Lee, transforma y escribe de forma atómica: backup único (`O_EXCL`), archivo temporal en el
/// mismo directorio con los mismos permisos, `fsync` y `rename`. Si el original cambió mientras
/// tanto (Claude Code también lo escribe), reintenta. Si no es JSON válido, no escribe nada.
fn edit_file(path: &Path, change: impl Fn(&mut Value)) -> std::io::Result<()> {
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

pub fn install_file(path: &Path, url: &str, token: &str) -> std::io::Result<()> {
    edit_file(path, |v| install(v, url, token))?;
    info!(event = "hooks.installed", path = %path.display(), url);
    Ok(())
}

pub fn uninstall_file(path: &Path) -> std::io::Result<()> {
    edit_file(path, uninstall)?;
    info!(event = "hooks.uninstalled", path = %path.display());
    Ok(())
}

/// Lee el archivo y dice si los hooks de Kiti con esta URL ya están. `false` si no se puede leer.
pub fn is_installed_file(path: &Path, url: &str) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| is_installed(&v, url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URL: &str = "http://127.0.0.1:47821/v1/hooks/claude";

    fn user_settings() -> Value {
        // Forma real del usuario: claves propias y un hook PreToolUse ajeno.
        json!({
            "permissions": { "allow": ["Bash(ls:*)"] },
            "model": "opus",
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [ { "type": "command", "command": "/usr/local/bin/guard" } ] }
                ]
            },
            "theme": "dark"
        })
    }

    fn kiti_groups(v: &Value, event: &str) -> usize {
        v["hooks"][event]
            .as_array()
            .map_or(0, |a| a.iter().filter(|g| is_kiti_group(g)).count())
    }

    #[test]
    fn instala_un_grupo_por_evento_y_respeta_lo_ajeno() {
        let mut v = user_settings();
        install(&mut v, URL, "tok");
        for event in EVENTS {
            assert_eq!(kiti_groups(&v, event), 1, "{event}");
        }
        // El hook del usuario sigue primero e intacto.
        assert_eq!(
            v["hooks"]["PreToolUse"][0],
            user_settings()["hooks"]["PreToolUse"][0]
        );
        let hook = &v["hooks"]["Stop"][0]["hooks"][0];
        assert_eq!(hook["type"], "http");
        assert_eq!(hook["url"], URL);
        assert_eq!(hook["timeout"], 2);
        assert_eq!(hook["headers"]["Authorization"], "Bearer tok");
        assert!(hook.get("onFailure").is_none(), "nunca block");
        // Las claves de primer nivel conservan su orden.
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["permissions", "model", "hooks", "theme"]);
    }

    #[test]
    fn reinstalar_es_idempotente_y_actualiza_puerto_y_token() {
        let mut v = user_settings();
        install(&mut v, URL, "viejo");
        install(&mut v, "http://127.0.0.1:50000/v1/hooks/claude", "nuevo");
        assert_eq!(kiti_groups(&v, "Stop"), 1);
        assert_eq!(
            v["hooks"]["Stop"][0]["hooks"][0]["headers"]["Authorization"],
            "Bearer nuevo"
        );
        assert!(is_installed(&v, "http://127.0.0.1:50000/v1/hooks/claude"));
        assert!(!is_installed(&v, URL));
    }

    #[test]
    fn desinstalar_deja_el_archivo_como_estaba() {
        let mut v = user_settings();
        install(&mut v, URL, "tok");
        uninstall(&mut v);
        assert_eq!(v, user_settings());
        // Sin hooks previos, desinstalar quita también la clave "hooks" vacía.
        let mut bare = json!({ "model": "opus" });
        install(&mut bare, URL, "tok");
        uninstall(&mut bare);
        assert_eq!(bare, json!({ "model": "opus" }));
    }

    #[test]
    fn archivo_en_disco_con_backup_y_sin_tocar_si_es_invalido() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kiti-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let original = serde_json::to_string_pretty(&user_settings()).unwrap();
        std::fs::write(&path, &original).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        install_file(&path, URL, "tok").unwrap();
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(is_installed(&written, URL));
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.kiti.bak")).unwrap(),
            original
        );
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "conserva los permisos");

        // El backup es único: una segunda instalación no lo sobrescribe.
        install_file(&path, URL, "otro").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.kiti.bak")).unwrap(),
            original
        );

        uninstall_file(&path).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back, user_settings());

        // JSON inválido: error y el archivo queda byte a byte igual.
        std::fs::write(&path, "{ roto").unwrap();
        assert!(install_file(&path, URL, "tok").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ roto");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sin_settings_previo_crea_uno_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("kiti-settings-new-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("settings.json");
        install_file(&path, URL, "tok").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
