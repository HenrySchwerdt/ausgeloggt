//! ausgeloggt desktop shell: holds the unlocked vault and exposes the `ausgeloggt-*`
//! crates as typed IPC commands (contract: docs/IPC.md).
//!
//! Layout: `state` (vault, debounced save, auto-lock), `jobs` (background work with
//! progress events), `work::*` (job logic), `letters` / `ops` / `dto` / `logos`
//! (pure logic), `schedule` (auto-lock tick, auto-rescan, notifications) and
//! `commands::*` (thin Tauri wrappers). Everything except `commands` and `host::TauriHost`
//! is unit-tested without a webview.

// Tauri commands mirror the flat argument lists of docs/IPC.md.
#![allow(clippy::too_many_arguments)]

pub mod commands;
pub mod dto;
pub mod error;
pub mod host;
pub mod jobs;
pub mod letters;
pub mod logos;
pub mod ops;
pub mod schedule;
pub mod state;
pub mod work;

use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::host::TauriHost;
use crate::state::AppState;

pub fn run() {
    let host = Arc::new(TauriHost::default());
    let state = AppState::production(host.clone());

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state.clone())
        .setup({
            let state = state.clone();
            move |app| {
                host.attach(app.handle().clone());
                schedule::spawn(state.clone());
                Ok(())
            }
        })
        .on_window_event(|window, event| {
            // Never lose a pending change when the window goes away.
            if let WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed = event {
                window.app_handle().state::<AppState>().flush();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_status,
            commands::vault::vault_create,
            commands::vault::vault_unlock,
            commands::vault::vault_lock,
            commands::vault::vault_touch,
            commands::vault::vault_change_password,
            commands::vault::vault_export_json,
            commands::data::get_snapshot,
            commands::data::get_insights,
            commands::data::catalog_info,
            commands::data::reference_data,
            commands::data::logo_get,
            commands::data::account_set_status,
            commands::data::account_set_notes,
            commands::data::account_add,
            commands::data::account_remove,
            commands::data::account_add_deletion_step,
            commands::mail::mail_presets,
            commands::mail::mailbox_add,
            commands::mail::mailbox_update,
            commands::mail::mailbox_remove,
            commands::mail::mailbox_test,
            commands::mail::scan_start,
            commands::mail::import_detect,
            commands::mail::import_start,
            commands::mail::screenshot_read,
            commands::mail::screenshot_commit,
            commands::newsletters::subscription_set_status,
            commands::newsletters::subscription_set_category,
            commands::newsletters::unsubscribe_start,
            commands::newsletters::ai_classify_start,
            commands::security::breach_check_start,
            commands::letters::letter_preview,
            commands::letters::letter_save_pdf,
            commands::letters::request_create,
            commands::letters::request_update,
            commands::letters::request_remove,
            commands::letters::mailto_url,
            commands::settings::settings_update,
            commands::settings::secret_set,
            commands::settings::ai_test,
            commands::settings::ai_models,
            commands::settings::catalog_refresh_start,
            commands::settings::generate_pdf_kit,
            commands::jobs::job_cancel,
            commands::jobs::jobs_running,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ausgeloggt");

    app.run(move |_app, event| {
        if let RunEvent::Exit | RunEvent::ExitRequested { .. } = event {
            state.shutdown();
        }
    });
}
