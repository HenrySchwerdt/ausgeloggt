//! Letters (German, optionally English) and printable PDFs.
//!
//! Every letter is first built as a [`LetterDoc`] (structured, testable), then rendered
//! to plain text (e-mail) or PDF (DIN 5008 Form B: address window for a DL window
//! envelope, sender line, date, subject, body, signature line, page numbers,
//! attachments on new pages). PDF via pure-Rust `printpdf` with an embedded open font
//! (e.g. Inter / Liberation Sans, OFL) so output is identical on every OS.
//!
//! Every letter ends with the note: "Dieses Schreiben wurde mit Spurlos erstellt.
//! Es ist eine Vorlage und keine Rechtsberatung." — but only in the app UI / a
//! separate cover sheet, NOT inside the letter sent to the recipient.

mod content;
mod kits;
mod layout;
mod pdf;
mod text;
mod util;

pub use content::{
    access_request, address_broker_objection, cancellation, cease_and_desist, complaint,
    credit_bureau_access, identity_lines, objection_and_access, recipient_from_catalog, reminder,
    sender_from,
};
pub use kits::{emergency_kit_pdf, privacy_report_pdf};
pub use pdf::render_pdf;
pub use text::render_text;

use chrono::{DateTime, Utc};

#[derive(Debug, thiserror::Error)]
pub enum LetterError {
    #[error("Absenderadresse fehlt: bitte in den Einstellungen ausfüllen ({0})")]
    MissingSender(&'static str),
    #[error("Empfängeradresse fehlt")]
    MissingRecipient,
    #[error("PDF-Erzeugung fehlgeschlagen: {0}")]
    Pdf(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Party {
    pub name: String,
    /// Street, postal code + city, country (multi-line).
    pub address_lines: Vec<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Attachment {
    /// Pre-formulated "strafbewehrte Unterlassungserklärung" (Hamburger Brauch) for the
    /// recipient to sign and return.
    CeaseAndDesistDeclaration {
        sender: Party,
        recipient: Party,
        scope: String,
    },
    /// Evidence table: title + header + rows (date, from, subject, Message-ID) and an
    /// optional raw-headers appendix (monospace, small).
    EvidenceTable {
        title: String,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        raw_headers: Vec<String>,
    },
    /// Plain titled section (account list, notes ...).
    Section {
        title: String,
        paragraphs: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetterDoc {
    pub sender: Party,
    pub recipient: Party,
    /// e.g. "Berlin, 12.11.2026"
    pub place_date: String,
    pub subject: String,
    /// Optional reference line ("Ihr Zeichen / Kundennummer / Kennung: ...").
    pub reference: Option<String>,
    pub salutation: String,
    pub paragraphs: Vec<String>,
    pub closing: String,
    pub attachments: Vec<Attachment>,
    /// Suggested deadline stated in the letter.
    pub deadline: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests;
