//! Estado, instalación y desinstalación de los hooks de Kiti en `~/.claude/settings.json`.
//!
//! `cargo run -p kiti-core --example hooks -- status|uninstall`
//! (instalar lo hace la app al arrancar, con su token).

use std::path::PathBuf;

use kiti_core::adapters::claude_settings::{EVENTS, is_kiti_group, uninstall_file};

fn main() {
    let settings =
        PathBuf::from(std::env::var("HOME").expect("HOME")).join(".claude/settings.json");
    match std::env::args().nth(1).as_deref() {
        Some("uninstall") => {
            uninstall_file(&settings).expect("desinstalar");
            println!("Hooks de Kiti quitados de {}", settings.display());
        }
        _ => {
            let value: serde_json::Value = std::fs::read(&settings)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let installed = EVENTS
                .iter()
                .filter(|e| {
                    value["hooks"][**e]
                        .as_array()
                        .is_some_and(|g| g.iter().any(is_kiti_group))
                })
                .count();
            println!(
                "{}: hooks de Kiti en {installed} de {} eventos",
                settings.display(),
                EVENTS.len()
            );
        }
    }
}
