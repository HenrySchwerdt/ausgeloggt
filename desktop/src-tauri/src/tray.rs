//! System tray: Öffnen, Jetzt scannen, Sperren, Beenden. Not unit-tested (needs a window
//! system); the decisions behind it (`AppState::hint`, `scan::start_scan`) are.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::work::scan;

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn build(app: &tauri::App, state: AppState) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Öffnen", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "check", "Jetzt scannen", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Sperren", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &check, &lock, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("ausgeloggt")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "check" => {
                // Only rescans all mailboxes (empty list = all). Never sends anything.
                if let Err(e) = scan::start_scan(&state, Vec::new(), false, scan::imap_scanner()) {
                    state.host.notify("ausgeloggt", &e.message);
                }
            }
            "lock" => state.lock(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}
