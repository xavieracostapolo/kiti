use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::process_read::{current_uid, snapshot};
use crate::domain::{ProcessSnapshot, is_candidate_exe};
use crate::ports::{ProcessScanner, ScanError};

/// `ProcessScanner` con sysinfo, en dos pasadas para no pagar por los ~600 procesos del usuario:
/// primero solo `exe` y usuario de todos; después `argv` y `cwd` solo de los candidatos.
/// Conserva el `System` entre escaneos, así que cada dato se lee una sola vez por proceso.
pub struct SysinfoScanner {
    system: System,
    uid: Option<u32>,
}

impl SysinfoScanner {
    pub fn new() -> Self {
        let mut system = System::new();
        let uid = current_uid(&mut system);
        Self { system, uid }
    }
}

impl Default for SysinfoScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessScanner for SysinfoScanner {
    fn scan(&mut self) -> Result<Vec<ProcessSnapshot>, ScanError> {
        let uid = self
            .uid
            .ok_or_else(|| ScanError::Failed("no se pudo leer el uid propio".into()))?;
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        // Solo se puede leer argv de procesos del mismo usuario, y solo esos son nuestros.
        let candidates: Vec<sysinfo::Pid> = self
            .system
            .processes()
            .values()
            .filter(|p| p.user_id().is_some_and(|u| **u == uid))
            .filter(|p| {
                p.exe()
                    .is_some_and(|e| is_candidate_exe(&e.to_string_lossy()))
            })
            .map(sysinfo::Process::pid)
            .collect();
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&candidates),
            false,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::OnlyIfNotSet),
        );
        Ok(candidates
            .iter()
            .filter_map(|pid| self.system.process(*pid))
            .filter_map(snapshot)
            .collect())
    }
}
