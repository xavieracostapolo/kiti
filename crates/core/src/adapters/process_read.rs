//! Lectura de procesos con sysinfo, compartida por el escáner y el killer.

use sysinfo::{Process, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

use crate::domain::{Pid, ProcessSnapshot, ProcessStartTime};

/// Solo lo que hace falta para clasificar: ni CPU, ni memoria, ni entorno.
pub(crate) fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_cwd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet)
}

pub(crate) fn current_uid(system: &mut System) -> Option<u32> {
    let me = sysinfo::get_current_pid().ok()?;
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[me]), true, refresh_kind());
    system.process(me)?.user_id().map(|u| **u)
}

/// `None` para zombis: el proceso ya terminó aunque su padre no lo haya recogido.
pub(crate) fn snapshot(p: &Process) -> Option<ProcessSnapshot> {
    if p.status() == ProcessStatus::Zombie {
        return None;
    }
    Some(ProcessSnapshot {
        pid: Pid(p.pid().as_u32()),
        ppid: p.parent().map(|pp| Pid(pp.as_u32())),
        start_time: ProcessStartTime(p.start_time().saturating_mul(1000)),
        uid: p.user_id().map(|u| **u)?,
        exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
        argv: p
            .cmd()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect(),
        cwd: p.cwd().map(|c| c.to_string_lossy().into_owned()),
    })
}

/// Lee un único PID desde cero (exe, argv, cwd y usuario).
pub fn read_one(pid: Pid) -> Option<ProcessSnapshot> {
    let mut system = System::new();
    let spid = sysinfo::Pid::from_u32(pid.0);
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[spid]), true, refresh_kind());
    system.process(spid).and_then(snapshot)
}
