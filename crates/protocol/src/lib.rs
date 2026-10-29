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
    /// Primer pedido de la sesión (ADR 0018): conserva el objetivo inicial aunque luego haya "sí".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub initial_prompt: Option<String>,
    /// Último pedido del usuario, recortado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_prompt: Option<String>,
    /// Archivos modificados durante la sesión (`Edit`, `Write`…), ADR 0018.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub touched_files: Option<Vec<String>>,
    /// Checklist de tareas (`TodoWrite`), ADR 0018.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<Vec<TaskItem>>,
    /// Modelo de la sesión (ADR 0016).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Contexto usado, en porcentaje.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub context_pct: Option<u8>,
    /// Coste estimado de la sesión, en céntimos de dólar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cost_cents: Option<u32>,
    /// Últimos cambios de estado, el más reciente al final (línea de tiempo del tablero).
    #[serde(default)]
    pub history: Vec<StatusSpan>,
}

/// Un cambio de estado: desde `since` el agente estuvo en `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatusSpan {
    pub kind: StatusKind,
    pub since: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskItem {
    pub text: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MemoryKind {
    Decision,
    Gotcha,
    Handoff,
    Note,
}

/// Un recuerdo guardado en la memoria de un proyecto (ADR 0018).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MemoryItem {
    pub id: u64,
    pub project: String,
    pub kind: MemoryKind,
    pub title: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    pub author: String,
    pub created_at: u64,
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
    /// Mensajes entre agentes aceptados desde que arrancó Kiti (ADR 0017).
    #[serde(default)]
    pub mails_total: u64,
    /// Mensajes aún sin entregar (por aprobar, en cola o retenidos).
    #[serde(default)]
    pub mail_pending: u32,
    /// Recuerdos de proyecto guardados (ADR 0018).
    #[serde(default)]
    pub memories_total: u64,
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
    /// Estado de git de un proyecto (ADR 0016); `git` vacío si no es un repositorio. Se reenvía
    /// el último tras cada Snapshot.
    Git {
        project: String,
        git: Option<GitInfo>,
    },
    /// Cuota de uso de un proveedor (ADR 0015). Se reenvía la última tras cada Snapshot.
    Quota {
        provider: Provider,
        windows: Vec<QuotaWindow>,
    },
    /// Enfocar y seleccionar esa oficina (clic en una notificación, ADR 0010).
    Focus {
        id: AgentId,
    },
    /// Mensajes entre agentes (ADR 0017): el estado completo al cambiar algo y tras cada
    /// Snapshot.
    Mail(MailState),
    /// Recuerdos de los proyectos (ADR 0018): se envía al cambiar y tras cada Snapshot.
    Memory {
        memories: Vec<MemoryItem>,
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

/// Estado de git de un proyecto para el tablero de la oficina (ADR 0016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GitInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MailMode {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MailMessageState {
    /// Modo manual: espera la aprobación del usuario.
    Pending,
    /// Espera a que el destinatario esté libre (o lo lea con `kiti_inbox`).
    Queued,
    /// Su hilo está pausado por el límite de saltos.
    Held,
    Delivered,
    Discarded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum MailThreadState {
    Open,
    Paused,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailMessage {
    pub id: u64,
    pub from: AgentId,
    pub from_alias: String,
    pub to: AgentId,
    pub to_alias: String,
    pub text: String,
    pub sent_at: u64,
    pub state: MailMessageState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailThread {
    pub id: u64,
    pub subject: String,
    pub hops: u32,
    pub limit: u32,
    pub state: MailThreadState,
    pub messages: Vec<MailMessage>,
}

/// Alias corto de un agente vivo (`claude-1`), con el que los demás le escriben.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailAlias {
    pub agent: AgentId,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailState {
    pub mode: MailMode,
    pub paused: bool,
    /// Mensajes por aprobar más hilos pausados.
    pub attention: u32,
    pub aliases: Vec<MailAlias>,
    /// Del más reciente al más antiguo.
    pub threads: Vec<MailThread>,
}

/// Lo que la UI pide sobre los mensajes (command `mail_action`, ADR 0017).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum MailCommand {
    Mode { mode: MailMode },
    Pause { paused: bool },
    Approve { id: u64 },
    Discard { id: u64 },
    Continue { id: u64 },
    Close { id: u64 },
}

/// API local entre `kiti-mcp` y el core (ADR 0017). No va a la UI.
pub mod mcp {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Peer {
        pub alias: String,
        pub provider: String,
        /// Nombre de la carpeta del proyecto.
        pub project: String,
        /// `working`, `waiting`, `idle`…
        pub status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        /// Kiti puede escribirle en su terminal (lanzado desde Kiti); si no, lee con `kiti_inbox`.
        pub reachable: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct AgentsReply {
        /// Alias de quien pregunta.
        pub you: String,
        pub peers: Vec<Peer>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SendRequest {
        pub to: String,
        pub text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reply_to: Option<u64>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct SendReply {
        pub id: u64,
        pub thread: u64,
        /// `queued`, `pending` (espera al usuario) o `held` (hilo pausado por el límite).
        pub state: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InboxMessage {
        pub id: u64,
        pub from: String,
        pub project: String,
        pub thread: u64,
        pub subject: String,
        pub text: String,
        pub sent_at: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct InboxReply {
        pub messages: Vec<InboxMessage>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RememberRequest {
        pub kind: String,
        pub title: String,
        pub detail: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub files: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RememberReply {
        pub id: u64,
        pub project: String,
        pub kind: String,
        pub title: String,
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RecallRequest {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub query: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RecallSession {
        pub alias: String,
        pub provider: String,
        pub status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub initial_prompt: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub last_prompt: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub touched_files: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub pending_tasks: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RecallEntry {
        pub id: u64,
        pub kind: String,
        pub title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub detail: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub files: Vec<String>,
        pub author: String,
        pub created_at: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct RecallReply {
        pub project: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub query: Option<String>,
        pub sessions: Vec<RecallSession>,
        pub memories: Vec<RecallEntry>,
    }

    /// Cuerpo de las respuestas 4xx: `code` estable y `message` legible para el modelo.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ErrorReply {
        pub code: String,
        pub message: String,
    }
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
            initial_prompt: None,
            last_prompt: None,
            touched_files: None,
            tasks: None,
            model: None,
            context_pct: None,
            cost_cents: None,
            history: vec![],
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
                    "title": "Hito 11",
                    "history": []
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

    #[test]
    fn mail_lleva_type_y_camel_case() {
        let msg = ServerMsg::Mail(MailState {
            mode: MailMode::Auto,
            paused: false,
            attention: 1,
            aliases: vec![MailAlias {
                agent: AgentId("claude:1:2".into()),
                alias: "claude-1".into(),
            }],
            threads: vec![MailThread {
                id: 3,
                subject: "Revisión".into(),
                hops: 1,
                limit: 12,
                state: MailThreadState::Paused,
                messages: vec![MailMessage {
                    id: 7,
                    from: AgentId("claude:1:2".into()),
                    from_alias: "claude-1".into(),
                    to: AgentId("codex:3:4".into()),
                    to_alias: "codex-1".into(),
                    text: "hola".into(),
                    sent_at: 5,
                    state: MailMessageState::Held,
                }],
            }],
        });
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(value["type"], "Mail");
        assert_eq!(value["mode"], "auto");
        assert_eq!(value["aliases"][0]["alias"], "claude-1");
        assert_eq!(value["threads"][0]["state"], "paused");
        assert_eq!(value["threads"][0]["messages"][0]["fromAlias"], "claude-1");
        assert_eq!(value["threads"][0]["messages"][0]["sentAt"], 5);
        assert_eq!(value["threads"][0]["messages"][0]["state"], "held");
        let back: ServerMsg = serde_json::from_value(value).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn mail_command_usa_kind() {
        let c: MailCommand =
            serde_json::from_value(json!({ "kind": "mode", "mode": "manual" })).unwrap();
        assert_eq!(
            c,
            MailCommand::Mode {
                mode: MailMode::Manual
            }
        );
        let c: MailCommand =
            serde_json::from_value(json!({ "kind": "continue", "id": 3 })).unwrap();
        assert_eq!(c, MailCommand::Continue { id: 3 });
    }

    #[test]
    fn mcp_send_en_camel_case_y_reply_to_opcional() {
        let req: mcp::SendRequest =
            serde_json::from_value(json!({ "to": "codex-1", "text": "x", "replyTo": 4 })).unwrap();
        assert_eq!(req.reply_to, Some(4));
        let req: mcp::SendRequest =
            serde_json::from_value(json!({ "to": "codex-1", "text": "x" })).unwrap();
        assert_eq!(req.reply_to, None);
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
