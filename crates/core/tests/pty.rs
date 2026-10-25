//! PtyManager contra procesos inofensivos (`sh`, `cat`). Nunca lanza un agente real.
#![cfg(all(unix, feature = "pty"))]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kiti_core::adapters::pty::{LaunchSpec, PtyManager};

fn spec(program: &str, args: &[&str]) -> LaunchSpec {
    LaunchSpec {
        program: program.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: std::env::temp_dir(),
        cols: 80,
        rows: 24,
    }
}

/// Recoge lo que llega por el stream de una sesión.
type Buffer = Arc<Mutex<Vec<u8>>>;

fn collector() -> (Buffer, impl Fn(Vec<u8>) + Send + 'static) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = buf.clone();
    (buf, move |bytes: Vec<u8>| {
        sink.lock().unwrap().extend(bytes)
    })
}

fn wait_for(buf: &Buffer, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if String::from_utf8_lossy(&buf.lock().unwrap()).contains(needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn lanza_un_proceso_y_su_salida_queda_en_el_scrollback() {
    let pty = PtyManager::default();
    let info = pty
        .spawn(spec("/bin/sh", &["-c", "echo hola-kiti; sleep 5"]))
        .unwrap();
    assert!(info.pid > 0);
    std::thread::sleep(Duration::from_millis(300));
    // Quien se conecta después recibe lo ya escrito.
    let (buf, sink) = collector();
    pty.attach(&info.id, sink).unwrap();
    assert!(wait_for(&buf, "hola-kiti"));
    assert_eq!(
        pty.list().iter().map(|t| t.pid).collect::<Vec<_>>(),
        vec![info.pid]
    );
    pty.kill(&info.id).unwrap();
}

#[test]
fn la_entrada_llega_al_proceso_y_su_eco_vuelve() {
    let pty = PtyManager::default();
    let info = pty.spawn(spec("/bin/cat", &[])).unwrap();
    let (buf, sink) = collector();
    pty.attach(&info.id, sink).unwrap();
    pty.write(&info.id, b"escrito-desde-kiti\n").unwrap();
    assert!(wait_for(&buf, "escrito-desde-kiti"));
    pty.resize(&info.id, 120, 40).unwrap();
    pty.kill(&info.id).unwrap();
}

#[test]
fn reconectar_no_pierde_bytes() {
    let pty = PtyManager::default();
    let info = pty
        .spawn(spec(
            "/bin/sh",
            &[
                "-c",
                "for i in 1 2 3 4 5 6; do echo linea-$i; sleep 0.1; done; sleep 5",
            ],
        ))
        .unwrap();
    let (_first, sink1) = collector();
    pty.attach(&info.id, sink1).unwrap();
    std::thread::sleep(Duration::from_millis(250));
    // Nueva ventana: primero recibe el scrollback y después el stream, en orden.
    let (second, sink2) = collector();
    pty.attach(&info.id, sink2).unwrap();
    assert!(wait_for(&second, "linea-6"));
    let text = String::from_utf8_lossy(&second.lock().unwrap()).to_string();
    for i in 1..=6 {
        assert_eq!(
            text.matches(&format!("linea-{i}")).count(),
            1,
            "linea-{i} en {text:?}"
        );
    }
    pty.kill(&info.id).unwrap();
}

#[test]
fn el_scrollback_esta_acotado() {
    let pty = PtyManager::with_scrollback(1024);
    let info = pty
        .spawn(spec(
            "/bin/sh",
            &["-c", "yes 0123456789 | head -c 20000; sleep 5"],
        ))
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let (buf, sink) = collector();
    pty.attach(&info.id, sink).unwrap();
    assert!(buf.lock().unwrap().len() <= 1024);
    pty.kill(&info.id).unwrap();
}

#[test]
fn ids_desconocidos_dan_error_y_kill_cierra_el_proceso() {
    let pty = PtyManager::default();
    assert!(pty.write("no-existe", b"x").is_err());
    let info = pty.spawn(spec("/bin/cat", &[])).unwrap();
    pty.kill(&info.id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while pty.list().iter().any(|t| t.id == info.id && t.running) {
        assert!(Instant::now() < deadline, "el proceso sigue vivo");
        std::thread::sleep(Duration::from_millis(20));
    }
}
