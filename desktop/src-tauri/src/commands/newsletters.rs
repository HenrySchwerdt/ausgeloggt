use chrono::Utc;
use tauri::State;
use uuid::Uuid;

use crate::error::IpcResult;
use crate::ops;
use crate::state::AppState;
use crate::work::{ai, unsub};
use ausgeloggt_core::model::{MailCategory, SubscriptionStatus};

#[tauri::command]
pub async fn subscription_set_status(
    state: State<'_, AppState>,
    ids: Vec<Uuid>,
    status: SubscriptionStatus,
) -> IpcResult<()> {
    state.try_mutate(|d| ops::set_subscription_status(d, &ids, status, Utc::now()))
}

#[tauri::command]
pub async fn subscription_set_category(
    state: State<'_, AppState>,
    ids: Vec<Uuid>,
    category: MailCategory,
) -> IpcResult<()> {
    state.try_mutate(|d| ops::set_subscription_category(d, &ids, category))
}

#[tauri::command]
pub async fn unsubscribe_start(state: State<'_, AppState>, ids: Vec<Uuid>) -> IpcResult<String> {
    unsub::start_unsubscribe(&state, ids, unsub::real_unsubscriber())
}

#[tauri::command]
pub async fn unsubscribe_manual_links(
    state: State<'_, AppState>,
    subscription_id: Uuid,
    from_mail: Option<bool>,
) -> IpcResult<serde_json::Value> {
    let st = state.inner().clone();
    super::blocking(move || {
        unsub::manual_links(
            &st,
            subscription_id,
            from_mail.unwrap_or(false),
            unsub::real_link_fetcher(),
        )
    })
    .await
}

#[tauri::command]
pub async fn ai_classify_start(state: State<'_, AppState>) -> IpcResult<String> {
    ai::start_classify(&state, ai::real_classifier())
}
