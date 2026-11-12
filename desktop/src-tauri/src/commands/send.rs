//! Sending letters by mail, the send queue and automations (docs/IPC.md, "Sending and
//! automation").

use chrono::Utc;
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::automation;
use crate::error::{IpcError, IpcResult};
use crate::letters::{LetterArgs, LetterKind, PartyDto};
use crate::state::AppState;
use crate::work::scan;
use crate::work::send::{self, BulkItem};

#[tauri::command]
pub async fn letter_send(
    state: State<'_, AppState>,
    kind: LetterKind,
    account_id: Option<Uuid>,
    subscription_id: Option<Uuid>,
    request_id: Option<Uuid>,
    org_id: Option<String>,
    authority_state: Option<String>,
    recipient: Option<PartyDto>,
    mailbox_id: Uuid,
    attach_pdf: bool,
) -> IpcResult<String> {
    let args = LetterArgs {
        kind,
        account_id,
        subscription_id,
        request_id,
        org_id,
        authority_state,
        recipient,
    };
    send::start_send(&state, args, mailbox_id, attach_pdf, send::real_sender())
}

#[tauri::command]
pub async fn letters_send_bulk(
    state: State<'_, AppState>,
    items: Vec<BulkItem>,
    mailbox_id: Uuid,
    attach_pdf: bool,
) -> IpcResult<String> {
    send::start_bulk(&state, items, mailbox_id, attach_pdf, send::real_sender())
}

/// Dry run of `letters_send_bulk` (same arguments, nothing is sent).
#[tauri::command]
pub async fn letters_send_check(
    state: State<'_, AppState>,
    items: Vec<BulkItem>,
    mailbox_id: Uuid,
    attach_pdf: Option<bool>,
) -> IpcResult<Value> {
    let _ = attach_pdf;
    let catalog = state.catalog();
    state.read(|d| send::check(d, &catalog, &items, mailbox_id, Utc::now()))
}

#[tauri::command]
pub async fn account_set_queued(
    state: State<'_, AppState>,
    ids: Vec<Uuid>,
    queued: bool,
) -> IpcResult<()> {
    state.try_mutate(|d| automation::set_queued(d, &ids, queued))
}

#[tauri::command]
pub async fn automation_list(state: State<'_, AppState>) -> IpcResult<Value> {
    state.read(|d| json!(d.automations))
}

#[tauri::command]
pub async fn automation_upsert(state: State<'_, AppState>, rule: Value) -> IpcResult<Uuid> {
    state.try_mutate(|d| automation::upsert_rule(d, &rule, Utc::now()))
}

#[tauri::command]
pub async fn automation_remove(state: State<'_, AppState>, id: Uuid) -> IpcResult<()> {
    state.try_mutate(|d| automation::remove_rule(d, id))
}

#[tauri::command]
pub async fn automation_run_now(state: State<'_, AppState>, id: Uuid) -> IpcResult<String> {
    if !state.read(|d| d.automations.iter().any(|r| r.id == id))? {
        return Err(IpcError::not_found("Automatik"));
    }
    automation::start_automation(&state, vec![id], send::real_sender(), scan::imap_scanner())
}

/// Switch "start with the system" on or off and remember it in the settings.
#[tauri::command]
pub async fn autostart_set(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> IpcResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let res = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    res.map_err(|_| IpcError::internal("Der Autostart konnte nicht geändert werden"))?;
    if state.is_unlocked() {
        state.mutate(|d| d.settings.autostart = enabled)?;
    }
    Ok(())
}
