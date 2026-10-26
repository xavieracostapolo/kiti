//! `kiti-hook` contra un servidor TCP mínimo que captura la petición. Nunca debe frenar ni
//! romper al agente: siempre `exit 0`, rápido, y en Gemini solo `{}` por stdout.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TOKEN: &str = "abc123";

struct Captured {
    request_line: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Acepta una conexión, lee la petición y responde 204.
fn server() -> (u16, mpsc::Receiver<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            let (n, v) = line.split_once(':').unwrap();
            headers.push((n.trim().to_owned(), v.trim().to_owned()));
        }
        let len: usize = headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
            .map_or(0, |(_, v)| v.parse().unwrap());
        let mut body = vec![0; len];
        reader.read_exact(&mut body).unwrap();
        let mut stream = stream;
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        tx.send(Captured {
            request_line: request_line.trim_end().to_owned(),
            headers,
            body,
        })
        .unwrap();
    });
    (port, rx)
}

fn token_file() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("kiti-hook-token-{}", std::process::id()));
    std::fs::write(&path, format!("{TOKEN}\n")).unwrap();
    path
}

/// `args`: el proveedor y, para Antigravity, el evento (`"antigravity PreToolUse"`).
fn run(args: &str, port: u16, token: &std::path::Path, stdin: &[u8]) -> (Output, Duration) {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kiti-hook"))
        .args(args.split(' '))
        .env("KITI_HOOK_PORT", port.to_string())
        .env("KITI_TOKEN_FILE", token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (out, start.elapsed())
}

#[test]
fn reenvia_el_hook_con_token_json_y_ppid() {
    let (port, rx) = server();
    let body = br#"{"session_id":"thr_1","hook_event_name":"PreToolUse"}"#;
    let (out, _) = run("codex", port, &token_file(), body);
    assert!(out.status.success());
    assert!(out.stdout.is_empty(), "Codex no espera nada por stdout");
    let req = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(req.request_line, "POST /v1/hooks/codex HTTP/1.1");
    assert_eq!(req.header("authorization"), Some("Bearer abc123"));
    assert_eq!(req.header("content-type"), Some("application/json"));
    assert_eq!(
        req.header("host"),
        Some(format!("127.0.0.1:{port}").as_str())
    );
    // El padre del forwarder es este test (en la vida real, el CLI o su `sh -c`).
    assert_eq!(
        req.header("x-kiti-ppid"),
        Some(std::process::id().to_string().as_str())
    );
    assert_eq!(req.body, body);
}

#[test]
fn gemini_recibe_exactamente_un_json_vacio_por_stdout() {
    let (port, _rx) = server();
    let (out, _) = run("gemini", port, &token_file(), b"{}");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "{}");
}

#[test]
fn con_kiti_cerrada_sale_en_silencio_y_rapido() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    // El listener ya se cerró: el puerto rechaza la conexión.
    let (out, took) = run("gemini", port, &token_file(), b"{}");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "{}");
    assert!(
        out.stderr.is_empty(),
        "nada que el agente pueda mostrar como error"
    );
    assert!(took < Duration::from_millis(800), "tardó {took:?}");
}

#[test]
fn sin_token_ni_proveedor_valido_tampoco_rompe_al_agente() {
    let (port, rx) = server();
    let missing = std::env::temp_dir().join("kiti-no-existe-token");
    let (out, _) = run("codex", port, &missing, b"{}");
    assert!(out.status.success());
    let (out, _) = run("otro", port, &token_file(), b"{}");
    assert!(out.status.success());
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "no envía nada"
    );
}

#[test]
fn antigravity_manda_el_evento_y_responde_lo_neutro() {
    // Su JSON no dice qué evento es: va en X-Kiti-Event (ADR 0014).
    let (port, rx) = server();
    let body = br#"{"conversationId":"c-1","toolCall":{"name":"run_command","args":{}}}"#;
    let (out, _) = run("antigravity PreToolUse", port, &token_file(), body);
    assert!(out.status.success());
    // PreToolUse exige una decisión: "allow" delega en la política normal (Kiti no decide).
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        r#"{"decision":"allow"}"#
    );
    let req = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(req.request_line, "POST /v1/hooks/antigravity HTTP/1.1");
    assert_eq!(req.header("x-kiti-event"), Some("PreToolUse"));
    assert_eq!(req.body, body);
}

#[test]
fn antigravity_responde_igual_con_kiti_cerrada() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    for (event, expected) in [
        ("PreToolUse", r#"{"decision":"allow"}"#),
        ("Stop", r#"{"decision":"allow"}"#),
        ("PostToolUse", "{}"),
        ("SessionStart", "{}"),
    ] {
        let (out, _) = run(&format!("antigravity {event}"), port, &token_file(), b"{}");
        assert!(out.status.success());
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{event}");
        assert!(out.stderr.is_empty());
    }
    // Un evento con caracteres raros no se reenvía, pero sigue respondiendo.
    let (out, _) = run("antigravity Pre;rm", port, &token_file(), b"{}");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "{}");
}
