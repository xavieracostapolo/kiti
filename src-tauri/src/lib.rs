//! Capa fina de Tauri v2 para Kiti (ADR 0004 §1–§2).
//!
//! Commands, Channel IPC, diálogo nativo de confirmación y observabilidad con tracing.

use std::sync::Arc;
use std::time::Duration;

use kiti_core::application::{Runtime, RuntimeHandle, Telemetry};
use kiti_core::domain::{AgentId, AgentInput, Timestamp};
use kiti_core::ports::DiffPublisher;
use kiti_core::sim::{SimFleet, SimKiller, SimSpec, TICK_MS};
use kiti_protocol::{Hello, IpcError, KillResult, ServerMsg, Source, UiMetrics};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub struct AppState {
    pub runtime: RuntimeHandle,
}

struct ChannelPublisher(Channel<ServerMsg>);

impl DiffPublisher for ChannelPublisher {
    fn publish(&mut self, msg: ServerMsg) {
        let _ = self.0.send(msg);
    }
}

#[tauri::command]
async fn subscribe(
    on_msg: Channel<ServerMsg>,
    state: State<'_, AppState>,
) -> Result<Hello, IpcError> {
    info!("UI conectada al canal IPC");
    state
        .runtime
        .subscribe(ChannelPublisher(on_msg))
        .await
        .map_err(|e| IpcError {
            code: "SUBSCRIBE_FAILED".into(),
            message: e,
        })
}

#[tauri::command]
async fn kill_agent(
    id: kiti_protocol::AgentId,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<KillResult, IpcError> {
    info!(agent_id = %id.0, "Petición de kill recibida; solicitando confirmación nativa");

    let confirmed = app
        .dialog()
        .message(format!(
            "¿Deseas cerrar el agente {}?\nEsta acción terminará su proceso.",
            id.0
        ))
        .title("Cerrar oficina")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Cerrar oficina".into(),
            "Cancelar".into(),
        ))
        .blocking_show();

    if !confirmed {
        info!(agent_id = %id.0, "Kill cancelado por el usuario");
        return Ok(KillResult::Cancelled);
    }

    info!(agent_id = %id.0, "Kill confirmado; procediendo a terminar");
    // Parse id: "{provider}:{pid}:{start_ms}"
    let parts: Vec<&str> = id.0.split(':').collect();
    if parts.len() != 3 {
        return Ok(KillResult::NotFound);
    }
    let provider = match parts[0] {
        "claude" => kiti_core::domain::Provider::Claude,
        "codex" => kiti_core::domain::Provider::Codex,
        _ => kiti_core::domain::Provider::Gemini,
    };
    let pid = match parts[1].parse::<u32>() {
        Ok(p) => kiti_core::domain::Pid(p),
        Err(_) => return Ok(KillResult::NotFound),
    };
    let start_time = match parts[2].parse::<u64>() {
        Ok(t) => kiti_core::domain::ProcessStartTime(t),
        Err(_) => return Ok(KillResult::NotFound),
    };

    let core_id = AgentId {
        provider,
        pid,
        start_time,
    };

    state
        .runtime
        .kill_agent(core_id)
        .await
        .map_err(|e| IpcError {
            code: "KILL_FAILED".into(),
            message: e,
        })
}

#[tauri::command]
fn report_ui_metrics(m: UiMetrics) {
    info!(
        fps = m.fps,
        p95_frame = m.frame_ms_p95,
        p95_diff = m.diff_to_paint_ms_p95,
        idle = m.idle_frames,
        visible = m.offices_visible,
        "Métricas UI recibidas"
    );
}

#[tauri::command]
async fn set_debug_stream(on: bool, state: State<'_, AppState>) -> Result<(), IpcError> {
    state.runtime.set_debug_stream(on).await;
    Ok(())
}

pub fn run() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_env("KITI_LOG").unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().pretty())
        .try_init();

    info!("Iniciando Kiti shell Tauri v2");

    let sim_env = std::env::var("KITI_SIM").unwrap_or_else(|_| "20:42".to_string());
    let spec = SimSpec::parse(&sim_env).unwrap_or(SimSpec {
        count: 20,
        seed: 42,
        freeze: false,
    });

    info!(
        count = spec.count,
        seed = spec.seed,
        freeze = spec.freeze,
        "Configuración SimFleet"
    );

    let telemetry = Arc::new(Telemetry::new());
    let runtime = Runtime::spawn(SimKiller, Arc::clone(&telemetry), Source::Sim, "0.1.0");

    // Tarea de simulación en segundo plano
    let runtime_clone = runtime.clone();
    tokio::spawn(async move {
        let mut fleet = SimFleet::new(spec.clone());
        let detected_list = fleet.detected_agents();
        for detected in detected_list {
            runtime_clone
                .register_agent(detected, Timestamp(fleet.now))
                .await;
        }

        if !spec.freeze {
            loop {
                tokio::time::sleep(Duration::from_millis(TICK_MS)).await;
                let signals = fleet.step();
                for (id, sig) in signals {
                    runtime_clone
                        .apply_input(id, AgentInput::Signal(sig), Timestamp(fleet.now))
                        .await;
                }
            }
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { runtime })
        .invoke_handler(tauri::generate_handler![
            subscribe,
            kill_agent,
            report_ui_metrics,
            set_debug_stream,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar la aplicación Tauri");
}
