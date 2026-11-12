//! Spurlos desktop shell. Phase 1: window + custom titlebar support only.
//! IPC commands wrapping the `spurlos-*` crates are added in a later phase.

// Linked now so the dependency graph is validated; used from phase 2 on.
use spurlos_core as _;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running Spurlos");
}
