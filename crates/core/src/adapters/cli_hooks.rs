//! Hooks de Kiti para Codex (`~/.codex/hooks.json`) y Gemini (`~/.gemini/settings.json`), que solo
//! admiten `type: "command"`: invocan `kiti-hook <cli>` (ADR 0005 §2 y §3).
//!
//! **No se instalan solos**: hacen falta el CLI instalado y el permiso del usuario (ADR 0006).

use std::path::Path;

use serde_json::{Value, json};
use tracing::info;

use super::hook_config::{add_groups, edit_file, remove_groups};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cli {
    Codex,
    Gemini,
}

impl Cli {
    pub fn events(self) -> &'static [&'static str] {
        match self {
            Cli::Codex => &[
                "SessionStart",
                "UserPromptSubmit",
                "PreToolUse",
                "PostToolUse",
                "PermissionRequest",
                "Stop",
                "Interrupt",
                "SessionEnd",
            ],
            // Sin BeforeModel, AfterModel ni BeforeToolSelection: llevan el prompt completo.
            Cli::Gemini => &[
                "SessionStart",
                "SessionEnd",
                "BeforeAgent",
                "AfterAgent",
                "BeforeTool",
                "AfterTool",
                "Notification",
            ],
        }
    }

    fn arg(self) -> &'static str {
        match self {
            Cli::Codex => "codex",
            Cli::Gemini => "gemini",
        }
    }
}

/// Firma de lo propio: `name: "kiti"` (Gemini) o un `command` que llama a `kiti-hook`.
fn is_ours(group: &Value) -> bool {
    group["hooks"].as_array().is_some_and(|hooks| {
        hooks.iter().any(|h| {
            h["name"] == "kiti"
                || h["command"]
                    .as_str()
                    .is_some_and(|c| c.contains("/kiti-hook "))
        })
    })
}

fn group(cli: Cli, forwarder: &str, event: &str) -> Value {
    let command = format!("{forwarder} {}", cli.arg());
    match cli {
        // Codex: timeout en segundos. El forwarder ya corta a los 150 ms.
        Cli::Codex => {
            json!({ "hooks": [ { "type": "command", "command": command, "timeout": 1 } ] })
        }
        // Gemini: timeout en ms; los eventos de herramienta llevan matcher "*".
        Cli::Gemini => {
            let hook =
                json!({ "name": "kiti", "type": "command", "command": command, "timeout": 1000 });
            if event.ends_with("Tool") {
                json!({ "matcher": "*", "hooks": [hook] })
            } else {
                json!({ "hooks": [hook] })
            }
        }
    }
}

pub fn uninstall(cli: Cli, settings: &mut Value) {
    remove_groups(settings, cli.events(), is_ours);
}

/// `forwarder` es la ruta absoluta y estable de `kiti-hook`: Codex confía en el hook por el hash
/// de su definición, así que no debe cambiar entre versiones.
pub fn install(cli: Cli, settings: &mut Value, forwarder: &str) {
    uninstall(cli, settings);
    add_groups(settings, cli.events(), |event| group(cli, forwarder, event));
}

pub fn is_installed(cli: Cli, settings: &Value, forwarder: &str) -> bool {
    let command = format!("{forwarder} {}", cli.arg());
    cli.events().iter().all(|event| {
        settings["hooks"][*event].as_array().is_some_and(|groups| {
            groups
                .iter()
                .any(|g| is_ours(g) && g["hooks"][0]["command"] == command.as_str())
        })
    })
}

pub fn install_file(cli: Cli, path: &Path, forwarder: &str) -> std::io::Result<()> {
    edit_file(path, |v| install(cli, v, forwarder))?;
    info!(event = "hooks.installed", cli = cli.arg(), path = %path.display());
    Ok(())
}

pub fn uninstall_file(cli: Cli, path: &Path) -> std::io::Result<()> {
    edit_file(path, |v| uninstall(cli, v))?;
    info!(event = "hooks.uninstalled", cli = cli.arg(), path = %path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FWD: &str = "/Users/dev/.kiti/bin/kiti-hook";

    fn gemini_user() -> Value {
        json!({
            "theme": "GitHub",
            "hooks": {
                "BeforeTool": [
                    { "matcher": "write_file", "hooks": [ { "name": "lint", "type": "command", "command": "eslint" } ] }
                ]
            },
            "hooksConfig": { "enabled": true }
        })
    }

    #[test]
    fn gemini_respeta_lo_ajeno_y_usa_name_kiti_y_matcher() {
        let mut v = gemini_user();
        install(Cli::Gemini, &mut v, FWD);
        assert_eq!(
            v["hooks"]["BeforeTool"][0],
            gemini_user()["hooks"]["BeforeTool"][0]
        );
        let ours = &v["hooks"]["BeforeTool"][1];
        assert_eq!(ours["matcher"], "*");
        assert_eq!(ours["hooks"][0]["name"], "kiti");
        assert_eq!(ours["hooks"][0]["command"], format!("{FWD} gemini"));
        assert!(v["hooks"]["SessionStart"][0].get("matcher").is_none());
        for model_event in ["BeforeModel", "AfterModel", "BeforeToolSelection"] {
            assert!(v["hooks"].get(model_event).is_none(), "{model_event}");
        }
        assert!(is_installed(Cli::Gemini, &v, FWD));
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["theme", "hooks", "hooksConfig"]);
        uninstall(Cli::Gemini, &mut v);
        assert_eq!(v, gemini_user());
    }

    #[test]
    fn codex_instala_en_un_hooks_json_vacio_y_se_desinstala_limpio() {
        let mut v = json!({});
        install(Cli::Codex, &mut v, FWD);
        assert_eq!(
            v["hooks"]["Interrupt"][0]["hooks"][0]["command"],
            format!("{FWD} codex")
        );
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"], 1);
        assert!(is_installed(Cli::Codex, &v, FWD));
        assert!(!is_installed(Cli::Codex, &v, "/otra/ruta/kiti-hook"));
        install(Cli::Codex, &mut v, FWD);
        assert_eq!(
            v["hooks"]["Stop"].as_array().unwrap().len(),
            1,
            "idempotente"
        );
        uninstall(Cli::Codex, &mut v);
        assert_eq!(v, json!({}));
    }

    #[test]
    fn archivo_de_codex_en_disco() {
        let dir = std::env::temp_dir().join(format!("kiti-codex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("hooks.json");
        install_file(Cli::Codex, &path, FWD).unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(is_installed(Cli::Codex, &v, FWD));
        uninstall_file(Cli::Codex, &path).unwrap();
        let v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v, json!({}));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
