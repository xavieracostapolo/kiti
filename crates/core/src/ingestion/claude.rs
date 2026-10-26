//! Hooks de Claude Code → `Signal` (ADR 0005 §1 y §5).

use serde::Deserialize;

use super::clip;
pub use super::{AclError, HookEvent};
use crate::domain::{
    AgentInfo, Pid, SessionActivity, SessionId, Signal, Timestamp, ToolName, ToolOutcome,
    ToolUseKey,
};

/// Lista blanca de campos (ADR 0005 §7 y 0009): de `tool_input` solo se leen los campos que
/// describen la acción; `command`, el contenido de los archivos y `tool_response` nunca.
#[derive(Deserialize)]
struct Payload {
    session_id: Option<String>,
    cwd: Option<String>,
    hook_event_name: Option<String>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    notification_type: Option<String>,
    tool_input: Option<ToolInput>,
    prompt: Option<String>,
    session_title: Option<String>,
}

#[derive(Deserialize)]
struct ToolInput {
    description: Option<String>,
    file_path: Option<String>,
    subagent_type: Option<String>,
    todos: Option<Vec<Todo>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Todo {
    status: Option<String>,
    content: Option<String>,
    active_form: Option<String>,
}

const MAX_TEXT: usize = 140;

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// "Qué está haciendo" en palabras, a partir de la herramienta y de campos descriptivos.
fn activity(tool: &str, input: Option<&ToolInput>) -> String {
    let file = input.and_then(|i| i.file_path.as_deref()).map(file_name);
    let text = match (tool, file) {
        ("Bash", _) => input
            .and_then(|i| i.description.clone())
            .unwrap_or_else(|| "Ejecutando un comando".into()),
        ("Edit" | "MultiEdit" | "Write" | "NotebookEdit", Some(f)) => format!("Editando {f}"),
        ("Read", Some(f)) => format!("Leyendo {f}"),
        ("Grep" | "Glob", _) => "Buscando en el código".into(),
        ("WebFetch" | "WebSearch", _) => "Buscando en la web".into(),
        ("Task" | "Agent", _) => match input.and_then(|i| i.subagent_type.as_deref()) {
            Some(kind) => match input.and_then(|i| i.description.as_deref()) {
                Some(d) => format!("Subagente {kind}: {d}"),
                None => format!("Subagente {kind}"),
            },
            None => "Delegando a un subagente".into(),
        },
        ("TodoWrite", _) => input
            .and_then(|i| i.todos.as_ref())
            .and_then(|todos| {
                todos
                    .iter()
                    .find(|t| t.status.as_deref() == Some("in_progress"))
            })
            .and_then(|t| t.active_form.clone().or_else(|| t.content.clone()))
            .unwrap_or_else(|| "Actualizando su lista de tareas".into()),
        _ => tool.to_owned(),
    };
    clip(&text, MAX_TEXT)
}

fn info_of(name: &str, p: &Payload) -> AgentInfo {
    AgentInfo {
        title: p.session_title.as_deref().map(|t| clip(t, MAX_TEXT)),
        activity: match (name, p.tool_name.as_deref()) {
            ("PreToolUse", Some(tool)) => Some(activity(tool, p.tool_input.as_ref())),
            _ => None,
        },
        prompt: match name {
            "UserPromptSubmit" => p.prompt.as_deref().map(|t| clip(t, MAX_TEXT)),
            _ => None,
        },
    }
}

fn tool_end(p: &Payload, outcome: ToolOutcome) -> Option<Signal> {
    Some(Signal::ToolEnd {
        key: ToolUseKey::new(p.tool_use_id.clone()?),
        outcome,
    })
}

fn to_signal(name: &str, p: &Payload) -> Option<Signal> {
    Some(match name {
        "SessionStart" => Signal::SessionStart,
        "UserPromptSubmit" => Signal::PromptSubmit,
        "PreToolUse" => Signal::ToolStart {
            key: ToolUseKey::new(p.tool_use_id.clone()?),
            tool: ToolName::new(p.tool_name.clone()?),
        },
        "PostToolUse" => return tool_end(p, ToolOutcome::Ok),
        "PostToolUseFailure" => return tool_end(p, ToolOutcome::Failed),
        "PermissionDenied" => return tool_end(p, ToolOutcome::Denied),
        "PermissionRequest" => Signal::AwaitingInput,
        "Notification" => match p.notification_type.as_deref()? {
            "permission_prompt"
            | "elicitation_dialog"
            | "elicitation_url_dialog"
            | "agent_needs_input" => Signal::AwaitingInput,
            "idle_prompt" => Signal::Stop,
            _ => return None,
        },
        "Stop" => Signal::Stop,
        "StopFailure" => Signal::Failure,
        "SessionEnd" => Signal::SessionEnd,
        _ => return None,
    })
}

/// Contenido útil de `~/.claude/sessions/<pid>.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFile {
    pub pid: Pid,
    pub session: SessionId,
    /// `status` con su `updatedAt`. `None` si falta o no se reconoce.
    pub activity: Option<(SessionActivity, Timestamp)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionFilePayload {
    pid: Option<u32>,
    session_id: Option<String>,
    status: Option<String>,
    updated_at: Option<u64>,
}

/// Parser tolerante: el esquema no está documentado y puede cambiar entre versiones.
pub fn parse_session_file(body: &[u8]) -> Option<SessionFile> {
    let p: SessionFilePayload = serde_json::from_slice(body).ok()?;
    let activity = match p.status.as_deref() {
        Some("busy") => Some(SessionActivity::Busy),
        Some("idle") => Some(SessionActivity::Idle),
        _ => None,
    };
    Some(SessionFile {
        pid: Pid(p.pid?),
        session: SessionId::new(p.session_id?),
        activity: activity.map(|a| (a, Timestamp(p.updated_at.unwrap_or(0)))),
    })
}

/// Nombre de la última herramienta (`tool_use`) de una línea del transcript, si la hay. Solo
/// sale el nombre: el texto, los argumentos y las salidas se descartan aquí mismo.
pub fn transcript_tool(line: &[u8]) -> Option<ToolName> {
    // Filtro barato: la mayoría de líneas no traen herramientas y no merece la pena parsearlas.
    if !line.windows(10).any(|w| w == b"\"tool_use\"") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(line).ok()?;
    value["message"]["content"]
        .as_array()?
        .iter()
        .filter(|item| item["type"] == "tool_use")
        .filter_map(|item| item["name"].as_str())
        .next_back()
        .map(ToolName::new)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetaRecord {
    #[serde(rename = "type")]
    kind: String,
    ai_title: Option<String>,
    last_prompt: Option<String>,
}

/// Título (`ai-title`) o último pedido (`last-prompt`) de una línea del transcript (ADR 0009).
/// Se recortan aquí mismo y nunca se registran.
pub fn transcript_info(line: &[u8]) -> AgentInfo {
    let has = |needle: &[u8]| line.windows(needle.len()).any(|w| w == needle);
    if !has(b"\"ai-title\"") && !has(b"\"last-prompt\"") {
        return AgentInfo::default();
    }
    let Ok(r) = serde_json::from_slice::<MetaRecord>(line) else {
        return AgentInfo::default();
    };
    let text = |t: Option<String>| t.map(|t| clip(&t, MAX_TEXT)).filter(|t| !t.is_empty());
    match r.kind.as_str() {
        "ai-title" => AgentInfo {
            title: text(r.ai_title),
            ..AgentInfo::default()
        },
        "last-prompt" => AgentInfo {
            prompt: text(r.last_prompt),
            ..AgentInfo::default()
        },
        _ => AgentInfo::default(),
    }
}

/// Lo que Kiti muestra de una conversación cerrada (ADR 0009).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConversationMeta {
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Deserialize)]
struct CwdRecord {
    cwd: Option<String>,
}

/// Recorre las líneas (las últimas de un transcript) de la más nueva a la más vieja y se queda
/// con el `cwd`, el título y el último pedido más recientes.
pub fn conversation_meta(lines: &[Vec<u8>]) -> ConversationMeta {
    let mut meta = ConversationMeta::default();
    for line in lines.iter().rev() {
        let info = transcript_info(line);
        meta.title = meta.title.or(info.title);
        meta.prompt = meta.prompt.or(info.prompt);
        if meta.cwd.is_none() && line.windows(6).any(|w| w == b"\"cwd\":") {
            meta.cwd = serde_json::from_slice::<CwdRecord>(line)
                .ok()
                .and_then(|r| r.cwd)
                .filter(|c| c.starts_with('/'));
        }
        if meta.cwd.is_some() && meta.title.is_some() && meta.prompt.is_some() {
            break;
        }
    }
    meta
}

/// ¿Puede ser un id de conversación? Va como argumento de `claude --resume` dentro de una
/// línea de shell, así que solo se aceptan letras, dígitos y guiones.
pub fn is_session_id(id: &str) -> bool {
    (8..=64).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Nombre de la carpeta de `~/.claude/projects` para un `cwd`: todo carácter no alfanumérico
/// pasa a `-` (observado en local; no está documentado).
pub fn project_dir_name(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// ¿Hay conversaciones de Claude en esa carpeta? Para ofrecer `claude --continue` (ADR 0008).
pub fn has_conversations(projects: &std::path::Path, cwd: &str) -> bool {
    std::fs::read_dir(projects.join(project_dir_name(cwd))).is_ok_and(|entries| {
        entries
            .filter_map(Result::ok)
            .any(|e| e.path().extension().is_some_and(|x| x == "jsonl"))
    })
}

#[derive(Deserialize)]
struct RateLimit {
    used_percentage: Option<f64>,
    resets_at: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct RateLimits {
    five_hour: Option<RateLimit>,
    seven_day: Option<RateLimit>,
}

/// Lista blanca del JSON de la status line: solo `rate_limits` (ADR 0015).
#[derive(Deserialize)]
struct StatusLine {
    rate_limits: Option<RateLimits>,
}

/// `resets_at`: segundos (o ms) desde epoch, o RFC 3339.
fn reset_ms(v: &serde_json::Value) -> u64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().map_or(0, |s| {
            if s > 1e12 {
                s as u64
            } else {
                (s * 1000.0) as u64
            }
        }),
        serde_json::Value::String(t) => super::rfc3339_ms(t).unwrap_or(0),
        _ => 0,
    }
}

/// Cuota de Claude desde el JSON de su status line. `None` si no trae `rate_limits` (sin
/// suscripción, o antes de la primera respuesta de la sesión).
pub fn parse_rate_limits(body: &[u8]) -> Option<Vec<crate::domain::QuotaWindow>> {
    let s: StatusLine = serde_json::from_slice(body).ok()?;
    let limits = s.rate_limits?;
    let window = |label: &str, weekly: bool, r: Option<RateLimit>| {
        let r = r?;
        Some(crate::domain::QuotaWindow {
            label: label.to_owned(),
            weekly,
            remaining: (1.0 - r.used_percentage? / 100.0).clamp(0.0, 1.0),
            resets_at: r.resets_at.as_ref().map_or(0, reset_ms),
        })
    };
    let windows: Vec<_> = [
        window("5 h", false, limits.five_hour),
        window("semana", true, limits.seven_day),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!windows.is_empty()).then_some(windows)
}

pub fn parse_hook(body: &[u8]) -> Result<HookEvent, AclError> {
    let p: Payload = serde_json::from_slice(body).map_err(|e| AclError::Json(e.to_string()))?;
    let session = p
        .session_id
        .clone()
        .ok_or(AclError::Missing("session_id"))?;
    let name = p
        .hook_event_name
        .clone()
        .ok_or(AclError::Missing("hook_event_name"))?;
    // Lo que no se reconoce (o le falta un campo necesario) no cambia el semáforo: solo cuenta.
    let signal = to_signal(&name, &p).unwrap_or_else(|| Signal::Unknown { name: name.clone() });
    let info = info_of(&name, &p);
    Ok(HookEvent {
        session: SessionId::new(session),
        cwd: p.cwd.unwrap_or_default(),
        name,
        signal,
        info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        Pid, SessionActivity, SessionId, Signal, Timestamp, ToolName, ToolOutcome, ToolUseKey,
    };

    fn parse(json: &str) -> Result<HookEvent, AclError> {
        parse_hook(json.as_bytes())
    }

    fn signal(json: &str) -> Signal {
        parse(json).unwrap().signal
    }

    fn ev(name: &str, extra: &str) -> String {
        format!(
            r#"{{"session_id":"s1","cwd":"/w/kiti","transcript_path":"/t.jsonl","hook_event_name":"{name}"{extra}}}"#
        )
    }

    #[test]
    fn payload_real_de_post_tool_use() {
        // Capturado en local con Claude Code 2.1.310 (ADR 0005 §1), rutas recortadas.
        let body = r#"{
          "session_id": "aeef3d7a-13ea-4240-b0f3-ded249409206",
          "transcript_path": "/Users/USER/.claude/projects/-private-tmp-x/aeef3d7a.jsonl",
          "cwd": "/private/tmp/x/repo",
          "prompt_id": "0663c53c-0c5a-41d8-acfe-3f2bd144ed21",
          "permission_mode": "auto",
          "effort": { "level": "high" },
          "hook_event_name": "PostToolUse",
          "tool_name": "Bash",
          "tool_input": { "command": "echo hi", "description": "Print hi" },
          "tool_response": { "stdout": "hi", "stderr": "", "interrupted": false },
          "tool_use_id": "toolu_01WKRhg2GAzbf6bdNkXVwF7v",
          "duration_ms": 494
        }"#;
        let e = parse(body).unwrap();
        assert_eq!(e.session.as_str(), "aeef3d7a-13ea-4240-b0f3-ded249409206");
        assert_eq!(e.cwd, "/private/tmp/x/repo");
        assert_eq!(e.name, "PostToolUse");
        assert_eq!(
            e.signal,
            Signal::ToolEnd {
                key: ToolUseKey::new("toolu_01WKRhg2GAzbf6bdNkXVwF7v"),
                outcome: ToolOutcome::Ok,
            }
        );
    }

    #[test]
    fn mapeo_de_los_eventos_suscritos() {
        assert_eq!(
            signal(&ev("SessionStart", r#","source":"startup""#)),
            Signal::SessionStart
        );
        assert_eq!(
            signal(&ev("UserPromptSubmit", r#","prompt":"hola""#)),
            Signal::PromptSubmit
        );
        assert_eq!(
            signal(&ev(
                "PreToolUse",
                r#","tool_name":"Edit","tool_use_id":"t1""#
            )),
            Signal::ToolStart {
                key: ToolUseKey::new("t1"),
                tool: ToolName::new("Edit"),
            }
        );
        assert_eq!(
            signal(&ev(
                "PostToolUseFailure",
                r#","tool_use_id":"t1","error":"x""#
            )),
            Signal::ToolEnd {
                key: ToolUseKey::new("t1"),
                outcome: ToolOutcome::Failed,
            }
        );
        assert_eq!(
            signal(&ev("PermissionDenied", r#","tool_use_id":"t1""#)),
            Signal::ToolEnd {
                key: ToolUseKey::new("t1"),
                outcome: ToolOutcome::Denied,
            }
        );
        assert_eq!(
            signal(&ev("PermissionRequest", r#","tool_name":"Bash""#)),
            Signal::AwaitingInput
        );
        assert_eq!(signal(&ev("Stop", "")), Signal::Stop);
        assert_eq!(
            signal(&ev("StopFailure", r#","error":"rate_limit""#)),
            Signal::Failure
        );
        assert_eq!(
            signal(&ev("SessionEnd", r#","reason":"clear""#)),
            Signal::SessionEnd
        );
    }

    #[test]
    fn notification_segun_su_tipo() {
        for t in [
            "permission_prompt",
            "elicitation_dialog",
            "elicitation_url_dialog",
            "agent_needs_input",
        ] {
            let body = ev("Notification", &format!(r#","notification_type":"{t}""#));
            assert_eq!(signal(&body), Signal::AwaitingInput, "{t}");
        }
        // idle_prompt llega ~60 s después de terminar: es gris, no amarillo.
        assert_eq!(
            signal(&ev("Notification", r#","notification_type":"idle_prompt""#)),
            Signal::Stop
        );
        assert!(matches!(
            signal(&ev("Notification", r#","notification_type":"otra""#)),
            Signal::Unknown { .. }
        ));
    }

    #[test]
    fn eventos_no_suscritos_y_campos_nuevos_no_rompen() {
        assert!(matches!(
            signal(&ev("PreCompact", r#","campo_futuro":{"a":1}"#)),
            Signal::Unknown { name } if name == "PreCompact"
        ));
    }

    #[test]
    fn herramientas_sin_tool_use_id_se_ignoran_en_lugar_de_inventar_claves() {
        assert!(matches!(
            signal(&ev("PreToolUse", r#","tool_name":"Edit""#)),
            Signal::Unknown { .. }
        ));
    }

    #[test]
    fn archivo_de_sesion_real() {
        // Esquema observado en local (ADR 0005 §4); no está documentado.
        let body = r#"{ "pid": 46618, "sessionId": "abc-123", "cwd": "/Users/USER/proj",
          "startedAt": 1792886198259, "procStart": "x", "version": "2.1.310",
          "kind": "interactive", "entrypoint": "cli", "status": "busy", "updatedAt": 1792886999000 }"#;
        assert_eq!(
            parse_session_file(body.as_bytes()),
            Some(SessionFile {
                pid: Pid(46618),
                session: SessionId::new("abc-123"),
                activity: Some((SessionActivity::Busy, Timestamp(1_792_886_999_000))),
            })
        );
    }

    #[test]
    fn archivo_de_sesion_tolerante() {
        // Sin status o con uno desconocido: enlaza igual, sin cambiar el semáforo.
        let e = parse_session_file(br#"{"pid": 1, "sessionId": "s", "status": "raro"}"#).unwrap();
        assert_eq!(e.activity, None);
        assert_eq!(parse_session_file(br#"{"pid": 1}"#), None);
        assert_eq!(parse_session_file(b"basura"), None);
    }

    #[test]
    fn linea_de_transcript_con_tool_use() {
        // Forma observada en local: claves caller, id, input, name y type.
        let line = br#"{"type":"assistant","sessionId":"s1","timestamp":"t","message":{"role":"assistant","content":[{"type":"text","text":"secreto"},{"type":"tool_use","id":"toolu_1","name":"Bash","caller":"x","input":{"command":"rm -rf secreto"}},{"type":"tool_use","id":"toolu_2","name":"Edit","input":{}}]}}"#;
        assert_eq!(transcript_tool(line), Some(ToolName::new("Edit")));
    }

    #[test]
    fn lineas_de_transcript_sin_herramienta() {
        for line in [
            &br#"{"type":"user","message":{"content":"hola"}}"#[..],
            br#"{"type":"assistant","message":{"content":[{"type":"text","text":"x"}]}}"#,
            br#"{"type":"attachment","attachment":{}}"#,
            b"no es json",
            br#"{"type":"assistant","message":{"content":"texto suelto"}}"#,
        ] {
            assert_eq!(transcript_tool(line), None);
        }
    }

    #[test]
    fn carpeta_de_proyecto_como_la_codifica_claude() {
        // Nombres reales vistos en ~/.claude/projects.
        assert_eq!(
            project_dir_name("/Users/dev/Documents/source/kiti"),
            "-Users-dev-Documents-source-kiti"
        );
        assert_eq!(
            project_dir_name("/private/tmp/claude-501/-Users-x/scratchpad/hooktest-repo"),
            "-private-tmp-claude-501--Users-x-scratchpad-hooktest-repo"
        );
    }

    #[test]
    fn sabe_si_una_carpeta_tiene_conversaciones() {
        let projects = std::env::temp_dir().join(format!("kiti-projects-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&projects);
        std::fs::create_dir_all(projects.join("-w-con")).unwrap();
        std::fs::create_dir_all(projects.join("-w-vacia")).unwrap();
        std::fs::write(projects.join("-w-con/abc.jsonl"), "{}").unwrap();
        assert!(has_conversations(&projects, "/w/con"));
        assert!(!has_conversations(&projects, "/w/vacia"));
        assert!(!has_conversations(&projects, "/w/nunca"));
        std::fs::remove_dir_all(projects).unwrap();
    }

    fn activity(name: &str, extra: &str) -> Option<String> {
        parse(&ev(name, extra)).unwrap().info.activity
    }

    #[test]
    fn actividad_legible_segun_la_herramienta() {
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"Bash","tool_use_id":"t","tool_input":{"command":"npm test -- --token=SECRETO","description":"Ejecuta los tests"}"#
            ),
            Some("Ejecuta los tests".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"Bash","tool_use_id":"t","tool_input":{"command":"ls"}"#
            ),
            Some("Ejecutando un comando".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"Edit","tool_use_id":"t","tool_input":{"file_path":"/Users/dev/tienda/src/db/migrate.ts","old_string":"x"}"#
            ),
            Some("Editando migrate.ts".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"Read","tool_use_id":"t","tool_input":{"file_path":"/a/b/README.md"}"#
            ),
            Some("Leyendo README.md".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"Grep","tool_use_id":"t","tool_input":{"pattern":"password"}"#
            ),
            Some("Buscando en el código".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"Task","tool_use_id":"t","tool_input":{"subagent_type":"security-reviewer","description":"Revisar dependencias","prompt":"largo"}"#
            ),
            Some("Subagente security-reviewer: Revisar dependencias".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"TodoWrite","tool_use_id":"t","tool_input":{"todos":[{"content":"Auditar","status":"completed","activeForm":"Auditando"},{"content":"Migrar tabla users","status":"in_progress","activeForm":"Migrando la tabla users"}]}"#
            ),
            Some("Migrando la tabla users".into())
        );
        assert_eq!(
            activity(
                "PreToolUse",
                r#","tool_name":"mcp__x__y","tool_use_id":"t","tool_input":{}"#
            ),
            Some("mcp__x__y".into())
        );
    }

    #[test]
    fn el_comando_completo_nunca_sale_de_la_acl() {
        let e = parse(&ev(
            "PreToolUse",
            r#","tool_name":"Bash","tool_use_id":"t","tool_input":{"command":"curl -H 'Authorization: SECRETO'"}"#,
        ))
        .unwrap();
        assert!(!format!("{e:?}").contains("SECRETO"));
    }

    #[test]
    fn ultimo_pedido_recortado_y_titulo_de_sesion() {
        let long = "a".repeat(300);
        let e = parse(&ev(
            "UserPromptSubmit",
            &format!(r#","prompt":"Revisa   las\nvulnerabilidades {long}""#),
        ))
        .unwrap();
        let prompt = e.info.prompt.unwrap();
        assert!(
            prompt.starts_with("Revisa las vulnerabilidades a"),
            "{prompt}"
        );
        assert_eq!(prompt.chars().count(), 140);
        assert!(prompt.ends_with('…'));
        let e = parse(&ev(
            "SessionStart",
            r#","source":"startup","session_title":"Migración de datos""#,
        ))
        .unwrap();
        assert_eq!(e.info.title.as_deref(), Some("Migración de datos"));
    }

    #[test]
    fn errores_de_payload() {
        assert!(matches!(parse("no json"), Err(AclError::Json(_))));
        assert_eq!(
            parse(r#"{"cwd":"/w","hook_event_name":"Stop"}"#),
            Err(AclError::Missing("session_id"))
        );
        assert_eq!(
            parse(r#"{"session_id":"s","cwd":"/w"}"#),
            Err(AclError::Missing("hook_event_name"))
        );
    }

    #[test]
    fn el_transcript_da_titulo_y_ultimo_pedido() {
        let title = br#"{"type":"ai-title","aiTitle":"Revisar vulnerabilidades","sessionId":"s1"}"#;
        assert_eq!(
            transcript_info(title).title.as_deref(),
            Some("Revisar vulnerabilidades")
        );
        let prompt =
            br#"{"type":"last-prompt","lastPrompt":"migra   los datos\n de prueba","leafUuid":"u","sessionId":"s1"}"#;
        let info = transcript_info(prompt);
        assert_eq!(info.prompt.as_deref(), Some("migra los datos de prueba"));
        assert_eq!(info.title, None);
        // Un mensaje cualquiera que mencione esas palabras no cuenta.
        let user = br#"{"type":"user","message":{"content":"el \"ai-title\" es raro"}}"#;
        assert!(transcript_info(user).is_empty());
        assert!(transcript_info(b"no es json \"last-prompt\"").is_empty());
    }

    #[test]
    fn meta_de_una_conversacion_cerrada() {
        let lines: Vec<Vec<u8>> = [
            r#"{"type":"user","cwd":"/w/viejo","message":{"content":"hola"}}"#,
            r#"{"type":"ai-title","aiTitle":"Migrar datos","sessionId":"s1"}"#,
            r#"{"type":"user","cwd":"/w/tienda","message":{"content":"x"}}"#,
            r#"{"type":"last-prompt","lastPrompt":"pasa la tabla","sessionId":"s1"}"#,
            r#"{"type":"assistant","message":{"content":[]}}"#,
        ]
        .iter()
        .map(|l| l.as_bytes().to_vec())
        .collect();
        assert_eq!(
            conversation_meta(&lines),
            ConversationMeta {
                cwd: Some("/w/tienda".into()),
                title: Some("Migrar datos".into()),
                prompt: Some("pasa la tabla".into()),
            }
        );
        assert_eq!(conversation_meta(&[]), ConversationMeta::default());
    }

    #[test]
    fn solo_ids_de_conversacion_seguros_para_la_shell() {
        assert!(is_session_id("aeef3d7a-13ea-4240-b0f3-ded249409206"));
        assert!(!is_session_id("x'; rm -rf ~; '"));
        assert!(!is_session_id("a b c d e f"));
        assert!(!is_session_id("corto"));
        assert!(!is_session_id(&"a".repeat(65)));
    }

    #[test]
    fn la_cuota_sale_de_rate_limits_y_nada_mas() {
        let body = br#"{"session_id":"s","cwd":"/w/privado","model":{"id":"m"},"cost":{"total_cost_usd":3.2},
          "rate_limits":{"five_hour":{"used_percentage":28,"resets_at":1793572705},
                         "seven_day":{"used_percentage":60.5,"resets_at":"2026-11-01T22:38:25Z"}}}"#;
        let w = parse_rate_limits(body).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "5 h");
        assert!(!w[0].weekly);
        assert!((w[0].remaining - 0.72).abs() < 1e-9);
        assert_eq!(w[0].resets_at, 1_793_572_705_000);
        assert!(w[1].weekly);
        assert!((w[1].remaining - 0.395).abs() < 1e-9);
        assert_eq!(w[1].resets_at, 1_793_572_705_000);
        assert!(!format!("{w:?}").contains("privado"));
        // Sin suscripción o antes de la primera respuesta: no hay cuota.
        assert_eq!(parse_rate_limits(br#"{"session_id":"s"}"#), None);
        assert_eq!(parse_rate_limits(b"no json"), None);
    }
}
