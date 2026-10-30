//! Lanzador de agentes efímeros (ADR 0019): sesión tmux de Kiti, archivo del encargo, archivo
//! de la conversación y worktree. Es bloqueante: el runtime lo llama fuera del actor.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use kiti_core::adapters::conversations::HiddenConversations;
use kiti_core::adapters::tmux::Tmux;
use kiti_core::domain::{Pid, Provider, Task, worktree_main_repo};
use kiti_core::ingestion::claude::parse_session_file;
use kiti_core::ports::{CloseOutcome, LaunchSpec, Launched, TaskLauncher};
use tracing::{info, warn};

/// Worktrees de las tareas. Stub hasta que el encargo C esté en `main`: entonces este módulo se
/// sustituye por `kiti_core::adapters::worktree`, que tiene las mismas firmas.
mod worktree {
    use std::path::{Path, PathBuf};

    pub struct Worktree {
        pub path: PathBuf,
        // Se lee cuando entre la implementación real.
        #[allow(dead_code)]
        pub branch: String,
    }

    pub enum Removal {
        // Lo devuelve la implementación real; el stub nunca borra.
        #[allow(dead_code)]
        Removed,
        Kept(String),
    }

    pub fn create(_repo: &Path, _branch: &str) -> std::io::Result<Worktree> {
        Err(std::io::Error::other(
            "los worktrees de las tareas aún no están conectados",
        ))
    }

    pub fn remove_if_safe(_repo: &Path, _worktree: &Path) -> Removal {
        Removal::Kept("los worktrees de las tareas aún no están conectados".into())
    }
}

pub struct TmuxTaskLauncher {
    tmux: Option<Tmux>,
    /// `~/.kiti/tasks`: un `.md` por tarea, con el encargo.
    tasks_dir: PathBuf,
    /// `~/.claude/sessions`: pid ↔ sesión de Claude, para archivar su conversación.
    claude_sessions: Option<PathBuf>,
    /// Compartido con la lista de conversaciones cerradas (ADR 0009).
    hidden: Arc<Mutex<HiddenConversations>>,
    shell: String,
}

impl TmuxTaskLauncher {
    pub fn new(
        tmux: Option<Tmux>,
        home: &Path,
        hidden: Arc<Mutex<HiddenConversations>>,
        shell: String,
    ) -> Self {
        Self {
            tmux,
            tasks_dir: home.join(".kiti/tasks"),
            claude_sessions: Some(home.join(".claude/sessions")),
            hidden,
            shell,
        }
    }
}

/// Línea que ejecuta la shell en la sesión tmux. El modelo ya viene validado (letras, números,
/// `.`, `-` y `_`), así que cabe entre comillas simples.
pub fn agent_command(provider: Provider, model: &str) -> Option<String> {
    match provider {
        Provider::Claude => Some(format!(
            "exec claude --model {model} --permission-mode auto"
        )),
        Provider::Antigravity => Some("exec agy".to_owned()),
        Provider::Codex => Some("exec codex".to_owned()),
        Provider::Gemini => None,
    }
}

fn binary(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Antigravity => "agy",
        Provider::Codex => "codex",
        Provider::Gemini => "gemini",
    }
}

/// Guarda el encargo con permisos `0600` (ADR 0019): solo lo lee el usuario.
pub fn write_brief(dir: &Path, task: u64, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    let path = dir.join(format!("{task}.md"));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)?;
    // Si el archivo ya existía con otros permisos, `mode` no los cambia.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())?;
    Ok(path)
}

/// `sessionId` de la sesión de Claude que corre con ese pid.
pub fn claude_session_id(sessions_dir: &Path, pid: Pid) -> Option<String> {
    let body = std::fs::read(sessions_dir.join(format!("{}.json", pid.0))).ok()?;
    let file = parse_session_file(&body)?;
    (file.pid == pid).then(|| file.session.as_str().to_owned())
}

impl TmuxTaskLauncher {
    fn cli_exists(&self, bin: &str) -> bool {
        std::process::Command::new(&self.shell)
            .args(["-lic", &format!("command -v {bin}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

impl TaskLauncher for TmuxTaskLauncher {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, String> {
        let tmux = self
            .tmux
            .as_ref()
            .ok_or("tmux no está instalado: los agentes de tareas viven en sesiones de tmux")?;
        let command = agent_command(spec.provider, &spec.model)
            .ok_or("ese proveedor no se puede lanzar como tarea")?;
        let bin = binary(spec.provider);
        if !self.cli_exists(bin) {
            return Err(format!("no se encontró `{bin}` en tu PATH"));
        }
        let brief_path = write_brief(&self.tasks_dir, spec.task.0, &spec.brief)
            .map_err(|e| format!("no se pudo guardar el encargo: {e}"))?;
        let created = match worktree::create(&spec.repo, &spec.branch) {
            Ok(w) => w,
            Err(e) => {
                let _ = std::fs::remove_file(&brief_path);
                return Err(format!("no se pudo crear el worktree: {e}"));
            }
        };
        let line = format!("{} -lic '{command}'", self.shell);
        let session = match tmux.new_session(&created.path, &line, bin, 100, 30) {
            Ok(s) => s,
            Err(e) => {
                let _ = std::fs::remove_file(&brief_path);
                // El worktree acaba de nacer vacío: se limpia con la misma comprobación de siempre.
                let _ = worktree::remove_if_safe(&spec.repo, &created.path);
                return Err(format!("no se pudo abrir la sesión de tmux: {e}"));
            }
        };
        info!(
            event = "task.launch",
            task_id = spec.task.0,
            provider = bin,
            session = %session.name,
            pid = session.pid
        );
        Ok(Launched {
            worktree: created.path,
            session: session.name,
            pid: Pid(session.pid),
            brief_path,
        })
    }

    fn close(&self, task: &Task) -> CloseOutcome {
        let mut out = CloseOutcome::default();
        // Antes de matar la sesión: Claude borra su archivo de sesión al salir.
        let session_id = match (task.provider, task.pid, &self.claude_sessions) {
            (Provider::Claude, Some(pid), Some(dir)) => claude_session_id(dir, pid),
            _ => None,
        };
        if let (Some(tmux), Some(name)) = (&self.tmux, &task.session) {
            out.session_killed = tmux.kill_session(name);
        }
        if let Some(id) = session_id {
            match self.hidden.lock().expect("lock").hide(&id) {
                Ok(()) => out.conversation_archived = true,
                Err(e) => warn!(event = "task.archive_failed", task_id = task.id.0, error = %e),
            }
        }
        if let Some(wt) = &task.worktree {
            let repo = std::fs::read_to_string(wt.join(".git"))
                .ok()
                .and_then(|f| worktree_main_repo(&f));
            match repo {
                Some(repo) => match worktree::remove_if_safe(Path::new(&repo), wt) {
                    worktree::Removal::Removed => out.worktree_removed = true,
                    worktree::Removal::Kept(reason) => out.kept_reason = Some(reason),
                },
                None => out.kept_reason = Some("no se encontró el repositorio del worktree".into()),
            }
        }
        if let Some(brief) = &task.brief_path {
            let _ = std::fs::remove_file(brief);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kiti-tasks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn comando_por_proveedor() {
        assert_eq!(
            agent_command(Provider::Claude, "sonnet").as_deref(),
            Some("exec claude --model sonnet --permission-mode auto")
        );
        assert_eq!(
            agent_command(Provider::Antigravity, "default").as_deref(),
            Some("exec agy")
        );
        assert_eq!(
            agent_command(Provider::Codex, "default").as_deref(),
            Some("exec codex")
        );
        assert_eq!(agent_command(Provider::Gemini, "x"), None);
    }

    #[test]
    fn el_encargo_se_guarda_con_permisos_0600() {
        let dir = temp("brief");
        let path = write_brief(&dir, 7, "Haz X").unwrap();
        assert_eq!(path, dir.join("7.md"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Haz X");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // Reescribirlo no abre los permisos.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_brief(&dir, 7, "Haz Y").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn la_sesion_de_claude_se_lee_por_pid() {
        let dir = temp("sessions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("4242.json"),
            br#"{"pid":4242,"sessionId":"0b1f6d7e-5c1a-4a52-9d57-2d1f6f0b8a11","status":"idle"}"#,
        )
        .unwrap();
        assert_eq!(
            claude_session_id(&dir, Pid(4242)).as_deref(),
            Some("0b1f6d7e-5c1a-4a52-9d57-2d1f6f0b8a11")
        );
        assert_eq!(claude_session_id(&dir, Pid(1)), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Lanza un `claude` de verdad en una sesión tmux y la cierra: gasta cuota y necesita los
    /// worktrees reales (encargo C). Solo a mano y con el permiso del usuario:
    /// `cargo test -p kiti-app -- --ignored launcher_real`.
    #[test]
    #[ignore = "lanza un CLI de agente con coste; solo a mano y con permiso del usuario"]
    fn launcher_real() {
        use kiti_core::domain::{TaskId, TaskRole};
        let home = temp("real-home");
        let repo = temp("real-repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["commit", "-q", "--allow-empty", "-m", "init"],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(&args)
                    .current_dir(&repo)
                    // lefthook exporta GIT_DIR y otras: nunca deben llegar al repo real.
                    .env_remove("GIT_DIR")
                    .env_remove("GIT_WORK_TREE")
                    .env_remove("GIT_INDEX_FILE")
                    .env_remove("GIT_COMMON_DIR")
                    .env_remove("GIT_PREFIX")
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let hidden = Arc::new(Mutex::new(HiddenConversations::load(
            home.join("hidden.json"),
        )));
        let tmux = Tmux::detect("kiti-test-launcher", None);
        let launcher = TmuxTaskLauncher::new(tmux, &home, hidden, std::env::var("SHELL").unwrap());
        let launched = launcher
            .launch(LaunchSpec {
                task: TaskId(1),
                repo,
                branch: "chore/launcher-real".into(),
                provider: Provider::Claude,
                model: "haiku".into(),
                role: TaskRole::Implementar,
                brief: "No hagas nada.".into(),
            })
            .unwrap();
        assert!(launched.brief_path.is_file());
        assert!(launched.session.starts_with("kiti-"));
    }
}
