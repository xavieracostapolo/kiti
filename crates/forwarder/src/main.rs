//! `kiti-hook <claude|codex|gemini>`: reenvía el hook que el CLI escribe por stdin al HookServer
//! de Kiti (ADR 0005 §4, ADR 0006).
//!
//! Reglas: falla en abierto (siempre `exit 0`, sin nada por stderr), plazo total de 150 ms, no
//! clasifica (manda su `getppid()` en `X-Kiti-Ppid` y el core sube por los ancestros), envía los
//! bytes tal cual y, para Gemini, imprime `{}` porque su stdout debe ser solo JSON.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_PORT: u16 = 47821;
const MAX_BODY: u64 = 4 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_millis(150);

fn token_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KITI_TOKEN_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/com.kiti.desktop/hook-token"))
}

fn forward(provider: &str, body: &[u8]) -> Option<()> {
    let token = std::fs::read_to_string(token_path()?).ok()?;
    let port = std::env::var("KITI_HOOK_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(DEADLINE))
        .http_status_as_error(false)
        .build()
        .new_agent();
    agent
        .post(format!("http://127.0.0.1:{port}/v1/hooks/{provider}"))
        .header("Authorization", format!("Bearer {}", token.trim()))
        .header("Content-Type", "application/json")
        .header(
            "X-Kiti-Ppid",
            std::os::unix::process::parent_id().to_string(),
        )
        .send(body)
        .ok()
        .map(|_| ())
}

fn main() {
    let provider = std::env::args().nth(1).unwrap_or_default();
    let mut body = Vec::new();
    let _ = std::io::stdin().take(MAX_BODY).read_to_end(&mut body);
    if matches!(provider.as_str(), "claude" | "codex" | "gemini") {
        let _ = forward(&provider, &body);
    }
    if provider == "gemini" {
        let _ = std::io::stdout().write_all(b"{}");
    }
}
