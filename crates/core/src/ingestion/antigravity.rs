//! Hooks de Antigravity CLI → `Signal` (ADR 0014). Llegan por `kiti-hook antigravity <Evento>`:
//! su JSON no dice qué evento es, así que el forwarder lo manda en `X-Kiti-Event`.
//!
//! Antigravity no avisa cuando espera un permiso: eso lo detecta `AntigravityWatcher` leyendo su
//! base de datos de pasos. Aquí solo se traduce lo que llega por hook.

use serde::Deserialize;

use super::clip;
pub use super::{AclError, HookEvent};
use crate::domain::{AgentInfo, SessionId, Signal, ToolName, ToolOutcome, ToolUseKey};

/// Mismo recorte que en Claude (ADR 0009).
const MAX_TEXT: usize = 140;

/// Lista blanca: de `args` solo el resumen que escribe el modelo y la ruta del archivo; nunca el
/// comando ni el contenido.
#[derive(Deserialize, Default)]
struct ToolArgs {
    #[serde(rename = "toolSummary")]
    tool_summary: Option<String>,
    #[serde(rename = "Path")]
    path: Option<String>,
    #[serde(rename = "TargetFile")]
    target_file: Option<String>,
    #[serde(rename = "AbsolutePath")]
    absolute_path: Option<String>,
}

#[derive(Deserialize)]
struct ToolCall {
    name: Option<String>,
    #[serde(default)]
    args: ToolArgs,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    conversation_id: Option<String>,
    #[serde(default)]
    workspace_paths: Vec<String>,
    tool_call: Option<ToolCall>,
    step_idx: Option<u64>,
    error: Option<String>,
    invocation_num: Option<u64>,
    fully_idle: Option<bool>,
    last_user_input: Option<String>,
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Qué hace, en palabras: el resumen del modelo si lo hay; si no, según la herramienta.
fn activity(tool: &str, args: &ToolArgs) -> String {
    if let Some(summary) = args
        .tool_summary
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        return clip(summary, MAX_TEXT);
    }
    let file = args
        .path
        .as_deref()
        .or(args.target_file.as_deref())
        .or(args.absolute_path.as_deref())
        .map(file_name);
    let text = match (tool, file) {
        (t, Some(f)) if t.contains("write") || t.contains("replace") || t.contains("edit") => {
            format!("Editando {f}")
        }
        (t, Some(f)) if t.contains("view") || t.contains("read") => format!("Leyendo {f}"),
        ("run_command", _) => "Ejecutando un comando".into(),
        (t, _) if t.contains("grep") || t.contains("find") || t.contains("search_code") => {
            "Buscando en el código".into()
        }
        (t, _) if t.contains("web") || t.contains("url") => "Buscando en la web".into(),
        _ => tool.to_owned(),
    };
    clip(&text, MAX_TEXT)
}

fn tool_key(conversation: &str, step: Option<u64>, tool: &str) -> ToolUseKey {
    match step {
        Some(step) => ToolUseKey::new(format!("agy:{conversation}:{step}")),
        None => ToolUseKey::new(format!("agy:{conversation}:{tool}")),
    }
}

/// `event`: el nombre que puso el forwarder (`PreToolUse`, `Stop`…).
pub fn parse_hook(body: &[u8], event: Option<&str>) -> Result<HookEvent, AclError> {
    let name = event.ok_or(AclError::Missing("X-Kiti-Event"))?.to_owned();
    let p: Payload = serde_json::from_slice(body).map_err(|e| AclError::Json(e.to_string()))?;
    let conversation = p
        .conversation_id
        .clone()
        .ok_or(AclError::Missing("conversationId"))?;
    let tool = p.tool_call.as_ref().and_then(|t| t.name.clone());
    let failed = p.error.as_deref().is_some_and(|e| !e.is_empty());
    let signal = match (name.as_str(), &tool) {
        ("SessionStart", _) => Signal::SessionStart,
        // Cada turno empieza con la invocación 0; las siguientes son del mismo turno.
        ("PreInvocation", _) if p.invocation_num == Some(0) => Signal::PromptSubmit,
        ("PreToolUse", Some(t)) => Signal::ToolStart {
            key: tool_key(&conversation, p.step_idx, t),
            tool: ToolName::new(t.clone()),
        },
        ("PostToolUse", Some(t)) => Signal::ToolEnd {
            key: tool_key(&conversation, p.step_idx, t),
            outcome: if failed {
                ToolOutcome::Failed
            } else {
                ToolOutcome::Ok
            },
        },
        ("Stop", _) if failed => Signal::Failure,
        ("Stop", _) if p.fully_idle != Some(false) => Signal::Stop,
        _ => Signal::Unknown { name: name.clone() },
    };
    let info = AgentInfo {
        title: None,
        activity: match (name.as_str(), &p.tool_call) {
            ("PreToolUse", Some(call)) => call.name.as_deref().map(|t| activity(t, &call.args)),
            _ => None,
        },
        prompt: match name.as_str() {
            "SessionStart" | "PreInvocation" => p
                .last_user_input
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .map(|t| clip(t, MAX_TEXT)),
            _ => None,
        },
    };
    Ok(HookEvent {
        session: SessionId::new(conversation),
        cwd: p.workspace_paths.first().cloned().unwrap_or_default(),
        name,
        signal,
        info,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Con la forma de las capturas reales (ADR 0014), con datos inventados.
    fn body(extra: &str) -> String {
        format!(
            r#"{{"conversationId":"c-1","workspacePaths":["/w/tienda"],"transcriptPath":"/t.jsonl","artifactDirectoryPath":"/a","modelName":"m","lastUserInput":"migra la tabla"{extra}}}"#
        )
    }

    fn parse(event: &str, extra: &str) -> HookEvent {
        parse_hook(body(extra).as_bytes(), Some(event)).unwrap()
    }

    #[test]
    fn sesion_carpeta_y_turno() {
        let e = parse("SessionStart", "");
        assert_eq!(e.session.as_str(), "c-1");
        assert_eq!(e.cwd, "/w/tienda");
        assert_eq!(e.signal, Signal::SessionStart);
        assert_eq!(e.info.prompt.as_deref(), Some("migra la tabla"));
        let first = parse("PreInvocation", r#","invocationNum":0,"initialNumSteps":1"#);
        assert_eq!(first.signal, Signal::PromptSubmit);
        // Las demás invocaciones son del mismo turno: no cuentan como pedido nuevo.
        let next = parse("PreInvocation", r#","invocationNum":1,"initialNumSteps":3"#);
        assert!(matches!(next.signal, Signal::Unknown { .. }));
        assert!(matches!(
            parse("PostInvocation", r#","invocationNum":0"#).signal,
            Signal::Unknown { .. }
        ));
    }

    #[test]
    fn herramientas_por_paso_con_actividad_y_sin_el_comando() {
        let pre = parse(
            "PreToolUse",
            r#","stepIdx":2,"toolCall":{"name":"run_command","args":{"CommandLine":"rm -rf secreto","toolSummary":"Comprobar la fecha","WaitMsBeforeAsync":5000}}"#,
        );
        assert_eq!(
            pre.signal,
            Signal::ToolStart {
                key: ToolUseKey::new("agy:c-1:2"),
                tool: ToolName::new("run_command"),
            }
        );
        assert_eq!(pre.info.activity.as_deref(), Some("Comprobar la fecha"));
        assert!(!format!("{pre:?}").contains("secreto"));
        let write = parse(
            "PreToolUse",
            r#","stepIdx":3,"toolCall":{"name":"write_file","args":{"Path":"/w/tienda/db/schema.sql","Content":"x","Overwrite":true}}"#,
        );
        assert_eq!(write.info.activity.as_deref(), Some("Editando schema.sql"));
        let ok = parse(
            "PostToolUse",
            r#","stepIdx":2,"error":"","toolCall":{"name":"run_command","args":{}}"#,
        );
        assert_eq!(
            ok.signal,
            Signal::ToolEnd {
                key: ToolUseKey::new("agy:c-1:2"),
                outcome: ToolOutcome::Ok,
            }
        );
        let failed = parse(
            "PostToolUse",
            r#","stepIdx":3,"error":"denied","toolCall":{"name":"write_file","args":{}}"#,
        );
        assert!(matches!(
            failed.signal,
            Signal::ToolEnd {
                outcome: ToolOutcome::Failed,
                ..
            }
        ));
    }

    #[test]
    fn fin_de_turno_y_errores() {
        let stop = parse(
            "Stop",
            r#","executionNum":0,"terminationReason":"NO_TOOL_CALL","fullyIdle":true,"error":"""#,
        );
        assert_eq!(stop.signal, Signal::Stop);
        let failed = parse("Stop", r#","fullyIdle":true,"error":"quota exceeded""#);
        assert_eq!(failed.signal, Signal::Failure);
        assert!(parse_hook(body("").as_bytes(), None).is_err());
        assert!(parse_hook(br#"{"workspacePaths":[]}"#, Some("Stop")).is_err());
        assert!(parse_hook(b"no json", Some("Stop")).is_err());
    }
}
