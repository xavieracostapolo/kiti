//! Mensajes entre agentes (ADR 0017). Puro: alias, hilos, modos, límites y cola de entrega.
//!
//! El remitente nunca lo declara el mensaje: el runtime lo resuelve por el proceso y llama a
//! `send` con su `AgentId`. El destinatario se nombra por su alias (`claude-1`, `agy-2`…).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use super::alias::{AliasBook, AliasEntry, AliasKey};
use super::ids::{AgentId, Timestamp};

/// Caracteres como máximo por mensaje.
pub const MAX_TEXT: usize = 4000;
/// Envíos por agente en una hora móvil.
pub const RATE_PER_HOUR: usize = 20;
/// Saltos por hilo antes de pausarlo; "Continuar" suma otros tantos.
pub const HOP_STEP: u32 = 12;
/// Mensajes que se recuerdan (los más antiguos ya entregados o descartados se olvidan).
pub const KEEP_MESSAGES: usize = 200;
const HOUR_MS: u64 = 3_600_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MailMode {
    /// Los mensajes se entregan solos (por defecto).
    #[default]
    Auto,
    /// Cada mensaje espera a que el usuario lo apruebe.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageState {
    /// Modo manual: espera la aprobación del usuario.
    Pending,
    /// Listo: se entrega en cuanto el destinatario esté libre (o lo pida con `inbox`).
    Queued,
    /// Su hilo está pausado por el límite de saltos.
    Held,
    Delivered,
    Discarded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub thread: ThreadId,
    pub from: AgentId,
    pub from_alias: String,
    pub to: AgentId,
    pub to_alias: String,
    pub text: String,
    pub sent_at: Timestamp,
    pub state: MessageState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadState {
    Open,
    /// Llegó al límite de saltos: el usuario decide si sigue.
    Paused,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: ThreadId,
    pub subject: String,
    pub hops: u32,
    pub limit: u32,
    pub state: ThreadState,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    #[error("el remitente no es un agente conocido")]
    UnknownSender,
    #[error("no hay ningún agente con el alias «{0}»")]
    UnknownRecipient(String),
    #[error("un agente no puede enviarse mensajes a sí mismo")]
    SelfMessage,
    #[error("el mensaje está vacío")]
    EmptyText,
    #[error("el mensaje supera los {MAX_TEXT} caracteres")]
    TooLong,
    #[error("límite de {RATE_PER_HOUR} envíos por hora; reintenta en {retry_after_s} s")]
    RateLimited { retry_after_s: u64 },
    #[error("el mensaje al que respondes ya no existe")]
    UnknownMessage,
    #[error("el usuario cerró ese hilo")]
    ThreadClosed,
}

impl SendError {
    /// Código estable para los logs (nunca el texto del mensaje).
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownSender => "unknown_sender",
            Self::UnknownRecipient(_) => "unknown_recipient",
            Self::SelfMessage => "self",
            Self::EmptyText => "empty",
            Self::TooLong => "too_long",
            Self::RateLimited { .. } => "rate",
            Self::UnknownMessage => "unknown_message",
            Self::ThreadClosed => "thread_closed",
        }
    }
}

/// Resultado de un envío aceptado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub id: MessageId,
    pub thread: ThreadId,
    pub state: MessageState,
    /// Este envío pausó su hilo (hay que avisar al usuario).
    pub paused_thread: bool,
}

/// Mensajes que hay que escribir en la terminal de un destinatario, en orden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub to: AgentId,
    pub messages: Vec<Message>,
}

#[derive(Debug, Default)]
pub struct Mailbox {
    mode: MailMode,
    paused: bool,
    next_message: u64,
    next_thread: u64,
    aliases: AliasBook,
    keys: HashMap<AgentId, AliasKey>,
    messages: VecDeque<Message>,
    threads: BTreeMap<ThreadId, Thread>,
    sends: HashMap<AgentId, VecDeque<Timestamp>>,
}

/// Quita los caracteres de control (salvo salto de línea y tabulador): el texto acaba escrito
/// en una terminal y una secuencia de escape podría manejarla.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect::<String>()
        .trim()
        .to_owned()
}

fn subject_of(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    let mut subject: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        subject.push('…');
    }
    subject
}

impl Mailbox {
    pub fn mode(&self) -> MailMode {
        self.mode
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Alias de un agente vivo, asignándole uno si aún no tiene. El alias pertenece a `key`
    /// (ver `AliasBook`): la misma clave recupera el suyo.
    pub fn join(&mut self, id: AgentId, key: AliasKey, now: Timestamp) -> &str {
        if !self.keys.contains_key(&id) {
            self.aliases.assign(&key, id.provider, now);
            self.keys.insert(id, key.clone());
        }
        self.aliases.get(&self.keys[&id]).unwrap_or_default()
    }

    /// Solo tests: agente fuera de tmux.
    #[cfg(test)]
    pub(crate) fn join_process(&mut self, id: AgentId) -> &str {
        self.join(id, AliasKey::Process(id), Timestamp(0))
    }

    /// Clave estable con la que `id` tiene su alias.
    pub fn key_of(&self, id: &AgentId) -> Option<&AliasKey> {
        self.keys.get(id)
    }

    /// Cambia la clave de `id` (su proceso apareció en una sesión tmux de Kiti). Si la clave
    /// nueva no existía, hereda el alias actual sin renombrar; si existía (relanzado en la
    /// misma sesión), toma el alias guardado de esa sesión.
    pub fn rekey(&mut self, id: &AgentId, key: AliasKey, now: Timestamp) -> bool {
        let Some(old) = self.keys.get(id).cloned() else {
            return false;
        };
        if old == key {
            return false;
        }
        if !self.aliases.rekey(&old, key.clone()) {
            self.aliases.forget(&old);
            self.aliases.assign(&key, id.provider, now);
        }
        self.keys.insert(*id, key);
        true
    }

    pub fn alias(&self, id: &AgentId) -> Option<&str> {
        self.aliases.get(self.keys.get(id)?)
    }

    /// Agentes con alias, en cualquier orden.
    pub fn members(&self) -> impl Iterator<Item = (&AgentId, &str)> {
        self.keys
            .iter()
            .filter_map(|(id, k)| Some((id, self.aliases.get(k)?)))
    }

    pub fn resolve(&self, alias: &str) -> Option<AgentId> {
        let alias = alias.trim();
        self.members()
            .find(|(_, a)| a.eq_ignore_ascii_case(alias))
            .map(|(id, _)| *id)
    }

    /// El agente murió: libera su alias (reservado 24 h para su clave) y descarta lo que
    /// esperaba entregarle.
    pub fn leave(&mut self, id: &AgentId, now: Timestamp) -> bool {
        let mut changed = false;
        if let Some(key) = self.keys.remove(id) {
            changed = self.aliases.release(&key, now);
        }
        for m in self.messages.iter_mut().filter(|m| m.to == *id) {
            if matches!(
                m.state,
                MessageState::Pending | MessageState::Queued | MessageState::Held
            ) {
                m.state = MessageState::Discarded;
                changed = true;
            }
        }
        self.sends.remove(id);
        changed
    }

    /// Libera los alias vivos cuya clave no está en `wanted` y que ningún agente usa (filas
    /// restauradas de un arranque anterior). Reversible: la misma clave los recupera.
    pub fn release_unbound(&mut self, wanted: &HashSet<AliasKey>, now: Timestamp) -> bool {
        let bound: HashSet<&AliasKey> = self.keys.values().collect();
        let stale: Vec<AliasKey> = self
            .aliases
            .live()
            .map(|(k, _)| k.clone())
            .filter(|k| !wanted.contains(k) && !bound.contains(k))
            .collect();
        let changed = !stale.is_empty();
        for k in stale {
            self.aliases.release(&k, now);
        }
        changed
    }

    /// Borra las reservas de alias caducadas.
    pub fn prune_aliases(&mut self, now: Timestamp) -> bool {
        self.aliases.prune(now)
    }

    pub fn alias_entries(&self) -> Vec<AliasEntry> {
        self.aliases.entries()
    }

    pub fn restore_aliases(&mut self, entries: Vec<AliasEntry>) {
        self.aliases.restore(entries);
    }

    pub fn send(
        &mut self,
        from: AgentId,
        to_alias: &str,
        text: &str,
        reply_to: Option<MessageId>,
        now: Timestamp,
    ) -> Result<Sent, SendError> {
        let from_alias = self
            .alias(&from)
            .ok_or(SendError::UnknownSender)?
            .to_owned();
        let to = self
            .resolve(to_alias)
            .ok_or_else(|| SendError::UnknownRecipient(to_alias.trim().to_owned()))?;
        if to == from {
            return Err(SendError::SelfMessage);
        }
        let text = clean(text);
        if text.is_empty() {
            return Err(SendError::EmptyText);
        }
        if text.chars().count() > MAX_TEXT {
            return Err(SendError::TooLong);
        }
        let thread = match reply_to {
            Some(id) => {
                let thread = self
                    .messages
                    .iter()
                    .find(|m| m.id == id)
                    .map(|m| m.thread)
                    .ok_or(SendError::UnknownMessage)?;
                if self.threads[&thread].state == ThreadState::Closed {
                    return Err(SendError::ThreadClosed);
                }
                Some(thread)
            }
            None => None,
        };
        let recent = self.sends.entry(from).or_default();
        while recent
            .front()
            .is_some_and(|t| now.0.saturating_sub(t.0) >= HOUR_MS)
        {
            recent.pop_front();
        }
        if recent.len() >= RATE_PER_HOUR {
            let oldest = recent.front().copied().unwrap_or(now);
            let wait_ms = (oldest.0 + HOUR_MS).saturating_sub(now.0);
            return Err(SendError::RateLimited {
                retry_after_s: wait_ms.div_ceil(1000),
            });
        }
        recent.push_back(now);

        let thread = thread.unwrap_or_else(|| {
            self.next_thread += 1;
            let id = ThreadId(self.next_thread);
            self.threads.insert(
                id,
                Thread {
                    id,
                    subject: subject_of(&text),
                    hops: 0,
                    limit: HOP_STEP,
                    state: ThreadState::Open,
                },
            );
            id
        });
        let t = self.threads.get_mut(&thread).expect("hilo recién resuelto");
        t.hops += 1;
        let mut paused_thread = false;
        if t.state == ThreadState::Open && t.hops > t.limit {
            t.state = ThreadState::Paused;
            paused_thread = true;
        }
        let state = if t.state == ThreadState::Paused {
            MessageState::Held
        } else if self.mode == MailMode::Manual {
            MessageState::Pending
        } else {
            MessageState::Queued
        };
        self.next_message += 1;
        let id = MessageId(self.next_message);
        let to_alias = self.alias(&to).unwrap_or_default().to_owned();
        self.messages.push_back(Message {
            id,
            thread,
            from,
            from_alias,
            to,
            to_alias,
            text,
            sent_at: now,
            state,
        });
        self.prune();
        Ok(Sent {
            id,
            thread,
            state,
            paused_thread,
        })
    }

    /// Olvida los mensajes más antiguos ya cerrados por encima de `KEEP_MESSAGES`, y los
    /// hilos que se quedan sin mensajes.
    fn prune(&mut self) {
        while self.messages.len() > KEEP_MESSAGES {
            let Some(pos) = self
                .messages
                .iter()
                .position(|m| matches!(m.state, MessageState::Delivered | MessageState::Discarded))
            else {
                break;
            };
            self.messages.remove(pos);
        }
        let live: std::collections::HashSet<ThreadId> =
            self.messages.iter().map(|m| m.thread).collect();
        self.threads.retain(|id, _| live.contains(id));
    }

    pub fn set_mode(&mut self, mode: MailMode) -> bool {
        if self.mode == mode {
            return false;
        }
        self.mode = mode;
        if mode == MailMode::Auto {
            for m in self
                .messages
                .iter_mut()
                .filter(|m| m.state == MessageState::Pending)
            {
                m.state = MessageState::Queued;
            }
        }
        true
    }

    pub fn set_paused(&mut self, paused: bool) -> bool {
        std::mem::replace(&mut self.paused, paused) != paused
    }

    pub fn approve(&mut self, id: MessageId) -> bool {
        self.transition(id, &[MessageState::Pending], MessageState::Queued)
    }

    pub fn discard(&mut self, id: MessageId) -> bool {
        self.transition(
            id,
            &[
                MessageState::Pending,
                MessageState::Queued,
                MessageState::Held,
            ],
            MessageState::Discarded,
        )
    }

    fn transition(&mut self, id: MessageId, from: &[MessageState], to: MessageState) -> bool {
        match self.messages.iter_mut().find(|m| m.id == id) {
            Some(m) if from.contains(&m.state) => {
                m.state = to;
                true
            }
            _ => false,
        }
    }

    /// "Continuar": `HOP_STEP` saltos más, y lo retenido sigue su curso.
    pub fn continue_thread(&mut self, id: ThreadId) -> bool {
        let Some(t) = self.threads.get_mut(&id) else {
            return false;
        };
        if t.state != ThreadState::Paused {
            return false;
        }
        t.state = ThreadState::Open;
        t.limit = t.hops.max(t.limit) + HOP_STEP;
        let next = match self.mode {
            MailMode::Auto => MessageState::Queued,
            MailMode::Manual => MessageState::Pending,
        };
        for m in self
            .messages
            .iter_mut()
            .filter(|m| m.thread == id && m.state == MessageState::Held)
        {
            m.state = next;
        }
        true
    }

    /// "Cerrar hilo": descarta lo que no se entregó y rechaza las respuestas.
    pub fn close_thread(&mut self, id: ThreadId) -> bool {
        let Some(t) = self.threads.get_mut(&id) else {
            return false;
        };
        if t.state == ThreadState::Closed {
            return false;
        }
        t.state = ThreadState::Closed;
        for m in self.messages.iter_mut().filter(|m| {
            m.thread == id
                && matches!(
                    m.state,
                    MessageState::Pending | MessageState::Queued | MessageState::Held
                )
        }) {
            m.state = MessageState::Discarded;
        }
        true
    }

    /// Lo listo para escribir en la terminal de cada destinatario libre, agrupado por
    /// destinatario y en orden; queda marcado como entregado. Si la escritura falla, el shell
    /// lo devuelve con `undeliver`.
    pub fn take_deliveries(&mut self, is_free: impl Fn(&AgentId) -> bool) -> Vec<Delivery> {
        if self.paused {
            return Vec::new();
        }
        let mut out: Vec<Delivery> = Vec::new();
        for m in self
            .messages
            .iter_mut()
            .filter(|m| m.state == MessageState::Queued && is_free(&m.to))
        {
            m.state = MessageState::Delivered;
            match out.iter_mut().find(|d| d.to == m.to) {
                Some(d) => d.messages.push(m.clone()),
                None => out.push(Delivery {
                    to: m.to,
                    messages: vec![m.clone()],
                }),
            }
        }
        out
    }

    pub fn undeliver(&mut self, ids: &[MessageId]) {
        for m in self
            .messages
            .iter_mut()
            .filter(|m| ids.contains(&m.id) && m.state == MessageState::Delivered)
        {
            m.state = MessageState::Queued;
        }
    }

    /// `kiti_inbox`: lo listo para `agent`, que queda entregado.
    pub fn inbox(&mut self, agent: &AgentId) -> Vec<Message> {
        if self.paused {
            return Vec::new();
        }
        self.messages
            .iter_mut()
            .filter(|m| m.to == *agent && m.state == MessageState::Queued)
            .map(|m| {
                m.state = MessageState::Delivered;
                m.clone()
            })
            .collect()
    }

    /// Mensajes por aprobar más hilos pausados: lo único que pide la atención del usuario.
    pub fn attention(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.state == MessageState::Pending)
            .count()
            + self
                .threads
                .values()
                .filter(|t| t.state == ThreadState::Paused)
                .count()
    }

    /// Mensajes aún sin entregar (por aprobar, en cola o retenidos).
    pub fn pending(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| {
                matches!(
                    m.state,
                    MessageState::Pending | MessageState::Queued | MessageState::Held
                )
            })
            .count()
    }

    /// Mensajes listos esperando a `agent` (el sobre de su oficina).
    pub fn waiting_for(&self, agent: &AgentId) -> usize {
        self.messages
            .iter()
            .filter(|m| m.to == *agent && m.state == MessageState::Queued)
            .count()
    }

    pub fn thread(&self, id: ThreadId) -> Option<&Thread> {
        self.threads.get(&id)
    }

    pub fn threads(&self) -> impl Iterator<Item = &Thread> {
        self.threads.values()
    }

    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{Pid, ProcessStartTime, Provider};
    use proptest::prelude::*;

    fn agent(provider: Provider, pid: u32) -> AgentId {
        AgentId {
            provider,
            pid: Pid(pid),
            start_time: ProcessStartTime(1),
        }
    }

    fn claude(pid: u32) -> AgentId {
        agent(Provider::Claude, pid)
    }

    fn codex(pid: u32) -> AgentId {
        agent(Provider::Codex, pid)
    }

    fn two() -> (Mailbox, AgentId, AgentId) {
        let mut mb = Mailbox::default();
        let a = claude(1);
        let b = codex(2);
        mb.join_process(a);
        mb.join_process(b);
        (mb, a, b)
    }

    #[test]
    fn alias_corto_por_proveedor_con_el_numero_libre_mas_bajo() {
        let mut mb = Mailbox::default();
        assert_eq!(mb.join_process(claude(10)), "claude-1");
        assert_eq!(mb.join_process(claude(11)), "claude-2");
        assert_eq!(mb.join_process(agent(Provider::Antigravity, 12)), "agy-1");
        // Es estable mientras vive.
        assert_eq!(mb.join_process(claude(10)), "claude-1");
        mb.leave(&claude(10), Timestamp(5));
        // Reservado 24 h: el siguiente no hereda claude-1.
        assert_eq!(mb.join_process(claude(13)), "claude-3");
        assert_eq!(mb.resolve("CLAUDE-2"), Some(claude(11)));
        assert_eq!(mb.resolve("claude-9"), None);
    }

    #[test]
    fn en_automatico_se_entrega_al_destinatario_libre() {
        let (mut mb, a, b) = two();
        let sent = mb
            .send(a, "codex-1", "¿revisas mi diff?", None, Timestamp(100))
            .unwrap();
        assert_eq!(sent.state, MessageState::Queued);
        assert!(!sent.paused_thread);
        assert_eq!(mb.waiting_for(&b), 1);
        // Ocupado: no se entrega.
        assert!(mb.take_deliveries(|_| false).is_empty());
        let d = mb.take_deliveries(|id| *id == b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].to, b);
        assert_eq!(d[0].messages[0].from_alias, "claude-1");
        assert_eq!(d[0].messages[0].text, "¿revisas mi diff?");
        assert!(mb.take_deliveries(|_| true).is_empty());
        assert_eq!(mb.attention(), 0);
    }

    #[test]
    fn varios_mensajes_al_mismo_destinatario_van_juntos_y_en_orden() {
        let (mut mb, a, b) = two();
        mb.send(a, "codex-1", "uno", None, Timestamp(1)).unwrap();
        mb.send(a, "codex-1", "dos", None, Timestamp(2)).unwrap();
        let d = mb.take_deliveries(|id| *id == b);
        let texts: Vec<_> = d[0].messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["uno", "dos"]);
    }

    #[test]
    fn si_falla_la_escritura_vuelve_a_la_cola() {
        let (mut mb, a, b) = two();
        let sent = mb.send(a, "codex-1", "hola", None, Timestamp(1)).unwrap();
        assert_eq!(mb.take_deliveries(|_| true).len(), 1);
        mb.undeliver(&[sent.id]);
        assert_eq!(mb.waiting_for(&b), 1);
    }

    #[test]
    fn inbox_entrega_lo_listo_una_sola_vez() {
        let (mut mb, a, b) = two();
        mb.send(a, "codex-1", "hola", None, Timestamp(1)).unwrap();
        assert_eq!(mb.inbox(&b).len(), 1);
        assert!(mb.inbox(&b).is_empty());
        assert!(mb.inbox(&a).is_empty());
    }

    #[test]
    fn en_manual_espera_aprobacion_y_descartar_lo_quita() {
        let (mut mb, a, b) = two();
        mb.set_mode(MailMode::Manual);
        let s1 = mb.send(a, "codex-1", "uno", None, Timestamp(1)).unwrap();
        let s2 = mb.send(a, "codex-1", "dos", None, Timestamp(2)).unwrap();
        assert_eq!(s1.state, MessageState::Pending);
        assert_eq!(mb.attention(), 2);
        assert!(mb.take_deliveries(|_| true).is_empty());
        assert!(mb.inbox(&b).is_empty());
        assert!(mb.approve(s1.id));
        assert!(mb.discard(s2.id));
        assert_eq!(mb.attention(), 0);
        let d = mb.take_deliveries(|_| true);
        assert_eq!(d[0].messages.len(), 1);
        assert_eq!(d[0].messages[0].id, s1.id);
    }

    #[test]
    fn pasar_a_automatico_libera_lo_pendiente() {
        let (mut mb, a, b) = two();
        mb.set_mode(MailMode::Manual);
        mb.send(a, "codex-1", "uno", None, Timestamp(1)).unwrap();
        assert!(mb.set_mode(MailMode::Auto));
        assert_eq!(mb.waiting_for(&b), 1);
    }

    #[test]
    fn la_pausa_global_congela_entregas_e_inbox() {
        let (mut mb, a, b) = two();
        mb.send(a, "codex-1", "uno", None, Timestamp(1)).unwrap();
        mb.set_paused(true);
        assert!(mb.take_deliveries(|_| true).is_empty());
        assert!(mb.inbox(&b).is_empty());
        mb.set_paused(false);
        assert_eq!(mb.inbox(&b).len(), 1);
    }

    #[test]
    fn rechaza_remitente_desconocido_destinatario_desconocido_y_a_si_mismo() {
        let (mut mb, a, _) = two();
        assert_eq!(
            mb.send(claude(99), "codex-1", "x", None, Timestamp(1)),
            Err(SendError::UnknownSender)
        );
        assert_eq!(
            mb.send(a, "codex-7", "x", None, Timestamp(1)),
            Err(SendError::UnknownRecipient("codex-7".into()))
        );
        assert_eq!(
            mb.send(a, "claude-1", "x", None, Timestamp(1)),
            Err(SendError::SelfMessage)
        );
    }

    #[test]
    fn texto_vacio_largo_o_con_secuencias_de_control() {
        let (mut mb, a, b) = two();
        assert_eq!(
            mb.send(a, "codex-1", "  \u{7} ", None, Timestamp(1)),
            Err(SendError::EmptyText)
        );
        let long = "x".repeat(MAX_TEXT + 1);
        assert_eq!(
            mb.send(a, "codex-1", &long, None, Timestamp(1)),
            Err(SendError::TooLong)
        );
        mb.send(a, "codex-1", "hola\u{1b}[2J\nmundo\tya", None, Timestamp(1))
            .unwrap();
        assert_eq!(mb.inbox(&b)[0].text, "hola[2J\nmundo\tya");
    }

    #[test]
    fn limite_de_envios_por_hora_movil() {
        let (mut mb, a, _) = two();
        for i in 0..RATE_PER_HOUR as u64 {
            mb.send(a, "codex-1", "x", None, Timestamp(1_000 + i))
                .unwrap();
        }
        assert_eq!(
            mb.send(a, "codex-1", "x", None, Timestamp(2_000)),
            Err(SendError::RateLimited {
                retry_after_s: 3_599
            })
        );
        // Una hora después del primero, vuelve a poder.
        assert!(
            mb.send(a, "codex-1", "x", None, Timestamp(1_000 + HOUR_MS))
                .is_ok()
        );
    }

    #[test]
    fn responder_sigue_el_hilo_y_el_asunto_sale_del_primer_mensaje() {
        let (mut mb, a, b) = two();
        let first = mb
            .send(
                a,
                "codex-1",
                "Revisión del login\ndetalles…",
                None,
                Timestamp(1),
            )
            .unwrap();
        let reply = mb
            .send(b, "claude-1", "visto", Some(first.id), Timestamp(2))
            .unwrap();
        assert_eq!(reply.thread, first.thread);
        let t = mb.threads().next().unwrap();
        assert_eq!(t.subject, "Revisión del login");
        assert_eq!(t.hops, 2);
        assert_eq!(
            mb.send(b, "claude-1", "x", Some(MessageId(999)), Timestamp(3)),
            Err(SendError::UnknownMessage)
        );
    }

    #[test]
    fn al_pasar_el_limite_de_saltos_el_hilo_se_pausa_y_retiene() {
        let (mut mb, a, b) = two();
        let first = mb.send(a, "codex-1", "inicio", None, Timestamp(0)).unwrap();
        let mut last = first.clone();
        for i in 1..HOP_STEP as u64 {
            let (from, to) = if i % 2 == 1 {
                (b, "claude-1")
            } else {
                (a, "codex-1")
            };
            last = mb
                .send(from, to, "sigo", Some(last.id), Timestamp(i))
                .unwrap();
            assert!(!last.paused_thread);
        }
        let over = mb
            .send(b, "claude-1", "uno más", Some(last.id), Timestamp(99))
            .unwrap();
        assert!(over.paused_thread);
        assert_eq!(over.state, MessageState::Held);
        assert_eq!(mb.attention(), 1);
        mb.take_deliveries(|_| true);
        assert!(mb.inbox(&a).is_empty());
        // Continuar: lo retenido sale y hay otros HOP_STEP saltos.
        assert!(mb.continue_thread(first.thread));
        assert_eq!(mb.attention(), 0);
        assert_eq!(mb.inbox(&a).len(), 1);
        let t = mb.threads().next().unwrap();
        assert_eq!(t.limit, HOP_STEP * 2 + 1);
    }

    #[test]
    fn cerrar_un_hilo_descarta_lo_pendiente_y_rechaza_respuestas() {
        let (mut mb, a, b) = two();
        let s = mb.send(a, "codex-1", "x", None, Timestamp(1)).unwrap();
        assert!(mb.close_thread(s.thread));
        assert!(mb.inbox(&b).is_empty());
        assert_eq!(
            mb.send(b, "claude-1", "y", Some(s.id), Timestamp(2)),
            Err(SendError::ThreadClosed)
        );
    }

    #[test]
    fn si_muere_el_destinatario_se_descarta_lo_suyo_y_su_alias_queda_libre() {
        let (mut mb, a, b) = two();
        mb.send(a, "codex-1", "x", None, Timestamp(1)).unwrap();
        assert!(mb.leave(&b, Timestamp(5)));
        assert!(mb.take_deliveries(|_| true).is_empty());
        assert_eq!(mb.resolve("codex-1"), None);
        assert_eq!(
            mb.send(a, "codex-1", "x", None, Timestamp(2)),
            Err(SendError::UnknownRecipient("codex-1".into()))
        );
    }

    #[test]
    fn olvida_lo_antiguo_ya_entregado() {
        let (mut mb, a, b) = two();
        mb.sends.clear();
        for i in 0..(KEEP_MESSAGES as u64 + 10) {
            // Saltarse el límite por hora: un envío por hora.
            mb.send(a, "codex-1", "x", None, Timestamp(i * HOUR_MS))
                .unwrap();
            mb.inbox(&b);
        }
        assert_eq!(mb.messages().count(), KEEP_MESSAGES);
        assert_eq!(mb.threads().count(), KEEP_MESSAGES);
    }

    proptest! {
        /// Pase lo que pase, nadie envía más de RATE_PER_HOUR en una hora y un hilo abierto
        /// nunca tiene más saltos que su límite.
        #[test]
        fn los_limites_se_cumplen_siempre(gaps in proptest::collection::vec(0u64..400_000, 1..120)) {
            let (mut mb, a, _) = two();
            let mut now = 0u64;
            let mut accepted: Vec<u64> = Vec::new();
            let mut last: Option<MessageId> = None;
            for g in gaps {
                now += g;
                if let Ok(s) = mb.send(a, "codex-1", "x", last, Timestamp(now)) {
                    accepted.push(now);
                    last = Some(s.id);
                }
                for t in mb.threads() {
                    prop_assert!(t.state != ThreadState::Open || t.hops <= t.limit);
                }
            }
            for (i, t) in accepted.iter().enumerate() {
                let in_hour = accepted[i..].iter().take_while(|u| **u < t + HOUR_MS).count();
                prop_assert!(in_hour <= RATE_PER_HOUR);
            }
        }
    }
}
