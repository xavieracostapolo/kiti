//! El reducer real contra `fixtures/sim/conformance/*.json`, los mismos casos que pasa el
//! simulador TS de la UI (ADR 0006 §9).

use std::fs;
use std::path::PathBuf;

use kiti_core::domain::{
    Agent, AgentInput, DetectedAgent, DomainEvent, Pid, ProcessIdentity, ProcessStartTime,
    Provider, Signal, StatusKind, StatusReason, Timestamp, ToolName, ToolOutcome, ToolUseKey,
    WorkingDir,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    steps: Vec<Step>,
    #[serde(default)]
    then: Option<Vec<Step>>,
}

#[derive(Deserialize)]
struct Step {
    signal: FixtureSignal,
    expect: Expect,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum FixtureSignal {
    SessionStarted,
    PromptSubmitted,
    ToolStarted {
        tool: String,
    },
    ToolFinished {
        #[serde(default)]
        failed: bool,
    },
    AwaitingInput,
    Stopped,
    Error,
    SessionEnded,
    ProcessGone,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Expect {
    Removed(Removed),
    State {
        kind: String,
        reason: String,
        #[serde(rename = "lastTool")]
        last_tool: Option<String>,
    },
}

#[derive(Deserialize)]
enum Removed {
    #[serde(rename = "removed")]
    Removed,
}

fn kind_name(k: StatusKind) -> &'static str {
    match k {
        StatusKind::Working => "working",
        StatusKind::Waiting => "waiting",
        StatusKind::Idle => "idle",
        StatusKind::Error => "error",
        StatusKind::Dead => "dead",
    }
}

fn reason_name(r: StatusReason) -> &'static str {
    match r {
        StatusReason::Hook => "hook",
        StatusReason::SessionFile => "sessionFile",
        StatusReason::Stale => "stale",
        StatusReason::Failure => "failure",
        StatusReason::ProcessGone => "processGone",
        StatusReason::Startup => "startup",
    }
}

fn new_agent() -> Agent {
    Agent::new(
        DetectedAgent {
            provider: Provider::Claude,
            identity: ProcessIdentity {
                pid: Pid(1),
                start_time: ProcessStartTime(0),
                uid: 501,
                cmd_fingerprint: 0,
            },
            cwd: WorkingDir::new("/Users/dev/kiti"),
        },
        Timestamp(0),
    )
}

/// Las fixtures no llevan claves de herramienta: `ToolFinished` cierra la última abierta.
fn run(case: &str, steps: &[Step]) {
    let mut agent = new_agent();
    let mut open: Vec<ToolUseKey> = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let input = match &step.signal {
            FixtureSignal::SessionStarted => AgentInput::Signal(Signal::SessionStart),
            FixtureSignal::PromptSubmitted => AgentInput::Signal(Signal::PromptSubmit),
            FixtureSignal::ToolStarted { tool } => {
                let key = ToolUseKey::new(format!("k{i}"));
                open.push(key.clone());
                AgentInput::Signal(Signal::ToolStart {
                    key,
                    tool: ToolName::new(tool),
                })
            }
            FixtureSignal::ToolFinished { failed } => AgentInput::Signal(Signal::ToolEnd {
                key: open.pop().unwrap_or_else(|| ToolUseKey::new("huerfana")),
                outcome: if *failed {
                    ToolOutcome::Failed
                } else {
                    ToolOutcome::Ok
                },
            }),
            FixtureSignal::AwaitingInput => AgentInput::Signal(Signal::AwaitingInput),
            FixtureSignal::Stopped => AgentInput::Signal(Signal::Stop),
            FixtureSignal::Error => AgentInput::Signal(Signal::Failure),
            FixtureSignal::SessionEnded => AgentInput::Signal(Signal::SessionEnd),
            FixtureSignal::ProcessGone => AgentInput::ProcessGone,
        };
        let events = agent.apply(&input, Timestamp(1000 + i as u64));
        let was_removed = events
            .iter()
            .any(|e| matches!(e, DomainEvent::AgentRemoved { .. }));
        match &step.expect {
            Expect::Removed(_) => assert!(was_removed, "{case}, paso {i}: esperaba removed"),
            Expect::State {
                kind,
                reason,
                last_tool,
            } => {
                assert!(!was_removed, "{case}, paso {i}: no esperaba removed");
                let s = agent.status();
                assert_eq!(
                    (
                        kind_name(s.kind),
                        reason_name(s.reason),
                        agent.last_tool().map(ToolName::as_str)
                    ),
                    (kind.as_str(), reason.as_str(), last_tool.as_deref()),
                    "{case}, paso {i}"
                );
            }
        }
    }
}

#[test]
fn el_reducer_pasa_todas_las_fixtures_de_conformidad() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sim/conformance");
    let mut count = 0;
    for entry in fs::read_dir(&dir).expect("fixtures/sim/conformance") {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let case: Case = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        run(&case.name, &case.steps);
        if let Some(then) = &case.then {
            run(&case.name, then);
        }
        count += 1;
    }
    assert!(count >= 5, "solo {count} fixtures");
}
