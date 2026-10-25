//! Colección de agregados `Agent` (ADR 0002 §2).
//!
//! Enruta observaciones hacia los agregados y calcula el `RegistryDiff` (upserts y removed)
//! para enviar a la UI vía proyección.

use std::collections::HashMap;

use super::agent::{Agent, AgentInput, DetectedAgent, Events};
use super::events::DomainEvent;
use super::ids::{AgentId, Timestamp};
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

pub struct AgentRegistry {
    agents: HashMap<AgentId, Agent>,
    last_sent_revisions: HashMap<AgentId, u64>,
    removed_since_last_diff: Vec<AgentId>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
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

    /// Pasa el Tick por todos los agentes registrados.
    pub fn tick(&mut self, policy: &StalePolicy, now: Timestamp) -> Events {
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
}
