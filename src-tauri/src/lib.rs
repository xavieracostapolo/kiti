//! Capa fina de Tauri v2 para Kiti (ADR 0004 §1–§2).
//!
//! Commands, Channel IPC, diálogo nativo de confirmación y observabilidad con tracing.
//! Toda la lógica vive en `kiti-core`; aquí solo se conecta con Tauri.

use std::time::Duration;

use kiti_core::adapters::antigravity::{AgyConversation, AntigravityStore};
use kiti_core::adapters::claude_settings::{install_file, is_installed_file};
use kiti_core::adapters::conversations::{ConversationIndex, HiddenConversations};
use kiti_core::adapters::hooks::{
    hook_router, load_or_create_token, mcp_router, run_hook_pump, serve_hooks,
};
use kiti_core::adapters::memory_sqlite::SqliteMemoryStore;
use kiti_core::adapters::pty::{LaunchSpec, PtyManager};
use kiti_core::adapters::tmux::{KITI_TMUX_CONF, Tmux};
use kiti_core::adapters::{GitProjects, SysinfoScanner, SystemClock, UnixKiller};
use kiti_core::application::{
    Runtime, RuntimeHandle, Telemetry, alert_text, quota_text, run_scanner_with,
};
use kiti_core::domain::{AgentId, Timestamp};
use kiti_core::ingestion::claude::{has_conversations, is_session_id};
use kiti_core::ports::{
    AgentKiller, AlertNotice, AlertSink, Clock, DiffPublisher, KillError, KillOutcome, KillTarget,
    MailCourier, ProjectResolver, SelfMonitor,
};
use kiti_core::sim::{SimFleet, SimSpec, TICK_MS};
use kiti_protocol::{
    ConversationInfo, Hello, IpcError, KillResult, MailCommand, MailMode, ServerMsg, Source,
    TerminalInfo, UiMetrics,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::ipc::Channel;
use tauri::ipc::InvokeResponseBody;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Cada cuánto se escanean los procesos en modo real. 3 s: con ~600 procesos cada escaneo
/// cuesta ~6 ms y a 2 s superaba el 1 % de CPU en reposo (ADR 0006). `KITI_SCAN_MS` lo cambia.
fn scan_interval() -> Duration {
    std::env::var("KITI_SCAN_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(Duration::from_secs(3), Duration::from_millis)
}

const DEFAULT_HOOK_PORT: u16 = 47821;

pub struct AppState {
    pub runtime: RuntimeHandle,
    /// Terminales de los agentes lanzados desde Kiti (ADR 0007). `None` en modo simulado.
    pub terminals: Option<Arc<Terminals>>,
    /// Conversaciones cerradas de Claude (ADR 0009). `None` en modo simulado o sin `$HOME`.
    pub conversations: Option<Arc<Conversations>>,
    /// Dónde se guarda el modo de los mensajes entre agentes (ADR 0017).
    pub mail_mode_file: Option<std::path::PathBuf>,
}

pub struct Conversations {
    /// `~/.claude/projects`.
    dir: std::path::PathBuf,
    index: Mutex<ConversationIndex>,
    projects: Mutex<GitProjects>,
    /// Las que el usuario archivó: no vuelven a salir.
    hidden: Mutex<HiddenConversations>,
    /// Antigravity CLI (ADR 0014): `~/.gemini/antigravity-cli`.
    agy: AntigravityStore,
    /// Las de Antigravity de la última lista (para retomar) y su último pedido por id y fecha.
    agy_last: Mutex<Vec<AgyConversation>>,
    agy_prompts: Mutex<HashMap<String, (u64, Option<String>)>>,
}

impl Conversations {
    /// Conversaciones cerradas de Antigravity: sin las que siguen en curso.
    fn antigravity(&self) -> Vec<AgyConversation> {
        let list: Vec<AgyConversation> = self
            .agy
            .conversations(CONVERSATIONS_DAYS as u32)
            .into_iter()
            .filter(|c| !c.running && c.cwd.is_some())
            .collect();
        *self.agy_last.lock().expect("lock") = list.clone();
        list
    }

    /// Último pedido, leído una vez por versión de la conversación.
    fn agy_prompt(&self, c: &AgyConversation) -> Option<String> {
        let mut cache = self.agy_prompts.lock().expect("lock");
        let key = c.session.as_str().to_owned();
        if let Some((at, prompt)) = cache.get(&key)
            && *at == c.updated_at
        {
            return prompt.clone();
        }
        let prompt = self.agy.last_prompt(&c.session);
        cache.insert(key, (c.updated_at, prompt.clone()));
        prompt
    }
}

/// Las conversaciones cerradas que se ofrecen para retomar (ADR 0009).
const CONVERSATIONS_DAYS: u64 = 7;
const CONVERSATIONS_MAX: usize = 60;

pub struct Terminals {
    /// Clientes de tmux (o sesiones directas si no hay tmux) dentro de PTYs de Kiti.
    pty: PtyManager,
    /// Sesiones persistentes (ADR 0008). `None` si tmux no está instalado.
    tmux: Option<Tmux>,
    /// Sesión tmux → PTY del cliente conectado ahora.
    clients: Mutex<HashMap<String, String>>,
    /// Solo sin tmux: id del PTY → proveedor.
    providers: Mutex<HashMap<String, kiti_protocol::Provider>>,
}

impl Terminals {
    fn new(tmux: Option<Tmux>) -> Self {
        Self {
            pty: PtyManager::default(),
            tmux,
            clients: Mutex::default(),
            providers: Mutex::default(),
        }
    }

    fn list(&self) -> Vec<TerminalInfo> {
        if let Some(tmux) = &self.tmux {
            return tmux
                .list()
                .into_iter()
                .map(|s| TerminalInfo {
                    provider: s
                        .provider
                        .as_deref()
                        .and_then(provider_from_str)
                        .unwrap_or(kiti_protocol::Provider::Claude),
                    id: s.name,
                    pid: s.pid,
                    cwd: s.cwd,
                    running: true,
                })
                .collect();
        }
        let providers = self.providers.lock().expect("lock");
        self.pty
            .list()
            .into_iter()
            .map(|t| TerminalInfo {
                provider: providers
                    .get(&t.id)
                    .copied()
                    .unwrap_or(kiti_protocol::Provider::Claude),
                id: t.id,
                pid: t.pid,
                cwd: t.cwd,
                running: t.running,
            })
            .collect()
    }

    /// PTY al que escribir para la terminal `id`: el cliente tmux conectado o el PTY directo.
    fn pty_id(&self, id: &str) -> String {
        self.clients
            .lock()
            .expect("lock")
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_owned())
    }
}

fn provider_from_str(s: &str) -> Option<kiti_protocol::Provider> {
    match s {
        "claude" => Some(kiti_protocol::Provider::Claude),
        "codex" => Some(kiti_protocol::Provider::Codex),
        "gemini" => Some(kiti_protocol::Provider::Gemini),
        "agy" | "antigravity" => Some(kiti_protocol::Provider::Antigravity),
        _ => None,
    }
}

fn terminals(state: &AppState) -> Result<&Arc<Terminals>, IpcError> {
    state.terminals.as_ref().ok_or_else(|| IpcError {
        code: "NO_TERMINALS".into(),
        message: "las terminales solo existen en modo real".into(),
    })
}

fn pty_error(e: kiti_core::adapters::pty::PtyError) -> IpcError {
    IpcError {
        code: "TERMINAL".into(),
        message: e.to_string(),
    }
}

fn io_error(code: &str) -> impl FnOnce(std::io::Error) -> IpcError + '_ {
    move |e| IpcError {
        code: code.into(),
        message: e.to_string(),
    }
}

/// Binario de cada CLI. La UI solo elige el proveedor: nunca envía un comando.
fn cli_binary(provider: kiti_protocol::Provider) -> &'static str {
    match provider {
        kiti_protocol::Provider::Claude => "claude",
        kiti_protocol::Provider::Codex => "codex",
        kiti_protocol::Provider::Gemini => "gemini",
        kiti_protocol::Provider::Antigravity => "agy",
    }
}

/// Shell de login e interactiva del usuario: el agente hereda el mismo PATH y entorno que en
/// su terminal (una app abierta desde el Finder solo tiene el PATH mínimo del sistema).
fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
}

/// Diálogo nativo de dos botones. `true` si el usuario elige `yes`.
async fn ask(app: &AppHandle, title: &str, message: String, yes: &str, no: &str) -> bool {
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(yes.into(), no.into()))
        .show(move |answer| {
            let _ = tx.send(answer);
        });
    rx.await.unwrap_or(false)
}

/// La sesión de Kiti que conviene reutilizar al lanzar `provider` en una carpeta: la primera del
/// mismo agente en esa carpeta (`in_folder`).
fn session_to_reuse(
    sessions: Vec<TerminalInfo>,
    provider: kiti_protocol::Provider,
    in_folder: impl Fn(&str) -> bool,
) -> Option<TerminalInfo> {
    sessions
        .into_iter()
        .find(|t| t.provider == provider && in_folder(&t.cwd))
}

fn same_dir(a: &str, b: &std::path::Path) -> bool {
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
    canon(std::path::Path::new(a)) == canon(b)
}

/// Lanza un CLI de agente en la carpeta que el usuario elige con el selector nativo (ADR 0007).
/// Con tmux la sesión sobrevive a Kiti, y antes de crear otra se ofrece reutilizar la existente
/// o continuar la última conversación (ADR 0008). Devuelve `None` si el usuario cancela.
#[tauri::command]
async fn launch_agent(
    provider: kiti_protocol::Provider,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<TerminalInfo>, IpcError> {
    let terminals = terminals(&state)?.clone();
    let bin = cli_binary(provider);
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .set_title(format!("Carpeta donde trabajará {bin}"))
        .pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
    let Some(folder) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    let cwd = folder.into_path().map_err(|e| IpcError {
        code: "FOLDER".into(),
        message: e.to_string(),
    })?;
    let folder_name = cwd.file_name().map_or_else(
        || cwd.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );

    // 1. ¿Ya hay una sesión de Kiti de este mismo agente en esa carpeta? Mejor retomarla que
    //    crear otra. La de otro agente no cuenta: si pides Antigravity, no te ofrece la de Claude.
    if let Some(existing) = session_to_reuse(terminals.list(), provider, |dir| same_dir(dir, &cwd))
    {
        let reuse = ask(
            &app,
            "Ya hay una sesión aquí",
            format!(
                "Kiti ya tiene una sesión de {} en «{folder_name}».\n¿Abrir esa en lugar de crear otra?",
                cli_binary(existing.provider)
            ),
            "Abrir la existente",
            "Crear otra",
        )
        .await;
        if reuse {
            info!(event = "agent.reuse", id = %existing.id, pid = existing.pid);
            return Ok(Some(existing));
        }
    }

    // 2. Claude: si la carpeta tiene conversaciones, se puede continuar la última.
    let mut command = format!("exec {bin}");
    if provider == kiti_protocol::Provider::Claude
        && let Some(home) = app.path().home_dir().ok()
        && has_conversations(&home.join(".claude/projects"), &cwd.to_string_lossy())
        && ask(
            &app,
            "Conversaciones anteriores",
            format!("«{folder_name}» tiene conversaciones de Claude.\n¿Continuar la última o empezar una nueva?"),
            "Continuar la última",
            "Nueva",
        )
        .await
    {
        command = format!("exec {bin} --continue");
    }
    // Antigravity: su `--continue` no se limita a la carpeta; se retoma la última de esta por id.
    if provider == kiti_protocol::Provider::Antigravity
        && let Some(conv) = state.conversations.as_ref()
        && let Some(last) = conv
            .agy
            .conversations(CONVERSATIONS_DAYS as u32)
            .into_iter()
            .find(|c| c.cwd.as_deref().is_some_and(|d| same_dir(d, &cwd)))
        && ask(
            &app,
            "Conversaciones anteriores",
            format!("«{folder_name}» tiene conversaciones de Antigravity.\n¿Continuar la última o empezar una nueva?"),
            "Continuar la última",
            "Nueva",
        )
        .await
    {
        command = format!("exec {bin} --conversation {}", last.session.as_str());
    }

    spawn_cli(&terminals, provider, &cwd, &command)
        .await
        .map(Some)
}

/// Lanza `command` (`exec <cli> ...`, siempre armado aquí, nunca por la UI) en `cwd`: en tmux
/// si está, si no en un PTY directo.
async fn spawn_cli(
    terminals: &Terminals,
    provider: kiti_protocol::Provider,
    cwd: &std::path::Path,
    command: &str,
) -> Result<TerminalInfo, IpcError> {
    let bin = cli_binary(provider);
    let shell = user_shell();
    // Comprobación previa para dar un error claro en lugar de "command not found" en la terminal.
    let check_shell = shell.clone();
    let found = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new(check_shell)
            .args(["-lic", &format!("command -v {bin}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
    .await
    .unwrap_or(false);
    if !found {
        return Err(IpcError {
            code: "CLI_NOT_FOUND".into(),
            message: format!("No se encontró `{bin}` en tu PATH"),
        });
    }

    // Con tmux, la sesión vive en tmux y sobrevive a Kiti; sin tmux, PTY directo.
    if let Some(tmux) = &terminals.tmux {
        // Comillas simples: `command` solo trae el binario de la lista, `--continue` o
        // `--resume <id>` con un id ya validado (letras, dígitos y guiones).
        let line = format!("{shell} -lic '{command}'");
        let session = tmux
            .new_session(cwd, &line, bin, 100, 30)
            .map_err(io_error("TMUX"))?;
        info!(event = "agent.launch", provider = bin, cwd = %cwd.display(), pid = session.pid, session = %session.name, hint = %tmux.attach_hint(&session.name));
        return Ok(TerminalInfo {
            id: session.name,
            pid: session.pid,
            provider,
            cwd: session.cwd,
            running: true,
        });
    }
    let t = terminals
        .pty
        .spawn(LaunchSpec {
            program: shell.into(),
            // `exec` sustituye la shell por el CLI: el PID de la terminal es el del agente.
            args: vec!["-lic".into(), command.to_owned()],
            cwd: cwd.to_owned(),
            cols: 100,
            rows: 30,
        })
        .map_err(pty_error)?;
    terminals
        .providers
        .lock()
        .expect("lock")
        .insert(t.id.clone(), provider);
    info!(event = "agent.launch", provider = bin, cwd = %cwd.display(), pid = t.pid);
    Ok(TerminalInfo {
        id: t.id,
        pid: t.pid,
        provider,
        cwd: t.cwd,
        running: t.running,
    })
}

fn conversations(state: &AppState) -> Result<&Arc<Conversations>, IpcError> {
    state.conversations.as_ref().ok_or_else(|| IpcError {
        code: "NO_CONVERSATIONS".into(),
        message: "las conversaciones solo existen en modo real".into(),
    })
}

/// Conversaciones de Claude con actividad en los últimos 7 días, de la más reciente a la más
/// vieja (ADR 0009). La UI descarta las que ya tienen una oficina encendida.
#[tauri::command]
async fn list_conversations(state: State<'_, AppState>) -> Result<Vec<ConversationInfo>, IpcError> {
    let conv = conversations(&state)?.clone();
    let started = std::time::Instant::now();
    let list = tauri::async_runtime::spawn_blocking(move || {
        let since =
            std::time::SystemTime::now() - Duration::from_secs(CONVERSATIONS_DAYS * 24 * 3600);
        let found = conv.index.lock().expect("lock").list(&conv.dir, since);
        let agy = conv.antigravity();
        let hidden = conv.hidden.lock().expect("lock");
        let mut projects = conv.projects.lock().expect("lock");
        let claude = found.into_iter().map(|c| ConversationInfo {
            session_id: c.session.as_str().to_owned(),
            provider: kiti_protocol::Provider::Claude,
            project: projects.project_of(&c.cwd),
            cwd: c.cwd,
            title: c.title,
            last_prompt: c.prompt,
            updated_at: c.updated_at,
        });
        let mut all: Vec<ConversationInfo> = claude.collect();
        for c in &agy {
            let cwd = c.cwd.clone().unwrap_or_default();
            all.push(ConversationInfo {
                session_id: c.session.as_str().to_owned(),
                provider: kiti_protocol::Provider::Antigravity,
                project: projects.project_of(&cwd),
                cwd,
                title: c.title.clone(),
                last_prompt: conv.agy_prompt(c),
                updated_at: c.updated_at,
            });
        }
        all.retain(|c| !hidden.contains(&c.session_id));
        all.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        all.truncate(CONVERSATIONS_MAX);
        all
    })
    .await
    .map_err(|e| IpcError {
        code: "CONVERSATIONS".into(),
        message: e.to_string(),
    })?;
    debug!(
        event = "conversations.listed",
        count = list.len(),
        duration_ms = started.elapsed().as_millis() as u64
    );
    Ok(list)
}

/// Lleva al usuario a la terminal de un agente abierto fuera de Kiti (ADR 0012): Ghostty o
/// Terminal.app muestran su pestaña; otras apps, al menos, se traen al frente. La UI solo manda
/// el id del agente; la carpeta y el título los pone el core.
#[tauri::command]
async fn focus_agent_terminal(
    id: kiti_protocol::AgentId,
    state: State<'_, AppState>,
) -> Result<kiti_protocol::FocusResult, IpcError> {
    use kiti_core::adapters::focus::{FocusOutcome, focus_agent};
    use kiti_protocol::FocusResult;
    let Ok(core_id) = id.0.parse::<AgentId>() else {
        return Ok(FocusResult::NotFound);
    };
    let Some(brief) = state.runtime.describe(core_id).await else {
        return Ok(FocusResult::NotFound);
    };
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        focus_agent(core_id.pid, &brief.cwd, brief.title.as_deref())
    })
    .await
    .map_err(|e| IpcError {
        code: "FOCUS".into(),
        message: e.to_string(),
    })?;
    let result = match outcome {
        FocusOutcome::Focused(app) => FocusResult::Focused { app },
        FocusOutcome::AppOnly(app) => FocusResult::AppOnly { app },
        FocusOutcome::Unknown => FocusResult::Unknown,
    };
    info!(event = "focus.external", agent_id = %core_id, result = ?result);
    Ok(result)
}

/// Archiva una conversación cerrada: deja de salir como oficina apagada (ADR 0009). No borra
/// nada de Claude; solo guarda su id en `hidden-conversations.json` de Kiti.
#[tauri::command]
fn hide_conversation(session_id: String, state: State<'_, AppState>) -> Result<(), IpcError> {
    let conv = conversations(&state)?;
    conv.hidden
        .lock()
        .expect("lock")
        .hide(&session_id)
        .map_err(io_error("HIDE"))?;
    info!(event = "conversation.hidden");
    Ok(())
}

/// Retoma una conversación cerrada con `claude --resume <id>` en su carpeta (ADR 0009). Solo
/// acepta ids que Kiti ya listó: la UI nunca elige ni el comando ni la carpeta.
#[tauri::command]
async fn resume_conversation(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<TerminalInfo, IpcError> {
    let terminals = terminals(&state)?.clone();
    let conv = conversations(&state)?.clone();
    let not_found = || IpcError {
        code: "CONVERSATION_NOT_FOUND".into(),
        message: "Esa conversación ya no está en la lista".into(),
    };
    if !is_session_id(&session_id) {
        return Err(not_found());
    }
    // Claude (`claude --resume`) o Antigravity (`agy --conversation`), según de qué lista sea.
    let claude = conv
        .index
        .lock()
        .expect("lock")
        .find(&session_id)
        .map(|c| c.cwd.clone());
    let agy = || {
        conv.agy_last
            .lock()
            .expect("lock")
            .iter()
            .find(|c| c.session.as_str() == session_id)
            .and_then(|c| c.cwd.clone())
    };
    let (provider, cwd, command) = match (claude, agy()) {
        (Some(cwd), _) => (
            kiti_protocol::Provider::Claude,
            cwd,
            format!("exec claude --resume {session_id}"),
        ),
        (None, Some(cwd)) => (
            kiti_protocol::Provider::Antigravity,
            cwd,
            format!("exec agy --conversation {session_id}"),
        ),
        (None, None) => return Err(not_found()),
    };
    let cwd = std::path::PathBuf::from(cwd);
    if !cwd.is_dir() {
        return Err(IpcError {
            code: "FOLDER".into(),
            message: format!("La carpeta {} ya no existe", cwd.display()),
        });
    }
    let info = spawn_cli(&terminals, provider, &cwd, &command).await?;
    info!(event = "conversation.resumed", pid = info.pid);
    Ok(info)
}

#[tauri::command]
fn list_terminals(state: State<'_, AppState>) -> Result<Vec<TerminalInfo>, IpcError> {
    Ok(terminals(&state)?.list())
}

/// Conecta la ventana a una terminal y envía su salida en binario por el Channel. Con tmux se
/// lanza un cliente nuevo cada vez (tmux redibuja la pantalla entera al conectarse).
#[tauri::command]
fn attach_terminal(
    id: String,
    on_data: Channel<InvokeResponseBody>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    let terminals = terminals(&state)?;
    let pty_id = match &terminals.tmux {
        Some(tmux) => {
            let client = terminals
                .pty
                .spawn(tmux.attach_spec(&id, 100, 30))
                .map_err(pty_error)?;
            let previous = terminals
                .clients
                .lock()
                .expect("lock")
                .insert(id.clone(), client.id.clone());
            if let Some(old) = previous {
                let _ = terminals.pty.kill(&old);
            }
            client.id
        }
        None => id,
    };
    terminals
        .pty
        .attach(&pty_id, move |bytes| {
            let _ = on_data.send(InvokeResponseBody::Raw(bytes));
        })
        .map_err(pty_error)
}

#[tauri::command]
fn terminal_input(id: String, data: String, state: State<'_, AppState>) -> Result<(), IpcError> {
    let terminals = terminals(&state)?;
    terminals
        .pty
        .write(&terminals.pty_id(&id), data.as_bytes())
        .map_err(pty_error)
}

#[tauri::command]
fn terminal_resize(
    id: String,
    cols: u16,
    rows: u16,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    let terminals = terminals(&state)?;
    terminals
        .pty
        .resize(&terminals.pty_id(&id), cols.max(2), rows.max(1))
        .map_err(pty_error)
}

/// Mantiene vivo el escritor de logs en segundo plano hasta que la app termina.
struct LogGuard(#[allow(dead_code)] WorkerGuard);

struct ChannelPublisher(Channel<ServerMsg>);

impl DiffPublisher for ChannelPublisher {
    /// Falla cuando la ventana se cerró: el runtime deja de publicarle.
    fn publish(&mut self, msg: ServerMsg) -> bool {
        self.0.send(msg).is_ok()
    }
}

/// Killer del modo simulado: no hay proceso real, solo silencia al agente en `SimFleet`.
#[derive(Clone)]
struct SimFleetKiller(mpsc::UnboundedSender<AgentId>);

impl AgentKiller for SimFleetKiller {
    async fn terminate(
        &self,
        target: KillTarget,
        _grace: Duration,
    ) -> Result<KillOutcome, KillError> {
        self.0
            .send(target.id)
            .map_err(|e| KillError::Failed(e.to_string()))?;
        Ok(KillOutcome::Killed)
    }
}

/// RSS y CPU del propio proceso de Kiti con sysinfo.
struct ProcessMonitor {
    system: System,
    pid: Pid,
}

impl SelfMonitor for ProcessMonitor {
    fn sample(&mut self) -> (u64, f64) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        self.system
            .process(self.pid)
            .map_or((0, 0.0), |p| (p.memory(), f64::from(p.cpu_usage())))
    }
}

fn ipc_error(code: &str) -> impl FnOnce(String) -> IpcError + '_ {
    move |message| IpcError {
        code: code.into(),
        message,
    }
}

#[tauri::command]
async fn subscribe(
    on_msg: Channel<ServerMsg>,
    state: State<'_, AppState>,
) -> Result<Hello, IpcError> {
    info!(event = "ipc.subscribe", "UI conectada al canal IPC");
    state
        .runtime
        .subscribe(ChannelPublisher(on_msg))
        .await
        .map_err(ipc_error("SUBSCRIBE_FAILED"))
}

#[tauri::command]
async fn kill_agent(
    id: kiti_protocol::AgentId,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<KillResult, IpcError> {
    // El id llega por IPC: se valida antes de enseñar nada al usuario.
    let Ok(core_id) = id.0.parse::<AgentId>() else {
        warn!(event = "kill.audit", agent_id = %id.0, "id inválido");
        return Ok(KillResult::NotFound);
    };
    info!(event = "kill.audit", agent_id = %core_id, step = "requested");

    // `show` con callback: no bloquea un hilo del runtime mientras el diálogo está abierto.
    let Some(cwd) = state.runtime.describe(core_id).await.map(|b| b.cwd) else {
        return Ok(KillResult::NotFound);
    };
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .message(format!(
            "Se terminará el agente {} de {cwd} (PID {}).\nSi está editando archivos, pueden quedar a medias.",
            core_id.provider, core_id.pid.0
        ))
        .title("¿Cerrar la oficina?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Cerrar oficina".into(),
            "Cancelar".into(),
        ))
        .show(move |confirmed| {
            let _ = tx.send(confirmed);
        });
    if !rx.await.unwrap_or(false) {
        info!(event = "kill.audit", agent_id = %core_id, step = "cancelled");
        return Ok(KillResult::Cancelled);
    }

    let result = state
        .runtime
        .kill_agent(core_id)
        .await
        .map_err(ipc_error("KILL_FAILED"))?;
    info!(event = "kill.audit", agent_id = %core_id, step = "done", result = ?result);
    Ok(result)
}

#[tauri::command]
fn report_ui_metrics(m: UiMetrics) {
    info!(
        event = "ui.metrics",
        fps = m.fps,
        frame_ms_p95 = m.frame_ms_p95,
        diff_to_paint_ms_p95 = m.diff_to_paint_ms_p95,
        idle_frames = m.idle_frames,
        offices_visible = m.offices_visible,
    );
}

/// Puente de logs del WebView: errores y avisos de la UI terminan en el mismo log JSON.
#[tauri::command]
fn ui_log(level: String, message: String) {
    let message: String = message.chars().take(2_000).collect();
    match level.as_str() {
        "error" => tracing::error!(target: "ui", "{message}"),
        "warn" => tracing::warn!(target: "ui", "{message}"),
        _ => tracing::info!(target: "ui", "{message}"),
    }
}

/// Modo, pausa, aprobación y control de hilos de los mensajes entre agentes (ADR 0017).
#[tauri::command]
async fn mail_action(command: MailCommand, state: State<'_, AppState>) -> Result<(), IpcError> {
    if let (MailCommand::Mode { mode }, Some(file)) = (command, &state.mail_mode_file) {
        let text = match mode {
            MailMode::Auto => "auto",
            MailMode::Manual => "manual",
        };
        if let Err(e) = std::fs::write(file, text) {
            warn!(event = "mail.mode_save", error = %e);
        }
    }
    state.runtime.mail(command).await;
    Ok(())
}

/// Modo guardado; si no hay nada, automático (ADR 0017).
fn saved_mail_mode(file: &std::path::Path) -> MailMode {
    match std::fs::read_to_string(file).as_deref().map(str::trim) {
        Ok("manual") => MailMode::Manual,
        _ => MailMode::Auto,
    }
}

/// Escribe los mensajes en la sesión tmux del destinatario (ADR 0017). Si falla, los devuelve
/// a la cola del core.
struct TmuxCourier {
    tmux: Option<Tmux>,
    runtime: RuntimeHandle,
}

impl MailCourier for TmuxCourier {
    fn deliver(&mut self, pid: kiti_core::domain::Pid, text: String, ids: Vec<u64>) {
        let tmux = self.tmux.clone();
        let runtime = self.runtime.clone();
        tauri::async_runtime::spawn(async move {
            let typed = tokio::task::spawn_blocking(move || match tmux {
                Some(t) => t.type_into(pid.0, &text),
                None => Err(std::io::Error::other("sin tmux")),
            })
            .await
            .unwrap_or_else(|e| Err(std::io::Error::other(e.to_string())));
            if let Err(e) = typed {
                warn!(event = "mail.type_failed", pid = pid.0, error = %e);
                runtime.mail_undeliver(ids).await;
            }
        });
    }
}

/// Cada 3 s, qué agentes viven en sesiones tmux de Kiti: a esos se les puede escribir.
fn start_mail_reachable(tmux: Option<Tmux>, runtime: RuntimeHandle) {
    let Some(tmux) = tmux else { return };
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(3));
        loop {
            ticker.tick().await;
            if runtime.is_closed() {
                return;
            }
            let t = tmux.clone();
            let pids = tokio::task::spawn_blocking(move || {
                t.list()
                    .into_iter()
                    .map(|s| kiti_core::domain::Pid(s.pid))
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            runtime.mail_reachable(pids).await;
        }
    });
}

#[tauri::command]
async fn set_debug_stream(on: bool, state: State<'_, AppState>) -> Result<(), IpcError> {
    state.runtime.set_debug_stream(on).await;
    Ok(())
}

/// Puerto fijo del HookServer: los hooks instalados en `~/.claude/settings.json` apuntan a él.
fn hook_port() -> u16 {
    std::env::var("KITI_HOOK_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_HOOK_PORT)
}

/// HookServer en 127.0.0.1, token `0600` y hooks de Claude instalados (permiso del usuario,
/// ADR 0006). Si algo falla, Kiti sigue funcionando sin semáforo de hooks y lo deja en el log.
fn start_claude_hooks(app: &AppHandle, home: Option<&std::path::Path>, runtime: RuntimeHandle) {
    let port = hook_port();
    let token = match app
        .path()
        .app_data_dir()
        .map_err(|e| std::io::Error::other(e.to_string()))
        .and_then(|dir| load_or_create_token(&dir.join("hook-token")))
    {
        Ok(token) => token,
        Err(e) => {
            error!(event = "hooks.token", error = %e, "sin token no se aceptan hooks");
            return;
        }
    };
    let (tx, rx) = mpsc::channel(1024);
    // La API de kiti-mcp (ADR 0017) comparte puerto y token con los hooks.
    let mcp = mcp_router(token.clone(), port, runtime.clone());
    tauri::async_runtime::spawn(run_hook_pump(rx, runtime, SystemClock));
    let router = hook_router(token.clone(), port, tx).merge(mcp);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = serve_hooks(port, router).await {
            error!(event = "hooks.server", port, error = %e, "¿puerto ocupado? (KITI_HOOK_PORT)");
        }
    });
    let Some(home) = home else { return };
    let settings = home.join(".claude/settings.json");
    let url = format!("http://127.0.0.1:{port}/v1/hooks/claude");
    if !is_installed_file(&settings, &url)
        && let Err(e) = install_file(&settings, &url, &token)
    {
        error!(event = "hooks.install", error = %e);
    }
}

/// Logs legibles por stderr y JSON con rotación diaria en el directorio de logs de la app.
fn init_tracing(app: &AppHandle) -> Option<WorkerGuard> {
    let filter = || EnvFilter::try_from_env("KITI_LOG").unwrap_or_else(|_| "info".into());
    let stderr = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    let (file, guard) = match app.path().app_log_dir() {
        Ok(dir) => {
            let (writer, guard) =
                tracing_appender::non_blocking(tracing_appender::rolling::daily(dir, "kiti.log"));
            let layer = tracing_subscriber::fmt::layer().json().with_writer(writer);
            (Some(layer), Some(guard))
        }
        Err(_) => (None, None),
    };
    let _ = tracing_subscriber::registry()
        .with(filter())
        .with(stderr)
        .with(file)
        .try_init();
    guard
}

/// Registra la flota simulada y la hace avanzar cada 100 ms con reloj alineado al real.
async fn run_sim(
    runtime: RuntimeHandle,
    spec: SimSpec,
    mut forget: mpsc::UnboundedReceiver<AgentId>,
) {
    let mut fleet = SimFleet::starting_at(spec.clone(), SystemClock.now().ms());
    for detected in fleet.detected_agents() {
        runtime.register_agent(detected, Timestamp(fleet.now)).await;
    }
    let mut ticker = tokio::time::interval(Duration::from_millis(TICK_MS));
    loop {
        tokio::select! {
            Some(id) = forget.recv() => fleet.forget(id),
            _ = ticker.tick(), if !spec.freeze => {
                for (id, input) in fleet.step() {
                    runtime.apply_input(id, input, Timestamp(fleet.now)).await;
                }
            }
            else => break,
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let guard = init_tracing(app.handle());
            app.manage(LogGuard(guard.unwrap_or_else(|| {
                tracing_appender::non_blocking(std::io::sink()).1
            })));

            let telemetry = Arc::new(Telemetry::new());
            let monitor = ProcessMonitor {
                system: System::new(),
                pid: Pid::from_u32(std::process::id()),
            };
            // Todo se arranca dentro del runtime de tokio de Tauri: fuera de él, tokio::spawn
            // entra en pánico.
            let handle = match std::env::var("KITI_SIM") {
                Ok(sim_env) => {
                    let spec = SimSpec::parse(&sim_env).unwrap_or(SimSpec {
                        count: 20,
                        seed: 42,
                        freeze: false,
                    });
                    info!(
                        event = "app.start",
                        mode = "sim",
                        count = spec.count,
                        seed = spec.seed,
                        freeze = spec.freeze
                    );
                    let (forget_tx, forget_rx) = mpsc::unbounded_channel();
                    let (runtime, handle) = Runtime::new(
                        SimFleetKiller(forget_tx),
                        SystemClock,
                        telemetry,
                        Source::Sim,
                        env!("CARGO_PKG_VERSION"),
                    );
                    tauri::async_runtime::spawn(runtime.with_monitor(monitor).run());
                    tauri::async_runtime::spawn(run_sim(handle.clone(), spec, forget_rx));
                    handle
                }
                Err(_) => {
                    info!(event = "app.start", mode = "live");
                    let mail_tmux = Tmux::detect("kiti", None);
                    let (runtime, handle) = Runtime::new(
                        UnixKiller::default(),
                        SystemClock,
                        telemetry,
                        Source::Live,
                        env!("CARGO_PKG_VERSION"),
                    );
                    let runtime = runtime
                        .with_monitor(monitor)
                        .with_projects(GitProjects::default())
                        .with_alerts(NativeAlerts {
                            app: app.handle().clone(),
                        })
                        .with_courier(TmuxCourier {
                            tmux: mail_tmux.clone(),
                            runtime: handle.clone(),
                        });
                    let runtime = match app.path().app_data_dir() {
                        Ok(dir) => {
                            let store = SqliteMemoryStore::new(dir.join("memory.db"));
                            let initial = store.load_all();
                            info!(event = "memory.loaded", count = initial.len());
                            runtime.with_memories(initial, store)
                        }
                        Err(_) => runtime,
                    };
                    start_mail_reachable(mail_tmux, handle.clone());
                    if let Ok(dir) = app.path().app_data_dir() {
                        let mode = saved_mail_mode(&dir.join("mail-mode"));
                        if mode == MailMode::Manual {
                            let h = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                h.mail(MailCommand::Mode { mode }).await;
                            });
                        }
                    }
                    on_alert_click(app.handle(), handle.clone());
                    start_antigravity_quota(handle.clone());
                    start_git_status(handle.clone());
                    tauri::async_runtime::spawn(runtime.run());
                    let home = app.path().home_dir().ok();
                    tauri::async_runtime::spawn(run_scanner_with(
                        SysinfoScanner::new(),
                        home.as_ref().map(|h| h.join(".claude/sessions")),
                        home.as_ref().map(|h| h.join(".gemini/antigravity-cli")),
                        handle.clone(),
                        SystemClock,
                        scan_interval(),
                    ));
                    start_claude_hooks(app.handle(), home.as_deref(), handle.clone());
                    handle
                }
            };
            start_tray(app.handle(), handle.clone())?;
            let terminals = std::env::var("KITI_SIM").is_err().then(|| {
                // Configuración propia de tmux para las sesiones persistentes (ADR 0008).
                let conf = app.path().app_data_dir().ok().and_then(|dir| {
                    let path = dir.join("tmux.conf");
                    std::fs::create_dir_all(&dir).ok()?;
                    std::fs::write(&path, KITI_TMUX_CONF).ok()?;
                    Some(path)
                });
                let tmux = Tmux::detect("kiti", conf);
                info!(event = "terminals.ready", persistent = tmux.is_some());
                Arc::new(Terminals::new(tmux))
            });
            let conversations = std::env::var("KITI_SIM")
                .is_err()
                .then(|| app.path().home_dir().ok())
                .flatten()
                .map(|home| {
                    let hidden = app.path().app_data_dir().map_or_else(
                        |_| home.join(".kiti/hidden-conversations.json"),
                        |dir| dir.join("hidden-conversations.json"),
                    );
                    Arc::new(Conversations {
                        dir: home.join(".claude/projects"),
                        index: Mutex::default(),
                        projects: Mutex::default(),
                        hidden: Mutex::new(HiddenConversations::load(hidden)),
                        agy: AntigravityStore::new(home.join(".gemini/antigravity-cli")),
                        agy_last: Mutex::default(),
                        agy_prompts: Mutex::default(),
                    })
                });
            let mail_mode_file = app.path().app_data_dir().ok().map(|d| d.join("mail-mode"));
            app.manage(AppState {
                runtime: handle,
                terminals,
                conversations,
                mail_mode_file,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            subscribe,
            kill_agent,
            report_ui_metrics,
            set_debug_stream,
            ui_log,
            launch_agent,
            list_terminals,
            attach_terminal,
            terminal_input,
            terminal_resize,
            list_conversations,
            resume_conversation,
            hide_conversation,
            focus_agent_terminal,
            mail_action,
        ])
        .build(tauri::generate_context!())
        .expect("error al construir la aplicación Tauri")
        .run(|app, event| match event {
            // Cerrar la última ventana no cierra Kiti: sigue en la barra de menú. Solo "Salir"
            // (exit con código) termina el proceso.
            RunEvent::ExitRequested {
                code: None, api, ..
            } => {
                info!(event = "window.closed", "Kiti sigue en la barra de menú");
                api.prevent_exit();
            }
            // Clic en el icono del Dock sin ventanas abiertas.
            RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } => show_window(app),
            _ => {}
        });
}

/// Muestra la ventana principal o la vuelve a crear: cerrarla la destruye para liberar el
/// WebView (~150 MB). Al recrearla, la UI se suscribe de nuevo y recibe un Snapshot.
fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let Some(config) = app.config().app.windows.first() else {
        return;
    };
    match tauri::WebviewWindowBuilder::from_config(app, config).and_then(|b| b.build()) {
        Ok(window) => {
            info!(event = "window.reopened");
            let _ = window.set_focus();
        }
        Err(e) => error!(event = "window.reopen", error = %e),
    }
}

/// Avisos como notificaciones de macOS (ADR 0010). Si la ventana de Kiti está al frente no
/// molesta: el usuario ya lo ve en el mapa.
struct NativeAlerts {
    app: AppHandle,
}

impl AlertSink for NativeAlerts {
    fn alert(&mut self, notice: AlertNotice) {
        let focused = self
            .app
            .get_webview_window("main")
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
        if focused {
            debug!(event = "alert.suppressed", agent_id = %notice.agent, reason = "focused");
            return;
        }
        let (title, body) = alert_text(&notice);
        let shown = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            // Para saber qué oficina enfocar si el usuario hace clic en ella.
            .extra("agentId", notice.agent.to_string())
            .show();
        match shown {
            Ok(()) => info!(event = "alert.sent", agent_id = %notice.agent, kind = ?notice.kind),
            Err(e) => warn!(event = "alert.error", error = %e),
        }
    }

    /// Un hilo de mensajes entre agentes llegó al límite (ADR 0017). Solo si Kiti no está al
    /// frente: dentro, el chip de la barra ya se pone amarillo.
    fn mail_paused(&mut self, subject: &str, between: &str) {
        let focused = self
            .app
            .get_webview_window("main")
            .and_then(|w| w.is_focused().ok())
            .unwrap_or(false);
        if focused {
            return;
        }
        match self
            .app
            .notification()
            .builder()
            .title("Mensajes en pausa")
            .body(format!(
                "{between}: «{subject}» llegó al límite de mensajes. Ábrelo en Kiti para continuar o cerrarlo."
            ))
            .show()
        {
            Ok(()) => info!(event = "alert.sent", kind = "MailPaused"),
            Err(e) => warn!(event = "alert.error", error = %e),
        }
    }

    /// Cuota semanal baja (ADR 0015). Avisa aunque Kiti esté al frente: el indicador de la barra
    /// es pequeño y esto pasa una vez por semana como mucho.
    fn quota_low(&mut self, alert: kiti_core::domain::QuotaAlert) {
        let (title, body) = quota_text(&alert, SystemClock.now().0);
        match self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            Ok(()) => info!(event = "alert.sent", provider = %alert.provider, kind = "QuotaLow"),
            Err(e) => warn!(event = "alert.error", error = %e),
        }
    }
}

/// Git de los proyectos con agentes vivos para el tablero de la oficina (ADR 0016), cada 15 s.
fn start_git_status(runtime: RuntimeHandle) {
    use kiti_core::adapters::git_status::status;
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(15));
        loop {
            ticker.tick().await;
            if runtime.is_closed() {
                return;
            }
            let projects = runtime.live_projects().await;
            if projects.is_empty() {
                continue;
            }
            let started = std::time::Instant::now();
            let statuses = tauri::async_runtime::spawn_blocking(move || {
                projects
                    .into_iter()
                    .map(|p| {
                        let s = status(std::path::Path::new(&p));
                        (p, s)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default();
            debug!(
                event = "git.updated",
                projects = statuses.len(),
                duration_ms = started.elapsed().as_millis() as u64
            );
            runtime.git(statuses).await;
        }
    });
}

/// Cuota de Antigravity (ADR 0015): `agy -p /usage` al arrancar y cada 5 minutos, si `agy` está
/// instalado. No gasta tokens.
fn start_antigravity_quota(runtime: RuntimeHandle) {
    use kiti_core::adapters::antigravity::{find_agy, usage_json};
    use kiti_core::domain::QuotaRefresh;
    use kiti_core::ingestion::antigravity::parse_usage;
    use kiti_core::ports::Clock;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        // Al terminar cada turno de agy (sin bajar de 30 s entre consultas) y, si no, cada 5 min.
        let mut refresh = QuotaRefresh::new(Duration::from_secs(30), Duration::from_secs(300));
        loop {
            let wait = refresh.wait(SystemClock.now());
            if !wait.is_zero() {
                tokio::select! {
                    () = tokio::time::sleep(wait) => {}
                    () = runtime.quota_kicked() => refresh.kick(),
                }
                continue;
            }
            if runtime.is_closed() {
                return;
            }
            let read = tauri::async_runtime::spawn_blocking(|| {
                let agy = find_agy()?;
                let body = usage_json(&agy, Duration::from_secs(30))?;
                parse_usage(&body).ok()
            })
            .await
            .ok()
            .flatten();
            refresh.polled(SystemClock.now());
            match read {
                Some(windows) => {
                    runtime
                        .quota(kiti_core::domain::Provider::Antigravity, windows)
                        .await
                }
                None => debug!(event = "quota.error", provider = "antigravity"),
            }
        }
    });
}

/// Clic en un aviso: abre la ventana (aunque estuviera cerrada) y enfoca la oficina del agente.
/// El manejador se registra antes del primer aviso: los anteriores no informan del clic.
fn on_alert_click(app: &AppHandle, runtime: RuntimeHandle) {
    let handle = app.clone();
    let registered = app.notification().on_action(move |performed| {
        let id = performed
            .notification()
            .and_then(|n| n.extra().get("agentId"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<AgentId>().ok());
        info!(
            event = "alert.clicked",
            action = performed.action_id(),
            known = id.is_some()
        );
        show_window(&handle);
        if let Some(id) = id {
            let runtime = runtime.clone();
            tauri::async_runtime::spawn(async move { runtime.focus(id).await });
        }
    });
    if let Err(e) = registered {
        warn!(event = "alert.click_handler", error = %e);
    }
}

/// Icono en la barra de menú con el resumen del semáforo, actualizado cada segundo.
fn start_tray(app: &AppHandle, runtime: RuntimeHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Mostrar Kiti", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Salir de Kiti", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let mut tray = TrayIconBuilder::with_id("kiti")
        .title("Kiti")
        .tooltip("Kiti")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    let tray = tray.build(app)?;
    tauri::async_runtime::spawn(async move {
        let mut last = None;
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            if runtime.is_closed() {
                return;
            }
            let summary = runtime.summary().await;
            if last != Some(summary) {
                let title = summary.title();
                let _ = tray.set_title(Some(&title));
                let _ = tray.set_tooltip(Some(summary.tooltip()));
                debug!(event = "tray.updated", title);
                last = Some(summary);
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiti_protocol::Provider;

    fn session(id: &str, provider: Provider, cwd: &str) -> TerminalInfo {
        TerminalInfo {
            id: id.into(),
            pid: 1,
            provider,
            cwd: cwd.into(),
            running: true,
        }
    }

    #[test]
    fn solo_se_reutiliza_una_sesion_del_mismo_agente_en_la_carpeta() {
        let sessions = vec![
            session("kiti-1", Provider::Claude, "/w/tienda"),
            session("kiti-2", Provider::Antigravity, "/w/otra"),
        ];
        let in_tienda = |d: &str| d == "/w/tienda";
        // Antigravity en tienda: la de Claude no cuenta.
        assert_eq!(
            session_to_reuse(sessions.clone(), Provider::Antigravity, in_tienda),
            None
        );
        // Claude en tienda: esa sí.
        assert_eq!(
            session_to_reuse(sessions.clone(), Provider::Claude, in_tienda).map(|t| t.id),
            Some("kiti-1".to_owned())
        );
        // Antigravity en otra: la suya.
        assert_eq!(
            session_to_reuse(sessions, Provider::Antigravity, |d| d == "/w/otra").map(|t| t.id),
            Some("kiti-2".to_owned())
        );
    }
}
