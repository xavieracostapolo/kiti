//! Protocolo v1 entre el core y la UI (ADR 0004).
//!
//! Solo DTOs: sin lógica de dominio. Los tipos se exportan a TypeScript con
//! `mise run gen-types` y se versionan en `ui/src/protocol/generated/`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Sube solo con cambios incompatibles. Añadir un campo opcional no la sube.
pub const PROTOCOL_VERSION: u32 = 1;

/// `"{provider}:{pid}:{start_ms}"`. Opaco para la UI.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
    /// Antigravity CLI (`agy`), ADR 0014.
    Antigravity,
}

/// Color del semáforo: working = verde, waiting = amarillo, idle = gris, error/dead = rojo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum StatusKind {
    Working,
    Waiting,
    Idle,
    Error,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum StatusReason {
    Hook,
    SessionFile,
    Stale,
    Failure,
    ProcessGone,
    Startup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Status {
    pub kind: StatusKind,
    pub reason: StatusReason,
    /// Epoch ms desde el que el agente está en este estado.
    pub since: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentState {
    pub id: AgentId,
    pub provider: Provider,
    pub pid: u32,
    pub cwd: String,
    pub started_at: u64,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_tool: Option<String>,
    pub last_event_at: u64,
    /// Raíz del repositorio git del `cwd`, o el propio `cwd` (ADR 0009).
    pub project: String,
    /// Conversación abierta (Claude), si ya se enlazó.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_id: Option<String>,
    /// Título de la conversación.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    /// Qué hace ahora, en palabras ("Editando main.rs").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub activity: Option<String>,
    /// Último pedido del usuario, recortado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Source {
    Live,
    Sim,
}

/// Respuesta de `subscribe`. Si `protocol_version` no coincide, la UI no aplica diffs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Hello {
    pub protocol_version: u32,
    pub app_version: String,
    pub source: Source,
}

impl Hello {
    pub fn new(app_version: impl Into<String>, source: Source) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            app_version: app_version.into(),
            source,
        }
    }
}

/// Métricas del core, a 1 Hz y solo con el panel de debug abierto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CoreMetrics {
    pub agents: u32,
    pub events_per_sec: f64,
    pub hook_to_publish_ms_p95: f64,
    pub rss_bytes: u64,
    pub cpu_percent: f64,
}

/// Stream core -> UI. El primer mensaje es siempre `Snapshot`; `seq` es monotónico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum ServerMsg {
    Snapshot {
        seq: u64,
        agents: Vec<AgentState>,
    },
    Diff {
        seq: u64,
        ingest_ms: u64,
        upserts: Vec<AgentState>,
        removed: Vec<AgentId>,
    },
    Metrics(CoreMetrics),
    /// Cuota de uso de un proveedor (ADR 0015). Se reenvía la última tras cada Snapshot.
    Quota {
        provider: Provider,
        windows: Vec<QuotaWindow>,
    },
    /// Enfocar y seleccionar esa oficina (clic en una notificación, ADR 0010).
    Focus {
        id: AgentId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum KillResult {
    Killed,
    Cancelled,
    StaleProcess,
    NotFound,
    Failed { message: String },
}

/// Resumen de la UI cada 2 s (ADR 0004 §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiMetrics {
    pub frame_ms_p50: f64,
    pub frame_ms_p95: f64,
    pub diff_to_paint_ms_p50: f64,
    pub diff_to_paint_ms_p95: f64,
    pub fps: f64,
    pub idle_frames: u32,
    pub offices_visible: u32,
}

/// Terminal de un agente lanzado desde Kiti (ADR 0007). La UI la une a su oficina por `pid`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TerminalInfo {
    pub id: String,
    pub pid: u32,
    pub provider: Provider,
    pub cwd: String,
    pub running: bool,
}

/// Una ventana de límite de uso (ADR 0015): "5 h", "semana", "Gemini Models · semana"…
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QuotaWindow {
    pub label: String,
    pub weekly: bool,
    /// Lo que queda, en porcentaje (0-100).
    pub remaining_pct: u8,
    /// Cuándo se reinicia, en ms desde epoch (0 si no se sabe).
    pub resets_at: u64,
}

/// Resultado de "Ir a su terminal" para un agente abierto fuera de Kiti (ADR 0012).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum FocusResult {
    /// Se mostró su pestaña en `app`.
    Focused { app: String },
    /// `app` está al frente, pero no se supo qué pestaña es la suya.
    AppOnly { app: String },
    /// No se sabe en qué terminal está.
    Unknown,
    /// El agente ya no existe.
    NotFound,
}

/// Conversación de Claude cerrada en los últimos 7 días: una oficina apagada que se puede
/// retomar con `claude --resume` (ADR 0009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConversationInfo {
    pub session_id: String,
    pub provider: Provider,
    pub cwd: String,
    /// Raíz del repositorio git del `cwd`, o el propio `cwd`.
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_prompt: Option<String>,
    /// Última actividad, en ms desde epoch.
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent() -> AgentState {
        AgentState {
            id: AgentId("claude:4242:1700000000000".into()),
            provider: Provider::Claude,
            pid: 4242,
            cwd: "/Users/dev/kiti".into(),
            started_at: 1_700_000_000_000,
            status: Status {
                kind: StatusKind::Waiting,
                reason: StatusReason::Hook,
                since: 1_700_000_005_000,
            },
            last_tool: Some("Bash".into()),
            last_event_at: 1_700_000_005_000,
            project: "/Users/dev/kiti".into(),
            session_id: Some("s-1".into()),
            title: Some("Hito 11".into()),
            activity: None,
            last_prompt: None,
        }
    }

    #[test]
    fn diff_se_serializa_con_type_y_camel_case() {
        let msg = ServerMsg::Diff {
            seq: 7,
            ingest_ms: 12,
            upserts: vec![agent()],
            removed: vec![AgentId("codex:1:2".into())],
        };
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "Diff",
                "seq": 7,
                "ingestMs": 12,
                "upserts": [{
                    "id": "claude:4242:1700000000000",
                    "provider": "claude",
                    "pid": 4242,
                    "cwd": "/Users/dev/kiti",
                    "startedAt": 1_700_000_000_000u64,
                    "status": { "kind": "waiting", "reason": "hook", "since": 1_700_000_005_000u64 },
                    "lastTool": "Bash",
                    "lastEventAt": 1_700_000_005_000u64,
                    "project": "/Users/dev/kiti",
                    "sessionId": "s-1",
                    "title": "Hito 11"
                }],
                "removed": ["codex:1:2"]
            })
        );
    }

    #[test]
    fn last_tool_ausente_no_se_serializa() {
        let mut a = agent();
        a.last_tool = None;
        let value = serde_json::to_value(&a).unwrap();
        assert!(value.get("lastTool").is_none());
    }

    #[test]
    fn snapshot_ida_y_vuelta() {
        let msg = ServerMsg::Snapshot {
            seq: 0,
            agents: vec![agent()],
        };
        let text = serde_json::to_string(&msg).unwrap();
        let back: ServerMsg = serde_json::from_str(&text).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn kill_result_usa_kind() {
        assert_eq!(
            serde_json::to_value(KillResult::Killed).unwrap(),
            json!({ "kind": "killed" })
        );
        assert_eq!(
            serde_json::to_value(KillResult::Failed {
                message: "boom".into()
            })
            .unwrap(),
            json!({ "kind": "failed", "message": "boom" })
        );
    }

    #[test]
    fn hello_lleva_la_version_del_protocolo() {
        let hello = Hello::new("0.1.0", Source::Sim);
        let value = serde_json::to_value(&hello).unwrap();
        assert_eq!(
            value,
            json!({ "protocolVersion": PROTOCOL_VERSION, "appVersion": "0.1.0", "source": "sim" })
        );
    }

    #[test]
    fn terminal_info_en_camel_case() {
        let t = TerminalInfo {
            id: "pty-1".into(),
            pid: 77,
            provider: Provider::Claude,
            cwd: "/w".into(),
            running: true,
        };
        assert_eq!(
            serde_json::to_value(t).unwrap(),
            json!({ "id": "pty-1", "pid": 77, "provider": "claude", "cwd": "/w", "running": true })
        );
    }

    /// ts-rs no exporta constantes: la copia TS de PROTOCOL_VERSION se vigila aquí.
    #[test]
    fn la_ui_declara_la_misma_version_del_protocolo() {
        let index = include_str!("../../../ui/src/protocol/index.ts");
        let expected = format!("export const PROTOCOL_VERSION = {PROTOCOL_VERSION};");
        assert!(
            index.contains(&expected),
            "ui/src/protocol/index.ts debe contener `{expected}`"
        );
    }
}
