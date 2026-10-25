//! Colección de agregados `Agent` (ADR 0002 §2).
//!
//! Enruta observaciones hacia los agregados y calcula el `RegistryDiff` (upserts y removed)
//! para enviar a la UI vía proyección.

use std::collections::HashMap;

use super::agent::{Agent, AgentInput, DetectedAgent, Events, SessionActivity};
use super::events::DomainEvent;
use super::ids::ToolName;
use super::ids::{AgentId, Pid, Provider, SessionId, Timestamp};
use super::signal::{AgentInfo, Signal};
use super::stale::StalePolicy;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegistryDiff {
    pub upserts: Vec<Agent>,
    pub removed: Vec<AgentId>,
}

impl RegistryDiff {
    pub fn is_empty(&self) -> bool {
        self.upserts.is_empty() && self.removed.is_empty()
    }
}

/// Cuánto se retiene un hook cuya sesión aún no tiene oficina (ADR 0002 §2, Correlator).
const PENDING_TTL_MS: u64 = 5_000;
const PENDING_MAX: usize = 64;

struct Pending {
    first_at: Timestamp,
    hooks: Vec<(Signal, AgentInfo, Timestamp)>,
}

pub struct AgentRegistry {
    agents: HashMap<AgentId, Agent>,
    /// Correlator: `session_id` → agente.
    sessions: HashMap<SessionId, AgentId>,
    /// Hooks de sesiones aún sin enlazar.
    pending: HashMap<SessionId, Pending>,
    /// Último `status` visto en `sessions/<pid>.json` por agente: solo cuentan los cambios.
    file_status: HashMap<AgentId, SessionActivity>,
    dropped_hooks: u64,
    last_sent_revisions: HashMap<AgentId, u64>,
    removed_since_last_diff: Vec<AgentId>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            sessions: HashMap::new(),
            pending: HashMap::new(),
            file_status: HashMap::new(),
            dropped_hooks: 0,
            last_sent_revisions: HashMap::new(),
            removed_since_last_diff: Vec::new(),
        }
    }

    pub fn get(&self, id: &AgentId) -> Option<&Agent> {
        self.agents.get(id)
    }

    pub fn get_mut(&mut self, id: &AgentId) -> Option<&mut Agent> {
        self.agents.get_mut(id)
    }

    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    pub fn agents(&self) -> impl Iterator<Item = &Agent> {
        self.agents.values()
    }

    /// Registra un agente detectado. Si ya existe, es idempotente y no emite evento.
    pub fn register(&mut self, detected: DetectedAgent, now: Timestamp) -> (AgentId, Events) {
        let mut events = Events::new();
        let agent = Agent::new(detected, now);
        let id = agent.id();
        if self.agents.contains_key(&id) {
            return (id, events);
        }
        self.agents.insert(id, agent);
        events.push(DomainEvent::AgentAppeared { id });
        (id, events)
    }

    /// Enruta una entrada hacia un agente específico.
    pub fn apply(&mut self, id: &AgentId, input: &AgentInput, now: Timestamp) -> Events {
        let Some(agent) = self.agents.get_mut(id) else {
            return Events::new();
        };
        let events = agent.apply(input, now);
        self.handle_events(id, &events);
        events
    }

    /// Sincroniza con un escaneo de procesos: registra los agentes nuevos y aplica `ProcessGone`
    /// a los vivos que ya no aparecen. Los `Dead` se quedan hasta su TTL.
    pub fn reconcile(&mut self, detected: &[DetectedAgent], now: Timestamp) -> Events {
        let mut events = Events::new();
        let mut alive = std::collections::HashSet::with_capacity(detected.len());
        for d in detected {
            let (id, ev) = self.register(d.clone(), now);
            alive.insert(id);
            events.extend(ev);
        }
        let gone: Vec<AgentId> = self
            .agents
            .values()
            .filter(|a| !a.is_dead() && !alive.contains(&a.id()))
            .map(Agent::id)
            .collect();
        for id in gone {
            events.extend(self.apply(&id, &AgentInput::ProcessGone, now));
        }
        events
    }

    /// Hooks descartados porque su sesión no encontró oficina en 5 s.
    pub fn dropped_hooks(&self) -> u64 {
        self.dropped_hooks
    }

    fn live_agent(&self, provider: Provider, pid: Pid) -> Option<AgentId> {
        self.agents
            .values()
            .find(|a| !a.is_dead() && a.id().provider == provider && a.id().pid == pid)
            .map(Agent::id)
    }

    /// Enlaza la sesión con su agente (y desenlaza otras sesiones del mismo proceso, que tras
    /// un `/clear` ya no volverán), y aplica los hooks que esperaban.
    fn link(&mut self, session: &SessionId, id: AgentId, now: Timestamp) -> Events {
        self.sessions.retain(|s, a| *a != id || s == session);
        self.sessions.insert(session.clone(), id);
        let mut events = self.apply(&id, &AgentInput::Linked(session.clone()), now);
        if let Some(p) = self.pending.remove(session) {
            for (signal, info, at) in p.hooks {
                events.extend(self.deliver(id, &signal, &info, at));
            }
        }
        events
    }

    /// Aplica la señal y, si trae algo, la info del mismo hook.
    fn deliver(
        &mut self,
        id: AgentId,
        signal: &Signal,
        info: &AgentInfo,
        now: Timestamp,
    ) -> Events {
        let mut events = self.apply(&id, &AgentInput::Signal(signal.clone()), now);
        if !info.is_empty() {
            events.extend(self.apply(&id, &AgentInput::Info(info.clone()), now));
        }
        events
    }

    /// Un hook ya traducido por la ACL, sin ancestros (Claude llega por http directo).
    pub fn hook(
        &mut self,
        provider: Provider,
        session: &SessionId,
        cwd: &str,
        signal: &Signal,
        now: Timestamp,
    ) -> Events {
        self.hook_info(
            provider,
            session,
            cwd,
            &[],
            signal,
            &AgentInfo::default(),
            now,
        )
    }

    pub fn hook_from(
        &mut self,
        provider: Provider,
        session: &SessionId,
        cwd: &str,
        ancestry: &[Pid],
        signal: &Signal,
        now: Timestamp,
    ) -> Events {
        self.hook_info(
            provider,
            session,
            cwd,
            ancestry,
            signal,
            &AgentInfo::default(),
            now,
        )
    }

    /// Orden de enlace: sesión conocida; el primer ancestro del forwarder que sea un agente vivo
    /// del mismo provider (`X-Kiti-Ppid`, ADR 0005 §4); un único agente del mismo provider y
    /// `cwd` sin sesión; si no, se retiene hasta 5 s (con su info, ADR 0009).
    #[allow(clippy::too_many_arguments)]
    pub fn hook_info(
        &mut self,
        provider: Provider,
        session: &SessionId,
        cwd: &str,
        ancestry: &[Pid],
        signal: &Signal,
        info: &AgentInfo,
        now: Timestamp,
    ) -> Events {
        if let Some(id) = self.sessions.get(session).copied() {
            return self.deliver(id, signal, info, now);
        }
        if let Some(id) = ancestry
            .iter()
            .find_map(|pid| self.live_agent(provider, *pid))
        {
            let mut events = self.link(session, id, now);
            events.extend(self.deliver(id, signal, info, now));
            return events;
        }
        let linked: std::collections::HashSet<AgentId> = self.sessions.values().copied().collect();
        let mut candidates = self.agents.values().filter(|a| {
            !a.is_dead()
                && a.id().provider == provider
                && a.cwd().as_str() == cwd
                && !linked.contains(&a.id())
        });
        if let (Some(only), None) = (candidates.next(), candidates.next()) {
            let id = only.id();
            let mut events = self.link(session, id, now);
            events.extend(self.deliver(id, signal, info, now));
            return events;
        }
        let p = self.pending.entry(session.clone()).or_insert(Pending {
            first_at: now,
            hooks: Vec::new(),
        });
        if p.hooks.len() < PENDING_MAX {
            p.hooks.push((signal.clone(), info.clone(), now));
        } else {
            self.dropped_hooks += 1;
        }
        Events::new()
    }

    /// `~/.claude/sessions/<pid>.json`: enlaza `pid` ↔ `session_id` y aplica su `status` solo si
    /// cambió y su `updatedAt` es posterior al último evento del agente. El archivo se lee cada
    /// escaneo y puede ir por detrás de los hooks: un `idle` atrasado no debe apagar un verde.
    pub fn session_file(
        &mut self,
        provider: Provider,
        pid: Pid,
        session: &SessionId,
        activity: Option<(SessionActivity, Timestamp)>,
        now: Timestamp,
    ) -> Events {
        let Some(id) = self.live_agent(provider, pid) else {
            return Events::new();
        };
        let mut events = if self.sessions.get(session) == Some(&id) {
            Events::new()
        } else {
            self.link(session, id, now)
        };
        if let Some((activity, updated_at)) = activity {
            let changed = self.file_status.insert(id, activity) != Some(activity);
            // Sin eventos aún (recién detectado, p. ej. al arrancar Kiti) cualquier status vale:
            // es lo único que se sabe, aunque se escribiera antes de que Kiti lo viera.
            let newer = self.agents.get(&id).is_some_and(|a| {
                a.status().reason == super::status::StatusReason::Startup
                    || updated_at > a.last_event_at()
            });
            if changed && newer {
                events.extend(self.apply(&id, &AgentInput::SessionFile(activity), now));
            }
        }
        events
    }

    /// Última herramienta del transcript de una sesión ya enlazada; si no lo está, se ignora.
    pub fn transcript(&mut self, session: &SessionId, tool: &ToolName, now: Timestamp) -> Events {
        match self.sessions.get(session).copied() {
            Some(id) => self.apply(&id, &AgentInput::TranscriptTool(tool.clone()), now),
            None => Events::new(),
        }
    }

    /// Título y último pedido del transcript de una sesión ya enlazada (ADR 0009).
    pub fn transcript_info(
        &mut self,
        session: &SessionId,
        info: &AgentInfo,
        now: Timestamp,
    ) -> Events {
        match self.sessions.get(session).copied() {
            Some(id) => self.apply(&id, &AgentInput::Info(info.clone()), now),
            None => Events::new(),
        }
    }

    fn expire_pending(&mut self, now: Timestamp) {
        let mut dropped = 0;
        self.pending.retain(|_, p| {
            let keep = now.0.saturating_sub(p.first_at.0) < PENDING_TTL_MS;
            if !keep {
                dropped += p.hooks.len() as u64;
            }
            keep
        });
        self.dropped_hooks += dropped;
    }

    /// Pasa el Tick por todos los agentes registrados.
    pub fn tick(&mut self, policy: &StalePolicy, now: Timestamp) -> Events {
        self.expire_pending(now);
        let mut all_events = Events::new();
        let ids: Vec<AgentId> = self.agents.keys().copied().collect();
        let input = AgentInput::Tick(*policy);
        for id in ids {
            if let Some(agent) = self.agents.get_mut(&id) {
                let ev = agent.apply(&input, now);
                self.handle_events(&id, &ev);
                all_events.extend(ev);
            }
        }
        all_events
    }

    fn handle_events(&mut self, id: &AgentId, events: &Events) {
        let mut to_remove = false;
        for ev in events {
            if let DomainEvent::AgentRemoved { id: removed_id } = ev
                && removed_id == id
            {
                to_remove = true;
            }
        }
        if to_remove {
            self.sessions.retain(|_, a| a != id);
            self.file_status.remove(id);
            self.agents.remove(id);
            self.last_sent_revisions.remove(id);
            self.removed_since_last_diff.push(*id);
        }
    }

    /// Genera la lista completa de agentes para el `Snapshot` inicial.
    pub fn snapshot(&mut self) -> Vec<Agent> {
        let mut list: Vec<Agent> = self.agents.values().cloned().collect();
        list.sort_by_key(|a| a.id());
        for a in &list {
            self.last_sent_revisions.insert(a.id(), a.revision());
        }
        self.removed_since_last_diff.clear();
        list
    }

    /// Extrae los cambios desde el último flush_diff o snapshot.
    pub fn flush_diff(&mut self) -> RegistryDiff {
        let mut upserts = Vec::new();
        for (id, agent) in &self.agents {
            let last_rev = self.last_sent_revisions.get(id).copied().unwrap_or(0);
            if agent.revision() > last_rev || !self.last_sent_revisions.contains_key(id) {
                upserts.push(agent.clone());
            }
        }
        upserts.sort_by_key(|a| a.id());
        for a in &upserts {
            self.last_sent_revisions.insert(a.id(), a.revision());
        }
        let removed = std::mem::take(&mut self.removed_since_last_diff);
        RegistryDiff { upserts, removed }
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{Pid, ProcessIdentity, ProcessStartTime, Provider, WorkingDir};
    use crate::domain::signal::Signal;
    use crate::domain::status::StatusKind;

    fn sample_detected(pid: u32) -> DetectedAgent {
        DetectedAgent {
            provider: Provider::Claude,
            identity: ProcessIdentity {
                pid: Pid(pid),
                start_time: ProcessStartTime(1_000_000),
                uid: 501,
                cmd_fingerprint: 42,
            },
            cwd: WorkingDir::new("/Users/dev/test"),
        }
    }

    #[test]
    fn register_es_idempotente() {
        let mut reg = AgentRegistry::new();
        let (id1, ev1) = reg.register(sample_detected(100), Timestamp(1000));
        assert_eq!(ev1[..], [DomainEvent::AgentAppeared { id: id1 }]);
        assert_eq!(reg.len(), 1);

        let (id2, ev2) = reg.register(sample_detected(100), Timestamp(1005));
        assert_eq!(id1, id2);
        assert!(ev2.is_empty());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn snapshot_registra_revisiones_actuales() {
        let mut reg = AgentRegistry::new();
        reg.register(sample_detected(100), Timestamp(1000));
        let snap = reg.snapshot();
        assert_eq!(snap.len(), 1);

        // Sin cambios posteriores, flush_diff debe estar vacío
        let diff = reg.flush_diff();
        assert!(diff.is_empty());
    }

    #[test]
    fn flush_diff_detecta_upserts_y_removed() {
        let mut reg = AgentRegistry::new();
        let (id, _) = reg.register(sample_detected(100), Timestamp(1000));
        reg.snapshot();

        // Aplicar una señal para que cambie la revisión
        reg.apply(
            &id,
            &AgentInput::Signal(Signal::PromptSubmit),
            Timestamp(1010),
        );
        let diff = reg.flush_diff();
        assert_eq!(diff.upserts.len(), 1);
        assert_eq!(diff.upserts[0].id(), id);
        assert_eq!(diff.upserts[0].status().kind, StatusKind::Working);
        assert!(diff.removed.is_empty());

        // Otro flush inmediato sin cambios debe estar vacío
        assert!(reg.flush_diff().is_empty());

        // Ahora matamos el proceso con sesión cerrada esperada
        reg.apply(
            &id,
            &AgentInput::Signal(Signal::SessionEnd),
            Timestamp(1020),
        );
        reg.apply(&id, &AgentInput::ProcessGone, Timestamp(1030));
        assert_eq!(reg.len(), 0);

        let diff2 = reg.flush_diff();
        assert_eq!(diff2.removed, vec![id]);
    }

    fn id_of(d: &DetectedAgent) -> AgentId {
        AgentId {
            provider: d.provider,
            pid: d.identity.pid,
            start_time: d.identity.start_time,
        }
    }

    #[test]
    fn reconcile_da_de_alta_lo_nuevo_y_marca_dead_lo_que_desaparece() {
        let mut reg = AgentRegistry::new();
        let (a, b, c) = (sample_detected(1), sample_detected(2), sample_detected(3));
        reg.reconcile(&[a.clone(), b.clone()], Timestamp(10));
        assert_eq!(reg.len(), 2);

        // b muere sin SessionEnd y aparece c.
        let ev = reg.reconcile(&[a.clone(), c.clone()], Timestamp(20));
        assert!(ev.contains(&DomainEvent::AgentAppeared { id: id_of(&c) }));
        assert!(ev.contains(&DomainEvent::AgentExited {
            id: id_of(&b),
            expected: false
        }));
        assert_eq!(
            reg.get(&id_of(&b)).map(|x| x.status().kind),
            Some(StatusKind::Dead)
        );
        assert!(reg.get(&id_of(&c)).is_some());
    }

    #[test]
    fn reconcile_es_idempotente_y_no_repite_process_gone() {
        let mut reg = AgentRegistry::new();
        let a = sample_detected(1);
        reg.reconcile(std::slice::from_ref(&a), Timestamp(10));
        reg.reconcile(&[], Timestamp(20));
        let rev = reg.get(&id_of(&a)).map(Agent::revision);
        let ev = reg.reconcile(&[], Timestamp(30));
        assert!(ev.is_empty());
        assert_eq!(reg.get(&id_of(&a)).map(Agent::revision), rev);
    }

    #[test]
    fn reconcile_quita_al_momento_una_salida_esperada() {
        let mut reg = AgentRegistry::new();
        let a = sample_detected(1);
        reg.reconcile(std::slice::from_ref(&a), Timestamp(10));
        reg.apply(
            &id_of(&a),
            &AgentInput::Signal(Signal::SessionEnd),
            Timestamp(11),
        );
        reg.reconcile(&[], Timestamp(20));
        assert!(reg.get(&id_of(&a)).is_none());
        assert_eq!(reg.flush_diff().removed, vec![id_of(&a)]);
    }

    fn claude_at(pid: u32, cwd: &str) -> DetectedAgent {
        DetectedAgent {
            cwd: WorkingDir::new(cwd),
            ..sample_detected(pid)
        }
    }

    fn sid(s: &str) -> SessionId {
        SessionId::new(s)
    }

    fn kind_of(reg: &AgentRegistry, d: &DetectedAgent) -> Option<StatusKind> {
        reg.get(&id_of(d)).map(|a| a.status().kind)
    }

    #[test]
    fn un_hook_llega_a_su_oficina_por_el_archivo_de_sesion() {
        let mut reg = AgentRegistry::new();
        let (a, b) = (claude_at(1, "/w/x"), claude_at(2, "/w/x"));
        reg.reconcile(&[a.clone(), b.clone()], Timestamp(10));
        reg.session_file(Provider::Claude, Pid(2), &sid("s-b"), None, Timestamp(11));

        reg.hook(
            Provider::Claude,
            &sid("s-b"),
            "/w/x",
            &Signal::PromptSubmit,
            Timestamp(12),
        );
        assert_eq!(kind_of(&reg, &b), Some(StatusKind::Working));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Idle));
    }

    #[test]
    fn sin_archivo_enlaza_por_cwd_si_hay_un_unico_candidato() {
        let mut reg = AgentRegistry::new();
        let (a, b) = (claude_at(1, "/w/uno"), claude_at(2, "/w/dos"));
        reg.reconcile(&[a.clone(), b.clone()], Timestamp(10));
        reg.hook(
            Provider::Claude,
            &sid("s"),
            "/w/dos",
            &Signal::PromptSubmit,
            Timestamp(11),
        );
        assert_eq!(kind_of(&reg, &b), Some(StatusKind::Working));
        // Desde ahí la sesión ya está enlazada.
        reg.hook(
            Provider::Claude,
            &sid("s"),
            "/otro",
            &Signal::Stop,
            Timestamp(12),
        );
        assert_eq!(kind_of(&reg, &b), Some(StatusKind::Idle));
    }

    #[test]
    fn con_varios_candidatos_retiene_el_hook_hasta_saber_de_quien_es() {
        let mut reg = AgentRegistry::new();
        let (a, b) = (claude_at(1, "/w/x"), claude_at(2, "/w/x"));
        reg.reconcile(&[a.clone(), b.clone()], Timestamp(10));
        reg.hook(
            Provider::Claude,
            &sid("s-a"),
            "/w/x",
            &Signal::PromptSubmit,
            Timestamp(11),
        );
        assert_eq!(
            kind_of(&reg, &a),
            Some(StatusKind::Idle),
            "ambiguo: no adivina"
        );
        // Llega el archivo de sesión: se aplican los hooks retenidos.
        reg.session_file(Provider::Claude, Pid(1), &sid("s-a"), None, Timestamp(12));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Working));
        assert_eq!(kind_of(&reg, &b), Some(StatusKind::Idle));
    }

    #[test]
    fn los_hooks_retenidos_caducan_a_los_5_s() {
        let mut reg = AgentRegistry::new();
        let (a, b) = (claude_at(1, "/w/x"), claude_at(2, "/w/x"));
        reg.reconcile(&[a.clone(), b], Timestamp(10));
        reg.hook(
            Provider::Claude,
            &sid("s-a"),
            "/w/x",
            &Signal::PromptSubmit,
            Timestamp(11),
        );
        reg.tick(&StalePolicy::default(), Timestamp(5_100));
        reg.session_file(
            Provider::Claude,
            Pid(1),
            &sid("s-a"),
            None,
            Timestamp(5_200),
        );
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Idle));
        assert_eq!(reg.dropped_hooks(), 1);
    }

    #[test]
    fn el_archivo_de_sesion_aplica_busy_e_idle_cuando_cambian() {
        let mut reg = AgentRegistry::new();
        let a = claude_at(1, "/w/x");
        reg.reconcile(std::slice::from_ref(&a), Timestamp(10));
        let busy = Some((SessionActivity::Busy, Timestamp(11)));
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), busy, Timestamp(11));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Working));
        let idle = Some((SessionActivity::Idle, Timestamp(12)));
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), idle, Timestamp(12));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Idle));
    }

    #[test]
    fn un_agente_que_ya_trabajaba_al_arrancar_kiti_sale_en_verde() {
        let mut reg = AgentRegistry::new();
        let a = claude_at(1, "/w/x");
        // Kiti arranca a los 100; el turno empezó a los 50 (updatedAt más viejo que el registro).
        reg.reconcile(std::slice::from_ref(&a), Timestamp(100));
        let busy = Some((SessionActivity::Busy, Timestamp(50)));
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), busy, Timestamp(100));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Working));
    }

    #[test]
    fn un_archivo_de_sesion_atrasado_no_pisa_un_hook_mas_reciente() {
        let mut reg = AgentRegistry::new();
        let a = claude_at(1, "/w/x");
        reg.reconcile(std::slice::from_ref(&a), Timestamp(10));
        let idle = Some((SessionActivity::Idle, Timestamp(10)));
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), idle, Timestamp(10));
        // El prompt llega por hook; el archivo aún dice idle con su updatedAt viejo.
        reg.hook(
            Provider::Claude,
            &sid("s"),
            "/w/x",
            &Signal::PromptSubmit,
            Timestamp(20),
        );
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), idle, Timestamp(23));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Working));
        // Mismo status repetido con un updatedAt nuevo tampoco cuenta: no es un cambio.
        let idle_later = Some((SessionActivity::Idle, Timestamp(30)));
        reg.session_file(
            Provider::Claude,
            Pid(1),
            &sid("s"),
            idle_later,
            Timestamp(31),
        );
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Working));
        // Un cambio real posterior al hook sí se aplica (Esc: busy → idle).
        let busy = Some((SessionActivity::Busy, Timestamp(40)));
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), busy, Timestamp(41));
        let idle_esc = Some((SessionActivity::Idle, Timestamp(50)));
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), idle_esc, Timestamp(51));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Idle));
    }

    #[test]
    fn clear_cambia_la_sesion_del_mismo_proceso() {
        let mut reg = AgentRegistry::new();
        let a = claude_at(1, "/w/x");
        let b = claude_at(2, "/w/x");
        reg.reconcile(&[a.clone(), b.clone()], Timestamp(10));
        reg.session_file(Provider::Claude, Pid(1), &sid("vieja"), None, Timestamp(11));
        reg.session_file(Provider::Claude, Pid(1), &sid("nueva"), None, Timestamp(12));
        reg.hook(
            Provider::Claude,
            &sid("nueva"),
            "/w/x",
            &Signal::PromptSubmit,
            Timestamp(13),
        );
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Working));
    }

    #[test]
    fn el_ppid_del_forwarder_enlaza_aunque_el_cwd_sea_ambiguo() {
        let mut reg = AgentRegistry::new();
        let codex = |pid| DetectedAgent {
            provider: Provider::Codex,
            ..claude_at(pid, "/w/x")
        };
        let (a, b) = (codex(10), codex(20));
        reg.reconcile(&[a.clone(), b.clone()], Timestamp(1));
        // kiti-hook → sh -c → codex (pid 20) → zsh
        let ancestry = [Pid(901), Pid(20), Pid(5)];
        reg.hook_from(
            Provider::Codex,
            &sid("thr_1"),
            "/w/x",
            &ancestry,
            &Signal::PromptSubmit,
            Timestamp(2),
        );
        assert_eq!(kind_of(&reg, &b), Some(StatusKind::Working));
        assert_eq!(kind_of(&reg, &a), Some(StatusKind::Idle));
        // Ya enlazada, la sesión no necesita más ancestros.
        reg.hook(
            Provider::Codex,
            &sid("thr_1"),
            "/w/x",
            &Signal::Stop,
            Timestamp(3),
        );
        assert_eq!(kind_of(&reg, &b), Some(StatusKind::Idle));
    }

    #[test]
    fn un_ancestro_de_otro_cli_no_cuenta() {
        let mut reg = AgentRegistry::new();
        let claude = claude_at(20, "/w/x");
        reg.reconcile(std::slice::from_ref(&claude), Timestamp(1));
        reg.hook_from(
            Provider::Codex,
            &sid("thr_1"),
            "/otra",
            &[Pid(20)],
            &Signal::PromptSubmit,
            Timestamp(2),
        );
        assert_eq!(kind_of(&reg, &claude), Some(StatusKind::Idle));
    }

    #[test]
    fn el_transcript_llega_por_la_sesion_enlazada() {
        let mut reg = AgentRegistry::new();
        let a = claude_at(1, "/w/x");
        reg.reconcile(std::slice::from_ref(&a), Timestamp(10));
        reg.transcript(&sid("s"), &ToolName::new("Read"), Timestamp(11));
        assert_eq!(
            reg.get(&id_of(&a)).and_then(|x| x.last_tool().cloned()),
            None,
            "sin enlace"
        );
        reg.session_file(Provider::Claude, Pid(1), &sid("s"), None, Timestamp(12));
        reg.transcript(&sid("s"), &ToolName::new("Read"), Timestamp(13));
        assert_eq!(
            reg.get(&id_of(&a)).and_then(|x| x.last_tool().cloned()),
            Some(ToolName::new("Read"))
        );
    }

    #[test]
    fn la_info_del_hook_llega_aunque_el_hook_quede_retenido() {
        let mut reg = AgentRegistry::new();
        let (a, b) = (claude_at(1, "/w/x"), claude_at(2, "/w/x"));
        reg.reconcile(&[a.clone(), b], Timestamp(10));
        let info = AgentInfo {
            prompt: Some("revisa las vulnerabilidades".into()),
            ..AgentInfo::default()
        };
        reg.hook_info(
            Provider::Claude,
            &sid("s-a"),
            "/w/x",
            &[],
            &Signal::PromptSubmit,
            &info,
            Timestamp(11),
        );
        reg.session_file(Provider::Claude, Pid(1), &sid("s-a"), None, Timestamp(12));
        let agent = reg.get(&id_of(&a)).unwrap();
        assert_eq!(agent.last_prompt(), Some("revisa las vulnerabilidades"));
        assert_eq!(agent.session(), Some(&sid("s-a")));
        assert_eq!(agent.status().kind, StatusKind::Working);
    }
}
