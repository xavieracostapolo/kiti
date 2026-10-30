//! Agentes efímeros por tarea (ADR 0019): estado de cada tarea y política de topes. Puro: la
//! hora llega como argumento y nada toca el disco ni lanza procesos.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Duration;

use super::{AgentId, Pid, Provider, Timestamp};

/// Tareas vivas (`Launching` o `Running`) a la vez.
pub const MAX_LIVE_TASKS: usize = 3;
/// Lanzamientos por hora móvil.
pub const MAX_LAUNCHES_PER_HOUR: usize = 6;
/// Una tarea en `Running` sin `kiti_done` pasado este tiempo sale como atascada.
pub const STUCK_AFTER: Duration = Duration::from_secs(4 * 3600);
/// Tareas terminadas que se recuerdan para el tablero.
pub const KEEP_TASKS: usize = 50;
const HOUR: Duration = Duration::from_secs(3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

/// Ficha de rol de `docs/agentes/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRole {
    Implementar,
    Documentar,
    Revisar,
}

impl TaskRole {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "implementar" => Some(Self::Implementar),
            "documentar" => Some(Self::Documentar),
            "revisar" => Some(Self::Revisar),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Implementar => "implementar",
            Self::Documentar => "documentar",
            Self::Revisar => "revisar",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    Launching,
    Running,
    Done { pr: Option<String> },
    Failed { reason: String },
    Stuck,
}

impl TaskState {
    /// Cuenta para el tope de tareas simultáneas.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Launching | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: TaskId,
    pub orchestrator: AgentId,
    pub orchestrator_alias: String,
    pub role: TaskRole,
    pub provider: Provider,
    pub model: String,
    pub branch: String,
    pub worktree: Option<PathBuf>,
    pub session: Option<String>,
    pub pid: Option<Pid>,
    pub state: TaskState,
    pub created_at: Timestamp,
    pub running_since: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

/// Lo que pide el orquestador al crear una tarea.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub orchestrator: AgentId,
    pub orchestrator_alias: String,
    pub role: TaskRole,
    pub provider: Provider,
    pub model: String,
    pub branch: String,
}

/// Por qué no se puede lanzar otra tarea ahora.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitReason {
    #[error("ya hay {MAX_LIVE_TASKS} tareas en curso; espera a que alguna termine")]
    TooManyLive,
    #[error(
        "límite de {MAX_LAUNCHES_PER_HOUR} lanzamientos por hora; reintenta en {retry_after_s} s"
    )]
    TooManyPerHour { retry_after_s: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TaskError {
    #[error("la tarea no existe")]
    Unknown,
    #[error("la tarea no está en un estado que admita ese cambio")]
    InvalidTransition,
}

/// Todas las tareas conocidas y la política que decide si cabe otra.
#[derive(Debug, Default)]
pub struct TaskBook {
    tasks: Vec<Task>,
    launches: VecDeque<Timestamp>,
    next_id: u64,
    /// Cambió algo que el tablero debe mostrar.
    dirty: bool,
}

impl TaskBook {
    /// ¿Cabe una tarea más? No cambia nada.
    pub fn check(&mut self, now: Timestamp) -> Result<(), LimitReason> {
        if self.tasks.iter().filter(|t| t.state.is_live()).count() >= MAX_LIVE_TASKS {
            return Err(LimitReason::TooManyLive);
        }
        while self
            .launches
            .front()
            .is_some_and(|at| now.since(*at) >= HOUR)
        {
            self.launches.pop_front();
        }
        if self.launches.len() >= MAX_LAUNCHES_PER_HOUR {
            let oldest = self.launches.front().copied().unwrap_or(now);
            let wait = HOUR.saturating_sub(now.since(oldest));
            return Err(LimitReason::TooManyPerHour {
                retry_after_s: wait.as_secs().max(1),
            });
        }
        Ok(())
    }

    /// Registra una tarea nueva en `Launching` si la política lo permite.
    pub fn begin(&mut self, new: NewTask, now: Timestamp) -> Result<TaskId, LimitReason> {
        self.check(now)?;
        self.next_id += 1;
        let id = TaskId(self.next_id);
        self.launches.push_back(now);
        self.tasks.push(Task {
            id,
            orchestrator: new.orchestrator,
            orchestrator_alias: new.orchestrator_alias,
            role: new.role,
            provider: new.provider,
            model: new.model,
            branch: new.branch,
            worktree: None,
            session: None,
            pid: None,
            state: TaskState::Launching,
            created_at: now,
            running_since: None,
            finished_at: None,
        });
        self.dirty = true;
        Ok(id)
    }

    fn get_mut(&mut self, id: TaskId) -> Result<&mut Task, TaskError> {
        self.tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(TaskError::Unknown)
    }

    pub fn get(&self, id: TaskId) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// `Launching` → `Running`, con lo que devolvió el lanzador.
    pub fn launched(
        &mut self,
        id: TaskId,
        worktree: PathBuf,
        session: String,
        pid: Pid,
        now: Timestamp,
    ) -> Result<(), TaskError> {
        let t = self.get_mut(id)?;
        if t.state != TaskState::Launching {
            return Err(TaskError::InvalidTransition);
        }
        t.state = TaskState::Running;
        t.worktree = Some(worktree);
        t.session = Some(session);
        t.pid = Some(pid);
        t.running_since = Some(now);
        self.dirty = true;
        Ok(())
    }

    /// `Launching` → `Failed`: el lanzador no pudo. No gasta un hueco de la hora.
    pub fn launch_failed(
        &mut self,
        id: TaskId,
        reason: String,
        now: Timestamp,
    ) -> Result<(), TaskError> {
        let t = self.get_mut(id)?;
        if t.state != TaskState::Launching {
            return Err(TaskError::InvalidTransition);
        }
        t.state = TaskState::Failed { reason };
        t.finished_at = Some(now);
        self.dirty = true;
        Ok(())
    }

    /// El agente entregó: `Running` o `Stuck` → `Done` (con PR) o `Failed` (con motivo).
    pub fn finish(
        &mut self,
        id: TaskId,
        outcome: Result<Option<String>, String>,
        now: Timestamp,
    ) -> Result<(), TaskError> {
        let t = self.get_mut(id)?;
        if !matches!(t.state, TaskState::Running | TaskState::Stuck) {
            return Err(TaskError::InvalidTransition);
        }
        t.state = match outcome {
            Ok(pr) => TaskState::Done { pr },
            Err(reason) => TaskState::Failed { reason },
        };
        t.finished_at = Some(now);
        self.dirty = true;
        self.prune();
        Ok(())
    }

    /// Pasa a `Stuck` las tareas que llevan demasiado en `Running`. Devuelve las nuevas.
    pub fn tick(&mut self, now: Timestamp) -> Vec<TaskId> {
        let mut stuck = Vec::new();
        for t in &mut self.tasks {
            if t.state == TaskState::Running
                && t.running_since
                    .is_some_and(|since| now.since(since) >= STUCK_AFTER)
            {
                t.state = TaskState::Stuck;
                stuck.push(t.id);
            }
        }
        self.dirty |= !stuck.is_empty();
        stuck
    }

    /// La tarea (`Running` o `Stuck`) cuyo agente tiene este pid.
    pub fn open_for_pid(&self, pid: Pid) -> Option<&Task> {
        self.tasks.iter().find(|t| {
            t.pid == Some(pid) && matches!(t.state, TaskState::Running | TaskState::Stuck)
        })
    }

    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter()
    }

    /// Devuelve si hay cambios sin publicar y los marca como publicados.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn prune(&mut self) {
        let done = self.tasks.iter().filter(|t| !t.state.is_live()).count();
        let mut excess = done.saturating_sub(KEEP_TASKS);
        self.tasks.retain(|t| {
            if excess > 0 && !t.state.is_live() && t.state != TaskState::Stuck {
                excess -= 1;
                return false;
            }
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ProcessStartTime;

    fn orchestrator() -> AgentId {
        AgentId {
            provider: Provider::Claude,
            pid: Pid(1),
            start_time: ProcessStartTime(1),
        }
    }

    fn new_task(branch: &str) -> NewTask {
        NewTask {
            orchestrator: orchestrator(),
            orchestrator_alias: "claude-1".into(),
            role: TaskRole::Implementar,
            provider: Provider::Claude,
            model: "sonnet".into(),
            branch: branch.into(),
        }
    }

    fn run(book: &mut TaskBook, branch: &str, pid: u32, now: u64) -> TaskId {
        let id = book.begin(new_task(branch), Timestamp(now)).unwrap();
        book.launched(id, "/wt".into(), "kiti-1".into(), Pid(pid), Timestamp(now))
            .unwrap();
        id
    }

    #[test]
    fn el_cuarto_lanzamiento_simultaneo_se_rechaza() {
        let mut book = TaskBook::default();
        for n in 0..MAX_LIVE_TASKS {
            run(&mut book, &format!("feat/t{n}"), 10 + n as u32, 1_000);
        }
        assert_eq!(
            book.begin(new_task("feat/t4"), Timestamp(2_000)),
            Err(LimitReason::TooManyLive)
        );
    }

    #[test]
    fn una_tarea_terminada_libera_su_hueco() {
        let mut book = TaskBook::default();
        let first = run(&mut book, "feat/a", 10, 1_000);
        run(&mut book, "feat/b", 11, 1_000);
        run(&mut book, "feat/c", 12, 1_000);
        book.finish(first, Ok(Some("#1".into())), Timestamp(2_000))
            .unwrap();
        assert!(book.begin(new_task("feat/d"), Timestamp(3_000)).is_ok());
    }

    #[test]
    fn seis_por_hora_movil() {
        let mut book = TaskBook::default();
        for n in 0..MAX_LAUNCHES_PER_HOUR {
            let id = run(
                &mut book,
                &format!("feat/t{n}"),
                10 + n as u32,
                1_000 + n as u64,
            );
            book.finish(id, Ok(None), Timestamp(2_000)).unwrap();
        }
        let err = book
            .begin(new_task("feat/x"), Timestamp(3_000))
            .unwrap_err();
        match err {
            LimitReason::TooManyPerHour { retry_after_s } => {
                assert!(retry_after_s > 3_500 && retry_after_s <= 3_600);
            }
            other => panic!("{other:?}"),
        }
        // Pasada una hora desde el primero, vuelve a caber uno.
        assert!(
            book.begin(new_task("feat/x"), Timestamp(1_000 + 3_600_000))
                .is_ok()
        );
    }

    #[test]
    fn un_lanzamiento_fallido_no_deja_tarea_viva() {
        let mut book = TaskBook::default();
        let id = book.begin(new_task("feat/a"), Timestamp(1)).unwrap();
        book.launch_failed(id, "sin tmux".into(), Timestamp(2))
            .unwrap();
        assert_eq!(
            book.get(id).unwrap().state,
            TaskState::Failed {
                reason: "sin tmux".into()
            }
        );
        assert_eq!(book.tasks().filter(|t| t.state.is_live()).count(), 0);
    }

    #[test]
    fn atascada_tras_cuatro_horas_en_running() {
        let mut book = TaskBook::default();
        let id = run(&mut book, "feat/a", 10, 0);
        assert!(book.tick(Timestamp(3 * 3_600_000)).is_empty());
        assert_eq!(book.tick(Timestamp(4 * 3_600_000)), vec![id]);
        assert_eq!(book.get(id).unwrap().state, TaskState::Stuck);
        // No se repite el aviso.
        assert!(book.tick(Timestamp(5 * 3_600_000)).is_empty());
        // Una atascada ya no cuenta como viva, pero su agente aún puede entregar.
        assert!(book.open_for_pid(Pid(10)).is_some());
        book.finish(id, Ok(None), Timestamp(5 * 3_600_000)).unwrap();
    }

    #[test]
    fn transiciones_validas_e_invalidas() {
        let mut book = TaskBook::default();
        let id = book.begin(new_task("feat/a"), Timestamp(1)).unwrap();
        // Launching no puede entregar.
        assert_eq!(
            book.finish(id, Ok(None), Timestamp(2)),
            Err(TaskError::InvalidTransition)
        );
        book.launched(id, "/wt".into(), "kiti-1".into(), Pid(10), Timestamp(2))
            .unwrap();
        // Running no vuelve a lanzarse ni a fallar al lanzar.
        assert_eq!(
            book.launched(id, "/wt".into(), "kiti-1".into(), Pid(10), Timestamp(3)),
            Err(TaskError::InvalidTransition)
        );
        assert_eq!(
            book.launch_failed(id, "x".into(), Timestamp(3)),
            Err(TaskError::InvalidTransition)
        );
        book.finish(id, Err("rompió".into()), Timestamp(4)).unwrap();
        // Terminada es final.
        assert_eq!(
            book.finish(id, Ok(None), Timestamp(5)),
            Err(TaskError::InvalidTransition)
        );
        assert!(book.open_for_pid(Pid(10)).is_none());
        assert_eq!(
            book.finish(TaskId(99), Ok(None), Timestamp(5)),
            Err(TaskError::Unknown)
        );
    }

    #[test]
    fn el_rol_se_lee_de_su_nombre() {
        assert_eq!(TaskRole::parse("revisar"), Some(TaskRole::Revisar));
        assert_eq!(TaskRole::parse(" documentar "), Some(TaskRole::Documentar));
        assert_eq!(TaskRole::parse("otro"), None);
    }

    #[test]
    fn dirty_se_marca_y_se_consume() {
        let mut book = TaskBook::default();
        assert!(!book.take_dirty());
        book.begin(new_task("feat/a"), Timestamp(1)).unwrap();
        assert!(book.take_dirty());
        assert!(!book.take_dirty());
    }
}
