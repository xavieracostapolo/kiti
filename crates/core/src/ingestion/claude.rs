//! Hooks de Claude Code → `Signal` (ADR 0005 §1 y §5).

use serde::Deserialize;

pub use super::{AclError, HookEvent};
use crate::domain::{
    Pid, SessionActivity, SessionId, Signal, Timestamp, ToolName, ToolOutcome, ToolUseKey,
};

/// Lista blanca de campos (ADR 0005 §7): `prompt`, `tool_input` y `tool_response` no se leen.
#[derive(Deserialize)]
struct Payload {
    session_id: Option<String>,
    cwd: Option<String>,
    hook_event_name: Option<String>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
    notification_type: Option<String>,
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
    Ok(HookEvent {
        session: SessionId::new(session),
        cwd: p.cwd.unwrap_or_default(),
        name,
        signal,
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
}
