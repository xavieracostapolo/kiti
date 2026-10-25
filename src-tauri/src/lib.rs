//! Capa fina de Tauri v2 para Kiti (ADR 0004 §1–§2).
//!
//! Commands, Channel IPC, diálogo nativo de confirmación y observabilidad con tracing.
//! Toda la lógica vive en `kiti-core`; aquí solo se conecta con Tauri.

use std::time::Duration;

use kiti_core::adapters::claude_settings::{install_file, is_installed_file};
use kiti_core::adapters::hooks::{hook_router, load_or_create_token, run_hook_pump, serve_hooks};
use kiti_core::adapters::{SysinfoScanner, SystemClock, UnixKiller};
use kiti_core::application::{Runtime, RuntimeHandle, Telemetry, run_scanner};
use kiti_core::domain::{AgentId, Timestamp};
use kiti_core::ports::{
    AgentKiller, Clock, DiffPublisher, KillError, KillOutcome, KillTarget, SelfMonitor,
};
use kiti_core::sim::{SimFleet, SimSpec, TICK_MS};
use kiti_protocol::{Hello, IpcError, KillResult, ServerMsg, Source, UiMetrics};
use std::sync::Arc;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};
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
}

/// Mantiene vivo el escritor de logs en segundo plano hasta que la app termina.
struct LogGuard(#[allow(dead_code)] WorkerGuard);

struct ChannelPublisher(Channel<ServerMsg>);

impl DiffPublisher for ChannelPublisher {
    fn publish(&mut self, msg: ServerMsg) {
        if let Err(e) = self.0.send(msg) {
            warn!(error = %e, "no se pudo enviar por el Channel IPC");
        }
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
    let Some(cwd) = state.runtime.describe(core_id).await else {
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
    tauri::async_runtime::spawn(run_hook_pump(rx, runtime, SystemClock));
    let router = hook_router(token.clone(), port, tx);
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
                    let (runtime, handle) = Runtime::new(
                        UnixKiller::default(),
                        SystemClock,
                        telemetry,
                        Source::Live,
                        env!("CARGO_PKG_VERSION"),
                    );
                    tauri::async_runtime::spawn(runtime.with_monitor(monitor).run());
                    let home = app.path().home_dir().ok();
                    tauri::async_runtime::spawn(run_scanner(
                        SysinfoScanner::new(),
                        home.as_ref().map(|h| h.join(".claude/sessions")),
                        handle.clone(),
                        SystemClock,
                        scan_interval(),
                    ));
                    start_claude_hooks(app.handle(), home.as_deref(), handle.clone());
                    handle
                }
            };
            app.manage(AppState { runtime: handle });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            subscribe,
            kill_agent,
            report_ui_metrics,
            set_debug_stream,
            ui_log,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación Tauri");
}
