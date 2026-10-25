//! Adapters reales de procesos contra procesos inofensivos lanzados por el propio test
//! (`sleep`, `sh` y una copia de este binario llamada `claude`). Nunca toca agentes reales.
#![cfg(all(unix, feature = "scanner", feature = "killer"))]

use std::process::{Child, Command};
use std::time::{Duration, Instant};

use kiti_core::adapters::{SysinfoScanner, UnixKiller, read_process};
use kiti_core::domain::{AgentId, Pid, ProcessSnapshot, Provider};
use kiti_core::ports::{AgentKiller, KillOutcome, KillTarget, ProcessScanner};

/// Espera a que el hijo haya hecho `exec`: justo tras el `fork` sigue siendo una copia de este
/// test, con otro `exe` y otro `argv`, y su identidad no serviría.
fn snapshot_of(pid: u32, exe_name: &str) -> ProcessSnapshot {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let ready = |s: &ProcessSnapshot| {
            s.exe
                .as_deref()
                .and_then(|e| e.rsplit('/').next())
                .is_some_and(|name| name.starts_with(exe_name))
                && !s.argv.is_empty()
        };
        if let Some(s) = read_process(Pid(pid)).filter(ready) {
            return s;
        }
        assert!(Instant::now() < deadline, "no apareció el pid {pid}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Copia de este mismo binario de test llamada `claude`: el escáner solo lee a fondo los
/// candidatos, así que hace falta un proceso con ese nombre. Solo ejecuta `durmiente`.
fn fake_claude() -> Child {
    let dir = std::env::temp_dir().join(format!("kiti-fake-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("claude");
    if !bin.exists() {
        std::fs::copy(std::env::current_exe().unwrap(), &bin).unwrap();
    }
    Command::new(bin)
        .args(["durmiente", "--exact", "--ignored"])
        .current_dir(&dir)
        .spawn()
        .unwrap()
}

#[test]
#[ignore = "solo se ejecuta como proceso hijo de fake_claude"]
fn durmiente() {
    std::thread::sleep(Duration::from_secs(30));
}

fn target(s: &ProcessSnapshot) -> KillTarget {
    KillTarget {
        id: AgentId {
            provider: Provider::Codex,
            pid: s.pid,
            start_time: s.start_time,
        },
        identity: s.identity(),
    }
}

fn exited(child: &mut Child) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn el_escaner_ve_un_candidato_con_exe_argv_cwd_y_hora_de_inicio() {
    let mut child = fake_claude();
    let pid = Pid(child.id());
    let mut scanner = SysinfoScanner::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    let seen = loop {
        let found = scanner.scan().unwrap().into_iter().find(|s| s.pid == pid);
        if let Some(s) = found.filter(|s| !s.argv.is_empty()) {
            break s;
        }
        assert!(Instant::now() < deadline, "el escáner no vio al candidato");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(seen.exe.as_deref().is_some_and(|e| e.ends_with("/claude")));
    assert_eq!(seen.argv.get(1).map(String::as_str), Some("durmiente"));
    assert!(
        seen.cwd
            .as_deref()
            .is_some_and(|c| c.contains("kiti-fake-"))
    );
    // start_time en epoch ms, en el pasado reciente.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert!(seen.start_time.0 <= now_ms && now_ms - seen.start_time.0 < 600_000);
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn el_escaner_no_devuelve_procesos_que_no_son_candidatos() {
    let me = Pid(std::process::id());
    assert!(
        SysinfoScanner::new()
            .scan()
            .unwrap()
            .iter()
            .all(|s| s.pid != me)
    );
}

#[tokio::test]
async fn sigterm_cierra_un_proceso_que_coincide() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let t = target(&snapshot_of(child.id(), "sleep"));
    let outcome = UnixKiller::default()
        .terminate(t, Duration::from_secs(2))
        .await;
    assert_eq!(outcome.unwrap(), KillOutcome::Killed);
    assert!(exited(&mut child));
}

#[tokio::test]
async fn si_ignora_sigterm_recibe_sigkill_tras_la_gracia() {
    let mut child = Command::new("sh")
        .args(["-c", "trap '' TERM; while :; do sleep 0.05; done"])
        .spawn()
        .unwrap();
    let t = target(&snapshot_of(child.id(), "sh"));
    // Que el `trap` ya esté activo antes de la primera señal.
    std::thread::sleep(Duration::from_millis(200));
    let start = Instant::now();
    let outcome = UnixKiller::default()
        .terminate(t, Duration::from_millis(400))
        .await;
    assert_eq!(outcome.unwrap(), KillOutcome::Killed);
    assert!(start.elapsed() >= Duration::from_millis(400));
    assert!(exited(&mut child));
}

#[tokio::test]
async fn no_envia_senales_si_la_identidad_no_coincide() {
    let mut child = Command::new("sleep").arg("30").spawn().unwrap();
    let mut t = target(&snapshot_of(child.id(), "sleep"));
    t.identity.cmd_fingerprint ^= 1; // otro programa con el mismo PID
    let outcome = UnixKiller::default()
        .terminate(t, Duration::from_millis(200))
        .await;
    assert_eq!(outcome.unwrap(), KillOutcome::StaleProcess);
    assert!(
        child.try_wait().unwrap().is_none(),
        "el proceso debe seguir vivo"
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[tokio::test]
async fn rechaza_el_pid_1_y_el_propio() {
    let killer = UnixKiller::default();
    for pid in [1, std::process::id()] {
        let mut t = target(&snapshot_of(std::process::id(), "process_adapters"));
        t.identity.pid = Pid(pid);
        t.id.pid = Pid(pid);
        let outcome = killer.terminate(t, Duration::from_millis(100)).await;
        assert_eq!(outcome.unwrap(), KillOutcome::StaleProcess, "pid {pid}");
    }
}

#[test]
fn detecta_un_proceso_que_se_convierte_en_agente_con_exec() {
    // Así lanza Kiti los agentes: `zsh -lic 'exec claude'`. El PID no cambia, el exe sí.
    let fake = {
        let dir = std::env::temp_dir().join(format!("kiti-exec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("claude");
        if !bin.exists() {
            std::fs::copy(std::env::current_exe().unwrap(), &bin).unwrap();
        }
        bin
    };
    let script = format!(
        "sleep 0.4; exec '{}' durmiente --exact --ignored",
        fake.display()
    );
    let mut child = Command::new("/bin/sh")
        .args(["-c", &script])
        .spawn()
        .unwrap();
    let pid = Pid(child.id());
    let mut scanner = SysinfoScanner::new();
    // Primer escaneo: todavía es `sh`, no es candidato.
    assert!(scanner.scan().unwrap().iter().all(|s| s.pid != pid));
    std::thread::sleep(Duration::from_millis(800));
    let seen = scanner.scan().unwrap().into_iter().find(|s| s.pid == pid);
    let exe = seen.and_then(|s| s.exe).unwrap_or_default();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(
        exe.ends_with("/claude"),
        "tras el exec el escáner debe ver claude, vio {exe:?}"
    );
}
