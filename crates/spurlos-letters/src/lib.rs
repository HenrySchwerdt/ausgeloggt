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

use chrono::{DateTime, Utc};
use spurlos_core::{Account, DataRequest, Identity, Settings, Subscription};

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
    CeaseAndDesistDeclaration { sender: Party, recipient: Party, scope: String },
    /// Evidence table: title + header + rows (date, from, subject, Message-ID) and an
    /// optional raw-headers appendix (monospace, small).
    EvidenceTable { title: String, columns: Vec<String>, rows: Vec<Vec<String>>, raw_headers: Vec<String> },
    /// Plain titled section (account list, notes ...).
    Section { title: String, paragraphs: Vec<String> },
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

/// Sender party from settings (name + identity); error lists the missing field.
pub fn sender_from(settings: &Settings) -> Result<Party, LetterError> {
    let _ = settings;
    todo!("implemented by letters agent")
}

/// Recipient party from a catalog entry (legal name + postal address + email).
pub fn recipient_from_catalog(info: &spurlos_catalog::DeletionInfo) -> Party {
    let _ = info;
    todo!("implemented by letters agent")
}

/// Unterlassungsaufforderung (self-written cease-and-desist) after advertising mails
/// despite unsubscribe/objection. Cites Art. 21 Abs. 3 DSGVO, §§ 823 Abs. 1, 1004 BGB
/// analog (allgemeines Persönlichkeitsrecht), § 7 UWG as orientation; demands a
/// strafbewehrte Unterlassungserklärung (attached, Hamburger Brauch) within 14 days,
/// Art. 15 information about stored data and source, and erasure of the advertising
/// use; states that no costs are claimed (self-written). Attaches the evidence table
/// built from `sub.history` + `sub.post_unsubscribe_mails`.
pub fn cease_and_desist(settings: &Settings, recipient: &Party, sub: &Subscription, account: Option<&Account>, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, sub, account, now);
    todo!("implemented by letters agent")
}

/// Art. 21 objection to direct marketing + Art. 15 access in one letter (escalation step 1).
pub fn objection_and_access(settings: &Settings, recipient: &Party, account: &Account, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, account, now);
    todo!("implemented by letters agent")
}

/// Art. 15 access request ("Was wisst ihr über mich?").
pub fn access_request(settings: &Settings, recipient: &Party, account: Option<&Account>, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, account, now);
    todo!("implemented by letters agent")
}

/// Kündigung in Textform (§ 312k BGB context), "zum nächstmöglichen Zeitpunkt",
/// asks for written confirmation with end date, revokes direct-debit mandate /
/// payment authorisation for future charges, and asks to delete data afterwards (Art. 17).
pub fn cancellation(settings: &Settings, recipient: &Party, account: &Account, plan: Option<&str>, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, account, plan, now);
    todo!("implemented by letters agent")
}

/// Credit bureau self-disclosure (Art. 15, free copy), with birth date + previous addresses.
pub fn credit_bureau_access(settings: &Settings, recipient: &Party, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, now);
    todo!("implemented by letters agent")
}

/// Objection against address trading / advertising use (Art. 21) at address brokers,
/// incl. request to name recipients of the data (Art. 15 Abs. 1 lit. c).
pub fn address_broker_objection(settings: &Settings, recipient: &Party, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, now);
    todo!("implemented by letters agent")
}

/// Art. 77 complaint to a supervisory authority about an unanswered / refused request,
/// summarising the timeline of `request` (+ its follow-ups) with an evidence attachment.
pub fn complaint(settings: &Settings, authority: &Party, request: &DataRequest, followups: &[DataRequest], now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, authority, request, followups, now);
    todo!("implemented by letters agent")
}

/// Generic reminder for an overdue request (14-day final deadline, mentions Art. 77).
pub fn reminder(settings: &Settings, recipient: &Party, request: &DataRequest, now: DateTime<Utc>) -> Result<LetterDoc, LetterError> {
    let _ = (settings, recipient, request, now);
    todo!("implemented by letters agent")
}

/// Plain-text rendering for e-mail (no attachments inline except a short list of them;
/// evidence tables rendered as text).
pub fn render_text(doc: &LetterDoc) -> String {
    let _ = doc;
    todo!("implemented by letters agent")
}

/// DIN 5008 PDF.
pub fn render_pdf(doc: &LetterDoc) -> Result<Vec<u8>, LetterError> {
    let _ = doc;
    todo!("implemented by letters agent")
}

/// "Digitaler Nachlass" PDF for next of kin: all open accounts (name, domain, login
/// identifiers, SSO, active subscriptions with amounts, payment methods brand+last4,
/// deletion hints/links), grouped and sorted; never passwords. Cover page explains
/// how to use it and to store it safely.
pub fn emergency_kit_pdf(settings: &Settings, data: &spurlos_core::VaultData, now: DateTime<Utc>) -> Result<Vec<u8>, LetterError> {
    let _ = (settings, data, now);
    todo!("implemented by letters agent")
}

/// Privacy report PDF: score with breakdown, counts, top risks, subscriptions total,
/// open requests and deadlines.
pub fn privacy_report_pdf(data: &spurlos_core::VaultData, score: &spurlos_core::insights::PrivacyScore, now: DateTime<Utc>) -> Result<Vec<u8>, LetterError> {
    let _ = (data, score, now);
    todo!("implemented by letters agent")
}

/// Convenience: identity lines for a party ("Straße", "PLZ Ort", "Land" if not Deutschland).
pub fn identity_lines(identity: &Identity) -> Vec<String> {
    let _ = identity;
    todo!("implemented by letters agent")
}
