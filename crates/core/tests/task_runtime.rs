//! Agentes efímeros a través del runtime (ADR 0019): `kiti_spawn`, `kiti_done`, topes y cierre,
//! con un lanzador falso. Ningún test lanza un CLI de verdad.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kiti_core::application::runtime::MailAction;
use kiti_core::application::{Runtime, RuntimeHandle, Telemetry};
use kiti_core::domain::{
    DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider, Task, TaskState, Timestamp,
    WorkingDir,
};
use kiti_core::ports::{
    AgentKiller, CloseOutcome, DiffPublisher, KillOutcome, KillTarget, LaunchSpec, Launched,
    MailCourier, TaskLauncher,
};
use kiti_core::testing::ManualClock;
use kiti_protocol::mcp::{DoneRequest, SpawnRequest};
use kiti_protocol::{MailMode, ServerMsg, Source, TaskStateKind};

#[derive(Clone)]
struct NoKill;

impl AgentKiller for NoKill {
    async fn terminate(
        &self,
        _target: KillTarget,
        _grace: Duration,
    ) -> Result<KillOutcome, kiti_core::ports::KillError> {
        Ok(KillOutcome::Killed)
    }
}

type Typed = Arc<Mutex<Vec<(u32, String)>>>;

#[derive(Clone, Default)]
struct FakeCourier(Typed);

impl MailCourier for FakeCourier {
    fn deliver(&mut self, pid: Pid, text: String, _ids: Vec<u64>) {
        self.0.lock().unwrap().push((pid.0, text));
    }
}

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<ServerMsg>>>);

impl DiffPublisher for Sink {
    fn publish(&mut self, msg: ServerMsg) -> bool {
        self.0.lock().unwrap().push(msg);
        true
    }
}

/// Lanzador falso: reparte los pids 30, 31… y recuerda qué se le pidió y qué cerró.
#[derive(Clone, Default)]
struct FakeLauncher {
    launched: Arc<Mutex<Vec<LaunchSpec>>>,
    closed: Arc<Mutex<Vec<Task>>>,
    fail: bool,
}

impl TaskLauncher for FakeLauncher {
    fn launch(&self, spec: LaunchSpec) -> Result<Launched, String> {
        if self.fail {
            return Err("sin tmux".into());
        }
        let mut launched = self.launched.lock().unwrap();
        let pid = 30 + launched.len() as u32;
        let id = spec.task.0;
        launched.push(spec);
        Ok(Launched {
            worktree: PathBuf::from(format!("/Users/dev/tienda-web-{id}")),
            session: format!("kiti-{pid}"),
            pid: Pid(pid),
            brief_path: PathBuf::from(format!("/Users/dev/.kiti/tasks/{id}.md")),
        })
    }

    fn close(&self, task: &Task) -> CloseOutcome {
        self.closed.lock().unwrap().push(task.clone());
        CloseOutcome {
            session_killed: true,
            conversation_archived: true,
            worktree_removed: true,
            kept_reason: None,
        }
    }
}

fn detected(provider: Provider, pid: u32, cwd: &str) -> DetectedAgent {
    DetectedAgent {
        provider,
        identity: ProcessIdentity {
            pid: Pid(pid),
            start_time: ProcessStartTime(100),
            uid: 501,
            cmd_fingerprint: 1,
        },
        cwd: WorkingDir::new(cwd),
    }
}

struct World {
    h: RuntimeHandle,
    courier: FakeCourier,
    launcher: FakeLauncher,
    sink: Sink,
}

async fn setup_with(launcher: FakeLauncher) -> World {
    let courier = FakeCourier::default();
    let (runtime, h) = Runtime::new(
        NoKill,
        ManualClock::new(1_000),
        Arc::new(Telemetry::new()),
        Source::Sim,
        "test",
    );
    tokio::spawn(
        runtime
            .with_courier(courier.clone())
            .with_launcher(launcher.clone())
            .run(),
    );
    let sink = Sink::default();
    h.subscribe(sink.clone()).await.unwrap();
    h.mail_reachable(Vec::new()).await;
    // El orquestador (pid 10) ya corría.
    h.register_agent(
        detected(Provider::Claude, 10, "/Users/dev/tienda-web"),
        Timestamp(1_000),
    )
    .await;
    World {
        h,
        courier,
        launcher,
        sink,
    }
}

async fn setup() -> World {
    setup_with(FakeLauncher::default()).await
}

async fn settle() {
    for _ in 0..5 {
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
    }
}

/// `kiti-mcp` (900) es hijo del agente.
fn from(pid: u32) -> Vec<Pid> {
    vec![Pid(900), Pid(pid), Pid(1)]
}

fn spawn_req(slug: &str) -> SpawnRequest {
    SpawnRequest {
        role: "implementar".into(),
        task: "Haz el encargo X".into(),
        slug: slug.into(),
        branch_type: "feat".into(),
        provider: None,
        model: None,
    }
}

fn last_tasks(sink: &Sink) -> Vec<kiti_protocol::TaskInfo> {
    sink.0
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|m| match m {
            ServerMsg::Tasks { tasks } => Some(tasks.clone()),
            _ => None,
        })
        .expect("ningún ServerMsg::Tasks")
}

/// Registra al agente lanzado como lo haría el escáner y lo hace alcanzable por tmux.
async fn appear(w: &World, pid: u32) {
    w.h.mail_reachable(vec![(Pid(pid), format!("kiti-{pid}"))])
        .await;
    w.h.register_agent(
        detected(Provider::Claude, pid, "/Users/dev/tienda-web-1"),
        Timestamp(1_000),
    )
    .await;
    settle().await;
}

#[tokio::test(start_paused = true)]
async fn spawn_lanza_registra_el_pid_y_manda_el_mensaje_de_rol() {
    let w = setup().await;
    let r = w.h.mcp_spawn(from(10), spawn_req("cuota")).await.unwrap();
    assert_eq!(r.task_id, 1);
    assert_eq!(r.branch, "feat/cuota");
    assert_eq!(r.session.as_deref(), Some("kiti-30"));
    {
        let launched = w.launcher.launched.lock().unwrap();
        assert_eq!(launched[0].repo, PathBuf::from("/Users/dev/tienda-web"));
        assert_eq!(launched[0].model, "sonnet");
        assert_eq!(launched[0].brief, "Haz el encargo X");
    }

    // Aún no hay agente: no se escribe nada.
    settle().await;
    assert!(w.courier.0.lock().unwrap().is_empty());
    let tasks = last_tasks(&w.sink);
    assert_eq!(tasks[0].state, TaskStateKind::Running);
    assert_eq!(tasks[0].orchestrator, "claude-1");
    assert_eq!(tasks[0].alias, None);

    appear(&w, 30).await;
    let typed = w.courier.0.lock().unwrap().clone();
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0].0, 30);
    assert!(typed[0].1.contains("de claude-1"), "{}", typed[0].1);
    assert!(typed[0].1.contains(
        "Tu rol: docs/agentes/implementar.md (y comun.md). Tu encargo: \
         /Users/dev/.kiti/tasks/1.md. Léelos enteros y síguelos. Al terminar, llama a kiti_done."
    ));
    assert_eq!(last_tasks(&w.sink)[0].alias.as_deref(), Some("claude-2"));
}

#[tokio::test(start_paused = true)]
async fn done_avisa_al_orquestador_y_cierra_pasados_diez_segundos() {
    let w = setup().await;
    w.h.mcp_spawn(from(10), spawn_req("cuota")).await.unwrap();
    appear(&w, 30).await;
    // Se vacía el mensaje de rol para aislar el aviso de entrega; el orquestador vive en tmux.
    w.courier.0.lock().unwrap().clear();
    w.h.mail_reachable(vec![
        (Pid(10), "kiti-10".into()),
        (Pid(30), "kiti-30".into()),
    ])
    .await;

    let done =
        w.h.mcp_done(
            from(30),
            DoneRequest {
                pr: Some("#41".into()),
                failed: None,
                summary: "cuota lista".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(done.state, "done");
    settle().await;

    let tasks = last_tasks(&w.sink);
    assert_eq!(tasks[0].state, TaskStateKind::Done);
    assert_eq!(tasks[0].pr.as_deref(), Some("#41"));

    let typed = w.courier.0.lock().unwrap().clone();
    assert_eq!(typed.len(), 1);
    assert_eq!(typed[0].0, 10);
    assert!(
        typed[0]
            .1
            .contains("Tarea 1 entregada (rama feat/cuota): PR #41."),
        "{}",
        typed[0].1
    );
    assert!(typed[0].1.contains("Resumen: cuota lista"));

    // Todavía no se cierra.
    assert!(w.launcher.closed.lock().unwrap().is_empty());
    for _ in 0..40 {
        tokio::time::advance(Duration::from_millis(500)).await;
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(2));
    }
    let closed = w.launcher.closed.lock().unwrap().clone();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].branch, "feat/cuota");
    assert_eq!(
        closed[0].state,
        TaskState::Done {
            pr: Some("#41".into())
        }
    );
}

#[tokio::test(start_paused = true)]
async fn done_con_fallo_lo_cuenta_como_fallida() {
    let w = setup().await;
    w.h.mcp_spawn(from(10), spawn_req("cuota")).await.unwrap();
    appear(&w, 30).await;
    let done =
        w.h.mcp_done(
            from(30),
            DoneRequest {
                pr: None,
                failed: Some("no compila".into()),
                summary: String::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(done.state, "failed");
    settle().await;
    let tasks = last_tasks(&w.sink);
    assert_eq!(tasks[0].state, TaskStateKind::Failed);
    assert_eq!(tasks[0].reason.as_deref(), Some("no compila"));
}

#[tokio::test(start_paused = true)]
async fn done_de_quien_no_es_una_tarea_es_not_a_task() {
    let w = setup().await;
    let err =
        w.h.mcp_done(
            from(10),
            DoneRequest {
                pr: None,
                failed: None,
                summary: "x".into(),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "NOT_A_TASK");

    // Tampoco dos veces: la primera entrega la cierra.
    w.h.mcp_spawn(from(10), spawn_req("cuota")).await.unwrap();
    appear(&w, 30).await;
    let req = || DoneRequest {
        pr: None,
        failed: None,
        summary: "x".into(),
    };
    w.h.mcp_done(from(30), req()).await.unwrap();
    assert_eq!(
        w.h.mcp_done(from(30), req()).await.unwrap_err().code,
        "NOT_A_TASK"
    );
}

#[tokio::test(start_paused = true)]
async fn el_cuarto_spawn_simultaneo_es_limit() {
    let w = setup().await;
    for n in 0..3 {
        w.h.mcp_spawn(from(10), spawn_req(&format!("t{n}")))
            .await
            .unwrap();
    }
    let err = w.h.mcp_spawn(from(10), spawn_req("t3")).await.unwrap_err();
    assert_eq!(err.code, "LIMIT");
    assert_eq!(w.launcher.launched.lock().unwrap().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn en_modo_manual_spawn_pide_aprobacion() {
    let w = setup().await;
    w.h.mail(MailAction::Mode {
        mode: MailMode::Manual,
    })
    .await;
    settle().await;
    let err =
        w.h.mcp_spawn(from(10), spawn_req("cuota"))
            .await
            .unwrap_err();
    assert_eq!(err.code, "NEEDS_APPROVAL");
    assert!(w.launcher.launched.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn un_lanzamiento_fallido_devuelve_el_motivo_y_no_ocupa_hueco() {
    let w = setup_with(FakeLauncher {
        fail: true,
        ..FakeLauncher::default()
    })
    .await;
    for n in 0..4 {
        let err =
            w.h.mcp_spawn(from(10), spawn_req(&format!("t{n}")))
                .await
                .unwrap_err();
        assert_eq!(err.code, "LAUNCH_FAILED", "intento {n}");
        assert!(err.message.contains("sin tmux"));
    }
    settle().await;
    let tasks = last_tasks(&w.sink);
    assert!(tasks.iter().all(|t| t.state == TaskStateKind::Failed));
}

#[tokio::test(start_paused = true)]
async fn peticiones_invalidas_y_llamadas_de_desconocidos() {
    let w = setup().await;
    let mut bad = spawn_req("cuota");
    bad.slug = "x; rm -rf".into();
    assert_eq!(
        w.h.mcp_spawn(from(10), bad).await.unwrap_err().code,
        "bad_request"
    );
    assert_eq!(
        w.h.mcp_spawn(vec![Pid(900), Pid(1)], spawn_req("cuota"))
            .await
            .unwrap_err()
            .code,
        "unknown_sender"
    );
    assert!(w.launcher.launched.lock().unwrap().is_empty());
}
