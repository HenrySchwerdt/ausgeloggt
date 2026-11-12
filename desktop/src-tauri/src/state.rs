//! `AppState`: the unlocked vault, the debounced saver, auto-lock and shared caches.
//!
//! All state lives behind one cheap-to-clone handle so worker threads can own a copy.
//! Rules: the vault lock is only ever held briefly (never during network I/O), every
//! mutation goes through [`AppState::mutate`] (marks dirty, bumps the revision, emits
//! `vault://changed`) and the file is written by a background thread, debounced.

use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use ausgeloggt_catalog::Catalog;
use ausgeloggt_core::model::VaultData;
use ausgeloggt_core::vault::Vault;
use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};
use crate::host::Host;

/// Minimum length of a vault password.
pub const MIN_PASSWORD_CHARS: usize = 10;
/// Save debounce (mutations within this window are written once).
pub const SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

/// Lock a mutex, ignoring poisoning (a panicked worker must not brick the app).
pub trait LockExt<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn lock_safe(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Default)]
struct SaveFlags {
    dirty: bool,
    stop: bool,
}

pub struct SaveSignal {
    flags: Mutex<SaveFlags>,
    cv: Condvar,
    debounce: Duration,
}

/// What the shell needs to know about automations without opening the vault (it may be
/// locked): updated whenever the data is read or changed while unlocked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AutomationHint {
    /// At least one rule is enabled.
    pub armed: bool,
    /// Desktop notifications are on.
    pub notify: bool,
    /// Closing the window hides to the tray (automations enabled or tray mode chosen).
    pub hide_on_close: bool,
    /// The "paused, vault locked" notice was shown for this lock period.
    pub paused_notified: bool,
}

pub struct JobEntry {
    pub kind: String,
    pub cancel: Arc<AtomicBool>,
    pub done: u64,
    pub total: u64,
}

pub struct Inner {
    pub(crate) host: Arc<dyn Host>,
    vault: Mutex<Option<Vault>>,
    revision: AtomicU64,
    signal: Arc<SaveSignal>,
    pub(crate) jobs: Mutex<HashMap<String, JobEntry>>,
    activity: Mutex<Instant>,
    catalog: Mutex<Option<Arc<Catalog>>>,
    pub(crate) notified: Mutex<HashSet<String>>,
    pub(crate) rescan_attempts: Mutex<HashMap<Uuid, DateTime<Utc>>>,
    pub(crate) automation_hint: Mutex<AutomationHint>,
    custom_path: Mutex<Option<PathBuf>>,
    default_path: PathBuf,
    fast_kdf: bool,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.signal.flags.lock_safe().stop = true;
        self.signal.cv.notify_all();
    }
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

impl AppState {
    pub fn production(host: Arc<dyn Host>) -> Self {
        Self::new(host, Vault::default_path(), SAVE_DEBOUNCE, false)
    }

    pub fn new(
        host: Arc<dyn Host>,
        default_path: PathBuf,
        debounce: Duration,
        fast_kdf: bool,
    ) -> Self {
        let signal = Arc::new(SaveSignal {
            flags: Mutex::new(SaveFlags::default()),
            cv: Condvar::new(),
            debounce,
        });
        let inner = Arc::new(Inner {
            host,
            vault: Mutex::new(None),
            revision: AtomicU64::new(0),
            signal: signal.clone(),
            jobs: Mutex::new(HashMap::new()),
            activity: Mutex::new(Instant::now()),
            catalog: Mutex::new(None),
            notified: Mutex::new(HashSet::new()),
            rescan_attempts: Mutex::new(HashMap::new()),
            automation_hint: Mutex::new(AutomationHint::default()),
            custom_path: Mutex::new(None),
            default_path,
            fast_kdf,
        });
        let weak = Arc::downgrade(&inner);
        let spawned = std::thread::Builder::new()
            .name("vault-saver".into())
            .spawn(move || saver_loop(signal, weak));
        if let Err(e) = spawned {
            log::error!("Speicher-Thread konnte nicht gestartet werden: {e}");
        }
        AppState(inner)
    }

    // ---- events -----------------------------------------------------------------

    pub fn emit(&self, event: &str, payload: serde_json::Value) {
        self.host.emit(event, payload);
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    // ---- vault lifecycle --------------------------------------------------------

    /// Path in use: the custom one (last used) or the platform default.
    pub fn vault_path(&self) -> PathBuf {
        self.custom_path
            .lock_safe()
            .clone()
            .unwrap_or_else(|| self.default_path.clone())
    }

    fn resolve_path(&self, path: Option<PathBuf>) -> PathBuf {
        match path {
            Some(p) if !p.as_os_str().is_empty() => {
                *self.custom_path.lock_safe() = Some(p.clone());
                p
            }
            _ => self.vault_path(),
        }
    }

    pub fn is_unlocked(&self) -> bool {
        self.vault.lock_safe().is_some()
    }

    pub fn status(&self) -> serde_json::Value {
        let path = self.vault_path();
        json!({
            "exists": path.exists(),
            "unlocked": self.is_unlocked(),
            "path": path.to_string_lossy(),
        })
    }

    /// Create a new vault (Argon2 runs here: call off the main thread).
    pub fn create(&self, password: &str, path: Option<PathBuf>) -> IpcResult<()> {
        if password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(IpcError::invalid(format!(
                "Das Passwort muss mindestens {MIN_PASSWORD_CHARS} Zeichen lang sein"
            )));
        }
        if self.is_unlocked() {
            return Err(IpcError::invalid("Ein Tresor ist bereits entsperrt"));
        }
        let path = self.resolve_path(path);
        let vault = if self.fast_kdf {
            Vault::create_for_tests(&path, password)?
        } else {
            Vault::create(&path, password)?
        };
        self.install(vault);
        Ok(())
    }

    /// Open an existing vault (slow by design: call off the main thread).
    pub fn unlock(&self, password: &str, path: Option<PathBuf>) -> IpcResult<()> {
        if self.is_unlocked() {
            return Err(IpcError::invalid("Der Tresor ist bereits entsperrt"));
        }
        let path = self.resolve_path(path);
        let vault = Vault::open(&path, password)?;
        self.install(vault);
        Ok(())
    }

    fn install(&self, vault: Vault) {
        *self.catalog.lock_safe() = None;
        *self.vault.lock_safe() = Some(vault);
        self.automation_hint.lock_safe().paused_notified = false;
        self.refresh_hint();
        self.touch();
        self.signal.flags.lock_safe().dirty = false;
        self.revision.fetch_add(1, Ordering::SeqCst);
        self.emit("vault://changed", json!({ "revision": self.revision() }));
    }

    /// Change the master password. `current` is verified against the file first
    /// (Argon2, slow: call off the main thread); the key is re-derived under the lock.
    /// Sealed caches (logos) become unreadable and are rebuilt on demand.
    pub fn change_password(&self, current: &str, next: &str) -> IpcResult<()> {
        if next.chars().count() < MIN_PASSWORD_CHARS {
            return Err(IpcError::invalid(format!(
                "Das neue Passwort muss mindestens {MIN_PASSWORD_CHARS} Zeichen lang sein"
            )));
        }
        if !self.is_unlocked() {
            return Err(IpcError::locked());
        }
        Vault::open(&self.vault_path(), current)?;
        let mut guard = self.vault.lock_safe();
        let v = guard.as_mut().ok_or_else(IpcError::locked)?;
        v.change_password(next)?;
        drop(guard);
        self.take_dirty();
        Ok(())
    }

    /// Cancel all jobs, write pending changes, drop the vault from memory.
    pub fn lock(&self) {
        self.cancel_all_jobs();
        let mut guard = self.vault.lock_safe();
        if let Some(v) = guard.as_ref() {
            if self.take_dirty() {
                if let Err(e) = v.save() {
                    log::error!("Tresor konnte beim Sperren nicht gespeichert werden: {e}");
                    self.emit("vault://save-failed", json!({ "message": e.to_string() }));
                }
            }
        }
        let was_open = guard.take().is_some();
        drop(guard);
        self.notified.lock_safe().clear();
        if was_open {
            self.emit("vault://locked", json!({}));
        }
    }

    /// Write pending changes now (exit / window close). Keeps the vault open.
    pub fn flush(&self) {
        if !self.take_dirty() {
            return;
        }
        let guard = self.vault.lock_safe();
        if let Some(v) = guard.as_ref() {
            if let Err(e) = v.save() {
                log::error!("Tresor konnte nicht gespeichert werden: {e}");
                drop(guard);
                self.mark_dirty();
                self.emit("vault://save-failed", json!({ "message": e.to_string() }));
            }
        }
    }

    /// Final save on exit; also cancels workers.
    pub fn shutdown(&self) {
        self.cancel_all_jobs();
        self.flush();
    }

    fn take_dirty(&self) -> bool {
        std::mem::take(&mut self.signal.flags.lock_safe().dirty)
    }

    fn mark_dirty(&self) {
        self.signal.flags.lock_safe().dirty = true;
        self.signal.cv.notify_all();
    }

    // ---- access -----------------------------------------------------------------

    pub fn with_vault<R>(&self, f: impl FnOnce(&Vault) -> R) -> IpcResult<R> {
        let guard = self.vault.lock_safe();
        guard.as_ref().map(f).ok_or_else(IpcError::locked)
    }

    pub fn read<R>(&self, f: impl FnOnce(&VaultData) -> R) -> IpcResult<R> {
        self.with_vault(|v| f(&v.data))
    }

    /// Mutate the data. Marks the vault dirty (debounced save), bumps the revision and
    /// emits `vault://changed`.
    pub fn mutate<R>(&self, f: impl FnOnce(&mut VaultData) -> R) -> IpcResult<R> {
        let result = {
            let mut guard = self.vault.lock_safe();
            let v = guard.as_mut().ok_or_else(IpcError::locked)?;
            f(&mut v.data)
        };
        self.after_mutation();
        Ok(result)
    }

    /// Like [`mutate`](Self::mutate) for closures that can fail; an error leaves the
    /// revision untouched (closures must validate before changing anything).
    pub fn try_mutate<R>(&self, f: impl FnOnce(&mut VaultData) -> IpcResult<R>) -> IpcResult<R> {
        let result = {
            let mut guard = self.vault.lock_safe();
            let v = guard.as_mut().ok_or_else(IpcError::locked)?;
            f(&mut v.data)?
        };
        self.after_mutation();
        Ok(result)
    }

    fn after_mutation(&self) {
        self.refresh_hint();
        self.touch();
        self.mark_dirty();
        let rev = self.revision.fetch_add(1, Ordering::SeqCst) + 1;
        self.emit("vault://changed", json!({ "revision": rev }));
    }

    /// Refresh the cached automation / tray flags from the unlocked data.
    pub fn refresh_hint(&self) {
        let Ok((armed, notify, tray)) = self.read(|d| {
            (
                d.automations.iter().any(|a| a.enabled),
                d.settings.notifications,
                d.settings.start_minimized_to_tray,
            )
        }) else {
            return;
        };
        let mut h = self.automation_hint.lock_safe();
        h.armed = armed;
        h.notify = notify;
        h.hide_on_close = armed || tray;
    }

    pub fn hint(&self) -> AutomationHint {
        *self.automation_hint.lock_safe()
    }

    // ---- catalog ----------------------------------------------------------------

    /// The deletion catalog (bundled snapshot overlaid with the cached refresh).
    pub fn catalog(&self) -> Arc<Catalog> {
        if let Some(c) = self.catalog.lock_safe().as_ref() {
            return c.clone();
        }
        let dir = self.cache_dir();
        let loaded = Arc::new(Catalog::load(&dir));
        *self.catalog.lock_safe() = Some(loaded.clone());
        loaded
    }

    pub fn set_catalog(&self, c: Catalog) {
        *self.catalog.lock_safe() = Some(Arc::new(c));
    }

    /// `<vault dir>/cache`.
    pub fn cache_dir(&self) -> PathBuf {
        cache_dir_for(&self.vault_path())
    }

    // ---- auto-lock --------------------------------------------------------------

    pub fn touch(&self) {
        *self.activity.lock_safe() = Instant::now();
    }

    /// Idle time exceeded `settings.auto_lock_minutes` (0 = never) and no job running.
    pub fn auto_lock_due(&self, now: Instant) -> bool {
        let minutes = match self.read(|d| d.settings.auto_lock_minutes) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if minutes == 0 || self.has_running_jobs() {
            return false;
        }
        // Opt in (with a warning in the UI): stay unlocked while automations are enabled.
        let keep = self
            .read(|d| {
                d.settings.keep_unlocked_for_automation && d.automations.iter().any(|a| a.enabled)
            })
            .unwrap_or(false);
        if keep {
            return false;
        }
        let idle = now.saturating_duration_since(*self.activity.lock_safe());
        idle >= Duration::from_secs(u64::from(minutes) * 60)
    }

    /// Lock if the idle timeout elapsed. Returns whether it locked.
    pub fn tick_auto_lock(&self, now: Instant) -> bool {
        if self.auto_lock_due(now) {
            self.lock();
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub fn set_activity(&self, t: Instant) {
        *self.activity.lock_safe() = t;
    }
}

pub fn cache_dir_for(vault_path: &Path) -> PathBuf {
    vault_path
        .parent()
        .map(|p| p.join("cache"))
        .unwrap_or_else(|| PathBuf::from("cache"))
}

fn saver_loop(signal: Arc<SaveSignal>, inner: Weak<Inner>) {
    loop {
        {
            let mut g = signal.flags.lock_safe();
            while !g.dirty && !g.stop {
                g = signal
                    .cv
                    .wait_timeout(g, Duration::from_secs(1))
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
                if inner.strong_count() == 0 {
                    return;
                }
            }
            if g.stop {
                return;
            }
        }
        // Coalesce bursts of mutations into one write.
        std::thread::sleep(signal.debounce);
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let state = AppState(inner);
        state.flush();
        if signal.flags.lock_safe().dirty {
            // A save failed and re-marked the state: back off before retrying.
            std::thread::sleep(Duration::from_secs(3));
        }
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use crate::host::testing::MemHost;

    pub struct Fixture {
        pub state: AppState,
        pub host: Arc<MemHost>,
        pub dir: tempfile::TempDir,
    }

    impl Fixture {
        pub fn path(&self) -> PathBuf {
            self.dir.path().join("vault.ausgeloggt")
        }
    }

    /// Fresh state with a created + unlocked temp vault (cheap KDF, 80 ms debounce).
    pub fn unlocked() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let host = Arc::new(MemHost::default());
        let state = AppState::new(
            host.clone(),
            dir.path().join("vault.ausgeloggt"),
            Duration::from_millis(80),
            true,
        );
        state.create("correct horse battery", None).unwrap();
        Fixture { state, host, dir }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::unlocked;
    use super::*;
    use ausgeloggt_core::model::Account;

    #[test]
    fn password_length_is_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let st = AppState::new(
            Arc::new(crate::host::testing::MemHost::default()),
            dir.path().join("v"),
            Duration::from_millis(50),
            true,
        );
        let e = st.create("kurz", None).unwrap_err();
        assert_eq!(e.code, crate::error::ErrorCode::Invalid);
        assert!(!st.is_unlocked());
    }

    #[test]
    fn locked_vault_rejects_access() {
        let f = unlocked();
        f.state.lock();
        assert_eq!(
            f.state.read(|_| ()).unwrap_err().code,
            crate::error::ErrorCode::VaultLocked
        );
        assert!(f.host.wait_for("vault://locked", 1).is_some());
    }

    #[test]
    fn wrong_password_maps_to_code() {
        let f = unlocked();
        f.state.lock();
        let e = f.state.unlock("falsches passwort", None).unwrap_err();
        assert_eq!(e.code, crate::error::ErrorCode::WrongPassword);
        f.state.unlock("correct horse battery", None).unwrap();
        assert!(f.state.is_unlocked());
    }

    #[test]
    fn mutation_is_saved_debounced_and_coalesced() {
        let f = unlocked();
        let before = std::fs::read(f.path()).unwrap();
        for i in 0..5 {
            f.state
                .mutate(|d| d.accounts.push(Account::new(format!("a{i}.com"), "A")))
                .unwrap();
        }
        // Not written synchronously ...
        assert_eq!(std::fs::read(f.path()).unwrap(), before);
        // ... but shortly after, by the background thread.
        let mut saved = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(40));
            if std::fs::read(f.path()).unwrap() != before {
                saved = true;
                break;
            }
        }
        assert!(saved, "debounced save did not happen");
        let reopened = Vault::open(&f.path(), "correct horse battery").unwrap();
        assert_eq!(reopened.data.accounts.len(), 5);
        // five mutations -> five change events, revision advanced accordingly
        assert!(f.host.named("vault://changed").len() >= 5);
    }

    #[test]
    fn lock_saves_pending_changes_immediately() {
        let f = unlocked();
        f.state
            .mutate(|d| d.accounts.push(Account::new("x.com", "X")))
            .unwrap();
        f.state.lock();
        let reopened = Vault::open(&f.path(), "correct horse battery").unwrap();
        assert_eq!(reopened.data.accounts.len(), 1);
    }

    #[test]
    fn shutdown_flushes() {
        let f = unlocked();
        f.state
            .mutate(|d| d.accounts.push(Account::new("y.com", "Y")))
            .unwrap();
        f.state.shutdown();
        let reopened = Vault::open(&f.path(), "correct horse battery").unwrap();
        assert_eq!(reopened.data.accounts.len(), 1);
    }

    #[test]
    fn failed_closure_does_not_bump_revision() {
        let f = unlocked();
        let rev = f.state.revision();
        let r: IpcResult<()> = f.state.try_mutate(|_| Err(IpcError::invalid("nein")));
        assert!(r.is_err());
        assert_eq!(f.state.revision(), rev);
    }

    #[test]
    fn auto_lock_after_idle_timeout() {
        let f = unlocked();
        f.state
            .mutate(|d| d.settings.auto_lock_minutes = 5)
            .unwrap();
        let now = Instant::now();
        f.state.set_activity(now);
        assert!(!f.state.tick_auto_lock(now + Duration::from_secs(60)));
        assert!(f.state.is_unlocked());
        // a touch resets the timer
        assert!(!f.state.tick_auto_lock(now + Duration::from_secs(299)));
        assert!(f.state.tick_auto_lock(now + Duration::from_secs(301)));
        assert!(!f.state.is_unlocked());
        assert!(f.host.wait_for("vault://locked", 1).is_some());
    }

    #[test]
    fn auto_lock_zero_means_never() {
        let f = unlocked();
        f.state
            .mutate(|d| d.settings.auto_lock_minutes = 0)
            .unwrap();
        let now = Instant::now();
        assert!(!f.state.tick_auto_lock(now + Duration::from_secs(86_400)));
        assert!(f.state.is_unlocked());
    }

    #[test]
    fn change_password_verifies_current_and_keeps_data() {
        let f = unlocked();
        f.state
            .mutate(|d| d.accounts.push(Account::new("z.com", "Z")))
            .unwrap();
        let e = f
            .state
            .change_password("falsch falsch falsch", "ein neues langes passwort")
            .unwrap_err();
        assert_eq!(e.code, crate::error::ErrorCode::WrongPassword);
        let e = f
            .state
            .change_password("correct horse battery", "kurz")
            .unwrap_err();
        assert_eq!(e.code, crate::error::ErrorCode::Invalid);
        f.state
            .change_password("correct horse battery", "ein neues langes passwort")
            .unwrap();
        f.state.lock();
        assert!(f.state.unlock("correct horse battery", None).is_err());
        f.state.unlock("ein neues langes passwort", None).unwrap();
        f.state.read(|d| assert_eq!(d.accounts.len(), 1)).unwrap();
    }

    #[test]
    fn custom_path_is_remembered() {
        let f = unlocked();
        f.state.lock();
        let other = f.dir.path().join("other").join("v2.ausgeloggt");
        f.state
            .create("correct horse battery", Some(other.clone()))
            .unwrap();
        assert_eq!(f.state.vault_path(), other);
        assert!(other.exists());
        assert_eq!(f.state.status()["exists"], true);
    }
}
