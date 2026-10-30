//! Dominio de Monitoring (ADR 0002 §1–§3). Puro y determinista: sin I/O, sin reloj
//! (el tiempo llega como argumento) y sin aleatoriedad. No depende de `kiti-protocol`.

mod agent;
mod alerts;
mod alias;
mod events;
mod ids;
mod mailbox;
mod memory;
mod process;
mod project;
mod quota;
mod registry;
mod signal;
mod stale;
mod status;
mod task;

pub use agent::{
    Agent, AgentInput, AgentStats, DetectedAgent, Events, HISTORY_MAX, SessionActivity,
};
pub use alerts::{Alert, AlertKind, AlertPolicy};
pub use alias::{AliasBook, AliasEntry, AliasKey, RESERVE_MS as ALIAS_RESERVE_MS};
pub use events::DomainEvent;
pub use ids::{
    AgentId, ParseAgentIdError, Pid, ProcessIdentity, ProcessStartTime, Provider, SessionId,
    Timestamp, ToolName, ToolUseKey, WorkingDir,
};
pub use mailbox::{
    Delivery, HOP_STEP, MAX_TEXT, MailMode, MailSnapshot, Mailbox, Message, MessageId,
    MessageState, RATE_PER_HOUR, Rebound, SendError, Sent, Thread, ThreadId, ThreadState,
};
pub use memory::{
    KEEP_MEMORIES, MAX_DETAIL, MAX_MEMORY_FILES, MAX_TITLE, MemoryBook, MemoryEntry, MemoryId,
    MemoryKind, RememberError, SEARCH_LIMIT, TASKS_MAX, TOUCHED_MAX, TaskItem, TaskStatus,
};
pub use process::{ProcessSnapshot, Role, classify, dedupe_tree, is_candidate_exe};
pub use project::{GitStatus, project_root, worktree_main_repo};
pub use quota::{LOW as QUOTA_LOW, QuotaAlert, QuotaPolicy, QuotaRefresh, QuotaWindow};
pub use registry::{AgentRegistry, RegistryDiff};
pub use signal::{AgentInfo, Signal, ToolOutcome};
pub use stale::StalePolicy;
pub use status::{Status, StatusKind, StatusReason, TrafficLight};
pub use task::{
    KEEP_TASKS, LimitReason, MAX_LAUNCHES_PER_HOUR, MAX_LIVE_TASKS, NewTask, Placement,
    STUCK_AFTER, Task, TaskBook, TaskError, TaskId, TaskRole, TaskState,
};
