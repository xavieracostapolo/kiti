//! Avisos (ADR 0010): del hook al `AlertSink`, pasando por la política del dominio.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use kiti_core::application::{Runtime, Telemetry, alert_text};
use kiti_core::domain::{
    AgentInfo, AlertKind, DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider,
    SessionId, Signal, Timestamp, WorkingDir,
};
use kiti_core::ingestion::HookEvent;
use kiti_core::ports::{AgentKiller, AlertNotice, AlertSink, KillOutcome, KillTarget};
use kiti_core::testing::ManualClock;
use kiti_protocol::Source;

#[derive(Clone)]
struct NoKill;

impl AgentKiller for NoKill {
    async fn terminate(
        &self,
        _: KillTarget,
        _: Duration,
    ) -> Result<KillOutcome, kiti_core::ports::KillError> {
        Ok(KillOutcome::Killed)
    }
}

#[derive(Clone, Default)]
struct Collect(Arc<Mutex<Vec<AlertNotice>>>);

impl AlertSink for Collect {
    fn alert(&mut self, notice: AlertNotice) {
        self.0.lock().unwrap().push(notice);
    }
}

#[tokio::test]
async fn un_permiso_llega_al_sink_con_proyecto_y_titulo() {
    let sink = Collect::default();
    let (runtime, handle) = Runtime::new(
        NoKill,
        ManualClock::new(0),
        Arc::new(Telemetry::new()),
        Source::Live,
        "test",
    );
    tokio::spawn(runtime.with_alerts(sink.clone()).run());
    let agent = DetectedAgent {
        provider: Provider::Claude,
        identity: ProcessIdentity {
            pid: Pid(7),
            start_time: ProcessStartTime(1),
            uid: 501,
            cmd_fingerprint: 1,
        },
        cwd: WorkingDir::new("/Users/dev/thoth-assets"),
    };
    handle
        .reconcile(vec![agent], vec![], vec![], Timestamp(0))
        .await;
    let event = |signal, info| HookEvent {
        session: SessionId::new("s1"),
        cwd: "/Users/dev/thoth-assets".into(),
        name: "test".into(),
        signal,
        info,
    };
    let title = AgentInfo {
        title: Some("Revisar vulnerabilidades".into()),
        ..AgentInfo::default()
    };
    handle
        .hook(
            Provider::Claude,
            event(Signal::PromptSubmit, title),
            Timestamp(1_000),
        )
        .await;
    handle
        .hook(
            Provider::Claude,
            event(Signal::AwaitingInput, AgentInfo::default()),
            Timestamp(2_000),
        )
        .await;

    let mut notices = Vec::new();
    for _ in 0..50 {
        notices = sink.0.lock().unwrap().clone();
        if !notices.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(notices.len(), 1, "{notices:?}");
    let n = &notices[0];
    assert_eq!(n.kind, AlertKind::Permission);
    let (head, body) = alert_text(n);
    assert_eq!(head, "Claude pide permiso");
    assert_eq!(body, "thoth-assets · Revisar vulnerabilidades");
}
