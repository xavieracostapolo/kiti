//! Proyección del modelo de dominio a los DTOs de `kiti-protocol` (ADR 0002 §6, ADR 0004 §2).
//!
//! Trunca las cadenas a 256 caracteres antes de serializar hacia la UI.

use crate::domain::{Agent, Provider, RegistryDiff, Status, StatusKind, StatusReason};
use kiti_protocol as proto;

const MAX_STRING_LEN: usize = 256;

fn truncate_str(s: &str) -> String {
    if s.len() <= MAX_STRING_LEN {
        s.to_string()
    } else {
        // Truncar respetando límites de caracteres UTF-8
        let mut end = MAX_STRING_LEN;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

pub fn project_provider(p: Provider) -> proto::Provider {
    match p {
        Provider::Claude => proto::Provider::Claude,
        Provider::Codex => proto::Provider::Codex,
        Provider::Gemini => proto::Provider::Gemini,
    }
}

pub fn project_status_kind(k: StatusKind) -> proto::StatusKind {
    match k {
        StatusKind::Working => proto::StatusKind::Working,
        StatusKind::Waiting => proto::StatusKind::Waiting,
        StatusKind::Idle => proto::StatusKind::Idle,
        StatusKind::Error => proto::StatusKind::Error,
        StatusKind::Dead => proto::StatusKind::Dead,
    }
}

pub fn project_status_reason(r: StatusReason) -> proto::StatusReason {
    match r {
        StatusReason::Hook => proto::StatusReason::Hook,
        StatusReason::SessionFile => proto::StatusReason::SessionFile,
        StatusReason::Stale => proto::StatusReason::Stale,
        StatusReason::Failure => proto::StatusReason::Failure,
        StatusReason::ProcessGone => proto::StatusReason::ProcessGone,
        StatusReason::Startup => proto::StatusReason::Startup,
    }
}

pub fn project_status(s: Status) -> proto::Status {
    proto::Status {
        kind: project_status_kind(s.kind),
        reason: project_status_reason(s.reason),
        since: s.since.0,
    }
}

pub fn project_agent(a: &Agent) -> proto::AgentState {
    proto::AgentState {
        id: proto::AgentId(a.id().to_string()),
        provider: project_provider(a.id().provider),
        pid: a.identity().pid.0,
        cwd: truncate_str(a.cwd().as_str()),
        started_at: a.identity().start_time.0,
        status: project_status(a.status()),
        last_tool: a.last_tool().map(|t| truncate_str(t.as_str())),
        last_event_at: a.last_event_at().0,
    }
}

pub fn project_snapshot(seq: u64, agents: &[Agent]) -> proto::ServerMsg {
    proto::ServerMsg::Snapshot {
        seq,
        agents: agents.iter().map(project_agent).collect(),
    }
}

pub fn project_diff(seq: u64, ingest_ms: u64, diff: RegistryDiff) -> proto::ServerMsg {
    proto::ServerMsg::Diff {
        seq,
        ingest_ms,
        upserts: diff.upserts.iter().map(project_agent).collect(),
        removed: diff
            .removed
            .into_iter()
            .map(|id| proto::AgentId(id.to_string()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AgentId, DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Signal, Timestamp,
        ToolName, ToolUseKey, WorkingDir,
    };

    fn sample_agent() -> Agent {
        let mut a = Agent::new(
            DetectedAgent {
                provider: Provider::Claude,
                identity: ProcessIdentity {
                    pid: Pid(42),
                    start_time: ProcessStartTime(1_700_000_000),
                    uid: 501,
                    cmd_fingerprint: 10,
                },
                cwd: WorkingDir::new("/Users/test/workspace"),
            },
            Timestamp(1_700_000_005),
        );
        a.apply(
            &crate::domain::AgentInput::Signal(Signal::ToolStart {
                key: ToolUseKey::new("k1"),
                tool: ToolName::new("Bash"),
            }),
            Timestamp(1_700_000_010),
        );
        a
    }

    #[test]
    fn project_agent_trunca_cadenas_largas() {
        let long_cwd = "a".repeat(300);
        let a = Agent::new(
            DetectedAgent {
                provider: Provider::Codex,
                identity: ProcessIdentity {
                    pid: Pid(10),
                    start_time: ProcessStartTime(100),
                    uid: 501,
                    cmd_fingerprint: 0,
                },
                cwd: WorkingDir::new(long_cwd),
            },
            Timestamp(200),
        );
        let dto = project_agent(&a);
        assert_eq!(dto.cwd.len(), 256);
    }

    #[test]
    fn project_diff_y_snapshot_generan_server_msg_valido() {
        let a = sample_agent();
        let snap = project_snapshot(0, std::slice::from_ref(&a));
        match snap {
            proto::ServerMsg::Snapshot { seq, agents } => {
                assert_eq!(seq, 0);
                assert_eq!(agents.len(), 1);
                assert_eq!(agents[0].id.0, a.id().to_string());
                assert_eq!(agents[0].last_tool.as_deref(), Some("Bash"));
            }
            _ => panic!("esperaba Snapshot"),
        }

        let diff = RegistryDiff {
            upserts: vec![a.clone()],
            removed: vec![AgentId {
                provider: Provider::Gemini,
                pid: Pid(99),
                start_time: ProcessStartTime(50),
            }],
        };
        let diff_msg = project_diff(1, 1_700_000_100, diff);
        match diff_msg {
            proto::ServerMsg::Diff {
                seq,
                ingest_ms,
                upserts,
                removed,
            } => {
                assert_eq!(seq, 1);
                assert_eq!(ingest_ms, 1_700_000_100);
                assert_eq!(upserts.len(), 1);
                assert_eq!(removed.len(), 1);
                assert_eq!(removed[0].0, "gemini:99:50");
            }
            _ => panic!("esperaba Diff"),
        }
    }
}
