//! Mensajes en espera e hilos abiertos en `mailbox.json` (ADR 0017). Escritura atómica
//! (archivo temporal + rename) y permisos `0600`: el texto de los mensajes solo vive aquí.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::domain::{
    AgentId, MailSnapshot, Message, MessageId, MessageState, Thread, ThreadId, ThreadState,
    Timestamp,
};
use crate::ports::MailStore;

#[derive(Serialize, Deserialize)]
struct MessageRow {
    id: u64,
    thread: u64,
    from: String,
    from_alias: String,
    to: String,
    to_alias: String,
    text: String,
    sent_at: u64,
    /// `pending`, `queued` o `held`.
    state: String,
}

#[derive(Serialize, Deserialize)]
struct ThreadRow {
    id: u64,
    subject: String,
    hops: u32,
    limit: u32,
    /// `open` o `paused`.
    state: String,
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    next_message: u64,
    next_thread: u64,
    messages: Vec<MessageRow>,
    threads: Vec<ThreadRow>,
}

fn state_name(s: MessageState) -> Option<&'static str> {
    match s {
        MessageState::Pending => Some("pending"),
        MessageState::Queued => Some("queued"),
        MessageState::Held => Some("held"),
        MessageState::Delivered | MessageState::Discarded => None,
    }
}

fn state_from(s: &str) -> Option<MessageState> {
    match s {
        "pending" => Some(MessageState::Pending),
        "queued" => Some(MessageState::Queued),
        "held" => Some(MessageState::Held),
        _ => None,
    }
}

fn row_to_message(r: MessageRow) -> Option<Message> {
    Some(Message {
        id: MessageId(r.id),
        thread: ThreadId(r.thread),
        from: r.from.parse::<AgentId>().ok()?,
        from_alias: r.from_alias,
        to: r.to.parse::<AgentId>().ok()?,
        to_alias: r.to_alias,
        text: r.text,
        sent_at: Timestamp(r.sent_at),
        state: state_from(&r.state)?,
    })
}

fn row_to_thread(r: ThreadRow) -> Option<Thread> {
    Some(Thread {
        id: ThreadId(r.id),
        subject: r.subject,
        hops: r.hops,
        limit: r.limit,
        state: match r.state.as_str() {
            "open" => ThreadState::Open,
            "paused" => ThreadState::Paused,
            _ => return None,
        },
    })
}

pub struct FileMailStore {
    path: PathBuf,
}

impl FileMailStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Lo guardado; archivo ausente o ilegible = buzón vacío.
    pub fn load(&self) -> MailSnapshot {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return MailSnapshot::default();
        };
        let Ok(file) = serde_json::from_str::<File>(&text) else {
            return MailSnapshot::default();
        };
        MailSnapshot {
            next_message: file.next_message,
            next_thread: file.next_thread,
            messages: file
                .messages
                .into_iter()
                .filter_map(row_to_message)
                .collect(),
            threads: file.threads.into_iter().filter_map(row_to_thread).collect(),
        }
    }

    fn write(&self, json: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(json)?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.path)
    }
}

impl MailStore for FileMailStore {
    fn save(&mut self, snapshot: &MailSnapshot) {
        let file = File {
            next_message: snapshot.next_message,
            next_thread: snapshot.next_thread,
            messages: snapshot
                .messages
                .iter()
                .filter_map(|m| {
                    Some(MessageRow {
                        id: m.id.0,
                        thread: m.thread.0,
                        from: m.from.to_string(),
                        from_alias: m.from_alias.clone(),
                        to: m.to.to_string(),
                        to_alias: m.to_alias.clone(),
                        text: m.text.clone(),
                        sent_at: m.sent_at.0,
                        state: state_name(m.state)?.to_owned(),
                    })
                })
                .collect(),
            threads: snapshot
                .threads
                .iter()
                .filter_map(|t| {
                    Some(ThreadRow {
                        id: t.id.0,
                        subject: t.subject.clone(),
                        hops: t.hops,
                        limit: t.limit,
                        state: match t.state {
                            ThreadState::Open => "open",
                            ThreadState::Paused => "paused",
                            ThreadState::Closed => return None,
                        }
                        .to_owned(),
                    })
                })
                .collect(),
        };
        let result = serde_json::to_vec_pretty(&file)
            .map_err(std::io::Error::other)
            .and_then(|json| self.write(&json));
        if let Err(e) = result {
            warn!(event = "mail.save", error = %e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Pid, ProcessStartTime, Provider};
    use std::os::unix::fs::PermissionsExt;

    fn id(pid: u32) -> AgentId {
        AgentId {
            provider: Provider::Claude,
            pid: Pid(pid),
            start_time: ProcessStartTime(7),
        }
    }

    #[test]
    fn guarda_atomico_con_permisos_0600_y_carga_lo_mismo() {
        let dir = std::env::temp_dir().join(format!("kiti-mail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mailbox.json");
        let snap = MailSnapshot {
            next_message: 5,
            next_thread: 2,
            messages: vec![Message {
                id: MessageId(4),
                thread: ThreadId(2),
                from: id(1),
                from_alias: "claude-1".into(),
                to: id(2),
                to_alias: "claude-2".into(),
                text: "no borres las ramas".into(),
                sent_at: Timestamp(9),
                state: MessageState::Queued,
            }],
            threads: vec![Thread {
                id: ThreadId(2),
                subject: "ramas".into(),
                hops: 1,
                limit: 12,
                state: ThreadState::Paused,
            }],
        };
        let mut store = FileMailStore::new(&path);
        assert_eq!(store.load(), MailSnapshot::default());
        store.save(&snap);
        assert_eq!(store.load(), snap);
        assert!(!path.with_extension("json.tmp").exists());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::write(&path, "no es json").unwrap();
        assert_eq!(store.load(), MailSnapshot::default());
        std::fs::remove_dir_all(dir).ok();
    }
}
