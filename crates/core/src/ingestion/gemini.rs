//! Hooks de Gemini CLI → `Signal` (ADR 0005 §3 y §5). Llegan por `kiti-hook gemini`.
//!
//! Gemini no envía `tool_use_id`: `BeforeTool` y `AfterTool` se emparejan por nombre de
//! herramienta, que basta mientras no corran dos llamadas iguales a la vez.

use serde::Deserialize;
use serde::de::IgnoredAny;

pub use super::{AclError, HookEvent};
use crate::domain::{SessionId, Signal, ToolName, ToolOutcome, ToolUseKey};

/// Solo importa si `tool_response` trae `error`; su contenido no se lee.
#[derive(Deserialize)]
struct ToolResponse {
    error: Option<IgnoredAny>,
}

/// Lista blanca de campos: `prompt`, `prompt_response` y `tool_input` no se leen.
#[derive(Deserialize)]
struct Payload {
    session_id: Option<String>,
    cwd: Option<String>,
    hook_event_name: Option<String>,
    tool_name: Option<String>,
    tool_response: Option<ToolResponse>,
    notification_type: Option<String>,
}

fn tool_key(tool: &str) -> ToolUseKey {
    ToolUseKey::new(format!("gemini:{tool}"))
}

fn to_signal(name: &str, p: &Payload) -> Option<Signal> {
    Some(match name {
        "SessionStart" => Signal::SessionStart,
        "BeforeAgent" => Signal::PromptSubmit,
        "AfterAgent" => Signal::Stop,
        "BeforeTool" => {
            let tool = p.tool_name.clone()?;
            Signal::ToolStart {
                key: tool_key(&tool),
                tool: ToolName::new(tool),
            }
        }
        "AfterTool" => Signal::ToolEnd {
            key: tool_key(p.tool_name.as_deref()?),
            outcome: if p.tool_response.as_ref().is_some_and(|r| r.error.is_some()) {
                ToolOutcome::Failed
            } else {
                ToolOutcome::Ok
            },
        },
        "Notification" if p.notification_type.as_deref() == Some("ToolPermission") => {
            Signal::AwaitingInput
        }
        "SessionEnd" => Signal::SessionEnd,
        _ => return None,
    })
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
    use crate::domain::{Signal, ToolName, ToolOutcome, ToolUseKey};

    fn signal(name: &str, extra: &str) -> Signal {
        let body = format!(
            r#"{{"session_id":"g1","cwd":"/w","hook_event_name":"{name}","timestamp":"t"{extra}}}"#
        );
        parse_hook(body.as_bytes()).unwrap().signal
    }

    #[test]
    fn mapeo_de_eventos() {
        assert_eq!(
            signal("SessionStart", r#","source":"startup""#),
            Signal::SessionStart
        );
        assert_eq!(
            signal("BeforeAgent", r#","prompt":"x""#),
            Signal::PromptSubmit
        );
        assert_eq!(
            signal("AfterAgent", r#","prompt_response":"y""#),
            Signal::Stop
        );
        assert_eq!(
            signal(
                "BeforeTool",
                r#","tool_name":"run_shell_command","tool_input":{}"#
            ),
            Signal::ToolStart {
                key: ToolUseKey::new("gemini:run_shell_command"),
                tool: ToolName::new("run_shell_command"),
            }
        );
        assert_eq!(
            signal(
                "AfterTool",
                r#","tool_name":"run_shell_command","tool_response":{}"#
            ),
            Signal::ToolEnd {
                key: ToolUseKey::new("gemini:run_shell_command"),
                outcome: ToolOutcome::Ok,
            }
        );
        assert_eq!(
            signal(
                "AfterTool",
                r#","tool_name":"replace","tool_response":{"error":"boom"}"#
            ),
            Signal::ToolEnd {
                key: ToolUseKey::new("gemini:replace"),
                outcome: ToolOutcome::Failed,
            }
        );
        assert_eq!(
            signal(
                "Notification",
                r#","notification_type":"ToolPermission","message":"m""#
            ),
            Signal::AwaitingInput
        );
        assert_eq!(
            signal("SessionEnd", r#","reason":"exit""#),
            Signal::SessionEnd
        );
    }

    #[test]
    fn eventos_con_el_prompt_completo_no_se_usan() {
        for name in [
            "BeforeModel",
            "AfterModel",
            "BeforeToolSelection",
            "PreCompress",
        ] {
            assert!(matches!(signal(name, ""), Signal::Unknown { .. }), "{name}");
        }
    }
}
