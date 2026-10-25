//! Sesiones persistentes con tmux (ADR 0008): los agentes lanzados desde Kiti viven en un
//! servidor tmux con socket propio (`-L kiti`) y Kiti solo se conecta como cliente.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::pty::LaunchSpec;

/// Prefijo de las sesiones de Kiti: lo demás del socket (si lo hubiera) no se toca.
const PREFIX: &str = "kiti-";

/// Configuración de tmux para Kiti: sin barra de estado (la cabecera ya la pone Kiti), mucho
/// historial, sin retardo en Esc y rueda del ratón para desplazarse.
pub const KITI_TMUX_CONF: &str = "\
set -g status off
set -g history-limit 50000
set -g escape-time 0
set -g mouse on
set -g default-terminal \"xterm-256color\"
set -g remain-on-exit off
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSession {
    pub name: String,
    /// PID del proceso de la sesión: con `exec`, el del agente.
    pub pid: u32,
    pub cwd: String,
    /// Opción `@kiti_provider` de la sesión.
    pub provider: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Tmux {
    bin: PathBuf,
    socket: String,
    conf: Option<PathBuf>,
}

/// Una app abierta desde el Finder no tiene `/opt/homebrew/bin` en el `PATH`.
fn find_tmux() -> Option<PathBuf> {
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("tmux"));
    let known = [
        "/opt/homebrew/bin/tmux",
        "/usr/local/bin/tmux",
        "/usr/bin/tmux",
    ]
    .into_iter()
    .map(PathBuf::from);
    from_path.chain(known).find(|p| p.is_file())
}

impl Tmux {
    /// `None` si tmux no está instalado. Sin `conf` se usa `/dev/null`: nunca el
    /// `~/.tmux.conf` del usuario.
    pub fn detect(socket: &str, conf: Option<PathBuf>) -> Option<Self> {
        let bin = find_tmux()?;
        let ok = Command::new(&bin)
            .arg("-V")
            .output()
            .is_ok_and(|o| o.status.success());
        ok.then(|| Self {
            bin,
            socket: socket.to_owned(),
            conf,
        })
    }

    fn base_args(&self) -> Vec<String> {
        let conf = self.conf.as_ref().map_or_else(
            || "/dev/null".to_owned(),
            |c| c.to_string_lossy().into_owned(),
        );
        vec!["-L".into(), self.socket.clone(), "-f".into(), conf]
    }

    fn run(&self, args: &[&str]) -> std::io::Result<Output> {
        Command::new(&self.bin)
            .args(self.base_args())
            .args(args)
            .output()
    }

    fn checked(&self, args: &[&str]) -> std::io::Result<String> {
        let out = self.run(args)?;
        if !out.status.success() {
            return Err(std::io::Error::other(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Sesiones de Kiti vivas. Sin servidor (nada lanzado aún), lista vacía.
    pub fn list(&self) -> Vec<TmuxSession> {
        let Ok(out) = self.checked(&[
            "list-panes",
            "-a",
            "-F",
            "#{session_name}\t#{pane_pid}\t#{session_path}\t#{@kiti_provider}",
        ]) else {
            return Vec::new();
        };
        out.lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let (name, pid, cwd, provider) = (f.next()?, f.next()?, f.next()?, f.next());
                name.starts_with(PREFIX).then(|| TmuxSession {
                    name: name.to_owned(),
                    pid: pid.parse().unwrap_or(0),
                    cwd: cwd.to_owned(),
                    provider: provider.filter(|p| !p.is_empty()).map(str::to_owned),
                })
            })
            .collect()
    }

    fn next_name(&self) -> String {
        let used: Vec<u32> = self
            .list()
            .iter()
            .filter_map(|s| s.name.strip_prefix(PREFIX)?.parse().ok())
            .collect();
        let n = (1..).find(|n| !used.contains(n)).unwrap_or(1);
        format!("{PREFIX}{n}")
    }

    /// Lanza `command` (lo ejecuta la shell por defecto de tmux) en una sesión nueva.
    pub fn new_session(
        &self,
        cwd: &Path,
        command: &str,
        provider: &str,
        cols: u16,
        rows: u16,
    ) -> std::io::Result<TmuxSession> {
        let name = self.next_name();
        let (cols, rows) = (cols.to_string(), rows.to_string());
        let cwd_str = cwd.to_string_lossy();
        self.checked(&[
            "new-session",
            "-d",
            "-s",
            &name,
            "-c",
            &cwd_str,
            "-x",
            &cols,
            "-y",
            &rows,
            command,
        ])?;
        // Si el proceso ya terminó (comando muy corto), estas dos órdenes fallan: no es un error.
        let _ = self.run(&["set-option", "-t", &name, "@kiti_provider", provider]);
        let pid = self
            .checked(&["display-message", "-p", "-t", &name, "#{pane_pid}"])
            .ok()
            .and_then(|o| o.trim().parse().ok())
            .unwrap_or(0);
        Ok(TmuxSession {
            name,
            pid,
            cwd: cwd_str.into_owned(),
            provider: Some(provider.to_owned()),
        })
    }

    /// Cliente de tmux para ver la sesión dentro de un PTY de Kiti.
    pub fn attach_spec(&self, name: &str, cols: u16, rows: u16) -> LaunchSpec {
        let mut args = self.base_args();
        args.extend(["attach-session".into(), "-t".into(), name.to_owned()]);
        LaunchSpec {
            program: self.bin.clone(),
            args,
            cwd: std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from),
            cols,
            rows,
        }
    }

    /// Orden para entrar en la sesión desde otra terminal.
    pub fn attach_hint(&self, name: &str) -> String {
        format!("tmux -L {} attach -t {name}", self.socket)
    }

    pub fn kill_server(&self) {
        let _ = self.run(&["kill-server"]);
    }
}
