//! `statusLine` de Claude Code (ADR 0015): Kiti se pone delante del comando del usuario para
//! leer la cuota, sin cambiar lo que se ve.
//!
//! `bash ~/.claude/statusline.sh` pasa a ser `<forwarder> statusline 'bash ~/.claude/statusline.sh'`.
//! El comando original va entre comillas simples dentro del nuevo, así que desinstalar lo
//! recupera exacto. Sin status line previa, queda `<forwarder> statusline` y al desinstalar se
//! quita la clave.

use std::path::Path;

use serde_json::{Value, json};
use tracing::info;

use super::hook_config::edit_file;

/// Comillas simples de shell: `it's` → `'it'\''s'`.
pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Inversa de `shell_quote`: solo acepta lo que produce `shell_quote`.
pub fn shell_unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    let parts: Vec<&str> = inner.split(r"'\''").collect();
    if parts.iter().any(|p| p.contains('\'')) {
        return None;
    }
    Some(parts.join("'"))
}

fn ours(forwarder: &str) -> String {
    format!("{forwarder} statusline")
}

fn is_ours(command: &str) -> bool {
    command.contains("/kiti-hook statusline")
}

/// El comando que había antes de Kiti, si lo hay.
fn original(command: &str) -> Option<String> {
    let rest = command.split_once(" statusline")?.1.trim();
    if rest.is_empty() {
        return None;
    }
    shell_unquote(rest)
}

pub fn install(settings: &mut Value, forwarder: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let current = settings["statusLine"]["command"]
        .as_str()
        .map(str::to_owned);
    // Reinstalar sobre lo propio conserva el original.
    let previous = match current.as_deref() {
        Some(c) if is_ours(c) => original(c),
        other => other.map(str::to_owned),
    };
    let command = match previous {
        Some(prev) => format!("{} {}", ours(forwarder), shell_quote(&prev)),
        None => ours(forwarder),
    };
    if settings["statusLine"].is_object() {
        settings["statusLine"]["command"] = json!(command);
        settings["statusLine"]["type"] = json!("command");
    } else {
        settings["statusLine"] = json!({ "type": "command", "command": command });
    }
}

pub fn uninstall(settings: &mut Value) {
    let Some(command) = settings["statusLine"]["command"]
        .as_str()
        .map(str::to_owned)
    else {
        return;
    };
    if !is_ours(&command) {
        return;
    }
    match original(&command) {
        Some(prev) => settings["statusLine"]["command"] = json!(prev),
        None => {
            if let Some(obj) = settings.as_object_mut() {
                obj.shift_remove("statusLine");
            }
        }
    }
}

pub fn is_installed(settings: &Value) -> bool {
    settings["statusLine"]["command"]
        .as_str()
        .is_some_and(is_ours)
}

pub fn install_file(path: &Path, forwarder: &str) -> std::io::Result<()> {
    edit_file(path, |v| install(v, forwarder))?;
    info!(event = "statusline.installed", path = %path.display());
    Ok(())
}

pub fn uninstall_file(path: &Path) -> std::io::Result<()> {
    edit_file(path, uninstall)?;
    info!(event = "statusline.uninstalled", path = %path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FWD: &str = "/Users/dev/.kiti/bin/kiti-hook";

    #[test]
    fn comillas_de_shell_ida_y_vuelta() {
        for text in [
            "bash ~/.claude/statusline.sh",
            "echo 'hola' \"x\" $HOME",
            "it's",
            "",
        ] {
            assert_eq!(shell_unquote(&shell_quote(text)).as_deref(), Some(text));
        }
        assert_eq!(shell_unquote("sin comillas"), None);
        assert_eq!(shell_unquote("'a'b'"), None);
    }

    #[test]
    fn envuelve_la_del_usuario_y_la_restaura_exacta() {
        let mut v = json!({
            "theme": "dark",
            "statusLine": { "type": "command", "command": "bash ~/.claude/statusline.sh", "padding": 0 }
        });
        install(&mut v, FWD);
        assert_eq!(
            v["statusLine"]["command"],
            format!("{FWD} statusline 'bash ~/.claude/statusline.sh'")
        );
        assert_eq!(v["statusLine"]["padding"], 0);
        assert!(is_installed(&v));
        // Reinstalar no anida.
        install(&mut v, FWD);
        assert_eq!(
            v["statusLine"]["command"],
            format!("{FWD} statusline 'bash ~/.claude/statusline.sh'")
        );
        uninstall(&mut v);
        assert_eq!(
            v,
            json!({
                "theme": "dark",
                "statusLine": { "type": "command", "command": "bash ~/.claude/statusline.sh", "padding": 0 }
            })
        );
        assert!(!is_installed(&v));
    }

    #[test]
    fn sin_status_line_previa_se_pone_y_se_quita_entera() {
        let mut v = json!({ "theme": "dark" });
        install(&mut v, FWD);
        assert_eq!(v["statusLine"]["command"], format!("{FWD} statusline"));
        uninstall(&mut v);
        assert_eq!(v, json!({ "theme": "dark" }));
        // Una status line ajena no se toca al desinstalar.
        let mut other = json!({ "statusLine": { "type": "command", "command": "otra" } });
        uninstall(&mut other);
        assert_eq!(other["statusLine"]["command"], "otra");
    }
}
