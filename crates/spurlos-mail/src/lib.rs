//! Mailbox scanning over IMAP (TLS via rustls, no OpenSSL) and mail classification.
//!
//! Privacy: only headers are fetched (`BODY.PEEK[HEADER.FIELDS (...)]`, never marks
//! mails as read). Bodies are fetched only for the few SSO security mails where the
//! app name is in the body. Nothing is sent anywhere except the user's IMAP server.

pub mod classify;
pub mod unsubscribe;

use std::sync::atomic::AtomicBool;

use spurlos_core::{Finding, FolderProgress, MailboxConfig, SubscriptionObservation};

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Verbindung zu {0} fehlgeschlagen: {1}")]
    Connect(String, String),
    #[error("Anmeldung fehlgeschlagen. Ist das App-Passwort korrekt?")]
    Auth,
    #[error("IMAP-Fehler: {0}")]
    Imap(String),
    #[error("Abgebrochen")]
    Cancelled,
}

/// Known mail providers with IMAP settings and how to get an app password.
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub imap_host: &'static str,
    pub imap_port: u16,
    /// Domains whose addresses belong to this provider (`gmail.com`, `googlemail.com`).
    pub domains: &'static [&'static str],
    /// Folders to scan by default (e.g. Gmail `[Gmail]/All Mail` resolved via SPECIAL-USE \All).
    pub default_folders: &'static [&'static str],
    /// Where the user creates an app password.
    pub app_password_url: Option<&'static str>,
    /// German hint shown in the setup dialog.
    pub hint_de: &'static str,
}

/// Gmail, Outlook/Hotmail (hint: needs OAuth, not yet supported), iCloud, Yahoo, GMX,
/// WEB.DE, T-Online, Posteo, mailbox.org, Proton (Bridge, 127.0.0.1:1143), Fastmail, AOL, freenet.
pub fn presets() -> &'static [ProviderPreset] {
    todo!("implemented by mail agent")
}

pub fn preset_for_address(address: &str) -> Option<&'static ProviderPreset> {
    let _ = address;
    todo!("implemented by mail agent")
}

/// Log in and list folders. Used by the "Verbindung testen" button.
pub fn test_connection(cfg: &MailboxConfig) -> Result<Vec<String>, MailError> {
    let _ = cfg;
    todo!("implemented by mail agent")
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScanEvent {
    Connected,
    FolderStarted { folder: String, total: u64 },
    Progress { folder: String, done: u64, total: u64, findings: u64 },
    FolderFinished { folder: String },
}

#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub findings: Vec<Finding>,
    /// One entry per list seen in this scan, aggregated over the scanned mails.
    pub subscriptions: Vec<SubscriptionObservation>,
    pub items_scanned: u64,
    /// New progress markers to store back into `MailboxConfig.progress`.
    pub progress: Vec<FolderProgress>,
}

/// Scan the configured folders. Incremental by default: only UIDs above the stored
/// progress (if UIDVALIDITY matches). `full_rescan` ignores progress.
/// Runs blocking; call from a worker thread. Checks `cancel` between batches.
pub fn scan_mailbox(
    cfg: &MailboxConfig,
    full_rescan: bool,
    on_event: &mut dyn FnMut(ScanEvent),
    cancel: &AtomicBool,
) -> Result<ScanOutcome, MailError> {
    let _ = (cfg, full_rescan, on_event, cancel);
    todo!("implemented by mail agent")
}
