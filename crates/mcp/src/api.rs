//! Llamadas al HookServer de Kiti: `POST /v1/mcp/{agents,send,inbox}` con token y PPID.

use std::path::PathBuf;
use std::time::Duration;

use kiti_protocol::mcp;
use serde::de::DeserializeOwned;

const DEFAULT_PORT: u16 = 47821;
const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
pub enum ApiError {
    /// Kiti no está abierto (o no responde).
    Unavailable,
    /// Kiti rechazó la petición con un motivo para el modelo.
    Rejected(mcp::ErrorReply),
}

pub trait Api: Send + Sync + 'static {
    fn agents(&self) -> Result<mcp::AgentsReply, ApiError>;
    fn send(&self, request: &mcp::SendRequest) -> Result<mcp::SendReply, ApiError>;
    fn inbox(&self) -> Result<mcp::InboxReply, ApiError>;
    fn remember(&self, request: &mcp::RememberRequest) -> Result<mcp::RememberReply, ApiError>;
    fn recall(&self, request: &mcp::RecallRequest) -> Result<mcp::RecallReply, ApiError>;
}

pub struct HttpApi {
    port: u16,
    token_file: Option<PathBuf>,
    agent: ureq::Agent,
}

fn token_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KITI_TOKEN_FILE") {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/com.kiti.desktop/hook-token"))
}

impl HttpApi {
    pub fn from_env() -> Self {
        Self {
            port: std::env::var("KITI_HOOK_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT),
            token_file: token_path(),
            agent: ureq::Agent::config_builder()
                .timeout_global(Some(TIMEOUT))
                .http_status_as_error(false)
                .build()
                .new_agent(),
        }
    }

    fn call<T: DeserializeOwned>(&self, tool: &str, body: &[u8]) -> Result<T, ApiError> {
        // El token se lee en cada llamada: Kiti puede arrancar después que el agente.
        let token = self
            .token_file
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .ok_or(ApiError::Unavailable)?;
        let mut res = self
            .agent
            .post(format!("http://127.0.0.1:{}/v1/mcp/{tool}", self.port))
            .header("Authorization", format!("Bearer {}", token.trim()))
            .header("Content-Type", "application/json")
            .header(
                "X-Kiti-Ppid",
                std::os::unix::process::parent_id().to_string(),
            )
            .send(body)
            .map_err(|_| ApiError::Unavailable)?;
        let status = res.status().as_u16();
        let bytes = res
            .body_mut()
            .read_to_vec()
            .map_err(|_| ApiError::Unavailable)?;
        if status == 200 {
            return serde_json::from_slice(&bytes).map_err(|_| ApiError::Unavailable);
        }
        Err(match serde_json::from_slice::<mcp::ErrorReply>(&bytes) {
            Ok(e) => ApiError::Rejected(e),
            Err(_) => ApiError::Unavailable,
        })
    }
}

impl Api for HttpApi {
    fn agents(&self) -> Result<mcp::AgentsReply, ApiError> {
        self.call("agents", b"{}")
    }

    fn send(&self, request: &mcp::SendRequest) -> Result<mcp::SendReply, ApiError> {
        let body = serde_json::to_vec(request).map_err(|_| ApiError::Unavailable)?;
        self.call("send", &body)
    }

    fn inbox(&self) -> Result<mcp::InboxReply, ApiError> {
        self.call("inbox", b"{}")
    }

    fn remember(&self, request: &mcp::RememberRequest) -> Result<mcp::RememberReply, ApiError> {
        let body = serde_json::to_vec(request).map_err(|_| ApiError::Unavailable)?;
        self.call("remember", &body)
    }

    fn recall(&self, request: &mcp::RecallRequest) -> Result<mcp::RecallReply, ApiError> {
        let body = serde_json::to_vec(request).map_err(|_| ApiError::Unavailable)?;
        self.call("recall", &body)
    }
}
