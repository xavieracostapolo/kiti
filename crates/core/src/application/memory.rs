//! Memoria de proyecto y de sesión (ADR 0018): proyección a la UI y respuestas para `kiti-mcp`.

use crate::application::mail::project_name;
use crate::application::projection::project_status_kind;
use crate::domain::{Agent, MemoryBook, MemoryEntry, MemoryKind, RememberError, TaskStatus};
use kiti_protocol as proto;
use kiti_protocol::mcp;

pub fn project_memory_kind(k: MemoryKind) -> proto::MemoryKind {
    match k {
        MemoryKind::Decision => proto::MemoryKind::Decision,
        MemoryKind::Gotcha => proto::MemoryKind::Gotcha,
        MemoryKind::Handoff => proto::MemoryKind::Handoff,
        MemoryKind::Note => proto::MemoryKind::Note,
    }
}

pub fn project_memory_item(e: &MemoryEntry) -> proto::MemoryItem {
    proto::MemoryItem {
        id: e.id.0,
        project: e.project.clone(),
        kind: project_memory_kind(e.kind),
        title: e.title.clone(),
        detail: e.detail.clone(),
        files: e.files.clone(),
        author: e.author.clone(),
        created_at: e.created_at.0,
    }
}

pub fn memory_msg(book: &MemoryBook) -> proto::ServerMsg {
    proto::ServerMsg::Memory {
        memories: book.all().map(project_memory_item).collect(),
    }
}

pub fn remember_error(e: &RememberError) -> mcp::ErrorReply {
    mcp::ErrorReply {
        code: e.code().to_owned(),
        message: e.to_string(),
    }
}

pub fn recall_session(agent: &Agent, alias: &str) -> mcp::RecallSession {
    let status = match project_status_kind(agent.status().kind) {
        proto::StatusKind::Working => "working",
        proto::StatusKind::Waiting => "waiting",
        proto::StatusKind::Idle => "idle",
        proto::StatusKind::Error => "error",
        proto::StatusKind::Dead => "dead",
    };
    let pending_tasks: Vec<String> = agent
        .tasks()
        .iter()
        .filter(|t| matches!(t.status, TaskStatus::InProgress | TaskStatus::Pending))
        .map(|t| {
            let prefix = match t.status {
                TaskStatus::InProgress => "[en curso] ",
                TaskStatus::Pending => "[pendiente] ",
                TaskStatus::Completed => "",
            };
            format!("{prefix}{}", t.text)
        })
        .collect();
    mcp::RecallSession {
        alias: alias.to_owned(),
        provider: agent.id().provider.to_string(),
        status: status.to_owned(),
        title: agent.title().map(str::to_owned),
        initial_prompt: agent.initial_prompt().map(str::to_owned),
        last_prompt: agent.last_prompt().map(str::to_owned),
        touched_files: agent.touched_files().iter().cloned().collect(),
        pending_tasks,
    }
}

/// Si `include_detail` es `false` (modo pulso sin `query`), solo viaja el índice de títulos para
/// no consumir ventana de contexto del agente.
pub fn recall_entry(e: &MemoryEntry, include_detail: bool) -> mcp::RecallEntry {
    mcp::RecallEntry {
        id: e.id.0,
        kind: e.kind.as_str().to_owned(),
        title: e.title.clone(),
        detail: include_detail.then(|| e.detail.clone()),
        files: e.files.clone(),
        author: e.author.clone(),
        created_at: e.created_at.0,
    }
}

pub fn remember_reply(e: &MemoryEntry) -> mcp::RememberReply {
    mcp::RememberReply {
        id: e.id.0,
        project: project_name(&e.project),
        kind: e.kind.as_str().to_owned(),
        title: e.title.clone(),
    }
}
