use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use super::blocking;
use crate::dto::{self, MailboxDraft, MailboxPatch};
use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use crate::work::{ai, import, scan};
use ausgeloggt_core::model::SsoProvider;

#[tauri::command]
pub async fn mail_presets() -> IpcResult<Vec<Value>> {
    Ok(dto::mail_presets())
}

#[tauri::command]
pub async fn mailbox_add(
    state: State<'_, AppState>,
    address: String,
    imap_host: String,
    imap_port: u16,
    username: String,
    password: String,
    folders: Vec<String>,
) -> IpcResult<Uuid> {
    let draft = MailboxDraft {
        address,
        imap_host,
        imap_port,
        username,
        password,
        folders,
    };
    draft.validate()?;
    let cfg = draft.into_config();
    let id = cfg.id;
    state.try_mutate(|d| {
        if d.mailboxes
            .iter()
            .any(|m| m.address.eq_ignore_ascii_case(&cfg.address))
        {
            return Err(IpcError::invalid(
                "Dieses Postfach ist bereits eingerichtet",
            ));
        }
        d.mailboxes.push(cfg);
        Ok(())
    })?;
    Ok(id)
}

#[tauri::command]
pub async fn mailbox_update(
    state: State<'_, AppState>,
    id: Uuid,
    patch: MailboxPatch,
) -> IpcResult<()> {
    state.try_mutate(|d| {
        let mb = d
            .mailboxes
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or_else(|| IpcError::not_found("Postfach"))?;
        patch.apply(mb);
        Ok(())
    })
}

#[tauri::command]
pub async fn mailbox_remove(state: State<'_, AppState>, id: Uuid) -> IpcResult<()> {
    state.try_mutate(|d| {
        let before = d.mailboxes.len();
        d.mailboxes.retain(|m| m.id != id);
        if d.mailboxes.len() == before {
            return Err(IpcError::not_found("Postfach"));
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn mailbox_test(
    state: State<'_, AppState>,
    id: Option<Uuid>,
    draft: Option<MailboxDraft>,
) -> IpcResult<Vec<String>> {
    let cfg = match (id, draft) {
        (Some(id), _) => state
            .read(|d| d.mailboxes.iter().find(|m| m.id == id).cloned())?
            .ok_or_else(|| IpcError::not_found("Postfach"))?,
        (None, Some(draft)) => {
            draft.validate()?;
            draft.into_config()
        }
        (None, None) => return Err(IpcError::invalid("Postfach oder Entwurf erforderlich")),
    };
    blocking(move || Ok(ausgeloggt_mail::test_connection(&cfg)?)).await
}

#[tauri::command]
pub async fn scan_start(
    state: State<'_, AppState>,
    mailbox_ids: Vec<Uuid>,
    full: bool,
) -> IpcResult<String> {
    scan::start_scan(&state, mailbox_ids, full, scan::imap_scanner())
}

// ---- imports ----------------------------------------------------------------------

#[tauri::command]
pub async fn import_detect(path: String) -> IpcResult<Value> {
    blocking(move || import::detect(&PathBuf::from(path))).await
}

#[tauri::command]
pub async fn import_start(
    state: State<'_, AppState>,
    paths: Vec<String>,
    check_pwned: bool,
) -> IpcResult<String> {
    import::start_import(
        &state,
        paths.into_iter().map(PathBuf::from).collect(),
        check_pwned,
    )
}

#[tauri::command]
pub async fn screenshot_read(state: State<'_, AppState>, paths: Vec<String>) -> IpcResult<String> {
    ai::start_screenshot_read(
        &state,
        paths.into_iter().map(PathBuf::from).collect(),
        ai::real_vision(),
    )
}

#[tauri::command]
pub async fn screenshot_commit(
    state: State<'_, AppState>,
    provider: Option<SsoProvider>,
    entries: Vec<ai::ScreenshotEntryDto>,
) -> IpcResult<Value> {
    let (new_accounts, updated) = ai::commit_screenshot(&state, provider, entries)?;
    Ok(json!({ "new_accounts": new_accounts, "updated": updated }))
}
