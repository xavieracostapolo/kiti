//! Llevar al usuario a la terminal de un agente abierto fuera de Kiti (ADR 0012). Solo macOS.
//!
//! Se sube por los padres del agente hasta la primera `.app` (Ghostty, Terminal, VS Code…) y se
//! le pide que muestre esa pestaña: Ghostty por carpeta y título, Terminal.app por tty. Si no se
//! puede saber cuál, se trae la app al frente. Los datos van a `osascript` como argumentos, nunca
//! dentro del texto del script.

use std::path::Path;
use std::process::Command;

use crate::domain::Pid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKind {
    Ghostty,
    Terminal,
    Other,
}

/// App que aloja al agente: la primera `.app` entre sus padres.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostApp {
    /// Ruta del bundle, p. ej. `/Applications/Ghostty.app`.
    pub bundle: String,
    /// Nombre visible, p. ej. `Ghostty`.
    pub name: String,
    pub kind: HostKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusOutcome {
    /// Se mostró la pestaña exacta del agente.
    Focused(String),
    /// Se trajo la app al frente, pero no se supo qué pestaña.
    AppOnly(String),
    /// No está dentro de ninguna app conocida (p. ej. en un tmux ajeno o por ssh).
    Unknown,
}

/// La `.app` más externa del ejecutable: en VS Code el terminal cuelga de
/// `Visual Studio Code.app/Contents/Frameworks/Code Helper.app/...`, y lo que se activa es la
/// app principal.
pub fn bundle_of(exe: &str) -> Option<HostApp> {
    let end = exe.find(".app/")? + ".app".len();
    let bundle = &exe[..end];
    let name = Path::new(bundle)
        .file_stem()?
        .to_string_lossy()
        .into_owned();
    let kind = match name.as_str() {
        "Ghostty" => HostKind::Ghostty,
        "Terminal" => HostKind::Terminal,
        _ => HostKind::Other,
    };
    Some(HostApp {
        bundle: bundle.to_owned(),
        name,
        kind,
    })
}

/// Primer padre (sin contar el propio agente) que vive dentro de una `.app`. `exes`: el
/// ejecutable del agente y los de sus padres, en orden.
pub fn host_app(exes: &[String]) -> Option<HostApp> {
    exes.iter().skip(1).find_map(|exe| bundle_of(exe))
}

/// Una terminal de Ghostty tal como la describe su AppleScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhosttyTerm {
    pub id: String,
    pub name: String,
    pub cwd: String,
}

/// El título sin el indicador que Claude Code pone delante (`◑ `, `✳ `…).
fn bare_title(name: &str) -> &str {
    name.trim_start_matches(|c: char| !c.is_alphanumeric())
}

/// Elige la terminal del agente: por carpeta y, si hay varias, por título. Ante la duda,
/// ninguna (mejor traer la app al frente que enseñar la pestaña equivocada).
pub fn pick_ghostty<'a>(
    terms: &'a [GhosttyTerm],
    cwd: &str,
    title: Option<&str>,
) -> Option<&'a str> {
    let norm = |p: &str| p.trim_end_matches('/').to_owned();
    let titled =
        |t: &&GhosttyTerm| title.is_some_and(|title| bare_title(&t.name).starts_with(title));
    let same_dir: Vec<&GhosttyTerm> = terms.iter().filter(|t| norm(&t.cwd) == norm(cwd)).collect();
    let chosen: Vec<&GhosttyTerm> = match same_dir.len() {
        1 => same_dir,
        0 => terms.iter().filter(titled).collect(),
        _ => same_dir.into_iter().filter(titled).collect(),
    };
    match chosen.as_slice() {
        [only] => Some(only.id.as_str()),
        _ => None,
    }
}

/// Separador de campos en la salida del AppleScript (no aparece en títulos ni rutas).
const SEP: char = '\u{1f}';

pub fn parse_ghostty(out: &str) -> Vec<GhosttyTerm> {
    out.lines()
        .filter_map(|line| {
            let mut f = line.split(SEP);
            Some(GhosttyTerm {
                id: f.next()?.to_owned(),
                name: f.next()?.to_owned(),
                cwd: f.next()?.to_owned(),
            })
        })
        .collect()
}

fn osascript(script: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

const GHOSTTY_LIST: &str = r#"on run argv
  set sep to ASCII character 31
  set out to ""
  tell application "Ghostty"
    repeat with w in windows
      repeat with t in tabs of w
        repeat with s in terminals of t
          set out to out & (id of s) & sep & (name of s) & sep & (working directory of s) & linefeed
        end repeat
      end repeat
    end repeat
  end tell
  return out
end run"#;

const GHOSTTY_FOCUS: &str = r#"on run argv
  tell application "Ghostty"
    focus (first terminal whose id is (item 1 of argv))
    activate
  end tell
  return "ok"
end run"#;

const TERMINAL_FOCUS: &str = r#"on run argv
  tell application "Terminal"
    repeat with w in windows
      repeat with t in tabs of w
        if tty of t is (item 1 of argv) then
          set selected of t to true
          set index of w to 1
          activate
          return "ok"
        end if
      end repeat
    end repeat
  end tell
  return "no"
end run"#;

/// `/dev/ttys018` del agente, o `None` si no tiene terminal.
fn tty_of(pid: Pid) -> Option<String> {
    let out = Command::new("/bin/ps")
        .args(["-o", "tty=", "-p", &pid.0.to_string()])
        .output()
        .ok()?;
    let tty = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!tty.is_empty() && tty != "??").then(|| format!("/dev/{tty}"))
}

/// Ejecutables del agente y de sus padres, hasta `launchd` (como mucho 32 saltos). Con `ps`:
/// sysinfo no lee los procesos de root de la cadena, como `/usr/bin/login`.
fn ancestry(pid: Pid) -> Vec<String> {
    let mut exes = Vec::new();
    let mut next = pid.0;
    while next > 1 && exes.len() < 32 {
        let Ok(out) = Command::new("/bin/ps")
            .args(["-o", "ppid=,comm=", "-p", &next.to_string()])
            .output()
        else {
            break;
        };
        let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let Some((ppid, exe)) = line.split_once(' ') else {
            break;
        };
        exes.push(exe.trim().to_owned());
        next = ppid.trim().parse().unwrap_or(0);
    }
    exes
}

/// Lleva al usuario a la terminal del agente `pid`, que trabaja en `cwd` con `title`.
pub fn focus_agent(pid: Pid, cwd: &str, title: Option<&str>) -> FocusOutcome {
    let Some(host) = host_app(&ancestry(pid)) else {
        return FocusOutcome::Unknown;
    };
    let exact = match host.kind {
        HostKind::Ghostty => osascript(GHOSTTY_LIST, &[])
            .map(|out| parse_ghostty(&out))
            .and_then(|terms| pick_ghostty(&terms, cwd, title).map(str::to_owned))
            .and_then(|id| osascript(GHOSTTY_FOCUS, &[&id]))
            .is_some(),
        HostKind::Terminal => tty_of(pid)
            .and_then(|tty| osascript(TERMINAL_FOCUS, &[&tty]))
            .is_some_and(|r| r == "ok"),
        HostKind::Other => false,
    };
    if exact {
        return FocusOutcome::Focused(host.name);
    }
    // Al menos, la app al frente.
    let _ = Command::new("/usr/bin/open")
        .args(["-a", &host.bundle])
        .status();
    FocusOutcome::AppOnly(host.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_app_es_la_primera_app_entre_los_padres() {
        let chain = [
            "claude",
            "-/bin/zsh",
            "/usr/bin/login",
            "/Applications/Ghostty.app/Contents/MacOS/ghostty",
        ]
        .map(String::from);
        let host = host_app(&chain).unwrap();
        assert_eq!(host.name, "Ghostty");
        assert_eq!(host.kind, HostKind::Ghostty);
        assert_eq!(host.bundle, "/Applications/Ghostty.app");
        // Sin app entre los padres (tmux ajeno, ssh): no se sabe.
        assert_eq!(host_app(&chain[..3]), None);
    }

    #[test]
    fn en_vs_code_se_activa_la_app_principal() {
        let exe = "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper";
        let host = bundle_of(exe).unwrap();
        assert_eq!(host.bundle, "/Applications/Visual Studio Code.app");
        assert_eq!(host.name, "Visual Studio Code");
        assert_eq!(host.kind, HostKind::Other);
        assert_eq!(
            bundle_of("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal")
                .unwrap()
                .kind,
            HostKind::Terminal
        );
        assert_eq!(bundle_of("/bin/zsh"), None);
    }

    fn term(id: &str, name: &str, cwd: &str) -> GhosttyTerm {
        GhosttyTerm {
            id: id.into(),
            name: name.into(),
            cwd: cwd.into(),
        }
    }

    #[test]
    fn ghostty_por_carpeta_y_si_hay_varias_por_titulo() {
        let terms = [
            term("a", "◑ Oficina virtual 2D para agentes", "/w/kiti"),
            term("b", "…/source/thoth", "/w/thoth"),
            term("c", "✳ Migración de datos", "/w/thoth/"),
        ];
        // Una sola en la carpeta.
        assert_eq!(pick_ghostty(&terms, "/w/kiti", None), Some("a"));
        // Dos en la carpeta: decide el título.
        assert_eq!(
            pick_ghostty(&terms, "/w/thoth", Some("Migración de datos")),
            Some("c")
        );
        // Dos y sin título que las distinga: ninguna (mejor no equivocarse).
        assert_eq!(pick_ghostty(&terms, "/w/thoth", None), None);
        // Ninguna en la carpeta: por título en todas.
        assert_eq!(
            pick_ghostty(&terms, "/otra", Some("Oficina virtual")),
            Some("a")
        );
        assert_eq!(pick_ghostty(&terms, "/otra", Some("nada")), None);
    }

    #[test]
    fn la_salida_del_applescript_se_lee_por_campos() {
        let out = "id1\u{1f}◑ Kiti\u{1f}/w/kiti\nid2\u{1f}zsh\u{1f}/w\nroto";
        assert_eq!(
            parse_ghostty(out),
            vec![term("id1", "◑ Kiti", "/w/kiti"), term("id2", "zsh", "/w")]
        );
    }
}
