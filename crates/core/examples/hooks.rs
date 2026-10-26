//! Estado e instalación manual de los hooks de Kiti.
//!
//! `cargo run -p kiti-core --example hooks -- <orden>`:
//! - `status`: Claude (`~/.claude/settings.json`), Codex (`~/.codex/hooks.json`) y Gemini
//!   (`~/.gemini/settings.json`).
//! - `uninstall`: quita los de Claude (la app los vuelve a poner al arrancar en modo real).
//! - `install-codex` / `install-gemini <ruta de kiti-hook>`: copia el forwarder a
//!   `~/.kiti/bin/kiti-hook` (ruta estable: Codex confía en el hook por su hash) y añade los hooks.
//! - `uninstall-codex` / `uninstall-gemini`.
//! - `install-antigravity <ruta de kiti-hook>` / `uninstall-antigravity`: `~/.gemini/config/hooks.json`
//!   (ADR 0014).
//! - `install-statusline <ruta de kiti-hook>` / `uninstall-statusline`: la `statusLine` de Claude
//!   pasa por Kiti para leer la cuota (ADR 0015); desinstalar deja la original.
//!
//! Codex y Gemini no se instalan solos: solo con esta orden explícita del usuario.

use std::path::{Path, PathBuf};

use kiti_core::adapters::claude_settings::{self, EVENTS, is_kiti_group};
use kiti_core::adapters::cli_hooks::{self, Cli, antigravity};
use kiti_core::adapters::statusline;
use serde_json::Value;

fn read(path: &Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn stable_forwarder(home: &Path, source: Option<String>) -> PathBuf {
    let target = home.join(".kiti/bin/kiti-hook");
    let source = source.expect("falta la ruta de kiti-hook (target/release/kiti-hook)");
    std::fs::create_dir_all(target.parent().unwrap()).expect("crear ~/.kiti/bin");
    std::fs::copy(&source, &target).expect("copiar kiti-hook");
    target
}

fn main() {
    let home = PathBuf::from(std::env::var("HOME").expect("HOME"));
    let claude = home.join(".claude/settings.json");
    let codex = home.join(".codex/hooks.json");
    let gemini = home.join(".gemini/settings.json");
    let agy = home.join(".gemini/config/hooks.json");
    let forwarder = home.join(".kiti/bin/kiti-hook");
    let fwd = forwarder.to_string_lossy().into_owned();
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("uninstall") => {
            claude_settings::uninstall_file(&claude).expect("desinstalar Claude");
            println!("Hooks de Kiti quitados de {}", claude.display());
        }
        Some("install-codex") => {
            let fwd = stable_forwarder(&home, args.next());
            cli_hooks::install_file(Cli::Codex, &codex, &fwd.to_string_lossy()).expect("Codex");
            println!(
                "Hooks de Kiti en {}. Ábrelos en Codex con /hooks y confía en ellos.",
                codex.display()
            );
        }
        Some("install-gemini") => {
            let fwd = stable_forwarder(&home, args.next());
            cli_hooks::install_file(Cli::Gemini, &gemini, &fwd.to_string_lossy()).expect("Gemini");
            println!("Hooks de Kiti en {}", gemini.display());
        }
        Some("install-antigravity") => {
            let fwd = stable_forwarder(&home, args.next());
            antigravity::install_file(&agy, &fwd.to_string_lossy()).expect("Antigravity");
            println!("Hooks de Kiti en {}", agy.display());
        }
        Some("install-statusline") => {
            let fwd = stable_forwarder(&home, args.next());
            statusline::install_file(&claude, &fwd.to_string_lossy()).expect("status line");
            println!(
                "La status line de Claude pasa por Kiti ({})",
                claude.display()
            );
        }
        Some("uninstall-statusline") => {
            statusline::uninstall_file(&claude).expect("status line");
            println!("Status line de Claude restaurada ({})", claude.display());
        }
        Some("uninstall-antigravity") => antigravity::uninstall_file(&agy).expect("Antigravity"),
        Some("uninstall-codex") => cli_hooks::uninstall_file(Cli::Codex, &codex).expect("Codex"),
        Some("uninstall-gemini") => {
            cli_hooks::uninstall_file(Cli::Gemini, &gemini).expect("Gemini")
        }
        _ => {
            let value = read(&claude);
            let installed = EVENTS
                .iter()
                .filter(|e| {
                    value["hooks"][**e]
                        .as_array()
                        .is_some_and(|g| g.iter().any(is_kiti_group))
                })
                .count();
            println!(
                "Claude: hooks de Kiti en {installed} de {} eventos",
                EVENTS.len()
            );
            for (name, cli, path) in [
                ("Codex", Cli::Codex, &codex),
                ("Gemini", Cli::Gemini, &gemini),
            ] {
                let state = if cli_hooks::is_installed(cli, &read(path), &fwd) {
                    "instalados"
                } else {
                    "no instalados"
                };
                println!("{name}: {state} ({})", path.display());
            }
            let state = if antigravity::is_installed(&read(&agy), &fwd) {
                "instalados"
            } else {
                "no instalados"
            };
            println!("Antigravity: {state} ({})", agy.display());
            let state = if statusline::is_installed(&read(&claude)) {
                "pasa por Kiti"
            } else {
                "sin Kiti"
            };
            println!("Status line de Claude (cuota): {state}");
        }
    }
}
