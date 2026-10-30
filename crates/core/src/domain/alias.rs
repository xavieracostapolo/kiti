//! Alias estables de los agentes para los mensajes (ADR 0017). Puro: sin I/O ni reloj.
//!
//! Un alias pertenece a una clave estable (`AliasKey`), no a un PID: la sesión tmux de Kiti
//! sobrevive a que el agente se relance, y el libro se persiste entre arranques. Un alias
//! liberado queda reservado para su clave durante `RESERVE_MS`.

use std::collections::HashMap;

use super::ids::{AgentId, Provider, Timestamp};

/// Cuánto tiempo no se reutiliza un alias liberado.
pub const RESERVE_MS: u64 = 24 * 3_600_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AliasKey {
    /// Sesión tmux de Kiti (`kiti-N`): sobrevive a relanzar el agente.
    Tmux(String),
    /// Agente fuera de tmux: dura lo que dure el proceso.
    Process(AgentId),
}

/// Una fila del libro, en forma simple para persistirla.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasEntry {
    pub key: AliasKey,
    pub alias: String,
    /// `Some` si está liberado (reservado hasta `released_at + RESERVE_MS`).
    pub released_at: Option<Timestamp>,
}

pub fn alias_prefix(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
        Provider::Gemini => "gemini",
        Provider::Antigravity => "agy",
    }
}

#[derive(Debug, Default)]
pub struct AliasBook {
    entries: HashMap<AliasKey, (String, Option<Timestamp>)>,
}

impl AliasBook {
    /// Alias de `key`: el suyo si ya lo tiene (aunque esté liberado, lo recupera), o el número
    /// libre más bajo de `provider`, saltándose los usados y los reservados.
    pub fn assign(&mut self, key: &AliasKey, provider: Provider, _now: Timestamp) -> String {
        if let Some((alias, released)) = self.entries.get_mut(key) {
            *released = None;
            return alias.clone();
        }
        let prefix = alias_prefix(provider);
        let n = (1..)
            .find(|n| {
                let candidate = format!("{prefix}-{n}");
                !self.entries.values().any(|(a, _)| *a == candidate)
            })
            .unwrap_or(1);
        let alias = format!("{prefix}-{n}");
        self.entries.insert(key.clone(), (alias.clone(), None));
        alias
    }

    /// Alias vivo de `key` (no liberado).
    pub fn get(&self, key: &AliasKey) -> Option<&str> {
        match self.entries.get(key) {
            Some((alias, None)) => Some(alias),
            _ => None,
        }
    }

    /// Alias guardado de `key`, vivo o reservado.
    pub fn stored(&self, key: &AliasKey) -> Option<&str> {
        self.entries.get(key).map(|(a, _)| a.as_str())
    }

    pub fn contains(&self, key: &AliasKey) -> bool {
        self.entries.contains_key(key)
    }

    /// Libera el alias, reservado 24 h para la misma clave. Devuelve si cambió algo.
    pub fn release(&mut self, key: &AliasKey, now: Timestamp) -> bool {
        match self.entries.get_mut(key) {
            Some((_, released @ None)) => {
                *released = Some(now);
                true
            }
            _ => false,
        }
    }

    /// Mueve el alias de `from` a `to` sin renombrarlo. No hace nada si `to` ya existe.
    pub fn rekey(&mut self, from: &AliasKey, to: AliasKey) -> bool {
        if self.entries.contains_key(&to) {
            return false;
        }
        match self.entries.remove(from) {
            Some(v) => {
                self.entries.insert(to, v);
                true
            }
            None => false,
        }
    }

    /// Borra la reserva de `key` (p. ej. un alias `Process` ya traspasado a su sesión tmux).
    pub fn forget(&mut self, key: &AliasKey) -> bool {
        self.entries.remove(key).is_some()
    }

    /// Borra las reservas caducadas. Devuelve si borró alguna.
    pub fn prune(&mut self, now: Timestamp) -> bool {
        let before = self.entries.len();
        self.entries.retain(|_, (_, released)| {
            released.is_none_or(|at| now.0.saturating_sub(at.0) < RESERVE_MS)
        });
        self.entries.len() != before
    }

    /// Filas vivas (no liberadas).
    pub fn live(&self) -> impl Iterator<Item = (&AliasKey, &str)> {
        self.entries
            .iter()
            .filter(|(_, (_, r))| r.is_none())
            .map(|(k, (a, _))| (k, a.as_str()))
    }

    /// Todas las filas, ordenadas por alias para que el JSON sea estable.
    pub fn entries(&self) -> Vec<AliasEntry> {
        let mut out: Vec<AliasEntry> = self
            .entries
            .iter()
            .map(|(key, (alias, released_at))| AliasEntry {
                key: key.clone(),
                alias: alias.clone(),
                released_at: *released_at,
            })
            .collect();
        out.sort_by(|a, b| a.alias.cmp(&b.alias));
        out
    }

    /// Carga lo guardado. Descarta duplicados de alias (gana el primero).
    pub fn restore(&mut self, entries: Vec<AliasEntry>) {
        for e in entries {
            let dup = self.entries.values().any(|(a, _)| *a == e.alias);
            if !dup {
                self.entries.insert(e.key, (e.alias, e.released_at));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::{Pid, ProcessStartTime};

    fn process(provider: Provider, pid: u32) -> AliasKey {
        AliasKey::Process(AgentId {
            provider,
            pid: Pid(pid),
            start_time: ProcessStartTime(1),
        })
    }

    fn tmux(name: &str) -> AliasKey {
        AliasKey::Tmux(name.into())
    }

    const T0: Timestamp = Timestamp(1_000);

    #[test]
    fn el_alias_es_estable_para_la_misma_clave() {
        let mut b = AliasBook::default();
        let k = process(Provider::Claude, 1);
        assert_eq!(b.assign(&k, Provider::Claude, T0), "claude-1");
        assert_eq!(
            b.assign(&process(Provider::Claude, 2), Provider::Claude, T0),
            "claude-2"
        );
        assert_eq!(b.assign(&k, Provider::Claude, Timestamp(9_999)), "claude-1");
    }

    #[test]
    fn un_alias_liberado_no_se_reutiliza_en_24h_pero_si_despues() {
        let mut b = AliasBook::default();
        let a = process(Provider::Claude, 1);
        b.assign(&a, Provider::Claude, T0);
        assert!(b.release(&a, T0));
        let c = process(Provider::Claude, 3);
        assert_eq!(
            b.assign(&c, Provider::Claude, Timestamp(T0.0 + 1)),
            "claude-2"
        );
        let later = Timestamp(T0.0 + RESERVE_MS);
        assert!(b.prune(later));
        let d = process(Provider::Claude, 4);
        assert_eq!(b.assign(&d, Provider::Claude, later), "claude-1");
    }

    #[test]
    fn la_misma_clave_recupera_su_alias_liberado() {
        let mut b = AliasBook::default();
        let s = tmux("kiti-10");
        b.assign(&s, Provider::Claude, T0);
        b.release(&s, T0);
        assert_eq!(b.get(&s), None);
        assert_eq!(
            b.assign(&s, Provider::Claude, Timestamp(T0.0 + 5)),
            "claude-1"
        );
        assert_eq!(b.get(&s), Some("claude-1"));
    }

    #[test]
    fn restore_y_assign_respetan_lo_guardado() {
        let mut b = AliasBook::default();
        let s = tmux("kiti-3");
        let p = process(Provider::Antigravity, 7);
        b.assign(&p, Provider::Antigravity, T0);
        b.assign(&s, Provider::Claude, T0);
        b.release(&p, T0);
        let saved = b.entries();

        let mut c = AliasBook::default();
        c.restore(saved);
        assert_eq!(c.assign(&s, Provider::Claude, T0), "claude-1");
        // agy-1 sigue reservado para su clave.
        assert_eq!(
            c.assign(
                &process(Provider::Antigravity, 8),
                Provider::Antigravity,
                T0
            ),
            "agy-2"
        );
        assert_eq!(c.assign(&p, Provider::Antigravity, T0), "agy-1");
    }

    #[test]
    fn rekey_conserva_el_alias_sin_renombrar() {
        let mut b = AliasBook::default();
        let p = process(Provider::Claude, 1);
        b.assign(&p, Provider::Claude, T0);
        assert!(b.rekey(&p, tmux("kiti-1")));
        assert_eq!(b.get(&tmux("kiti-1")), Some("claude-1"));
        assert!(!b.contains(&p));
    }
}
