//! Adapters de los ports (ADR 0002 §5). Los que traen dependencias (sysinfo, y más adelante
//! axum y notify) van detrás de *features*.

mod clock;
#[cfg(feature = "killer")]
mod killer;
#[cfg(any(feature = "scanner", feature = "killer"))]
mod process_read;
#[cfg(feature = "scanner")]
mod scanner;

pub use clock::SystemClock;
#[cfg(feature = "killer")]
pub use killer::UnixKiller;
#[cfg(any(feature = "scanner", feature = "killer"))]
pub use process_read::read_one as read_process;
#[cfg(feature = "scanner")]
pub use scanner::SysinfoScanner;
