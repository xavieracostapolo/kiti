//! HookServer (ADR 0002 §5, ADR 0004 §2): `POST /v1/hooks/{provider}` en `127.0.0.1`.
//!
//! Responde `204` vacío en cuanto valida la petición y encola el cuerpo con `try_send`: nunca
//! bloquea al agente ni le devuelve decisiones. La ACL se aplica después, fuera del handler.
//!
//! `POST /v1/mcp/{agents,send,inbox}` (ADR 0017) es la API de `kiti-mcp`: con los mismos
//! controles, pero responde JSON con datos (nunca decisiones de un hook).

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::post;
use tokio::sync::mpsc;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tracing::{debug, warn};

use crate::domain::Provider;

pub const MAX_BODY: usize = 4 * 1024 * 1024;

/// Un hook aceptado, aún sin interpretar.
#[derive(Debug, Clone)]
pub struct HookRequest {
    pub provider: Provider,
    /// `X-Kiti-Ppid` del forwarder (Codex y Gemini): punto de partida para buscar el agente.
    pub ppid: Option<u32>,
    /// `X-Kiti-Event` del forwarder (Antigravity): su JSON no dice qué evento es (ADR 0014).
    pub event: Option<String>,
    pub body: Bytes,
}

#[derive(Clone)]
struct Ctx {
    token: String,
    host: String,
    tx: mpsc::Sender<HookRequest>,
}

fn reject(reason: &'static str, status: StatusCode) -> StatusCode {
    warn!(event = "hook.rejected", reason, status = status.as_u16());
    status
}

/// Controles comunes a hooks y MCP: host exacto, sin navegador, token y JSON.
fn authorize(token: &str, host: &str, headers: &HeaderMap) -> Result<(), StatusCode> {
    let header_str = |name: header::HeaderName| headers.get(name).and_then(|v| v.to_str().ok());
    // Host exacto: corta el DNS rebinding (un dominio ajeno que resuelve a 127.0.0.1).
    if header_str(header::HOST) != Some(host) {
        return Err(reject("host", StatusCode::MISDIRECTED_REQUEST));
    }
    // Un CLI no envía Origin ni Sec-Fetch-*; un navegador sí.
    if headers.contains_key(header::ORIGIN)
        || headers.keys().any(|k| k.as_str().starts_with("sec-fetch-"))
    {
        return Err(reject("origin", StatusCode::FORBIDDEN));
    }
    let bearer = header_str(header::AUTHORIZATION).and_then(|v| v.strip_prefix("Bearer "));
    if !bearer.is_some_and(|t| constant_time_eq::constant_time_eq(t.as_bytes(), token.as_bytes())) {
        return Err(reject("token", StatusCode::UNAUTHORIZED));
    }
    // Exigir JSON obliga a un navegador a un preflight CORS que nunca se responde.
    if !header_str(header::CONTENT_TYPE).is_some_and(|c| c.starts_with("application/json")) {
        return Err(reject("content_type", StatusCode::UNSUPPORTED_MEDIA_TYPE));
    }
    Ok(())
}

async fn handle(
    State(ctx): State<Ctx>,
    UrlPath(provider): UrlPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let header_str = |name: header::HeaderName| headers.get(name).and_then(|v| v.to_str().ok());
    if let Err(status) = authorize(&ctx.token, &ctx.host, &headers) {
        return status;
    }
    // La status line de Claude (ADR 0015) entra como un evento más de Claude.
    let (provider, forced_event) = if provider == "statusline" {
        (Provider::Claude, Some("StatusLine".to_owned()))
    } else {
        let Ok(provider) = Provider::from_str(&provider) else {
            return reject("provider", StatusCode::NOT_FOUND);
        };
        (provider, None)
    };
    let ppid =
        header_str(header::HeaderName::from_static("x-kiti-ppid")).and_then(|v| v.parse().ok());
    // Solo un nombre de evento: letras, hasta 32.
    let event = header_str(header::HeaderName::from_static("x-kiti-event"))
        .filter(|v| !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphabetic()))
        .map(str::to_owned)
        .or(forced_event);
    debug!(event = "hook.accepted", provider = %provider, bytes = body.len());
    if ctx
        .tx
        .try_send(HookRequest {
            provider,
            ppid,
            event,
            body,
        })
        .is_err()
    {
        warn!(event = "hook.dropped", provider = %provider);
    }
    StatusCode::NO_CONTENT
}

pub fn hook_router(token: String, port: u16, tx: mpsc::Sender<HookRequest>) -> Router {
    let ctx = Ctx {
        token,
        host: format!("127.0.0.1:{port}"),
        tx,
    };
    Router::new()
        .route("/v1/hooks/{provider}", post(handle))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(ConcurrencyLimitLayer::new(64))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(2),
        ))
        .with_state(ctx)
}

#[derive(Clone)]
struct McpCtx {
    token: String,
    host: String,
    runtime: crate::application::RuntimeHandle,
}

type JsonReply = (StatusCode, [(header::HeaderName, &'static str); 1], Vec<u8>);

fn json_reply(status: StatusCode, value: &impl serde::Serialize) -> JsonReply {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_vec(value).unwrap_or_default(),
    )
}

fn mcp_error(status: StatusCode, code: &str, message: &str) -> JsonReply {
    json_reply(
        status,
        &kiti_protocol::mcp::ErrorReply {
            code: code.to_owned(),
            message: message.to_owned(),
        },
    )
}

async fn handle_mcp(
    State(ctx): State<McpCtx>,
    UrlPath(tool): UrlPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> JsonReply {
    if let Err(status) = authorize(&ctx.token, &ctx.host, &headers) {
        return mcp_error(status, "unauthorized", "petición rechazada");
    }
    // Sin el PID de quien llama no se sabe qué agente es: nunca se acepta un remitente declarado.
    let Some(ppid) = headers
        .get("x-kiti-ppid")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok())
    else {
        return mcp_error(StatusCode::BAD_REQUEST, "no_ppid", "falta X-Kiti-Ppid");
    };
    let chain = tokio::task::spawn_blocking(move || ancestry(ppid))
        .await
        .unwrap_or_default();
    let result = match tool.as_str() {
        "agents" => ctx
            .runtime
            .mcp_agents(chain)
            .await
            .map(|r| json_reply(StatusCode::OK, &r)),
        "send" => match serde_json::from_slice::<kiti_protocol::mcp::SendRequest>(&body) {
            Ok(req) => ctx
                .runtime
                .mcp_send(chain, req)
                .await
                .map(|r| json_reply(StatusCode::OK, &r)),
            Err(_) => {
                return mcp_error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    "se esperaba {\"to\", \"text\", \"replyTo\"?}",
                );
            }
        },
        "inbox" => ctx
            .runtime
            .mcp_inbox(chain)
            .await
            .map(|r| json_reply(StatusCode::OK, &r)),
        _ => {
            return mcp_error(
                StatusCode::NOT_FOUND,
                "unknown_tool",
                "herramienta desconocida",
            );
        }
    };
    result.unwrap_or_else(|e| json_reply(StatusCode::UNPROCESSABLE_ENTITY, &e))
}

/// API de `kiti-mcp` (ADR 0017). Se une al router de los hooks con `merge`.
pub fn mcp_router(token: String, port: u16, runtime: crate::application::RuntimeHandle) -> Router {
    let ctx = McpCtx {
        token,
        host: format!("127.0.0.1:{port}"),
        runtime,
    };
    Router::new()
        .route("/v1/mcp/{tool}", post(handle_mcp))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(ConcurrencyLimitLayer::new(16))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(5),
        ))
        .with_state(ctx)
}

/// Escucha solo en `127.0.0.1` (nunca `0.0.0.0`).
pub async fn serve_hooks(port: u16, router: Router) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    axum::serve(listener, router).await
}

/// Token de 32 bytes del CSPRNG, en hex, en un archivo `0600` creado con `O_EXCL`. Si el archivo
/// existe con otros permisos, se rechaza: alguien lo tocó.
pub fn load_or_create_token(path: &Path) -> std::io::Result<String> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("{} debe tener permisos 0600", path.display()),
                ));
            }
            Ok(std::fs::read_to_string(path)?.trim().to_owned())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
            let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            Ok(token)
        }
        Err(e) => Err(e),
    }
}

/// `pid` y sus ancestros (hasta 16 niveles), para enlazar un hook con el proceso del agente.
pub fn ancestry(pid: u32) -> Vec<crate::domain::Pid> {
    let mut chain = Vec::new();
    let mut cur = Some(crate::domain::Pid(pid));
    while let Some(p) = cur {
        if chain.len() >= 16 || p.0 <= 1 || chain.contains(&p) {
            break;
        }
        chain.push(p);
        cur = super::read_process(p).and_then(|s| s.ppid);
    }
    chain
}

/// Saca los hooks aceptados de la cola, aplica la ACL de su CLI y los entrega al runtime.
pub async fn run_hook_pump<C: crate::ports::Clock>(
    mut rx: mpsc::Receiver<HookRequest>,
    runtime: crate::application::RuntimeHandle,
    clock: C,
) {
    use crate::ingestion::{antigravity, claude, codex, gemini};
    while let Some(req) = rx.recv().await {
        // Status line de Claude: solo trae la cuota (ADR 0015), no es un hook del agente.
        if req.provider == Provider::Claude && req.event.as_deref() == Some("StatusLine") {
            if let Some(windows) = claude::parse_rate_limits(&req.body) {
                runtime.quota(Provider::Claude, windows).await;
            }
            // Y el tablero de la oficina (ADR 0016).
            if let Some((session, stats)) = claude::parse_session_stats(&req.body) {
                runtime.session_stats(session, stats, clock.now()).await;
            }
            continue;
        }
        let parsed = match req.provider {
            Provider::Claude => claude::parse_hook(&req.body),
            Provider::Codex => codex::parse_hook(&req.body),
            Provider::Gemini => gemini::parse_hook(&req.body),
            Provider::Antigravity => antigravity::parse_hook(&req.body, req.event.as_deref()),
        };
        let event = match parsed {
            Ok(event) => event,
            Err(e) => {
                warn!(event = "hook.invalid", provider = %req.provider, error = %e);
                continue;
            }
        };
        // Fin de turno de Antigravity: su cuota se pide, así que se pide ya (ADR 0015).
        if req.provider == Provider::Antigravity
            && matches!(
                event.signal,
                crate::domain::Signal::Stop | crate::domain::Signal::Failure
            )
        {
            runtime.kick_quota();
        }
        // Leer la cadena de procesos es bloqueante (sysinfo): fuera del runtime async.
        let chain = match req.ppid {
            Some(ppid) => tokio::task::spawn_blocking(move || ancestry(ppid))
                .await
                .unwrap_or_default(),
            None => Vec::new(),
        };
        runtime
            .hook_from(req.provider, event, chain, clock.now())
            .await;
    }
}
