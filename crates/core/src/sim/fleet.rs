//! Flota de agentes simulados determinista (ADR 0002 §9, ADR 0006 §9).
//!
//! Lee `fixtures/sim/profiles.json`, usa `ChaCha8Rng` por agente con `seed ^ index`,
//! y avanza el tiempo virtual paso a paso.

use rand::RngExt;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use crate::domain::{
    AgentId, DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider, Signal, ToolName,
    ToolOutcome, ToolUseKey, WorkingDir,
};

pub const TICK_MS: u64 = 100;

#[derive(Debug, Clone, Deserialize)]
pub struct ProfilesConfig {
    #[serde(rename = "tickMs")]
    pub tick_ms: u64,
    #[serde(rename = "epochMs")]
    pub epoch_ms: u64,
    pub providers: Vec<ProviderSpec>,
    pub projects: Vec<String>,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSpec {
    pub provider: String,
    pub weight: f64,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Profile {
    pub name: String,
    pub weight: f64,
    #[serde(rename = "meanIdleMs")]
    pub mean_idle_ms: f64,
    #[serde(rename = "meanThinkMs")]
    pub mean_think_ms: f64,
    #[serde(rename = "meanToolMs")]
    pub mean_tool_ms: f64,
    #[serde(rename = "meanToolsPerTurn")]
    pub mean_tools_per_turn: f64,
    #[serde(rename = "permissionProb")]
    pub permission_prob: f64,
    #[serde(rename = "meanPermissionWaitMs")]
    pub mean_permission_wait_ms: f64,
    #[serde(rename = "errorProbPerTurn")]
    pub error_prob_per_turn: f64,
    #[serde(rename = "crashProbPerTurn")]
    pub crash_prob_per_turn: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimSpec {
    pub count: usize,
    pub seed: u64,
    pub freeze: bool,
}

impl SimSpec {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.is_empty() {
            return None;
        }
        let count = parts[0].parse().ok()?;
        let seed = if parts.len() > 1 {
            parts[1].parse().ok()?
        } else {
            42
        };
        let freeze = parts.len() > 2 && parts[2] == "freeze";
        Some(Self {
            count,
            seed,
            freeze,
        })
    }
}

struct SimAgent {
    id: AgentId,
    detected: DetectedAgent,
    rng: ChaCha8Rng,
    profile: Profile,
    tools: Vec<String>,
    tools_left: i32,
    tool_open: bool,
    permission_pending: bool,
    next_at: u64,
    current_tool_key: u64,
    current_status_kind: String, // "idle", "working", "waiting", "dead", "error"
}

impl SimAgent {
    fn exp_ms(&mut self, mean: f64) -> u64 {
        let u: f64 = self.rng.random();
        let ms = (-mean * (1.0 - u).ln()).round() as u64;
        ms.max(TICK_MS)
    }

    fn chance(&mut self, p: f64) -> bool {
        self.rng.random::<f64>() < p
    }

    fn next_signal(&mut self, now: u64) -> Option<Signal> {
        match self.current_status_kind.as_str() {
            "dead" => {
                self.next_at = u64::MAX;
                None
            }
            "idle" | "error" => {
                let u: f64 = self.rng.random();
                let tools =
                    1 + (-self.profile.mean_tools_per_turn * (1.0f64 - u).ln()).floor() as i32;
                self.tools_left = tools.max(1);
                let wait = self.exp_ms(self.profile.mean_think_ms);
                self.next_at = now + wait;
                self.current_status_kind = "working".to_string();
                Some(Signal::PromptSubmit)
            }
            "waiting" => {
                self.tool_open = false;
                self.tools_left -= 1;
                let wait = self.exp_ms(self.profile.mean_think_ms);
                self.next_at = now + wait;
                self.current_status_kind = "working".to_string();
                let key = ToolUseKey::new(format!("sim-tool-{}", self.current_tool_key));
                Some(Signal::ToolEnd {
                    key,
                    outcome: ToolOutcome::Ok,
                })
            }
            "working" => {
                if self.tool_open && self.permission_pending {
                    self.permission_pending = false;
                    let wait = self.exp_ms(self.profile.mean_permission_wait_ms);
                    self.next_at = now + wait;
                    self.current_status_kind = "waiting".to_string();
                    return Some(Signal::AwaitingInput);
                }
                if self.tool_open {
                    self.tool_open = false;
                    self.tools_left -= 1;
                    let wait = self.exp_ms(self.profile.mean_think_ms);
                    self.next_at = now + wait;
                    let key = ToolUseKey::new(format!("sim-tool-{}", self.current_tool_key));
                    let failed = self.chance(0.1);
                    return Some(Signal::ToolEnd {
                        key,
                        outcome: if failed {
                            ToolOutcome::Failed
                        } else {
                            ToolOutcome::Ok
                        },
                    });
                }
                if self.tools_left <= 0 {
                    let wait = self.exp_ms(self.profile.mean_idle_ms);
                    self.next_at = now + wait;
                    self.current_status_kind = "idle".to_string();
                    return Some(Signal::Stop);
                }
                if self.chance(self.profile.crash_prob_per_turn / self.profile.mean_tools_per_turn)
                {
                    self.next_at = u64::MAX;
                    self.current_status_kind = "dead".to_string();
                    return None;
                }
                if self.chance(self.profile.error_prob_per_turn / self.profile.mean_tools_per_turn)
                {
                    let wait = self.exp_ms(self.profile.mean_idle_ms);
                    self.next_at = now + wait;
                    self.current_status_kind = "error".to_string();
                    return Some(Signal::Failure);
                }

                self.tool_open = true;
                self.permission_pending = self.chance(self.profile.permission_prob);
                let wait = if self.permission_pending {
                    self.exp_ms(self.profile.mean_think_ms / 4.0)
                } else {
                    self.exp_ms(self.profile.mean_tool_ms)
                };
                self.next_at = now + wait;
                self.current_tool_key += 1;
                let tool_idx = self.rng.random_range(0..self.tools.len());
                let tool_name = self.tools[tool_idx].clone();
                let key = ToolUseKey::new(format!("sim-tool-{}", self.current_tool_key));
                Some(Signal::ToolStart {
                    key,
                    tool: ToolName::new(tool_name),
                })
            }
            _ => None,
        }
    }
}

pub struct SimFleet {
    _config: ProfilesConfig,
    agents: Vec<SimAgent>,
    pub now: u64,
}

impl SimFleet {
    pub fn new(spec: SimSpec) -> Self {
        let config_str = include_str!("../../../../fixtures/sim/profiles.json");
        let config: ProfilesConfig =
            serde_json::from_str(config_str).expect("fixtures/sim/profiles.json válido");

        let mut agents = Vec::with_capacity(spec.count);
        let now = config.epoch_ms;

        for i in 0..spec.count {
            // seed ^ índice multiplicado para dispersión uniforme
            let agent_seed = spec.seed ^ (i as u64).wrapping_mul(0x9e3779b185ebca87);
            let mut rng = ChaCha8Rng::seed_from_u64(agent_seed);

            // Elegir provider ponderado
            let total_provider_weight: f64 = config.providers.iter().map(|p| p.weight).sum();
            let mut u: f64 = rng.random::<f64>() * total_provider_weight;
            let mut chosen_provider = &config.providers[0];
            for p in &config.providers {
                u -= p.weight;
                if u < 0.0 {
                    chosen_provider = p;
                    break;
                }
            }

            // Elegir profile ponderado
            let total_profile_weight: f64 = config.profiles.iter().map(|p| p.weight).sum();
            let mut v: f64 = rng.random::<f64>() * total_profile_weight;
            let mut chosen_profile = &config.profiles[0];
            for pr in &config.profiles {
                v -= pr.weight;
                if v < 0.0 {
                    chosen_profile = pr;
                    break;
                }
            }

            let provider = match chosen_provider.provider.as_str() {
                "claude" => Provider::Claude,
                "codex" => Provider::Codex,
                _ => Provider::Gemini,
            };

            let pid = rng.random_range(1000..99_999);
            let started_offset = rng.random_range(60_000..3 * 3_600_000);
            let started_at = now.saturating_sub(started_offset);

            let project_idx = i % config.projects.len();
            let round = i / config.projects.len();
            let project_name = if round == 0 {
                config.projects[project_idx].clone()
            } else {
                format!("{}-{}", config.projects[project_idx], round + 1)
            };

            let identity = ProcessIdentity {
                pid: Pid(pid),
                start_time: ProcessStartTime(started_at),
                uid: 501,
                cmd_fingerprint: i as u64,
            };

            let detected = DetectedAgent {
                provider,
                identity,
                cwd: WorkingDir::new(format!("/Users/dev/{project_name}")),
            };

            let id = AgentId {
                provider,
                pid: identity.pid,
                start_time: identity.start_time,
            };

            let initial_wait = rng.random_range(0..chosen_profile.mean_idle_ms.max(100.0) as u64);

            agents.push(SimAgent {
                id,
                detected,
                rng,
                profile: chosen_profile.clone(),
                tools: chosen_provider.tools.clone(),
                tools_left: 0,
                tool_open: false,
                permission_pending: false,
                next_at: now + initial_wait,
                current_tool_key: 0,
                current_status_kind: "idle".to_string(),
            });
        }

        Self {
            _config: config,
            agents,
            now,
        }
    }

    pub fn detected_agents(&self) -> Vec<DetectedAgent> {
        self.agents.iter().map(|a| a.detected.clone()).collect()
    }

    pub fn step(&mut self) -> Vec<(AgentId, Signal)> {
        self.now += TICK_MS;
        let mut signals = Vec::new();

        for agent in &mut self.agents {
            if agent.next_at <= self.now
                && let Some(sig) = agent.next_signal(self.now)
            {
                signals.push((agent.id, sig));
            }
        }

        signals
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sim_spec_formats() {
        assert_eq!(
            SimSpec::parse("20:42"),
            Some(SimSpec {
                count: 20,
                seed: 42,
                freeze: false
            })
        );
        assert_eq!(
            SimSpec::parse("50:123:freeze"),
            Some(SimSpec {
                count: 50,
                seed: 123,
                freeze: true
            })
        );
        assert_eq!(
            SimSpec::parse("5"),
            Some(SimSpec {
                count: 5,
                seed: 42,
                freeze: false
            })
        );
    }

    #[test]
    fn determinismo_misma_semilla_da_mismos_agentes_y_senales() {
        let mut fleet1 = SimFleet::new(SimSpec {
            count: 5,
            seed: 42,
            freeze: false,
        });
        let mut fleet2 = SimFleet::new(SimSpec {
            count: 5,
            seed: 42,
            freeze: false,
        });

        assert_eq!(fleet1.detected_agents(), fleet2.detected_agents());

        for _ in 0..50 {
            let sigs1 = fleet1.step();
            let sigs2 = fleet2.step();
            assert_eq!(sigs1, sigs2);
        }
    }

    #[test]
    fn independencia_por_agente_primer_agente_identico_con_distinto_tamano() {
        let mut fleet1 = SimFleet::new(SimSpec {
            count: 1,
            seed: 99,
            freeze: false,
        });
        let mut fleet2 = SimFleet::new(SimSpec {
            count: 10,
            seed: 99,
            freeze: false,
        });

        assert_eq!(fleet1.detected_agents()[0], fleet2.detected_agents()[0]);

        let id = fleet1.detected_agents()[0].identity.pid;

        for _ in 0..30 {
            let s1: Vec<_> = fleet1
                .step()
                .into_iter()
                .filter(|(a, _)| a.pid == id)
                .collect();
            let s2: Vec<_> = fleet2
                .step()
                .into_iter()
                .filter(|(a, _)| a.pid == id)
                .collect();
            assert_eq!(s1, s2);
        }
    }
}
