//! Cada command registrado en `generate_handler!` tiene que estar en el manifiesto de
//! `build.rs` y permitido en la capability de la ventana: si no, Tauri lo rechaza en tiempo de
//! ejecución ("not allowed by ACL") y ningún otro test lo ve.

use std::path::Path;

fn read(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap()
}

/// Nombres entre comillas o identificadores del bloque que empieza en `start` y acaba en `end`.
fn names(text: &str, start: &str, end: &str) -> Vec<String> {
    let from = text.find(start).expect(start) + start.len();
    let block = &text[from..from + text[from..].find(end).expect(end)];
    block
        .split([',', '\n'])
        .map(|s| s.trim().trim_matches('"').to_owned())
        .filter(|s| !s.is_empty() && !s.starts_with("//"))
        .collect()
}

#[test]
fn todos_los_commands_estan_permitidos() {
    let handlers = names(&read("src/lib.rs"), "generate_handler![", "]");
    let manifest = names(&read("build.rs"), ".commands(&[", "]");
    let capability = read("capabilities/main.json");
    assert!(!handlers.is_empty());
    for cmd in &handlers {
        assert!(manifest.contains(cmd), "{cmd} falta en build.rs");
        let permission = format!("\"allow-{}\"", cmd.replace('_', "-"));
        assert!(
            capability.contains(&permission),
            "{permission} falta en la capability"
        );
    }
}
