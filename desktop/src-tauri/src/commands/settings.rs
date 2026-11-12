use chrono::Utc;
use serde_json::Value;
use tauri::State;

use super::blocking;
use crate::dto;
use crate::error::{IpcError, IpcResult};
use crate::letters;
use crate::state::AppState;
use crate::work::{ai, breach};

#[tauri::command]
pub async fn settings_update(state: State<'_, AppState>, patch: Value) -> IpcResult<()> {
    state.try_mutate(|d| {
        d.settings = dto::apply_settings_patch(&d.settings, &patch)?;
        Ok(())
    })
}

#[tauri::command]
pub async fn secret_set(state: State<'_, AppState>, kind: String, value: String) -> IpcResult<()> {
    let value = value.trim().to_string();
    if value.len() > 512 || value.chars().any(char::is_control) {
        return Err(IpcError::invalid("Ungültiger Schlüssel"));
    }
    state.try_mutate(|d| {
        match kind.as_str() {
            "anthropic" => d.settings.ai.anthropic_api_key = value,
            "hibp" => d.settings.hibp_api_key = value,
            _ => return Err(IpcError::invalid("Unbekannte Art von Schlüssel")),
        }
        Ok(())
    })
}

#[tauri::command]
pub async fn ai_test(state: State<'_, AppState>) -> IpcResult<String> {
    let settings = state.read(|d| d.settings.ai.clone())?;
    blocking(move || Ok(ausgeloggt_ai::test_provider(&settings)?)).await
}

#[tauri::command]
pub async fn ai_models(state: State<'_, AppState>) -> IpcResult<Value> {
    let settings = state.read(|d| d.settings.ai.clone())?;
    blocking(move || ai::list_models(&settings)).await
}

#[tauri::command]
pub async fn catalog_refresh_start(state: State<'_, AppState>) -> IpcResult<String> {
    breach::start_catalog_refresh(&state)
}

#[tauri::command]
pub async fn generate_pdf_kit(
    state: State<'_, AppState>,
    kind: String,
    path: String,
) -> IpcResult<()> {
    // Work on a copy so the vault lock is not held while rendering.
    let data = state.read(|d| d.clone())?;
    blocking(move || {
        let pdf = letters::pdf_kit(&data, &kind, Utc::now())?;
        letters::write_output(std::path::Path::new(&path), &pdf)
    })
    .await
}
