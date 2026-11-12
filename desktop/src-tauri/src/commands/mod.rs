//! Tauri commands (docs/IPC.md). Thin wrappers: argument decoding, `spawn_blocking`
//! for anything slow, then a call into the plain functions that hold the logic.
//! All commands are `async` so they never run on the main thread.

pub mod data;
pub mod jobs;
pub mod letters;
pub mod mail;
pub mod newsletters;
pub mod security;
pub mod send;
pub mod settings;
pub mod vault;

use crate::error::{IpcError, IpcResult};

/// Run blocking work (Argon2, network, file I/O) off the async executor threads.
pub async fn blocking<T, F>(f: F) -> IpcResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> IpcResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| IpcError::internal("Hintergrundaufgabe abgebrochen"))?
}

pub fn path_arg(p: Option<String>) -> Option<std::path::PathBuf> {
    p.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(Into::into)
}
