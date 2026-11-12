use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use super::blocking;
use crate::error::{IpcError, IpcResult};
use crate::letters::{self, LetterArgs, LetterKind, NewRequest, PartyDto, RequestPatch};
use crate::state::AppState;
use ausgeloggt_catalog::gdpr;
use ausgeloggt_core::model::{RequestChannel, RequestKind, RequestStatus};

fn args(
    kind: LetterKind,
    ids: (Option<Uuid>, Option<Uuid>, Option<Uuid>),
    org_id: Option<String>,
    authority_state: Option<String>,
    recipient: Option<PartyDto>,
) -> LetterArgs {
    LetterArgs {
        kind,
        account_id: ids.0,
        subscription_id: ids.1,
        request_id: ids.2,
        org_id,
        authority_state,
        recipient,
    }
}

#[tauri::command]
pub async fn letter_preview(
    state: State<'_, AppState>,
    kind: LetterKind,
    account_id: Option<Uuid>,
    subscription_id: Option<Uuid>,
    request_id: Option<Uuid>,
    org_id: Option<String>,
    authority_state: Option<String>,
    recipient: Option<PartyDto>,
) -> IpcResult<Value> {
    let a = args(
        kind,
        (account_id, subscription_id, request_id),
        org_id,
        authority_state,
        recipient,
    );
    let catalog = state.catalog();
    let built = state.read(|d| letters::build_letter(d, &catalog, &a, Utc::now()))??;
    Ok(built.preview())
}

#[tauri::command]
pub async fn letter_save_pdf(
    state: State<'_, AppState>,
    kind: LetterKind,
    account_id: Option<Uuid>,
    subscription_id: Option<Uuid>,
    request_id: Option<Uuid>,
    org_id: Option<String>,
    authority_state: Option<String>,
    recipient: Option<PartyDto>,
    path: String,
) -> IpcResult<()> {
    let a = args(
        kind,
        (account_id, subscription_id, request_id),
        org_id,
        authority_state,
        recipient,
    );
    let catalog = state.catalog();
    let built = state.read(|d| letters::build_letter(d, &catalog, &a, Utc::now()))??;
    built.ensure_complete()?;
    blocking(move || {
        let pdf = ausgeloggt_letters::render_pdf(&built.doc)?;
        letters::write_output(&PathBuf::from(path), &pdf)
    })
    .await
}

#[tauri::command]
pub async fn request_create(
    state: State<'_, AppState>,
    kind: RequestKind,
    target: String,
    domain: Option<String>,
    account_id: Option<Uuid>,
    subscription_id: Option<Uuid>,
    parent_id: Option<Uuid>,
    channel: RequestChannel,
    recipient: String,
    subject: String,
    body: String,
    sent: bool,
) -> IpcResult<Uuid> {
    let req = NewRequest {
        kind,
        target,
        domain,
        account_id,
        subscription_id,
        parent_id,
        channel,
        recipient,
        subject,
        body,
        sent,
    };
    state.try_mutate(|d| letters::create_request(d, req, Utc::now()))
}

#[tauri::command]
pub async fn request_update(
    state: State<'_, AppState>,
    id: Uuid,
    status: Option<RequestStatus>,
    notes: Option<String>,
    sent_at: Option<DateTime<Utc>>,
) -> IpcResult<()> {
    let patch = RequestPatch {
        id,
        status,
        notes,
        sent_at,
    };
    state.try_mutate(|d| letters::update_request(d, patch))
}

#[tauri::command]
pub async fn request_remove(state: State<'_, AppState>, id: Uuid) -> IpcResult<()> {
    state.try_mutate(|d| {
        let before = d.requests.len();
        d.requests.retain(|r| r.id != id);
        if d.requests.len() == before {
            return Err(IpcError::not_found("Anfrage"));
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn mailto_url(to: Option<String>, subject: String, body: String) -> IpcResult<String> {
    Ok(gdpr::mailto_url(&gdpr::Letter { to, subject, body }))
}
