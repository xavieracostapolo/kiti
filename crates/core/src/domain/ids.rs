//! Value objects del dominio. Inmutables; el tiempo siempre es epoch ms que llega de fuera.

use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Claude, Provider::Codex, Provider::Gemini];

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Gemini => "gemini",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pid(pub u32);

/// Epoch ms. Lo da el port `Clock`; el dominio nunca lee el reloj.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn ms(self) -> u64 {
        self.0
    }

    /// Tiempo transcurrido desde `earlier`; cero si `earlier` es posterior.
    pub fn since(self, earlier: Timestamp) -> Duration {
        Duration::from_millis(self.0.saturating_sub(earlier.0))
    }

    pub fn plus(self, d: Duration) -> Timestamp {
        Timestamp(self.0.saturating_add(d.as_millis() as u64))
    }
}

/// Hora de inicio del proceso en epoch ms. Con el PID, distingue un PID reutilizado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcessStartTime(pub u64);

/// Identidad del agregado: `(Provider, Pid, ProcessStartTime)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentId {
    pub provider: Provider,
    pub pid: Pid,
    pub start_time: ProcessStartTime,
}

/// `"{provider}:{pid}:{start_ms}"`, la forma opaca que ve la UI.
impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.provider, self.pid.0, self.start_time.0)
    }
}

/// Lo que se verifica antes de cada señal de kill (ADR 0004 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcessIdentity {
    pub pid: Pid,
    pub start_time: ProcessStartTime,
    pub uid: u32,
    pub cmd_fingerprint: u64,
}

macro_rules! string_value {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_value!(
    /// Nombre de herramienta tal como lo envía el CLI (`Bash`, `apply_patch`…).
    ToolName
);
string_value!(
    /// Clave que empareja `ToolStart` con su `ToolEnd` (`tool_use_id` o equivalente).
    ToolUseKey
);
string_value!(
    /// Directorio de trabajo del agente.
    WorkingDir
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_se_muestra_como_cadena_opaca() {
        let id = AgentId {
            provider: Provider::Codex,
            pid: Pid(4242),
            start_time: ProcessStartTime(1_700_000_000_000),
        };
        assert_eq!(id.to_string(), "codex:4242:1700000000000");
    }

    #[test]
    fn since_satura_en_cero() {
        assert_eq!(Timestamp(5).since(Timestamp(10)), Duration::ZERO);
        assert_eq!(
            Timestamp(1500).since(Timestamp(500)),
            Duration::from_secs(1)
        );
    }
}
