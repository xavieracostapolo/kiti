//! Resumen del semáforo para la barra de menú (ADR 0006, Hito 7).

use crate::domain::StatusKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub working: u32,
    pub waiting: u32,
    pub idle: u32,
    pub error: u32,
    pub dead: u32,
}

impl Summary {
    pub fn add(&mut self, kind: StatusKind) {
        match kind {
            StatusKind::Working => self.working += 1,
            StatusKind::Waiting => self.waiting += 1,
            StatusKind::Idle => self.idle += 1,
            StatusKind::Error => self.error += 1,
            StatusKind::Dead => self.dead += 1,
        }
    }

    fn total(&self) -> u32 {
        self.working + self.waiting + self.idle + self.error + self.dead
    }

    /// Título de la barra de menú: primero lo que pide atención (amarillo, rojo), sin ceros.
    pub fn title(&self) -> String {
        if self.total() == 0 {
            return "Kiti".into();
        }
        [
            ("🟡", self.waiting),
            ("🔴", self.error + self.dead),
            ("🟢", self.working),
            ("⚪", self.idle),
        ]
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(icon, n)| format!("{icon}{n}"))
        .collect::<Vec<_>>()
        .join(" ")
    }

    pub fn tooltip(&self) -> String {
        if self.total() == 0 {
            return "Kiti: sin agentes".into();
        }
        let parts: Vec<String> = [
            (self.waiting, "esperando"),
            (self.working, "trabajando"),
            (self.idle, "inactivos"),
            (self.error + self.dead, "con error o caídos"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect();
        format!("Kiti: {}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(working: u32, waiting: u32, idle: u32, error: u32, dead: u32) -> Summary {
        Summary {
            working,
            waiting,
            idle,
            error,
            dead,
        }
    }

    #[test]
    fn primero_lo_que_pide_atencion_y_sin_ceros() {
        assert_eq!(counts(3, 1, 2, 0, 0).title(), "🟡1 🟢3 ⚪2");
        assert_eq!(counts(0, 0, 4, 1, 1).title(), "🔴2 ⚪4");
        assert_eq!(counts(2, 0, 0, 0, 0).title(), "🟢2");
    }

    #[test]
    fn sin_agentes_lo_dice() {
        assert_eq!(Summary::default().title(), "Kiti");
    }

    #[test]
    fn tooltip_en_palabras() {
        assert_eq!(
            counts(3, 1, 2, 0, 1).tooltip(),
            "Kiti: 1 esperando, 3 trabajando, 2 inactivos, 1 con error o caídos"
        );
        assert_eq!(Summary::default().tooltip(), "Kiti: sin agentes");
    }
}
