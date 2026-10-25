//! Qué cambios de estado merecen un aviso al usuario (ADR 0010). Puro: recibe los eventos del
//! registro y el tiempo; devuelve los avisos. Dónde y cómo se muestran es cosa del adapter.

use std::collections::HashMap;

use super::events::DomainEvent;
use super::ids::{AgentId, Timestamp};
use super::status::{StatusKind, StatusReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertKind {
    /// Pasó a amarillo: está bloqueado esperando un permiso.
    Permission,
    /// Pasó de trabajando a inactivo tras un turno largo: acabó lo que se le pidió.
    Finished,
    /// `StopFailure` del agente.
    Failed,
    /// El proceso murió sin cerrarse bien.
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alert {
    pub id: AgentId,
    pub kind: AlertKind,
}

/// Un turno más corto que esto no avisa al terminar: el usuario seguramente sigue mirando.
pub const MIN_TURN_MS: u64 = 15_000;
/// Mismo aviso para el mismo agente: como mucho uno cada tanto (permisos en ráfaga).
pub const COOLDOWN_MS: u64 = 30_000;

#[derive(Debug, Default)]
pub struct AlertPolicy {
    /// Desde cuándo trabaja (sin pasar por inactivo) cada agente.
    turn_started: HashMap<AgentId, Timestamp>,
    last: HashMap<(AgentId, AlertKind), Timestamp>,
}

impl AlertPolicy {
    pub fn observe(&mut self, events: &[DomainEvent], now: Timestamp) -> Vec<Alert> {
        let mut alerts = Vec::new();
        for ev in events {
            match ev {
                DomainEvent::StatusChanged {
                    id, to, reason, at, ..
                } => {
                    if let Some(kind) = self.on_status(*id, *to, *reason, *at) {
                        alerts.push(Alert { id: *id, kind });
                    }
                }
                DomainEvent::AgentExited {
                    id,
                    expected: false,
                } => {
                    self.turn_started.remove(id);
                    alerts.push(Alert {
                        id: *id,
                        kind: AlertKind::Crashed,
                    });
                }
                DomainEvent::AgentRemoved { id } => {
                    self.turn_started.remove(id);
                    self.last.retain(|(a, _), _| a != id);
                }
                _ => {}
            }
        }
        alerts.retain(|a| self.allow(*a, now));
        alerts
    }

    fn on_status(
        &mut self,
        id: AgentId,
        to: StatusKind,
        reason: StatusReason,
        at: Timestamp,
    ) -> Option<AlertKind> {
        match to {
            StatusKind::Working => {
                self.turn_started.entry(id).or_insert(at);
                None
            }
            StatusKind::Waiting => {
                // Esperar un permiso no corta el turno.
                self.turn_started.entry(id).or_insert(at);
                Some(AlertKind::Permission)
            }
            StatusKind::Idle => {
                let started = self.turn_started.remove(&id)?;
                // "Inactivo por silencio" (30 s sin eventos) no es haber terminado.
                let real = matches!(reason, StatusReason::Hook | StatusReason::SessionFile);
                (real && at.0.saturating_sub(started.0) >= MIN_TURN_MS)
                    .then_some(AlertKind::Finished)
            }
            StatusKind::Error => {
                self.turn_started.remove(&id);
                Some(AlertKind::Failed)
            }
            // Ya avisa `AgentExited`; un cierre esperado no avisa.
            StatusKind::Dead => None,
        }
    }

    fn allow(&mut self, alert: Alert, now: Timestamp) -> bool {
        let key = (alert.id, alert.kind);
        if self
            .last
            .get(&key)
            .is_some_and(|t| now.0.saturating_sub(t.0) < COOLDOWN_MS)
        {
            return false;
        }
        self.last.insert(key, now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Pid, ProcessStartTime, Provider};

    fn id(pid: u32) -> AgentId {
        AgentId {
            provider: Provider::Claude,
            pid: Pid(pid),
            start_time: ProcessStartTime(1),
        }
    }

    fn change(
        pid: u32,
        from: StatusKind,
        to: StatusKind,
        reason: StatusReason,
        at: u64,
    ) -> DomainEvent {
        DomainEvent::StatusChanged {
            id: id(pid),
            from,
            to,
            reason,
            at: Timestamp(at),
        }
    }

    fn kinds(alerts: Vec<Alert>) -> Vec<AlertKind> {
        alerts.into_iter().map(|a| a.kind).collect()
    }

    use StatusKind::{Error, Idle, Waiting, Working};
    use StatusReason::{Hook, Stale};

    #[test]
    fn pedir_permiso_avisa() {
        let mut p = AlertPolicy::default();
        let ev = [change(1, Working, Waiting, Hook, 1_000)];
        assert_eq!(
            kinds(p.observe(&ev, Timestamp(1_000))),
            [AlertKind::Permission]
        );
    }

    #[test]
    fn terminar_un_turno_largo_avisa_y_uno_corto_no() {
        let mut p = AlertPolicy::default();
        p.observe(&[change(1, Idle, Working, Hook, 0)], Timestamp(0));
        // Esperar un permiso a mitad del turno no lo reinicia.
        p.observe(
            &[change(1, Working, Waiting, Hook, 5_000)],
            Timestamp(5_000),
        );
        p.observe(
            &[change(1, Waiting, Working, Hook, 8_000)],
            Timestamp(8_000),
        );
        let done = p.observe(&[change(1, Working, Idle, Hook, 20_000)], Timestamp(20_000));
        assert_eq!(kinds(done), [AlertKind::Finished]);

        p.observe(&[change(1, Idle, Working, Hook, 30_000)], Timestamp(30_000));
        let quick = p.observe(&[change(1, Working, Idle, Hook, 34_000)], Timestamp(34_000));
        assert!(quick.is_empty());
    }

    #[test]
    fn quedarse_inactivo_por_silencio_no_es_terminar() {
        let mut p = AlertPolicy::default();
        p.observe(&[change(1, Idle, Working, Hook, 0)], Timestamp(0));
        let stale = p.observe(
            &[change(1, Working, Idle, Stale, 60_000)],
            Timestamp(60_000),
        );
        assert!(stale.is_empty());
    }

    #[test]
    fn error_y_caida_avisan_pero_un_cierre_esperado_no() {
        let mut p = AlertPolicy::default();
        let failed = p.observe(&[change(1, Working, Error, Hook, 0)], Timestamp(0));
        assert_eq!(kinds(failed), [AlertKind::Failed]);
        let crashed = [
            change(2, Working, StatusKind::Dead, StatusReason::ProcessGone, 0),
            DomainEvent::AgentExited {
                id: id(2),
                expected: false,
            },
        ];
        assert_eq!(
            kinds(p.observe(&crashed, Timestamp(0))),
            [AlertKind::Crashed]
        );
        let closed = [DomainEvent::AgentExited {
            id: id(3),
            expected: true,
        }];
        assert!(p.observe(&closed, Timestamp(0)).is_empty());
    }

    #[test]
    fn el_mismo_aviso_no_se_repite_en_rafaga() {
        let mut p = AlertPolicy::default();
        let ask = |at| [change(1, Working, Waiting, Hook, at)];
        assert_eq!(p.observe(&ask(0), Timestamp(0)).len(), 1);
        assert!(p.observe(&ask(10_000), Timestamp(10_000)).is_empty());
        assert_eq!(p.observe(&ask(40_000), Timestamp(40_000)).len(), 1);
        // Otro agente avisa aunque el primero esté en pausa.
        let other = [change(2, Working, Waiting, Hook, 41_000)];
        assert_eq!(p.observe(&other, Timestamp(41_000)).len(), 1);
    }
}
