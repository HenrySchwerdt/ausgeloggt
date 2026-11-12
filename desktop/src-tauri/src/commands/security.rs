use tauri::State;

use crate::error::IpcResult;
use crate::state::AppState;
use crate::work::breach::{self, BreachApi};

#[tauri::command]
pub async fn breach_check_start(state: State<'_, AppState>) -> IpcResult<String> {
    breach::start_breach_check(&state, BreachApi::real())
}
