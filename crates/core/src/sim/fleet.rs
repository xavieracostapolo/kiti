//! Flota de agentes simulados determinista (ADR 0002 §9, ADR 0006 §9).
//!
//! Lee `fixtures/sim/profiles.json`, usa `ChaCha8Rng` por agente con `seed ^ index`,
//! y avanza el tiempo virtual paso a paso.

use rand::RngExt;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;

use crate::domain::{
    AgentId, AgentInput, DetectedAgent, Pid, ProcessIdentity, ProcessStartTime, Provider, Signal,
    StatusKind, ToolName, ToolOutcome, ToolUseKey, WorkingDir,
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

/// Oficinas por proyecto, en ciclo: como quien abre varias sesiones en el mismo repositorio
/// (ADR 0009). Igual que `projectFor` del simulador de la UI.
const GROUP_SIZES: [usize; 8] = [3, 1, 4, 2, 2, 1, 3, 2];

/// Proyecto del agente `i`: no depende de cuántos agentes haya.
fn project_index(i: usize, projects: usize) -> usize {
    let cycle: usize = GROUP_SIZES.iter().sum();
    let mut rest = i % cycle;
    let mut group = (i / cycle) * GROUP_SIZES.len();
    for size in GROUP_SIZES {
        if rest < size {
            break;
        }
        rest -= size;
        group += 1;
    }
    group % projects
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
    /// Ya se emitió `ProcessGone`: el agente no vuelve a hablar.
    gone_sent: bool,
    /// Lo que el simulador cree que verá el core; solo guía el modelo de turnos.
    kind: StatusKind,
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

    fn silence(&mut self) {
        self.next_at = u64::MAX;
        self.kind = StatusKind::Dead;
    }

    fn next_input(&mut self, now: u64) -> Option<AgentInput> {
        self.next_signal(now).map(AgentInput::Signal).or_else(|| {
            // `next_signal` devuelve None solo al caerse el proceso (o si ya estaba caído).
            (self.next_at == u64::MAX && self.kind == StatusKind::Dead && !self.gone_sent).then(
                || {
                    self.gone_sent = true;
                    AgentInput::ProcessGone
                },
            )
        })
    }

    fn next_signal(&mut self, now: u64) -> Option<Signal> {
        match self.kind {
            StatusKind::Dead => {
                self.next_at = u64::MAX;
                None
            }
            StatusKind::Idle | StatusKind::Error => {
                let u: f64 = self.rng.random();
                let tools =
                    1 + (-self.profile.mean_tools_per_turn * (1.0f64 - u).ln()).floor() as i32;
                self.tools_left = tools.max(1);
                let wait = self.exp_ms(self.profile.mean_think_ms);
                self.next_at = now + wait;
                self.kind = StatusKind::Working;
                Some(Signal::PromptSubmit)
            }
            StatusKind::Waiting => {
                self.tool_open = false;
                self.tools_left -= 1;
                let wait = self.exp_ms(self.profile.mean_think_ms);
                self.next_at = now + wait;
                self.kind = StatusKind::Working;
                let key = ToolUseKey::new(format!("sim-tool-{}", self.current_tool_key));
                Some(Signal::ToolEnd {
                    key,
                    outcome: ToolOutcome::Ok,
                })
            }
            StatusKind::Working => {
                if self.tool_open && self.permission_pending {
                    self.permission_pending = false;
                    let wait = self.exp_ms(self.profile.mean_permission_wait_ms);
                    self.next_at = now + wait;
                    self.kind = StatusKind::Waiting;
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
                    self.kind = StatusKind::Idle;
                    return Some(Signal::Stop);
                }
                if self.chance(self.profile.crash_prob_per_turn / self.profile.mean_tools_per_turn)
                {
                    self.silence();
                    return None;
                }
                if self.chance(self.profile.error_prob_per_turn / self.profile.mean_tools_per_turn)
                {
                    let wait = self.exp_ms(self.profile.mean_idle_ms);
                    self.next_at = now + wait;
                    self.kind = StatusKind::Error;
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
        }
    }
}

pub struct SimFleet {
    _config: ProfilesConfig,
    agents: Vec<SimAgent>,
    pub now: u64,
}

impl SimFleet {
    /// Reloj virtual desde `epochMs` del fixture: determinista para tests y benches.
    pub fn new(spec: SimSpec) -> Self {
        Self::build(spec, None)
    }

    /// Reloj virtual alineado con el reloj real (`Clock::now()`), para la app: si no, la
    /// caducidad del runtime vería a todos los agentes en silencio desde hace meses.
    pub fn starting_at(spec: SimSpec, epoch_ms: u64) -> Self {
        Self::build(spec, Some(epoch_ms))
    }

    fn build(spec: SimSpec, epoch_ms: Option<u64>) -> Self {
        let config_str = include_str!("../../../../fixtures/sim/profiles.json");
        let config: ProfilesConfig =
            serde_json::from_str(config_str).expect("fixtures/sim/profiles.json válido");

        let mut agents = Vec::with_capacity(spec.count);
        let now = epoch_ms.unwrap_or(config.epoch_ms);

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

            let project_name = &config.projects[project_index(i, config.projects.len())];

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
                gone_sent: false,
                kind: StatusKind::Idle,
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

    /// El usuario cerró la oficina: el agente deja de emitir (el runtime ya lo quitó).
    pub fn forget(&mut self, id: AgentId) {
        if let Some(a) = self.agents.iter_mut().find(|a| a.id == id) {
            a.silence();
            a.gone_sent = true;
        }
    }

    pub fn step(&mut self) -> Vec<(AgentId, AgentInput)> {
        self.now += TICK_MS;
        let mut inputs = Vec::new();
        for agent in &mut self.agents {
            let due =
                agent.next_at <= self.now || (agent.kind == StatusKind::Dead && !agent.gone_sent);
            if due && let Some(input) = agent.next_input(self.now) {
                inputs.push((agent.id, input));
            }
        }
        inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(count: usize, seed: u64) -> SimSpec {
        SimSpec {
            count,
            seed,
            freeze: false,
        }
    }

    #[test]
    fn un_crash_emite_process_gone_y_el_agente_calla() {
        let mut fleet = SimFleet::new(spec(20, 42));
        let mut gone = std::collections::HashSet::new();
        for _ in 0..2_000 {
            for (id, input) in fleet.step() {
                assert!(!gone.contains(&id), "{id} habló después de morir");
                if input == AgentInput::ProcessGone {
                    gone.insert(id);
                }
            }
        }
        assert!(
            !gone.is_empty(),
            "con el perfil flaky algún agente debe caerse"
        );
    }

    #[test]
    fn starting_at_alinea_el_reloj_virtual_con_el_real() {
        let fleet = SimFleet::starting_at(spec(3, 1), 1_800_000_000_000);
        assert_eq!(fleet.now, 1_800_000_000_000);
        for d in fleet.detected_agents() {
            assert!(d.identity.start_time.0 < 1_800_000_000_000);
        }
    }

    #[test]
    fn forget_silencia_al_agente_cerrado() {
        let mut fleet = SimFleet::new(spec(5, 7));
        let target = fleet.detected_agents()[0].clone();
        let id = AgentId {
            provider: target.provider,
            pid: target.identity.pid,
            start_time: target.identity.start_time,
        };
        fleet.forget(id);
        for _ in 0..500 {
            assert!(fleet.step().iter().all(|(a, _)| *a != id));
        }
    }

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
