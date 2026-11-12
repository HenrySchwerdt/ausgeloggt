//! GDPR Art. 17 erasure requests (and Art. 15 access requests as an option).

use chrono::{DateTime, Utc};
use spurlos_core::{Account, Settings};

#[derive(Debug, Clone, PartialEq)]
pub struct Letter {
    pub to: Option<String>,
    pub subject: String,
    pub body: String,
}

/// Build an erasure request for `account` in `settings.letter_language` ("de"/"en"),
/// signed with `settings.full_name`, listing the account's identifiers. Cites Art. 17
/// and Art. 12(3) GDPR (one month), asks for confirmation, and for erasure at processors.
pub fn erasure_request(account: &Account, settings: &Settings, to: Option<&str>) -> Letter {
    let _ = (account, settings, to);
    todo!("implemented by catalog agent")
}

/// Follow-up letter when the deadline has passed without an answer
/// (mentions the right to complain to a supervisory authority, Art. 77).
pub fn reminder(account: &Account, settings: &Settings, sent_at: DateTime<Utc>, to: Option<&str>) -> Letter {
    let _ = (account, settings, sent_at, to);
    todo!("implemented by catalog agent")
}

/// `mailto:` URL with percent-encoded subject and body, for the default mail client.
pub fn mailto_url(letter: &Letter) -> String {
    let _ = letter;
    todo!("implemented by catalog agent")
}

/// GDPR deadline: one calendar month after `sent_at` (Art. 12(3)).
pub fn deadline_from(sent_at: DateTime<Utc>) -> DateTime<Utc> {
    let _ = sent_at;
    todo!("implemented by catalog agent")
}
