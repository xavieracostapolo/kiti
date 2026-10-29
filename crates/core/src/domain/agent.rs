//! Agregado raíz `Agent` y su reducer puro (ADR 0002 §2–§3).

use smallvec::SmallVec;

use super::events::DomainEvent;
use super::ids::{
    AgentId, ProcessIdentity, Provider, SessionId, Timestamp, ToolName, ToolUseKey, WorkingDir,
};
use super::memory::{TASKS_MAX, TOUCHED_MAX, TaskItem};
use super::signal::{AgentInfo, Signal};
use super::stale::StalePolicy;
use super::status::{Status, StatusKind, StatusReason, TrafficLight};

pub type Events = SmallVec<[DomainEvent; 2]>;

/// Cambios de estado que se recuerdan para la línea de tiempo del tablero (ADR 0016).
pub const HISTORY_MAX: usize = 32;

/// Datos de la sesión que da la status line de Claude (ADR 0016). `None` = no se sabe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentStats {
    pub model: Option<String>,
    /// Contexto usado, en porcentaje.
    pub context_pct: Option<u8>,
    /// Coste estimado de la sesión, en céntimos de dólar.
    pub cost_cents: Option<u32>,
}

/// Un proceso ya clasificado como agente. En el Hito 3 lo produce `classify` desde un
/// `ProcessSnapshot`; en el Hito 2, `SimFleet`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedAgent {
    pub provider: Provider,
    pub identity: ProcessIdentity,
    pub cwd: WorkingDir,
}

/// Entrada ya enrutada a un agente concreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentInput {
    Signal(Signal),
    /// El escaneo de procesos ya no ve el proceso.
    ProcessGone,
    /// El usuario confirmó cerrar la oficina: la salida que siga será esperada.
    KillRequested,
    Tick(StalePolicy),
    /// Solo Claude: `status` de `~/.claude/sessions/<pid>.json`. Segunda fuente tras los hooks.
    SessionFile(SessionActivity),
    /// Última herramienta vista en el transcript. Solo enriquece: no cambia el color.
    TranscriptTool(ToolName),
    /// Título, actividad y último pedido (ADR 0009). Solo enriquece: no cambia el color.
    Info(AgentInfo),
    /// El registro enlazó la conversación que el proceso tiene abierta (ADR 0009).
    Linked(SessionId),
    /// Modelo, contexto y coste de la sesión (ADR 0016). Solo enriquece: no cambia el color.
    Stats(AgentStats),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionActivity {
    Busy,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Agent {
    id: AgentId,
    identity: ProcessIdentity,
    cwd: WorkingDir,
    status: Status,
    pending_tools: SmallVec<[(ToolUseKey, ToolName); 4]>,
    last_tool: Option<ToolName>,
    title: Option<String>,
    activity: Option<String>,
    initial_prompt: Option<String>,
    last_prompt: Option<String>,
    touched_files: std::collections::VecDeque<String>,
    tasks: Vec<TaskItem>,
    session: Option<SessionId>,
    stats: AgentStats,
    /// Últimos cambios de estado `(estado, desde)`, el más reciente al final.
    history: std::collections::VecDeque<(StatusKind, Timestamp)>,
    last_event_at: Timestamp,
    exit_expected: bool,
    kill_requested: bool,
    /// Sube con cada cambio visible para la UI. El registro lo usa para saber qué enviar.
    revision: u64,
}

impl Agent {
    pub fn new(detected: DetectedAgent, now: Timestamp) -> Self {
        let id = AgentId {
            provider: detected.provider,
            pid: detected.identity.pid,
            start_time: detected.identity.start_time,
        };
        Self {
            id,
            identity: detected.identity,
            cwd: detected.cwd,
            status: Status {
                kind: StatusKind::Idle,
                reason: StatusReason::Startup,
                since: now,
            },
            pending_tools: SmallVec::new(),
            last_tool: None,
            title: None,
            activity: None,
            initial_prompt: None,
            last_prompt: None,
            touched_files: std::collections::VecDeque::new(),
            tasks: Vec::new(),
            session: None,
            stats: AgentStats::default(),
            history: std::collections::VecDeque::from([(StatusKind::Idle, now)]),
            last_event_at: now,
            exit_expected: false,
            kill_requested: false,
            revision: 0,
        }
    }

    pub fn id(&self) -> AgentId {
        self.id
    }

    pub fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }

    pub fn cwd(&self) -> &WorkingDir {
        &self.cwd
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn light(&self) -> TrafficLight {
        self.status.kind.into()
    }

    pub fn is_dead(&self) -> bool {
        self.status.kind == StatusKind::Dead
    }

    pub fn last_tool(&self) -> Option<&ToolName> {
        self.last_tool.as_ref()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn activity(&self) -> Option<&str> {
        self.activity.as_deref()
    }

    pub fn last_prompt(&self) -> Option<&str> {
        self.last_prompt.as_deref()
    }

    pub fn initial_prompt(&self) -> Option<&str> {
        self.initial_prompt.as_deref()
    }

    pub fn touched_files(&self) -> &std::collections::VecDeque<String> {
        &self.touched_files
    }

    pub fn tasks(&self) -> &[TaskItem] {
        &self.tasks
    }

    pub fn stats(&self) -> &AgentStats {
        &self.stats
    }

    /// Últimos cambios de estado, el más reciente al final (ADR 0016).
    pub fn history(&self) -> &std::collections::VecDeque<(StatusKind, Timestamp)> {
        &self.history
    }

    /// Conversación abierta, si ya se enlazó.
    pub fn session(&self) -> Option<&SessionId> {
        self.session.as_ref()
    }

    pub fn last_event_at(&self) -> Timestamp {
        self.last_event_at
    }

    pub fn pending_tools(&self) -> impl ExactSizeIterator<Item = &ToolUseKey> {
        self.pending_tools.iter().map(|(key, _)| key)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Puro y determinista: sin I/O, sin reloj, sin aleatoriedad.
    pub fn apply(&mut self, input: &AgentInput, now: Timestamp) -> Events {
        let mut ev = Events::new();
        if self.is_dead() {
            if let AgentInput::Tick(policy) = input
                && now.since(self.status.since) >= policy.dead_ttl
            {
                ev.push(DomainEvent::AgentRemoved { id: self.id });
            }
            return ev;
        }
        match input {
            AgentInput::Signal(signal) => self.on_signal(signal, now, &mut ev),
            AgentInput::ProcessGone => {
                if self.exit_expected || self.kill_requested {
                    ev.push(DomainEvent::AgentExited {
                        id: self.id,
                        expected: true,
                    });
                    ev.push(DomainEvent::AgentRemoved { id: self.id });
                } else {
                    self.pending_tools.clear();
                    self.set_status(StatusKind::Dead, StatusReason::ProcessGone, now, &mut ev);
                    ev.push(DomainEvent::AgentExited {
                        id: self.id,
                        expected: false,
                    });
                }
            }
            AgentInput::KillRequested => self.kill_requested = true,
            AgentInput::Tick(policy) => self.on_tick(policy, now, &mut ev),
            AgentInput::SessionFile(activity) => self.on_session_file(*activity, now, &mut ev),
            AgentInput::Info(info) => {
                let mut changed = false;
                if info.prompt.is_some() && self.initial_prompt.is_none() {
                    self.initial_prompt.clone_from(&info.prompt);
                    changed = true;
                }
                for (field, value) in [
                    (&mut self.title, &info.title),
                    (&mut self.activity, &info.activity),
                    (&mut self.last_prompt, &info.prompt),
                ] {
                    if value.is_some() && field != value {
                        field.clone_from(value);
                        changed = true;
                    }
                }
                if let Some(file) = &info.touched_file
                    && !file.is_empty()
                    && self.touched_files.back() != Some(file)
                {
                    self.touched_files.retain(|f| f != file);
                    self.touched_files.push_back(file.clone());
                    while self.touched_files.len() > TOUCHED_MAX {
                        self.touched_files.pop_front();
                    }
                    changed = true;
                }
                if let Some(tasks) = &info.tasks {
                    let capped: Vec<TaskItem> = tasks.iter().take(TASKS_MAX).cloned().collect();
                    if self.tasks != capped {
                        self.tasks = capped;
                        changed = true;
                    }
                }
                if changed {
                    self.revision += 1;
                }
            }
            AgentInput::Stats(stats) => {
                if &self.stats != stats {
                    self.stats = stats.clone();
                    self.revision += 1;
                }
            }
            AgentInput::Linked(session) => {
                if self.session.as_ref() != Some(session) {
                    self.session = Some(session.clone());
                    self.revision += 1;
                }
            }
            AgentInput::TranscriptTool(tool) => {
                if self.last_tool.as_ref() != Some(tool) {
                    self.last_tool = Some(tool.clone());
                    self.revision += 1;
                }
            }
        }
        ev
    }

    fn on_signal(&mut self, signal: &Signal, now: Timestamp, ev: &mut Events) {
        use StatusKind::*;
        match signal {
            Signal::Unknown { .. } => return,
            Signal::ToolEnd { key, .. } if !self.pending_tools.iter().any(|(k, _)| k == key) => {
                return;
            }
            _ => {}
        }
        self.last_event_at = now;
        self.revision += 1;
        match signal {
            Signal::SessionStart => {
                // Conversación nueva (arranque, /clear, resume): su título y pedido llegarán.
                self.title = None;
                self.activity = None;
                self.initial_prompt = None;
                self.last_prompt = None;
                self.touched_files.clear();
                self.tasks.clear();
                self.pending_tools.clear();
                self.exit_expected = false;
                ev.push(DomainEvent::SessionStarted { id: self.id });
                self.set_status(Idle, StatusReason::Hook, now, ev);
            }
            Signal::PromptSubmit => self.set_status(Working, StatusReason::Hook, now, ev),
            Signal::ToolStart { key, tool } => {
                if !self.pending_tools.iter().any(|(k, _)| k == key) {
                    self.pending_tools.push((key.clone(), tool.clone()));
                }
                self.last_tool = Some(tool.clone());
                ev.push(DomainEvent::ToolStarted {
                    id: self.id,
                    tool: tool.clone(),
                });
                self.set_status(Working, StatusReason::Hook, now, ev);
            }
            Signal::ToolEnd { key, outcome } => {
                let index = self
                    .pending_tools
                    .iter()
                    .position(|(k, _)| k == key)
                    .expect("filtrado arriba");
                let (_, tool) = self.pending_tools.remove(index);
                ev.push(DomainEvent::ToolFinished {
                    id: self.id,
                    tool,
                    outcome: *outcome,
                });
                if matches!(self.status.kind, Working | Waiting) {
                    self.set_status(Working, StatusReason::Hook, now, ev);
                }
            }
            Signal::AwaitingInput => {
                if self.status.kind == Working {
                    self.set_status(Waiting, StatusReason::Hook, now, ev);
                }
            }
            Signal::Stop => {
                self.activity = None;
                self.pending_tools.clear();
                if matches!(self.status.kind, Working | Waiting) {
                    self.set_status(Idle, StatusReason::Hook, now, ev);
                }
            }
            Signal::Failure => {
                self.pending_tools.clear();
                self.set_status(Error, StatusReason::Failure, now, ev);
            }
            Signal::SessionEnd => {
                self.pending_tools.clear();
                self.exit_expected = true;
                self.set_status(Idle, StatusReason::Hook, now, ev);
            }
            Signal::Unknown { .. } => unreachable!("filtrado arriba"),
        }
    }

    /// `idle` cierra verde y amarillo (cubre Esc, que no envía `Stop`); `busy` solo abre el
    /// verde desde gris: un amarillo o un rojo vienen de hooks más precisos.
    fn on_session_file(&mut self, activity: SessionActivity, now: Timestamp, ev: &mut Events) {
        use StatusKind::*;
        match (activity, self.status.kind) {
            (SessionActivity::Idle, Working | Waiting) => {
                self.pending_tools.clear();
                self.set_status(Idle, StatusReason::SessionFile, now, ev);
            }
            (SessionActivity::Busy, Idle) => {
                self.set_status(Working, StatusReason::SessionFile, now, ev);
            }
            _ => {}
        }
    }

    fn on_tick(&mut self, policy: &StalePolicy, now: Timestamp, ev: &mut Events) {
        if self.status.kind != StatusKind::Working {
            return;
        }
        let quiet = now.since(self.last_event_at);
        let limit = if self.pending_tools.is_empty() {
            policy.idle_after
        } else {
            policy.pending_max
        };
        if quiet >= limit {
            self.pending_tools.clear();
            self.set_status(StatusKind::Idle, StatusReason::Stale, now, ev);
        }
    }

    /// `since` solo cambia con el color; la revisión sube si cambia el color o el motivo.
    fn set_status(
        &mut self,
        kind: StatusKind,
        reason: StatusReason,
        now: Timestamp,
        ev: &mut Events,
    ) {
        let from = self.status.kind;
        if from == kind && self.status.reason == reason {
            return;
        }
        if from != kind {
            self.status.since = now;
            self.history.push_back((kind, now));
            if self.history.len() > HISTORY_MAX {
                self.history.pop_front();
            }
            ev.push(DomainEvent::StatusChanged {
                id: self.id,
                from,
                to: kind,
                reason,
                at: now,
            });
        }
        self.status.kind = kind;
        self.status.reason = reason;
        self.revision += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{Pid, ProcessStartTime};
    use crate::domain::signal::{AgentInfo, ToolOutcome};

    const T0: Timestamp = Timestamp(1_000_000);

    fn t(ms: u64) -> Timestamp {
        Timestamp(T0.0 + ms)
    }

    fn agent() -> Agent {
        Agent::new(
            DetectedAgent {
                provider: Provider::Claude,
                identity: ProcessIdentity {
                    pid: Pid(4242),
                    start_time: ProcessStartTime(900_000),
                    uid: 501,
                    cmd_fingerprint: 7,
                },
                cwd: WorkingDir::new("/Users/dev/kiti"),
            },
            T0,
        )
    }

    fn sig(a: &mut Agent, s: Signal, at: Timestamp) -> Events {
        a.apply(&AgentInput::Signal(s), at)
    }

    fn tool_start(key: &str, tool: &str) -> Signal {
        Signal::ToolStart {
            key: ToolUseKey::new(key),
            tool: ToolName::new(tool),
        }
    }

    fn tool_end(key: &str, outcome: ToolOutcome) -> Signal {
        Signal::ToolEnd {
            key: ToolUseKey::new(key),
            outcome,
        }
    }

    fn tick(a: &mut Agent, at: Timestamp) -> Events {
        a.apply(&AgentInput::Tick(StalePolicy::default()), at)
    }

    fn working() -> Agent {
        let mut a = agent();
        sig(&mut a, Signal::PromptSubmit, t(0));
        a
    }

    fn kind(a: &Agent) -> StatusKind {
        a.status().kind
    }

    fn removed(ev: &Events) -> bool {
        ev.iter()
            .any(|e| matches!(e, DomainEvent::AgentRemoved { .. }))
    }

    #[test]
    fn nace_en_gris_por_arranque() {
        let a = agent();
        assert_eq!(a.status().reason, StatusReason::Startup);
        assert_eq!(a.light(), TrafficLight::Gray);
        assert_eq!(a.id().to_string(), "claude:4242:900000");
    }

    #[test]
    fn permission_request_pone_amarillo_y_stop_vuelve_a_gris() {
        let mut a = working();
        let ev = sig(&mut a, Signal::AwaitingInput, t(10));
        assert_eq!(a.light(), TrafficLight::Yellow);
        assert_eq!(
            ev[..],
            [DomainEvent::StatusChanged {
                id: a.id(),
                from: StatusKind::Working,
                to: StatusKind::Waiting,
                reason: StatusReason::Hook,
                at: t(10),
            }]
        );
        sig(&mut a, Signal::Stop, t(20));
        assert_eq!(a.light(), TrafficLight::Gray);
    }

    #[test]
    fn prompt_y_tool_start_ponen_verde_desde_idle_waiting_y_error() {
        for prepare in [
            |_: &mut Agent| {},
            |a: &mut Agent| {
                sig(a, Signal::PromptSubmit, t(1));
                sig(a, Signal::AwaitingInput, t(2));
            },
            |a: &mut Agent| {
                sig(a, Signal::Failure, t(1));
            },
        ] {
            let mut a = agent();
            prepare(&mut a);
            sig(&mut a, tool_start("k1", "Bash"), t(5));
            assert_eq!(kind(&a), StatusKind::Working);
            assert_eq!(a.last_tool().map(ToolName::as_str), Some("Bash"));
        }
    }

    #[test]
    fn un_fallo_de_herramienta_no_es_rojo() {
        let mut a = working();
        sig(&mut a, tool_start("k1", "Edit"), t(1));
        let ev = sig(&mut a, tool_end("k1", ToolOutcome::Failed), t(2));
        assert_eq!(kind(&a), StatusKind::Working);
        assert_eq!(a.pending_tools().len(), 0);
        assert!(ev.contains(&DomainEvent::ToolFinished {
            id: a.id(),
            tool: ToolName::new("Edit"),
            outcome: ToolOutcome::Failed,
        }));
    }

    #[test]
    fn tool_end_sin_su_tool_start_se_ignora() {
        let mut a = working();
        sig(&mut a, Signal::AwaitingInput, t(1));
        let rev = a.revision();
        let ev = sig(&mut a, tool_end("huerfana", ToolOutcome::Ok), t(2));
        assert!(ev.is_empty());
        assert_eq!(kind(&a), StatusKind::Waiting);
        assert_eq!(a.revision(), rev);
    }

    #[test]
    fn awaiting_input_solo_cuenta_desde_working() {
        let mut a = agent();
        sig(&mut a, Signal::AwaitingInput, t(1));
        assert_eq!(kind(&a), StatusKind::Idle);
    }

    #[test]
    fn stop_vacia_las_herramientas_abiertas() {
        let mut a = working();
        sig(&mut a, tool_start("k1", "Bash"), t(1));
        sig(&mut a, tool_start("k2", "Read"), t(2));
        sig(&mut a, Signal::Stop, t(3));
        assert_eq!(kind(&a), StatusKind::Idle);
        assert_eq!(a.pending_tools().len(), 0);
    }

    #[test]
    fn failure_es_rojo_hasta_el_siguiente_prompt() {
        let mut a = working();
        sig(&mut a, Signal::Failure, t(1));
        assert_eq!(a.status().reason, StatusReason::Failure);
        assert_eq!(a.light(), TrafficLight::Red);
        sig(&mut a, Signal::Stop, t(2));
        assert_eq!(kind(&a), StatusKind::Error);
        sig(&mut a, Signal::PromptSubmit, t(3));
        assert_eq!(kind(&a), StatusKind::Working);
    }

    #[test]
    fn process_gone_sin_causa_es_dead_y_absorbente() {
        let mut a = working();
        let ev = a.apply(&AgentInput::ProcessGone, t(1));
        assert_eq!(kind(&a), StatusKind::Dead);
        assert_eq!(a.status().reason, StatusReason::ProcessGone);
        assert!(ev.contains(&DomainEvent::AgentExited {
            id: a.id(),
            expected: false,
        }));
        assert!(!removed(&ev));
        for s in [Signal::PromptSubmit, Signal::SessionStart, Signal::Stop] {
            assert!(sig(&mut a, s, t(2)).is_empty());
            assert_eq!(kind(&a), StatusKind::Dead);
        }
    }

    #[test]
    fn session_end_hace_esperada_la_salida() {
        let mut a = working();
        sig(&mut a, Signal::SessionEnd, t(1));
        assert_eq!(kind(&a), StatusKind::Idle);
        let ev = a.apply(&AgentInput::ProcessGone, t(2));
        assert!(ev.contains(&DomainEvent::AgentExited {
            id: a.id(),
            expected: true,
        }));
        assert!(removed(&ev));
    }

    #[test]
    fn session_start_olvida_un_session_end_anterior() {
        let mut a = working();
        sig(&mut a, Signal::SessionEnd, t(1));
        let ev = sig(&mut a, Signal::SessionStart, t(2));
        assert!(ev.contains(&DomainEvent::SessionStarted { id: a.id() }));
        a.apply(&AgentInput::ProcessGone, t(3));
        assert_eq!(kind(&a), StatusKind::Dead);
    }

    #[test]
    fn kill_pedido_hace_esperada_la_salida() {
        let mut a = working();
        a.apply(&AgentInput::KillRequested, t(1));
        let ev = a.apply(&AgentInput::ProcessGone, t(2));
        assert!(removed(&ev));
    }

    #[test]
    fn since_solo_cambia_cuando_cambia_el_color() {
        let mut a = working();
        sig(&mut a, tool_start("k1", "Read"), t(10));
        assert_eq!(a.status().since, t(0));
        sig(&mut a, Signal::AwaitingInput, t(30));
        assert_eq!(a.status().since, t(30));
    }

    #[test]
    fn working_sin_senales_30_s_pasa_a_idle_por_stale() {
        let mut a = working();
        tick(&mut a, t(29_999));
        assert_eq!(kind(&a), StatusKind::Working);
        tick(&mut a, t(30_000));
        assert_eq!(kind(&a), StatusKind::Idle);
        assert_eq!(a.status().reason, StatusReason::Stale);
    }

    #[test]
    fn con_herramientas_abiertas_espera_10_min() {
        let mut a = working();
        sig(&mut a, tool_start("k1", "Bash"), t(0));
        tick(&mut a, t(60_000));
        assert_eq!(kind(&a), StatusKind::Working);
        tick(&mut a, t(600_000));
        assert_eq!(kind(&a), StatusKind::Idle);
        assert_eq!(a.pending_tools().len(), 0);
    }

    #[test]
    fn waiting_nunca_caduca() {
        let mut a = working();
        sig(&mut a, Signal::AwaitingInput, t(0));
        tick(&mut a, t(24 * 3_600_000));
        assert_eq!(kind(&a), StatusKind::Waiting);
    }

    #[test]
    fn dead_se_retira_tras_el_ttl() {
        let mut a = working();
        a.apply(&AgentInput::ProcessGone, t(0));
        assert!(!removed(&tick(&mut a, t(59_999))));
        assert!(removed(&tick(&mut a, t(60_000))));
    }

    #[test]
    fn cada_senal_refresca_last_event_at_y_la_revision() {
        let mut a = working();
        let rev = a.revision();
        sig(&mut a, tool_start("k1", "Grep"), t(7));
        assert_eq!(a.last_event_at(), t(7));
        assert!(a.revision() > rev);
    }

    #[test]
    fn unknown_no_cambia_nada() {
        let mut a = working();
        let rev = a.revision();
        let ev = sig(&mut a, Signal::Unknown { name: "Foo".into() }, t(5));
        assert!(ev.is_empty());
        assert_eq!(a.revision(), rev);
        assert_eq!(a.last_event_at(), t(0));
    }

    #[test]
    fn un_tick_sin_cambios_no_toca_la_revision() {
        let mut a = working();
        let rev = a.revision();
        tick(&mut a, t(1_000));
        assert_eq!(a.revision(), rev);
    }

    fn session_file(a: &mut Agent, activity: SessionActivity, at: Timestamp) -> Events {
        a.apply(&AgentInput::SessionFile(activity), at)
    }

    #[test]
    fn session_file_idle_cierra_el_verde_aunque_no_llegue_stop() {
        // Interrupción con Esc: Claude no envía Stop, pero sessions/<pid>.json pasa a idle.
        let mut a = working();
        sig(&mut a, tool_start("k1", "Bash"), t(1));
        session_file(&mut a, SessionActivity::Idle, t(2));
        assert_eq!(kind(&a), StatusKind::Idle);
        assert_eq!(a.status().reason, StatusReason::SessionFile);
        assert_eq!(a.pending_tools().len(), 0);
    }

    #[test]
    fn session_file_idle_tambien_cierra_el_amarillo() {
        let mut a = working();
        sig(&mut a, Signal::AwaitingInput, t(1));
        session_file(&mut a, SessionActivity::Idle, t(2));
        assert_eq!(kind(&a), StatusKind::Idle);
    }

    #[test]
    fn session_file_busy_abre_el_verde_solo_desde_gris() {
        let mut a = agent();
        session_file(&mut a, SessionActivity::Busy, t(1));
        assert_eq!(kind(&a), StatusKind::Working);
        assert_eq!(a.status().reason, StatusReason::SessionFile);

        // No pisa un amarillo ni un rojo, que vienen de hooks más precisos.
        let mut waiting = working();
        sig(&mut waiting, Signal::AwaitingInput, t(1));
        session_file(&mut waiting, SessionActivity::Busy, t(2));
        assert_eq!(kind(&waiting), StatusKind::Waiting);
        let mut failed = working();
        sig(&mut failed, Signal::Failure, t(1));
        session_file(&mut failed, SessionActivity::Busy, t(2));
        assert_eq!(kind(&failed), StatusKind::Error);
    }

    #[test]
    fn session_file_sin_cambio_no_genera_eventos() {
        let mut a = agent();
        let rev = a.revision();
        assert!(session_file(&mut a, SessionActivity::Idle, t(1)).is_empty());
        assert_eq!(a.revision(), rev);
    }

    #[test]
    fn el_transcript_solo_actualiza_la_ultima_herramienta() {
        let mut a = agent();
        let rev = a.revision();
        let ev = a.apply(&AgentInput::TranscriptTool(ToolName::new("Grep")), t(5));
        assert!(ev.is_empty(), "no cambia el semáforo");
        assert_eq!(kind(&a), StatusKind::Idle);
        assert_eq!(a.last_tool().map(ToolName::as_str), Some("Grep"));
        assert!(a.revision() > rev, "la UI debe recibirlo");
        assert_eq!(
            a.last_event_at(),
            T0,
            "no cuenta como actividad para la caducidad"
        );
        let rev = a.revision();
        a.apply(&AgentInput::TranscriptTool(ToolName::new("Grep")), t(6));
        assert_eq!(a.revision(), rev, "repetida no genera diff");
    }

    fn info(title: Option<&str>, activity: Option<&str>, prompt: Option<&str>) -> AgentInput {
        AgentInput::Info(AgentInfo {
            title: title.map(Into::into),
            activity: activity.map(Into::into),
            prompt: prompt.map(Into::into),
            ..AgentInfo::default()
        })
    }

    #[test]
    fn info_actualiza_solo_lo_que_trae_y_sube_la_revision() {
        let mut a = working();
        let rev = a.revision();
        a.apply(
            &info(Some("Vulnerabilidades"), None, Some("revisa deps")),
            t(1),
        );
        a.apply(&info(None, Some("Editando a.ts"), None), t(2));
        assert_eq!(a.title(), Some("Vulnerabilidades"));
        assert_eq!(a.activity(), Some("Editando a.ts"));
        assert_eq!(a.last_prompt(), Some("revisa deps"));
        assert!(a.revision() > rev);
        let rev = a.revision();
        a.apply(&info(None, Some("Editando a.ts"), None), t(3));
        assert_eq!(a.revision(), rev, "sin cambios no hay diff");
    }

    #[test]
    fn stop_borra_la_actividad_y_session_start_empieza_de_cero() {
        let mut a = working();
        a.apply(
            &info(
                Some("Migración"),
                Some("Editando a.ts"),
                Some("migra users"),
            ),
            t(1),
        );
        sig(&mut a, Signal::Stop, t(2));
        assert_eq!(a.activity(), None);
        assert_eq!(
            a.title(),
            Some("Migración"),
            "el título sigue: es la misma conversación"
        );
        sig(&mut a, Signal::SessionStart, t(3));
        assert_eq!(
            (a.title(), a.last_prompt()),
            (None, None),
            "/clear: conversación nueva"
        );
    }

    #[test]
    fn guarda_las_estadisticas_de_la_sesion() {
        let mut a = agent();
        let rev = a.revision();
        let stats = AgentStats {
            model: Some("Opus 5.5".into()),
            context_pct: Some(62),
            cost_cents: Some(120),
        };
        a.apply(&AgentInput::Stats(stats.clone()), T0);
        assert_eq!(a.stats(), &stats);
        assert!(a.revision() > rev);
        // Lo mismo otra vez no es un cambio visible.
        let rev = a.revision();
        a.apply(&AgentInput::Stats(stats), T0);
        assert_eq!(a.revision(), rev);
    }

    #[test]
    fn recuerda_los_ultimos_cambios_de_estado() {
        let mut a = agent();
        assert_eq!(a.history().len(), 1, "el estado inicial cuenta");
        a.apply(
            &AgentInput::Signal(Signal::PromptSubmit),
            Timestamp(T0.0 + 10),
        );
        a.apply(&AgentInput::Signal(Signal::Stop), Timestamp(T0.0 + 20));
        let kinds: Vec<StatusKind> = a.history().iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            [StatusKind::Idle, StatusKind::Working, StatusKind::Idle]
        );
        assert_eq!(a.history().back().unwrap().1, Timestamp(T0.0 + 20));
        for i in 0..100 {
            let signal = if i % 2 == 0 {
                Signal::PromptSubmit
            } else {
                Signal::Stop
            };
            a.apply(&AgentInput::Signal(signal), Timestamp(T0.0 + 100 + i));
        }
        assert_eq!(a.history().len(), HISTORY_MAX);
    }

    #[test]
    fn recuerda_objetivo_inicial_archivos_tocados_y_tareas_de_la_sesion() {
        use crate::domain::memory::TaskStatus;

        let mut a = working();
        a.apply(&info(None, None, Some("implementa memoria SQLite")), t(1));
        a.apply(&info(None, None, Some("sí, continúa")), t(2));
        assert_eq!(a.initial_prompt(), Some("implementa memoria SQLite"));
        assert_eq!(a.last_prompt(), Some("sí, continúa"));

        for file in ["a.rs", "b.rs", "a.rs"] {
            a.apply(
                &AgentInput::Info(AgentInfo {
                    touched_file: Some(file.into()),
                    ..AgentInfo::default()
                }),
                t(3),
            );
        }
        assert_eq!(
            a.touched_files().iter().cloned().collect::<Vec<_>>(),
            vec!["b.rs", "a.rs"]
        );

        let tasks = vec![
            TaskItem {
                text: "Dominio".into(),
                status: TaskStatus::Completed,
            },
            TaskItem {
                text: "SQLite".into(),
                status: TaskStatus::InProgress,
            },
        ];
        a.apply(
            &AgentInput::Info(AgentInfo {
                tasks: Some(tasks.clone()),
                ..AgentInfo::default()
            }),
            t(4),
        );
        assert_eq!(a.tasks(), &tasks);

        // SessionStart limpia la memoria de la sesión.
        sig(&mut a, Signal::SessionStart, t(5));
        assert_eq!(a.initial_prompt(), None);
        assert!(a.touched_files().is_empty());
        assert!(a.tasks().is_empty());
    }
}
