//! Muestra qué procesos detecta Kiti como agentes y por qué (`mise run scan`).
//! Solo lee la lista de procesos del usuario; no envía señales.

use kiti_core::adapters::SysinfoScanner;
use kiti_core::domain::{classify, dedupe_tree};
use kiti_core::ports::ProcessScanner;

fn main() {
    let snapshots = SysinfoScanner::new().scan().expect("escaneo");
    let home = std::env::var("HOME").unwrap_or_default();
    let short = |s: &str| s.replacen(&home, "~", 1);
    println!("{} procesos del usuario", snapshots.len());
    for s in &snapshots {
        let exe = s.exe.as_deref().unwrap_or("-");
        let interesting = exe.contains("claude")
            || exe.contains("codex")
            || exe.contains("gemini")
            || s.argv
                .iter()
                .any(|a| a.contains("gemini") || a.contains("codex"));
        if interesting || classify(s).is_some() {
            println!(
                "  pid={} ppid={:?} class={:?}\n    exe={}\n    argv={:?}",
                s.pid.0,
                s.ppid.map(|p| p.0),
                classify(s),
                short(exe),
                s.argv.iter().map(|a| short(a)).collect::<Vec<_>>()
            );
        }
    }
    println!("agentes tras dedupe:");
    for (s, p) in dedupe_tree(&snapshots) {
        println!(
            "  {p} pid={} cwd={}",
            s.pid.0,
            short(s.cwd.as_deref().unwrap_or("?"))
        );
    }
}
