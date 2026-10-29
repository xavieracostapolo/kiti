//! HookServer: cada control de seguridad del ADR 0004 §2, sin red (`oneshot` sobre el router).
#![cfg(feature = "hooks")]

use axum::body::Body;
use axum::http::{Request, StatusCode};
use kiti_core::adapters::hooks::{HookRequest, hook_router, load_or_create_token};
use kiti_core::domain::Provider;
use tokio::sync::mpsc;
use tower::ServiceExt;

const PORT: u16 = 47821;
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn request() -> axum::http::request::Builder {
    with(&[])
}

/// Petición válida con las cabeceras de `overrides` sustituidas (el builder de `http` añade
/// una segunda cabecera en lugar de reemplazarla).
fn with(overrides: &[(&str, &str)]) -> axum::http::request::Builder {
    let host = format!("127.0.0.1:{PORT}");
    let auth = format!("Bearer {TOKEN}");
    let mut headers = vec![
        ("host", host.as_str()),
        ("content-type", "application/json"),
        ("authorization", auth.as_str()),
    ];
    for (name, value) in overrides {
        headers.retain(|(n, _)| n != name);
        headers.push((name, value));
    }
    headers
        .into_iter()
        .fold(Request::post("/v1/hooks/claude"), |b, (n, v)| {
            b.header(n, v)
        })
}

async fn send(req: Request<Body>) -> (StatusCode, Vec<u8>, mpsc::Receiver<HookRequest>) {
    let (tx, rx) = mpsc::channel(8);
    let res = hook_router(TOKEN.into(), PORT, tx)
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, body.to_vec(), rx)
}

#[tokio::test]
async fn un_hook_valido_responde_204_vacio_y_se_encola() {
    let (status, body, mut rx) = send(request().body(Body::from(r#"{"a":1}"#)).unwrap()).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty(), "Kiti nunca devuelve decisiones al agente");
    let got = rx.try_recv().unwrap();
    assert_eq!(got.provider, Provider::Claude);
    assert_eq!(&got.body[..], br#"{"a":1}"#);
}

#[tokio::test]
async fn la_status_line_de_claude_entra_como_claude_con_su_evento() {
    // ADR 0015: el forwarder reenvía el JSON de la status line a /v1/hooks/statusline.
    let host = format!("127.0.0.1:{PORT}");
    let req = Request::post("/v1/hooks/statusline")
        .header("host", host.as_str())
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::from(r#"{"rate_limits":{}}"#))
        .unwrap();
    let (status, _, mut rx) = send(req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let got = rx.try_recv().unwrap();
    assert_eq!(got.provider, Provider::Claude);
    assert_eq!(got.event.as_deref(), Some("StatusLine"));
}

#[tokio::test]
async fn rechaza_un_host_distinto_contra_dns_rebinding() {
    let req = with(&[("host", "evil.example:47821")])
        .body(Body::empty())
        .unwrap();
    let (status, _, mut rx) = send(req).await;
    assert_eq!(status, StatusCode::MISDIRECTED_REQUEST);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn rechaza_peticiones_de_navegador() {
    for (name, value) in [
        ("origin", "http://evil.example"),
        ("sec-fetch-site", "cross-site"),
    ] {
        let req = request().header(name, value).body(Body::empty()).unwrap();
        assert_eq!(send(req).await.0, StatusCode::FORBIDDEN, "{name}");
    }
}

#[tokio::test]
async fn exige_el_token() {
    let wrong = with(&[("authorization", "Bearer otro")])
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(wrong).await.0, StatusCode::UNAUTHORIZED);
    let missing = Request::post("/v1/hooks/claude")
        .header("host", format!("127.0.0.1:{PORT}"))
        .header("content-type", "application/json")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(missing).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn exige_json() {
    let req = with(&[("content-type", "text/plain")])
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(req).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn proveedor_desconocido_y_rutas_ajenas() {
    let req = Request::post("/v1/hooks/otro")
        .header("host", format!("127.0.0.1:{PORT}"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TOKEN}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(req).await.0, StatusCode::NOT_FOUND);
    let get = Request::get("/v1/hooks/claude")
        .header("host", format!("127.0.0.1:{PORT}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(get).await.0, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn limita_el_cuerpo_a_4_mib() {
    let big = vec![b' '; 4 * 1024 * 1024 + 1];
    let (status, _, _) = send(request().body(Body::from(big)).unwrap()).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn con_el_canal_lleno_responde_igual_y_no_bloquea() {
    let (tx, _rx) = mpsc::channel(1);
    let router = hook_router(TOKEN.into(), PORT, tx);
    for _ in 0..3 {
        let res = router
            .clone()
            .oneshot(request().body(Body::from("{}")).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }
}

#[test]
fn el_token_se_crea_con_permisos_0600_y_se_reutiliza() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("kiti-token-{}", std::process::id()));
    let path = dir.join("hook-token");
    let _ = std::fs::remove_dir_all(&dir);
    let first = load_or_create_token(&path).unwrap();
    assert_eq!(first.len(), 64);
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(load_or_create_token(&path).unwrap(), first);

    // Si alguien abrió los permisos, no se usa.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load_or_create_token(&path).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(start_paused = true)]
async fn de_extremo_a_extremo_un_hook_http_pone_verde_su_oficina() {
    use kiti_core::adapters::hooks::run_hook_pump;
    use kiti_core::application::{Runtime, Telemetry};
    use kiti_core::domain::{
        DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Timestamp, WorkingDir,
    };
    use kiti_core::ports::DiffPublisher;
    use kiti_core::sim::SimKiller;
    use kiti_core::testing::ManualClock;
    use kiti_protocol::{ServerMsg, Source, StatusKind};
    use std::sync::{Arc, Mutex};

    struct Collect(Arc<Mutex<Vec<ServerMsg>>>);
    impl DiffPublisher for Collect {
        fn publish(&mut self, msg: ServerMsg) -> bool {
            self.0.lock().unwrap().push(msg);
            true
        }
    }

    let clock = ManualClock::new(10_000);
    let runtime = Runtime::spawn(
        SimKiller,
        clock.clone(),
        Arc::new(Telemetry::new()),
        Source::Live,
        "test",
    );
    let agent = DetectedAgent {
        provider: Provider::Claude,
        identity: ProcessIdentity {
            pid: Pid(77),
            start_time: ProcessStartTime(1),
            uid: 501,
            cmd_fingerprint: 1,
        },
        cwd: WorkingDir::new("/Users/dev/kiti"),
    };
    runtime
        .reconcile(vec![agent], vec![], vec![], Timestamp(10_000))
        .await;
    let msgs = Arc::new(Mutex::new(Vec::new()));
    runtime.subscribe(Collect(msgs.clone())).await.unwrap();

    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(run_hook_pump(rx, runtime.clone(), clock));
    let body = r#"{"session_id":"s","cwd":"/Users/dev/kiti","hook_event_name":"UserPromptSubmit","prompt":"secreto"}"#;
    let res = hook_router(TOKEN.into(), PORT, tx)
        .oneshot(request().body(Body::from(body)).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let mut green = false;
    for _ in 0..20 {
        tokio::time::advance(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        green = msgs.lock().unwrap().iter().any(|m| {
            matches!(m, ServerMsg::Diff { upserts, .. }
                if upserts.iter().any(|a| a.pid == 77 && a.status.kind == StatusKind::Working))
        });
        if green {
            break;
        }
    }
    assert!(green, "el hook debía poner la oficina en verde");
    // Desde el ADR 0009 el último pedido viaja a la UI (solo en local, nunca a los logs).
    let prompt = msgs.lock().unwrap().iter().find_map(|m| match m {
        ServerMsg::Diff { upserts, .. } => upserts
            .iter()
            .find(|a| a.pid == 77)
            .and_then(|a| a.last_prompt.clone()),
        _ => None,
    });
    assert_eq!(prompt.as_deref(), Some("secreto"));
}

#[tokio::test]
async fn el_forwarder_manda_su_ppid_y_se_lee() {
    let req = Request::post("/v1/hooks/codex")
        .header("host", format!("127.0.0.1:{PORT}"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("x-kiti-ppid", "4242")
        .body(Body::from("{}"))
        .unwrap();
    let (status, _, mut rx) = send(req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let got = rx.try_recv().unwrap();
    assert_eq!(got.provider, Provider::Codex);
    assert_eq!(got.ppid, Some(4242));
}

#[test]
fn la_cadena_de_ancestros_empieza_en_el_pid_y_sube() {
    use kiti_core::adapters::hooks::ancestry;
    use kiti_core::domain::Pid;
    let me = std::process::id();
    let chain = ancestry(me);
    assert_eq!(chain.first(), Some(&Pid(me)));
    assert_eq!(
        chain.get(1),
        Some(&Pid(std::os::unix::process::parent_id()))
    );
}

mod mcp {
    //! `/v1/mcp/*` (ADR 0017): mismos controles que los hooks, JSON de vuelta y remitente
    //! resuelto por el proceso.
    use super::*;
    use kiti_core::adapters::hooks::mcp_router;
    use kiti_core::application::{Runtime, RuntimeHandle, Telemetry};
    use kiti_core::domain::{
        DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Timestamp, WorkingDir,
    };
    use kiti_core::ports::{AgentKiller, KillOutcome, KillTarget};
    use kiti_core::testing::ManualClock;
    use kiti_protocol::Source;
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Clone)]
    struct NoKill;

    impl AgentKiller for NoKill {
        async fn terminate(
            &self,
            _t: KillTarget,
            _g: Duration,
        ) -> Result<KillOutcome, kiti_core::ports::KillError> {
            Ok(KillOutcome::Killed)
        }
    }

    fn agent(provider: Provider, pid: u32) -> DetectedAgent {
        DetectedAgent {
            provider,
            identity: ProcessIdentity {
                pid: Pid(pid),
                start_time: ProcessStartTime(1),
                uid: 501,
                cmd_fingerprint: 1,
            },
            cwd: WorkingDir::new("/Users/dev/api"),
        }
    }

    /// Este proceso de test hace de agente Claude; otro Codex inventado recibe.
    async fn runtime() -> RuntimeHandle {
        let h = Runtime::spawn(
            NoKill,
            ManualClock::new(1),
            Arc::new(Telemetry::new()),
            Source::Sim,
            "t",
        );
        h.register_agent(agent(Provider::Claude, std::process::id()), Timestamp(1))
            .await;
        h.register_agent(agent(Provider::Codex, 4_000_000), Timestamp(1))
            .await;
        h
    }

    async fn call(
        rt: RuntimeHandle,
        tool: &str,
        ppid: Option<u32>,
        body: &str,
    ) -> (StatusCode, serde_json::Value) {
        let mut req = with(&[]).uri(format!("/v1/mcp/{tool}"));
        if let Some(p) = ppid {
            req = req.header("x-kiti-ppid", p.to_string());
        }
        let res = mcp_router(TOKEN.into(), PORT, rt)
            .oneshot(req.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    #[tokio::test]
    async fn agents_y_send_responden_json_con_el_remitente_del_proceso() {
        let rt = runtime().await;
        let me = Some(std::process::id());
        let (status, body) = call(rt.clone(), "agents", me, "{}").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["you"], "claude-1");
        assert_eq!(body["peers"][0]["alias"], "codex-1");
        let (status, body) =
            call(rt.clone(), "send", me, r#"{"to":"codex-1","text":"hola"}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["state"], "queued");
        let (status, body) = call(rt, "send", me, r#"{"to":"codex-9","text":"hola"}"#).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "unknown_recipient");
    }

    #[tokio::test]
    async fn sin_ppid_o_sin_token_no_hay_nada() {
        let rt = runtime().await;
        let (status, body) = call(rt.clone(), "agents", None, "{}").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "no_ppid");
        let res = mcp_router(TOKEN.into(), PORT, rt.clone())
            .oneshot(
                with(&[("authorization", "Bearer nope")])
                    .uri("/v1/mcp/agents")
                    .header("x-kiti-ppid", std::process::id().to_string())
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        // Un proceso que no es ningún agente (PID 1 no se recorre).
        let (status, body) = call(rt, "agents", Some(1), "{}").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "unknown_sender");
    }
}

#[tokio::test(start_paused = true)]
async fn el_fin_de_turno_de_antigravity_pide_refrescar_su_cuota() {
    use kiti_core::adapters::hooks::{HookRequest, run_hook_pump};
    use kiti_core::application::{Runtime, Telemetry};
    use kiti_core::sim::SimKiller;
    use kiti_core::testing::ManualClock;
    use kiti_protocol::Source;
    use std::sync::Arc;
    use std::time::Duration;

    let clock = ManualClock::new(10_000);
    let runtime = Runtime::spawn(
        SimKiller,
        clock.clone(),
        Arc::new(Telemetry::new()),
        Source::Live,
        "test",
    );
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(run_hook_pump(rx, runtime.clone(), clock));
    let send = |event: &str| HookRequest {
        provider: Provider::Antigravity,
        ppid: None,
        event: Some(event.into()),
        body:
            r#"{"conversationId":"c-1","workspacePaths":["/w/tienda"],"fullyIdle":true,"error":""}"#
                .into(),
    };
    // Empezar un turno no cambia la cuota: no pide nada.
    tx.send(send("PreToolUse")).await.unwrap();
    let early = tokio::time::timeout(Duration::from_millis(50), runtime.quota_kicked()).await;
    assert!(early.is_err(), "solo el fin de turno pide refrescar");
    tx.send(send("Stop")).await.unwrap();
    tokio::time::timeout(Duration::from_millis(50), runtime.quota_kicked())
        .await
        .expect("el Stop de Antigravity debía pedir refrescar la cuota");
}
