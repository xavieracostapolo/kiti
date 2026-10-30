//! Adapter de tmux contra un tmux real, en sockets aislados por test (nunca el del usuario ni el
//! de Kiti). Si tmux no está instalado, los tests no hacen nada.
#![cfg(all(unix, feature = "pty"))]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kiti_core::adapters::pty::PtyManager;
use kiti_core::adapters::tmux::Tmux;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// tmux con un socket único; mata su servidor y borra el archivo del socket al soltarse.
struct Fixture(Tmux, String);

impl Drop for Fixture {
    fn drop(&mut self) {
        self.0.kill_server();
        // tmux deja el archivo en /tmp/tmux-<uid>/ aunque el servidor ya haya terminado.
        use std::os::unix::fs::MetadataExt;
        if let Some(uid) = std::env::var_os("HOME").and_then(|h| std::fs::metadata(h).ok()) {
            let path = format!("/tmp/tmux-{}/{}", uid.uid(), self.1);
            let _ = std::fs::remove_file(path);
        }
    }
}

fn tmux() -> Option<Fixture> {
    let socket = format!(
        "kiti-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    Tmux::detect(&socket, None).map(|t| Fixture(t, socket))
}

fn until(cond: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    false
}

#[test]
fn crea_una_sesion_y_la_lista_con_su_pid_carpeta_y_proveedor() {
    let Some(t) = tmux() else { return };
    let cwd = std::env::temp_dir();
    let s =
        t.0.new_session(&cwd, "exec sleep 30", "codex", 100, 30)
            .unwrap();
    assert!(s.name.starts_with("kiti-"));
    assert!(s.pid > 0);
    let list = t.0.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, s.name);
    assert_eq!(list[0].pid, s.pid);
    assert_eq!(list[0].provider.as_deref(), Some("codex"));
    // macOS resuelve /var → /private/var: se compara la carpeta canónica.
    assert_eq!(
        std::fs::canonicalize(&list[0].cwd).unwrap(),
        std::fs::canonicalize(&cwd).unwrap()
    );
}

#[test]
fn kill_session_termina_la_sesion_pedida_y_solo_esa() {
    let Some(t) = tmux() else { return };
    let cwd = std::env::temp_dir();
    let a =
        t.0.new_session(&cwd, "exec sleep 30", "claude", 80, 24)
            .unwrap();
    let b =
        t.0.new_session(&cwd, "exec sleep 30", "claude", 80, 24)
            .unwrap();
    assert!(t.0.kill_session(&a.name));
    assert!(until(|| t.0.list().iter().all(|x| x.name != a.name)));
    assert!(t.0.list().iter().any(|x| x.name == b.name));
    assert!(!t.0.kill_session(&a.name));
}

#[test]
fn los_nombres_no_se_repiten() {
    let Some(t) = tmux() else { return };
    let cwd = std::env::temp_dir();
    let a =
        t.0.new_session(&cwd, "exec sleep 30", "claude", 80, 24)
            .unwrap();
    let b =
        t.0.new_session(&cwd, "exec sleep 30", "claude", 80, 24)
            .unwrap();
    assert_ne!(a.name, b.name);
    assert_eq!(t.0.list().len(), 2);
}

#[test]
fn el_cliente_se_conecta_por_pty_y_la_sesion_sobrevive_a_su_cierre() {
    let Some(t) = tmux() else { return };
    let cwd = std::env::temp_dir();
    let s = t.0.new_session(&cwd, "exec cat", "claude", 80, 24).unwrap();
    let pty = PtyManager::default();
    let client = pty.spawn(t.0.attach_spec(&s.name, 80, 24)).unwrap();
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = out.clone();
    pty.attach(&client.id, move |b| sink.lock().unwrap().extend(b))
        .unwrap();
    pty.write(&client.id, b"persistente\r").unwrap();
    assert!(until(
        || String::from_utf8_lossy(&out.lock().unwrap()).contains("persistente")
    ));
    // Muere el cliente (como al salir de Kiti): el agente sigue vivo en tmux.
    pty.kill(&client.id).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        t.0.list()
            .iter()
            .any(|x| x.name == s.name && x.pid == s.pid)
    );
}

#[test]
fn cuando_el_agente_termina_la_sesion_desaparece() {
    let Some(t) = tmux() else { return };
    let s =
        t.0.new_session(&std::env::temp_dir(), "exec sleep 0.3", "gemini", 80, 24)
            .unwrap();
    assert!(until(|| t.0.list().iter().all(|x| x.name != s.name)));
}

#[test]
fn escribe_un_mensaje_de_varias_lineas_en_la_sesion_de_un_pid() {
    let Some(t) = tmux() else { return };
    let out = std::env::temp_dir().join(format!(
        "kiti-type-{}-{}.txt",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let s =
        t.0.new_session(
            &std::env::temp_dir(),
            &format!("exec cat > {}", out.display()),
            "codex",
            80,
            24,
        )
        .unwrap();
    t.0.type_into(s.pid, "[Kiti] hola\nmundo").unwrap();
    assert!(until(|| std::fs::read_to_string(&out).is_ok_and(|x| x
        .contains("[Kiti] hola")
        && x.contains("mundo\n"))));
    assert!(t.0.type_into(999_999, "x").is_err());
    let _ = std::fs::remove_file(out);
}
