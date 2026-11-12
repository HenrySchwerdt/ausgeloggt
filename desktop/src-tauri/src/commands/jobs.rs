use serde_json::Value;
use tauri::State;

use crate::error::IpcResult;
use crate::state::AppState;

#[tauri::command]
pub async fn job_cancel(state: State<'_, AppState>, job_id: String) -> IpcResult<()> {
    state.cancel_job(&job_id)
}

#[tauri::command]
pub async fn jobs_running(state: State<'_, AppState>) -> IpcResult<Vec<Value>> {
    Ok(state.jobs_running())
}
