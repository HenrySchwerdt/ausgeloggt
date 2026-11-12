//! Unsubscribing from newsletters.
//!
//! Safety rules:
//! - Automated only for RFC 8058 one-click (HTTPS POST `List-Unsubscribe=One-Click`,
//!   no cookies, no redirects to other hosts followed blindly, 15 s timeout) and
//!   `mailto:` (sent via the mailbox's own SMTP server).
//! - Plain `Link` methods are NEVER fetched automatically: they may confirm the
//!   address to spammers. The UI opens them in the browser on explicit click.
//! - Never unsubscribe from `MailCategory::Transactional`.

use spurlos_core::{MailboxConfig, Subscription, UnsubscribeMethod};

#[derive(Debug, thiserror::Error)]
pub enum UnsubscribeError {
    #[error("Kein automatischer Abmeldeweg vorhanden, bitte Link im Browser öffnen")]
    NoAutomaticMethod,
    #[error("Abmeldung abgelehnt: HTTP {0}")]
    Http(u16),
    #[error("Netzwerkfehler: {0}")]
    Network(String),
    #[error("Mailversand fehlgeschlagen: {0}")]
    Smtp(String),
}

/// SMTP settings for a mailbox (derived from the provider preset; same credentials as IMAP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    /// true = implicit TLS (465), false = STARTTLS (587).
    pub implicit_tls: bool,
}

/// SMTP settings for a configured mailbox, via provider preset or `smtp.` + domain guess.
pub fn smtp_for(cfg: &MailboxConfig) -> Option<SmtpSettings> {
    let _ = cfg;
    todo!("implemented by mail agent")
}

/// The best automatic method: OneClick > Mailto. `None` if only links exist.
pub fn best_automatic(sub: &Subscription) -> Option<&UnsubscribeMethod> {
    let _ = sub;
    todo!("implemented by mail agent")
}

/// Perform an automatic unsubscribe. Blocking; call from a worker thread.
/// Returns a short German success message for the history.
pub fn unsubscribe(
    sub: &Subscription,
    mailbox: &MailboxConfig,
) -> Result<String, UnsubscribeError> {
    let _ = (sub, mailbox);
    todo!("implemented by mail agent")
}

/// Parse `List-Unsubscribe` (+ `List-Unsubscribe-Post`) header values into methods.
/// `<https://x/u?id=1>, <mailto:unsub@x.com?subject=unsubscribe>` with
/// `List-Unsubscribe-Post: List-Unsubscribe=One-Click` -> [OneClick, Mailto].
pub fn parse_list_unsubscribe(header: &str, post_header: Option<&str>) -> Vec<UnsubscribeMethod> {
    let _ = (header, post_header);
    todo!("implemented by mail agent")
}
