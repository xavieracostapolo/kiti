//! Propiedades del reducer (ADR 0002 §8) sobre secuencias arbitrarias de entradas.

use std::time::Duration;

use kiti_core::domain::{
    Agent, AgentInput, DetectedAgent, DomainEvent, Pid, ProcessIdentity, ProcessStartTime,
    Provider, Signal, StalePolicy, StatusKind, Timestamp, ToolName, ToolOutcome, ToolUseKey,
    WorkingDir,
};
use proptest::prelude::*;

fn new_agent() -> Agent {
    Agent::new(
        DetectedAgent {
            provider: Provider::Gemini,
            identity: ProcessIdentity {
                pid: Pid(77),
                start_time: ProcessStartTime(0),
                uid: 501,
                cmd_fingerprint: 0,
            },
            cwd: WorkingDir::new("/Users/dev/x"),
        },
        Timestamp(0),
    )
}

/// Pocas claves, para que ToolStart y ToolEnd coincidan a menudo.
fn key() -> impl Strategy<Value = ToolUseKey> {
    (0u8..4).prop_map(|k| ToolUseKey::new(format!("k{k}")))
}

fn outcome() -> impl Strategy<Value = ToolOutcome> {
    prop_oneof![
        Just(ToolOutcome::Ok),
        Just(ToolOutcome::Failed),
        Just(ToolOutcome::Denied)
    ]
}

fn signal() -> impl Strategy<Value = Signal> {
    prop_oneof![
        1 => Just(Signal::SessionStart),
        3 => Just(Signal::PromptSubmit),
        4 => (key(), "[A-Z][a-z]{2,6}").prop_map(|(key, t)| Signal::ToolStart { key, tool: ToolName::new(t) }),
        4 => (key(), outcome()).prop_map(|(key, outcome)| Signal::ToolEnd { key, outcome }),
        2 => Just(Signal::AwaitingInput),
        2 => Just(Signal::Stop),
        1 => Just(Signal::Failure),
        1 => Just(Signal::SessionEnd),
        1 => Just(Signal::Unknown { name: "Foo".into() }),
    ]
}

fn input() -> impl Strategy<Value = AgentInput> {
    prop_oneof![
        20 => signal().prop_map(AgentInput::Signal),
        1 => Just(AgentInput::ProcessGone),
        1 => Just(AgentInput::KillRequested),
        3 => Just(AgentInput::Tick(StalePolicy::default())),
    ]
}

/// Entradas con un avance de tiempo de 0 a 40 s entre una y otra.
fn script() -> impl Strategy<Value = Vec<(u64, AgentInput)>> {
    prop::collection::vec((0u64..40_000, input()), 0..300)
}

fn run(
    script: &[(u64, AgentInput)],
    mut check: impl FnMut(&Agent, &AgentInput, &Agent, &[DomainEvent]),
) -> Vec<DomainEvent> {
    let mut agent = new_agent();
    let mut now = 0;
    let mut all = Vec::new();
    for (dt, input) in script {
        now += dt;
        let before = agent.clone();
        let ev = agent.apply(input, Timestamp(now));
        check(&before, input, &agent, &ev);
        all.extend(ev.iter().cloned());
        if ev
            .iter()
            .any(|e| matches!(e, DomainEvent::AgentRemoved { .. }))
        {
            break;
        }
    }
    all
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn determinista(s in script()) {
        prop_assert_eq!(run(&s, |_, _, _, _| {}), run(&s, |_, _, _, _| {}));
    }

    #[test]
    fn dead_solo_con_process_gone_inesperado(s in script()) {
        run(&s, |before, input, after, _| {
            if after.is_dead() && !before.is_dead() {
                assert_eq!(input, &AgentInput::ProcessGone);
            }
        });
    }

    #[test]
    fn dead_es_absorbente(s in script()) {
        run(&s, |before, _, after, _| {
            if before.is_dead() {
                assert!(after.is_dead());
                assert_eq!(after.revision(), before.revision());
            }
        });
    }

    #[test]
    fn un_fallo_de_herramienta_nunca_pone_rojo(s in script()) {
        run(&s, |before, input, after, _| {
            if let AgentInput::Signal(Signal::ToolEnd { .. }) = input
                && before.status().kind != StatusKind::Error
            {
                assert_ne!(after.status().kind, StatusKind::Error);
            }
        });
    }

    #[test]
    fn stop_deja_gris_y_sin_herramientas(s in script()) {
        run(&s, |before, input, after, _| {
            if input == &AgentInput::Signal(Signal::Stop) && !before.is_dead() {
                assert_eq!(after.pending_tools().len(), 0);
                if matches!(before.status().kind, StatusKind::Working | StatusKind::Waiting) {
                    assert_eq!(after.status().kind, StatusKind::Idle);
                }
            }
        });
    }

    #[test]
    fn solo_hay_herramientas_abiertas_en_verde_o_amarillo(s in script()) {
        run(&s, |_, _, after, _| {
            if after.pending_tools().len() > 0 {
                assert!(matches!(after.status().kind, StatusKind::Working | StatusKind::Waiting));
            }
        });
    }

    #[test]
    fn since_nunca_va_hacia_atras_y_solo_cambia_con_el_color(s in script()) {
        run(&s, |before, _, after, _| {
            assert!(after.status().since >= before.status().since);
            if after.status().kind == before.status().kind {
                assert_eq!(after.status().since, before.status().since);
            }
        });
    }

    #[test]
    fn waiting_nunca_caduca(hours in 1u64..1000) {
        let mut a = new_agent();
        a.apply(&AgentInput::Signal(Signal::PromptSubmit), Timestamp(0));
        a.apply(&AgentInput::Signal(Signal::AwaitingInput), Timestamp(1));
        a.apply(&AgentInput::Tick(StalePolicy::default()), Timestamp(0).plus(Duration::from_secs(hours * 3600)));
        prop_assert_eq!(a.status().kind, StatusKind::Waiting);
    }
}
