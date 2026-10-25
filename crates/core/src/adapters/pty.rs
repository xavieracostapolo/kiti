//! Terminales propias para agentes lanzados desde Kiti (ADR 0007).
//!
//! Un hilo lector por sesión guarda un scrollback acotado y reenvía la salida al suscriptor
//! actual. `attach` copia el scrollback y cambia de suscriptor bajo el mismo lock, así que al
//! reabrir la ventana no se pierde ni se duplica ningún byte.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tracing::info;

pub const DEFAULT_SCROLLBACK: usize = 512 * 1024;

#[derive(Debug, Clone)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInfo {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub running: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("terminal desconocida: {0}")]
    NotFound(String),
    #[error("{0}")]
    Io(String),
}

fn io(e: impl std::fmt::Display) -> PtyError {
    PtyError::Io(e.to_string())
}

type Sink = Box<dyn Fn(Vec<u8>) + Send>;

struct Shared {
    scrollback: VecDeque<u8>,
    sink: Option<Sink>,
    running: bool,
}

struct Session {
    pid: u32,
    cwd: String,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    shared: Arc<Mutex<Shared>>,
}

pub struct PtyManager {
    cap: usize,
    next: AtomicU64,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::with_scrollback(DEFAULT_SCROLLBACK)
    }
}

impl PtyManager {
    pub fn with_scrollback(cap: usize) -> Self {
        Self {
            cap,
            next: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, PtyError> {
        self.sessions
            .lock()
            .expect("lock")
            .get(id)
            .cloned()
            .ok_or_else(|| PtyError::NotFound(id.to_owned()))
    }

    pub fn spawn(&self, spec: LaunchSpec) -> Result<TerminalInfo, PtyError> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(io)?;
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        cmd.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(cmd).map_err(io)?;
        // Sin el esclavo abierto en este proceso, el lector recibe EOF cuando el hijo termina.
        drop(pair.slave);
        let pid = child.process_id().unwrap_or(0);
        let reader = pair.master.try_clone_reader().map_err(io)?;
        let writer = pair.master.take_writer().map_err(io)?;
        let id = format!("pty-{}", self.next.fetch_add(1, Ordering::Relaxed));
        let shared = Arc::new(Mutex::new(Shared {
            scrollback: VecDeque::new(),
            sink: None,
            running: true,
        }));
        let child = Arc::new(Mutex::new(child));
        let session = Arc::new(Session {
            pid,
            cwd: spec.cwd.to_string_lossy().into_owned(),
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: child.clone(),
            shared: shared.clone(),
        });
        let cap = self.cap;
        std::thread::Builder::new()
            .name(format!("kiti-{id}"))
            .spawn(move || read_loop(reader, shared, child, cap))
            .map_err(io)?;
        info!(event = "pty.spawned", id, pid, program = %spec.program.display());
        let info = TerminalInfo {
            id: id.clone(),
            pid,
            cwd: session.cwd.clone(),
            running: true,
        };
        self.sessions.lock().expect("lock").insert(id, session);
        Ok(info)
    }

    /// Envía a `sink` el scrollback y, a continuación, la salida nueva, en orden: las dos cosas
    /// ocurren bajo el lock que también toma el lector. Sustituye al suscriptor anterior (la
    /// ventana que se cerró).
    pub fn attach(
        &self,
        id: &str,
        sink: impl Fn(Vec<u8>) + Send + 'static,
    ) -> Result<(), PtyError> {
        let session = self.get(id)?;
        let mut shared = session.shared.lock().expect("lock");
        let replay: Vec<u8> = shared.scrollback.iter().copied().collect();
        if !replay.is_empty() {
            sink(replay);
        }
        shared.sink = Some(Box::new(sink));
        Ok(())
    }

    pub fn write(&self, id: &str, data: &[u8]) -> Result<(), PtyError> {
        let session = self.get(id)?;
        let mut writer = session.writer.lock().expect("lock");
        writer.write_all(data).map_err(io)?;
        writer.flush().map_err(io)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.get(id)?
            .master
            .lock()
            .expect("lock")
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(io)
    }

    pub fn kill(&self, id: &str) -> Result<(), PtyError> {
        self.get(id)?.child.lock().expect("lock").kill().map_err(io)
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        let mut out: Vec<TerminalInfo> = self
            .sessions
            .lock()
            .expect("lock")
            .iter()
            .map(|(id, s)| TerminalInfo {
                id: id.clone(),
                pid: s.pid,
                cwd: s.cwd.clone(),
                running: s.shared.lock().expect("lock").running,
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }
}

fn read_loop(
    mut reader: Box<dyn Read + Send>,
    shared: Arc<Mutex<Shared>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    cap: usize,
) {
    let mut buf = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut s = shared.lock().expect("lock");
        s.scrollback.extend(&buf[..n]);
        let excess = s.scrollback.len().saturating_sub(cap);
        s.scrollback.drain(..excess);
        // Se envía con el lock tomado: `attach` no puede colarse entre el scrollback y el stream.
        if let Some(sink) = &s.sink {
            sink(buf[..n].to_vec());
        }
    }
    shared.lock().expect("lock").running = false;
    // Recoge el proceso para que no quede zombi.
    let _ = child.lock().expect("lock").wait();
}
