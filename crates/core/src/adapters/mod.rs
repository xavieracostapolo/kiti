//! Adapters de los ports (ADR 0002 §5). Los que traen dependencias (sysinfo, y más adelante
//! axum y notify) van detrás de *features*.

pub mod alias_file;
#[cfg(target_os = "macos")]
pub mod antigravity;
pub mod claude_settings;
pub mod cli_hooks;
mod clock;
pub mod conversations;
#[cfg(target_os = "macos")]
pub mod focus;
pub mod git_status;
mod hook_config;
#[cfg(feature = "hooks")]
pub mod hooks;
#[cfg(feature = "killer")]
mod killer;
pub mod mail_file;
pub mod memory_sqlite;
#[cfg(any(feature = "scanner", feature = "killer"))]
mod process_read;
mod projects;
#[cfg(feature = "pty")]
pub mod pty;
#[cfg(feature = "scanner")]
mod scanner;
pub mod statusline;
pub mod tail;
#[cfg(feature = "pty")]
pub mod tmux;
pub mod worktree;

pub use clock::SystemClock;
#[cfg(feature = "killer")]
pub use killer::UnixKiller;
#[cfg(any(feature = "scanner", feature = "killer"))]
pub use process_read::read_one as read_process;
pub use projects::GitProjects;
#[cfg(feature = "scanner")]
pub use scanner::SysinfoScanner;
