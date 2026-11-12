//! Abstraction over the Tauri `AppHandle` (events + notifications) so that all logic
//! can be unit-tested without a webview.

use serde_json::Value;

pub trait Host: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
    fn notify(&self, title: &str, body: &str);
}

/// Production host: Tauri events and the notification plugin. Created before the
/// Tauri builder (so the state can be managed up front); the handle is attached in `setup`.
#[derive(Default)]
pub struct TauriHost {
    handle: std::sync::OnceLock<tauri::AppHandle>,
}

impl TauriHost {
    pub fn attach(&self, handle: tauri::AppHandle) {
        let _ = self.handle.set(handle);
    }
}

impl Host for TauriHost {
    fn emit(&self, event: &str, payload: Value) {
        use tauri::Emitter;
        let Some(h) = self.handle.get() else { return };
        if let Err(e) = h.emit(event, payload) {
            log::debug!("Event {event} nicht gesendet: {e}");
        }
    }

    fn notify(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        let Some(h) = self.handle.get() else { return };
        if let Err(e) = h.notification().builder().title(title).body(body).show() {
            log::debug!("Benachrichtigung nicht angezeigt: {e}");
        }
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};

    /// Records events and notifications; lets tests wait for an event.
    #[derive(Default)]
    pub struct MemHost {
        pub events: Mutex<Vec<(String, Value)>>,
        pub notes: Mutex<Vec<(String, String)>>,
        cv: Condvar,
    }

    impl Host for MemHost {
        fn emit(&self, event: &str, payload: Value) {
            self.events.lock().unwrap().push((event.into(), payload));
            self.cv.notify_all();
        }
        fn notify(&self, title: &str, body: &str) {
            self.notes.lock().unwrap().push((title.into(), body.into()));
        }
    }

    impl MemHost {
        pub fn named(&self, name: &str) -> Vec<Value> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
                .collect()
        }

        /// Wait until an event with this name exists (returns the first match).
        pub fn wait_for(&self, name: &str, secs: u64) -> Option<Value> {
            let end = Instant::now() + Duration::from_secs(secs);
            let mut g = self.events.lock().unwrap();
            loop {
                if let Some((_, v)) = g.iter().find(|(n, _)| n == name) {
                    return Some(v.clone());
                }
                let left = end.checked_duration_since(Instant::now())?;
                g = self.cv.wait_timeout(g, left).unwrap().0;
            }
        }
    }
}
