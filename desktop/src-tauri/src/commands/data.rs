use chrono::Utc;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use super::blocking;
use crate::dto;
use crate::error::IpcResult;
use crate::logos;
use crate::ops;
use crate::state::AppState;
use ausgeloggt_core::model::{AccountStatus, DeletionMethod};

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> IpcResult<Value> {
    state.read(dto::snapshot)?
}

#[tauri::command]
pub async fn get_insights(state: State<'_, AppState>) -> IpcResult<Value> {
    state.read(|d| dto::insights_dto(d, Utc::now()))?
}

#[tauri::command]
pub async fn catalog_info(
    state: State<'_, AppState>,
    domain: Option<String>,
    name: Option<String>,
) -> IpcResult<Value> {
    dto::catalog_info(&state.catalog(), domain.as_deref(), name.as_deref())
}

#[tauri::command]
pub async fn reference_data() -> IpcResult<Value> {
    dto::reference_data()
}

#[tauri::command]
pub async fn logo_get(state: State<'_, AppState>, domain: String) -> IpcResult<Option<String>> {
    let st = state.inner().clone();
    blocking(move || logos::logo_get(&st, &domain, &logos::fetch_from_site, Utc::now())).await
}

// ---- accounts ---------------------------------------------------------------------

#[tauri::command]
pub async fn account_set_status(
    state: State<'_, AppState>,
    ids: Vec<Uuid>,
    status: AccountStatus,
) -> IpcResult<()> {
    state.try_mutate(|d| ops::set_account_status(d, &ids, status))
}

#[tauri::command]
pub async fn account_set_notes(
    state: State<'_, AppState>,
    id: Uuid,
    notes: String,
) -> IpcResult<()> {
    state.try_mutate(|d| ops::set_account_notes(d, id, notes))
}

#[tauri::command]
pub async fn account_add(
    state: State<'_, AppState>,
    domain: String,
    name: Option<String>,
    identifier: Option<String>,
) -> IpcResult<Uuid> {
    state.try_mutate(|d| {
        ops::add_account(
            d,
            &domain,
            name.as_deref(),
            identifier.as_deref(),
            Utc::now(),
        )
    })
}

#[tauri::command]
pub async fn account_remove(state: State<'_, AppState>, ids: Vec<Uuid>) -> IpcResult<()> {
    state.mutate(|d| ops::remove_accounts(d, &ids))
}

#[tauri::command]
pub async fn account_add_deletion_step(
    state: State<'_, AppState>,
    id: Uuid,
    method: DeletionMethod,
    note: String,
) -> IpcResult<()> {
    state.try_mutate(|d| ops::add_deletion_step(d, id, method, note, Utc::now()))
}
