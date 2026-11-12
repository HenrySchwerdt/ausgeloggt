use std::path::PathBuf;

use serde_json::Value;
use tauri::State;

use super::{blocking, path_arg};
use crate::error::IpcResult;
use crate::state::AppState;

#[tauri::command]
pub async fn vault_status(state: State<'_, AppState>) -> IpcResult<Value> {
    Ok(state.status())
}

#[tauri::command]
pub async fn vault_create(
    state: State<'_, AppState>,
    password: String,
    path: Option<String>,
) -> IpcResult<()> {
    let st = state.inner().clone();
    blocking(move || st.create(&password, path_arg(path))).await
}

#[tauri::command]
pub async fn vault_unlock(
    state: State<'_, AppState>,
    password: String,
    path: Option<String>,
) -> IpcResult<()> {
    let st = state.inner().clone();
    blocking(move || st.unlock(&password, path_arg(path))).await
}

#[tauri::command]
pub async fn vault_lock(state: State<'_, AppState>) -> IpcResult<()> {
    let st = state.inner().clone();
    // Saving encrypts the whole vault: keep it off the executor thread.
    blocking(move || {
        st.lock();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn vault_touch(state: State<'_, AppState>) -> IpcResult<()> {
    state.touch();
    Ok(())
}

#[tauri::command]
pub async fn vault_change_password(
    state: State<'_, AppState>,
    current: String,
    next: String,
) -> IpcResult<()> {
    let st = state.inner().clone();
    blocking(move || st.change_password(&current, &next)).await
}

#[tauri::command]
pub async fn vault_export_json(state: State<'_, AppState>, path: String) -> IpcResult<()> {
    let st = state.inner().clone();
    blocking(move || {
        let path = PathBuf::from(path);
        st.with_vault(|v| v.export_json(&path))??;
        Ok(())
    })
    .await
}
