//! Hooks de Codex CLI → `Signal` (ADR 0005 §2 y §5). Llegan por `kiti-hook codex`.

use serde::Deserialize;

pub use super::{AclError, HookEvent};
use crate::domain::{SessionId, Signal, ToolName, ToolOutcome, ToolUseKey};

/// Lista blanca de campos: `prompt` y `tool_input` no se leen.
#[derive(Deserialize)]
struct Payload {
    session_id: Option<String>,
    cwd: Option<String>,
    hook_event_name: Option<String>,
    tool_name: Option<String>,
    tool_use_id: Option<String>,
}

fn to_signal(name: &str, p: &Payload) -> Option<Signal> {
    Some(match name {
        "SessionStart" => Signal::SessionStart,
        "UserPromptSubmit" => Signal::PromptSubmit,
        "PreToolUse" => Signal::ToolStart {
            key: ToolUseKey::new(p.tool_use_id.clone()?),
            tool: ToolName::new(p.tool_name.clone()?),
        },
        "PostToolUse" => Signal::ToolEnd {
            key: ToolUseKey::new(p.tool_use_id.clone()?),
            outcome: ToolOutcome::Ok,
        },
        "PermissionRequest" => Signal::AwaitingInput,
        // `Interrupt` es la interrupción del usuario: como Stop, sin error.
        "Stop" | "Interrupt" => Signal::Stop,
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
        info: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Signal, ToolName, ToolOutcome, ToolUseKey};

    fn signal(name: &str, extra: &str) -> Signal {
        let body = format!(
            r#"{{"session_id":"thr_1","cwd":"/w","hook_event_name":"{name}","model":"gpt"{extra}}}"#
        );
        parse_hook(body.as_bytes()).unwrap().signal
    }

    #[test]
    fn payload_documentado_de_pre_tool_use() {
        let body = r#"{ "session_id": "thr_123", "transcript_path": null,
          "cwd": "/Users/USER/proj", "hook_event_name": "PreToolUse", "model": "gpt-x",
          "turn_id": "turn_1", "permission_mode": "default",
          "tool_name": "Bash", "tool_use_id": "call_9", "tool_input": { "command": "ls" } }"#;
        let e = parse_hook(body.as_bytes()).unwrap();
        assert_eq!(e.session.as_str(), "thr_123");
        assert_eq!(e.cwd, "/Users/USER/proj");
        assert_eq!(
            e.signal,
            Signal::ToolStart {
                key: ToolUseKey::new("call_9"),
                tool: ToolName::new("Bash"),
            }
        );
    }

    #[test]
    fn mapeo_de_eventos() {
        assert_eq!(signal("SessionStart", ""), Signal::SessionStart);
        assert_eq!(
            signal("UserPromptSubmit", r#","prompt":"x""#),
            Signal::PromptSubmit
        );
        assert_eq!(
            signal("PostToolUse", r#","tool_use_id":"c1""#),
            Signal::ToolEnd {
                key: ToolUseKey::new("c1"),
                outcome: ToolOutcome::Ok,
            }
        );
        assert_eq!(signal("PermissionRequest", ""), Signal::AwaitingInput);
        assert_eq!(signal("Stop", ""), Signal::Stop);
        // Interrupt es el Esc de Codex: deja la oficina en gris.
        assert_eq!(signal("Interrupt", ""), Signal::Stop);
        assert_eq!(signal("SessionEnd", ""), Signal::SessionEnd);
        assert!(matches!(signal("PreCompact", ""), Signal::Unknown { .. }));
    }
}
