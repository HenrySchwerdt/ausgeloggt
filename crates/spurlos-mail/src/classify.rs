//! Pure, offline mail classification (no network). Heavily unit-tested.

use chrono::{DateTime, Utc};
use spurlos_core::{EvidenceKind, SsoProvider};

/// The header fields we fetch for every mail.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MailHeader {
    /// Raw `From:` value, e.g. `"Spotify" <no-reply@spotify.com>` (may be RFC 2047 encoded).
    pub from: String,
    /// Raw `Subject:` (may be RFC 2047 encoded).
    pub subject: String,
    pub date: Option<DateTime<Utc>>,
    pub message_id: Option<String>,
    /// `To:` / `Delivered-To:`, used to learn which identifier was used (incl. Apple relay addresses).
    pub to: String,
    pub has_list_unsubscribe: bool,
    /// Raw `List-Unsubscribe` value.
    pub list_unsubscribe: Option<String>,
    /// Raw `List-Unsubscribe-Post` value.
    pub list_unsubscribe_post: Option<String>,
    /// Raw `List-Id` value.
    pub list_id: Option<String>,
    /// `Precedence: bulk/list`.
    pub precedence: Option<String>,
    /// `Return-Path` / `Sender`, for ESP-sent mail where From is the real brand.
    pub sender: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub kind: EvidenceKind,
    /// Service domain or `unresolved:<slug>`.
    pub domain: String,
    pub name: Option<String>,
    pub sso: Option<SsoProvider>,
    /// Address the mail was sent to (the identifier at the service).
    pub identifier: Option<String>,
}

/// Classify one mail by headers. Returns `None` for personal mail / irrelevant mail.
/// Multilingual keyword rules (de, en, fr, es, it, nl at least) for welcome, verification,
/// password reset, login alert, receipt; List-Unsubscribe without other signal -> Newsletter.
pub fn classify(header: &MailHeader) -> Option<Classification> {
    let _ = header;
    todo!("implemented by mail agent")
}

/// Rule-based category for bulk mail (used for subscriptions; AI may refine later).
pub fn categorize(header: &MailHeader) -> spurlos_core::MailCategory {
    let _ = header;
    todo!("implemented by mail agent")
}

/// True if this is an SSO-provider security mail whose body must be fetched to learn
/// the connected app (Google "Security alert" / "was granted access", GitHub
/// "A third-party OAuth application has been added", Microsoft "app connected", ...).
pub fn needs_body(header: &MailHeader) -> bool {
    let _ = header;
    todo!("implemented by mail agent")
}

/// Extract (provider, app name, app domain if found) from an SSO security mail body.
pub fn parse_sso_body(header: &MailHeader, body: &str) -> Option<(SsoProvider, String, Option<String>)> {
    let _ = (header, body);
    todo!("implemented by mail agent")
}
