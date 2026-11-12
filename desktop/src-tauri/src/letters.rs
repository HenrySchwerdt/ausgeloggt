//! Letter previews / PDFs and request bookkeeping (deadlines). Pure functions over
//! `VaultData` + `Catalog`, so everything is testable without Tauri.
//!
//! One source of truth per letter text: GDPR erasure letters come from
//! `ausgeloggt_catalog::gdpr::erasure_request` (also used for the mail body) and are
//! converted into a `LetterDoc` for the DIN 5008 PDF; all other kinds are built by
//! `ausgeloggt_letters`.

use ausgeloggt_catalog::{gdpr, orgs, Catalog};
use ausgeloggt_core::insights::billing_summary;
use ausgeloggt_core::model::{
    Account, DataRequest, RequestChannel, RequestKind, RequestStatus, Settings, Subscription,
    VaultData,
};
use ausgeloggt_letters as letters;
use ausgeloggt_letters::{LetterDoc, LetterError, Party};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LetterKind {
    Erasure,
    Access,
    ObjectionAndAccess,
    Cancellation,
    CeaseAndDesist,
    CreditBureauAccess,
    AddressBrokerObjection,
    Complaint,
    Reminder,
}

/// `Party` as JSON (the letters crate type has no serde).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyDto {
    pub name: String,
    #[serde(default)]
    pub address_lines: Vec<String>,
    #[serde(default)]
    pub email: Option<String>,
}

impl From<Party> for PartyDto {
    fn from(p: Party) -> Self {
        Self {
            name: p.name,
            address_lines: p.address_lines,
            email: p.email,
        }
    }
}

impl From<PartyDto> for Party {
    fn from(p: PartyDto) -> Self {
        Self {
            name: p.name.trim().to_string(),
            address_lines: p
                .address_lines
                .into_iter()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            email: p
                .email
                .map(|e| e.trim().to_string())
                .filter(|e| !e.is_empty()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LetterArgs {
    pub kind: LetterKind,
    pub account_id: Option<Uuid>,
    pub subscription_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    pub org_id: Option<String>,
    pub authority_state: Option<String>,
    pub recipient: Option<PartyDto>,
}

#[derive(Debug, Clone)]
pub struct BuiltLetter {
    pub doc: LetterDoc,
    pub text: String,
    pub recipient: PartyDto,
    pub missing_sender_fields: Vec<String>,
    pub missing_recipient: bool,
}

impl BuiltLetter {
    pub fn preview(&self) -> Value {
        json!({
            "subject": self.doc.subject,
            "text": self.text,
            "recipient": self.recipient,
            "recipient_email": self.recipient.email,
            "deadline": self.doc.deadline.map(|d| d.to_rfc3339()),
            "missing_sender_fields": self.missing_sender_fields,
            "missing_recipient": self.missing_recipient,
        })
    }

    /// Error for actions that need a complete letter (PDF, sending).
    pub fn ensure_complete(&self) -> IpcResult<()> {
        if !self.missing_sender_fields.is_empty() {
            return Err(IpcError::invalid(format!(
                "Absenderadresse fehlt: bitte in den Einstellungen ausfüllen ({})",
                self.missing_sender_fields.join(", ")
            )));
        }
        if self.missing_recipient {
            return Err(IpcError::invalid("Empfängeradresse fehlt"));
        }
        Ok(())
    }
}

/// Fill missing sender fields with `[Label]` placeholders; returns the labels.
fn fill_missing_sender(settings: &mut Settings) -> Vec<String> {
    let mut missing = Vec::new();
    for _ in 0..8 {
        match letters::sender_from(settings) {
            Ok(_) => break,
            Err(LetterError::MissingSender(field)) => {
                missing.push(field.to_string());
                let ph = format!("[{field}]");
                match field {
                    "Name" => settings.full_name = ph,
                    "Straße" => settings.identity.street = ph,
                    "PLZ" => settings.identity.postal_code = ph,
                    "Ort" => settings.identity.city = ph,
                    _ => break,
                }
            }
            Err(_) => break,
        }
    }
    missing
}

fn find_account(data: &VaultData, id: Option<Uuid>) -> IpcResult<Option<&Account>> {
    match id {
        None => Ok(None),
        Some(id) => data
            .accounts
            .iter()
            .find(|a| a.id == id)
            .map(Some)
            .ok_or_else(|| IpcError::not_found("Account")),
    }
}

fn need<T>(v: Option<T>, what: &str) -> IpcResult<T> {
    v.ok_or_else(|| IpcError::invalid(format!("{what} fehlt für diesen Brieftyp")))
}

fn find_subscription(data: &VaultData, id: Uuid) -> IpcResult<&Subscription> {
    data.subscriptions
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| IpcError::not_found("Newsletter"))
}

fn find_request(data: &VaultData, id: Uuid) -> IpcResult<&DataRequest> {
    data.requests
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| IpcError::not_found("Anfrage"))
}

/// Catalog contact for a service: by domain, else by name (for `unresolved:` accounts).
pub fn catalog_party(catalog: &Catalog, domain: &str, name: &str) -> Option<Party> {
    let by_domain = (!domain.starts_with("unresolved:"))
        .then(|| catalog.lookup_contact(domain))
        .flatten();
    by_domain
        .or_else(|| catalog.lookup_name(name))
        .map(letters::recipient_from_catalog)
}

fn org_party(id: &str) -> Option<Party> {
    orgs::organizations()
        .iter()
        .find(|o| o.id == id)
        .map(|o| Party {
            name: o.legal_name.clone(),
            address_lines: o.address_lines.clone(),
            email: o.email.clone(),
        })
}

fn authority_party(settings: &Settings, state_override: Option<&str>) -> Option<Party> {
    let state = state_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(settings.identity.state.as_str());
    orgs::authority_for(state, &settings.identity.postal_code).map(|a| Party {
        name: a.name.clone(),
        address_lines: a.address_lines.clone(),
        email: a.email.clone(),
    })
}

/// Recipient of a follow-up: the stored recipient of the original request, falling back
/// to the catalog.
fn request_party(catalog: &Catalog, r: &DataRequest) -> Option<Party> {
    let rec = r.recipient.trim();
    let (address_lines, email) = match r.channel {
        RequestChannel::Email if !rec.is_empty() => (Vec::new(), Some(rec.to_string())),
        RequestChannel::Post if !rec.is_empty() => (
            rec.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
            None,
        ),
        _ => (Vec::new(), None),
    };
    if !address_lines.is_empty() || email.is_some() {
        return Some(Party {
            name: r.target.trim().to_string(),
            address_lines,
            email,
        });
    }
    r.domain
        .as_deref()
        .and_then(|d| catalog_party(catalog, d, &r.target))
}

/// Who gets the letter: explicit `recipient`, else derived from catalog / organisations /
/// authorities / the original request.
pub fn resolve_recipient(
    data: &VaultData,
    catalog: &Catalog,
    args: &LetterArgs,
) -> IpcResult<Option<Party>> {
    if let Some(r) = args.recipient.clone() {
        let p: Party = r.into();
        if !p.name.is_empty() {
            return Ok(Some(p));
        }
    }
    let party = match args.kind {
        LetterKind::Erasure
        | LetterKind::Access
        | LetterKind::ObjectionAndAccess
        | LetterKind::Cancellation => find_account(data, args.account_id)?
            .and_then(|a| catalog_party(catalog, &a.domain, &a.name)),
        LetterKind::CeaseAndDesist => {
            let sub = find_subscription(data, need(args.subscription_id, "Newsletter")?)?;
            catalog_party(catalog, &sub.domain, &sub.name)
        }
        LetterKind::CreditBureauAccess | LetterKind::AddressBrokerObjection => {
            args.org_id.as_deref().and_then(org_party)
        }
        LetterKind::Complaint => authority_party(&data.settings, args.authority_state.as_deref()),
        LetterKind::Reminder => {
            let r = find_request(data, need(args.request_id, "Anfrage")?)?;
            request_party(catalog, r)
        }
    };
    Ok(party)
}

fn place_date(settings: &Settings, now: DateTime<Utc>) -> String {
    format!(
        "{}, {}",
        settings.identity.city.trim(),
        now.format("%d.%m.%Y")
    )
}

/// Turn the catalog's GDPR letter (`Salutation, paragraphs, closing, name`) into a
/// `LetterDoc` so that it can be rendered as DIN 5008 PDF.
pub fn doc_from_gdpr(
    letter: &gdpr::Letter,
    sender: Party,
    recipient: Party,
    place_date: String,
    deadline: Option<DateTime<Utc>>,
) -> LetterDoc {
    let parts: Vec<&str> = letter.body.trim().split("\n\n").collect();
    let (salutation, paragraphs, closing) = if parts.len() >= 4 {
        (
            parts[0].trim().to_string(),
            parts[1..parts.len() - 2]
                .iter()
                .map(|p| p.trim_end().to_string())
                .collect(),
            parts[parts.len() - 2].trim().to_string(),
        )
    } else {
        (
            String::new(),
            parts.iter().map(|p| p.to_string()).collect(),
            String::new(),
        )
    };
    LetterDoc {
        sender,
        recipient,
        place_date,
        subject: letter.subject.clone(),
        reference: None,
        salutation,
        paragraphs,
        closing,
        attachments: Vec::new(),
        deadline,
    }
}

/// Build a letter. Missing sender data does not fail: placeholders are inserted and the
/// missing fields reported. Same for a missing recipient.
pub fn build_letter(
    data: &VaultData,
    catalog: &Catalog,
    args: &LetterArgs,
    now: DateTime<Utc>,
) -> IpcResult<BuiltLetter> {
    let mut settings = data.settings.clone();
    let missing_sender_fields = fill_missing_sender(&mut settings);

    let found = resolve_recipient(data, catalog, args)?;
    let missing_recipient = found.is_none();
    let recipient = found.unwrap_or_else(|| Party {
        name: "[Empfänger]".into(),
        address_lines: vec!["[Anschrift]".into()],
        email: None,
    });

    let account = find_account(data, args.account_id)?;
    let doc = match args.kind {
        LetterKind::Erasure => {
            let account = need(account, "Account")?;
            let sender = letters::sender_from(&settings)?;
            let letter = gdpr::erasure_request(account, &settings, recipient.email.as_deref());
            doc_from_gdpr(
                &letter,
                sender,
                recipient.clone(),
                place_date(&settings, now),
                Some(gdpr::deadline_from(now)),
            )
        }
        LetterKind::Access => letters::access_request(&settings, &recipient, account, now)?,
        LetterKind::ObjectionAndAccess => {
            letters::objection_and_access(&settings, &recipient, need(account, "Account")?, now)?
        }
        LetterKind::Cancellation => {
            let account = need(account, "Account")?;
            let plan = billing_summary(account, now).and_then(|b| b.plan);
            letters::cancellation(&settings, &recipient, account, plan.as_deref(), now)?
        }
        LetterKind::CeaseAndDesist => {
            let sub = find_subscription(data, need(args.subscription_id, "Newsletter")?)?;
            let account = account.or_else(|| {
                data.accounts
                    .iter()
                    .find(|a| a.domain.eq_ignore_ascii_case(&sub.domain))
            });
            letters::cease_and_desist(&settings, &recipient, sub, account, now)?
        }
        LetterKind::CreditBureauAccess => {
            letters::credit_bureau_access(&settings, &recipient, now)?
        }
        LetterKind::AddressBrokerObjection => {
            letters::address_broker_objection(&settings, &recipient, now)?
        }
        LetterKind::Complaint => {
            let request = find_request(data, need(args.request_id, "Anfrage")?)?;
            let followups: Vec<DataRequest> = data
                .requests
                .iter()
                .filter(|r| r.parent_id == Some(request.id))
                .cloned()
                .collect();
            letters::complaint(&settings, &recipient, request, &followups, now)?
        }
        LetterKind::Reminder => {
            let request = find_request(data, need(args.request_id, "Anfrage")?)?;
            letters::reminder(&settings, &recipient, request, now)?
        }
    };

    let rendered = letters::render_text(&doc);
    // The subject travels separately (mail subject); keep the rest as the mail body.
    let text = match rendered.split_once('\n') {
        Some((first, rest)) if first.starts_with("Betreff: ") => rest.trim_start().to_string(),
        _ => rendered,
    };
    Ok(BuiltLetter {
        doc,
        text,
        recipient: recipient.into(),
        missing_sender_fields,
        missing_recipient,
    })
}

// ---- PDF kits -----------------------------------------------------------------------

/// `generate_pdf_kit`: `legacy` = digital estate kit, `report` = privacy report.
pub fn pdf_kit(data: &VaultData, kind: &str, now: DateTime<Utc>) -> IpcResult<Vec<u8>> {
    match kind {
        "legacy" => Ok(letters::emergency_kit_pdf(&data.settings, data, now)?),
        "report" => {
            let ctx = crate::dto::risk_context(data);
            let score = ausgeloggt_core::insights::privacy_score(data, &ctx, now);
            Ok(letters::privacy_report_pdf(data, &score, now)?)
        }
        _ => Err(IpcError::invalid("Unbekannte Art des PDF Pakets")),
    }
}

/// Write a user-chosen file (PDF export). Parent directory must exist.
pub fn write_output(path: &std::path::Path, bytes: &[u8]) -> IpcResult<()> {
    if path.as_os_str().is_empty() {
        return Err(IpcError::invalid("Kein Speicherort angegeben"));
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

// ---- requests ---------------------------------------------------------------------

/// Answer deadline for a request sent at `sent_at`: one month for GDPR requests
/// (`gdpr::deadline_from`), 14 days for cease-and-desist, reminders and cancellations,
/// none for complaints (the authority sets its own pace).
pub fn deadline_for(kind: RequestKind, sent_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match kind {
        RequestKind::Erasure
        | RequestKind::Access
        | RequestKind::Objection
        | RequestKind::CreditBureauAccess
        | RequestKind::AdvertisingObjection => Some(gdpr::deadline_from(sent_at)),
        RequestKind::CeaseAndDesist | RequestKind::Reminder | RequestKind::Cancellation => {
            Some(sent_at + Duration::days(14))
        }
        RequestKind::Complaint => None,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewRequest {
    pub kind: RequestKind,
    pub target: String,
    pub domain: Option<String>,
    pub account_id: Option<Uuid>,
    pub subscription_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub channel: RequestChannel,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub sent: bool,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub mailbox_id: Option<Uuid>,
    #[serde(default)]
    pub attachment: Option<String>,
}

pub fn create_request(
    data: &mut VaultData,
    req: NewRequest,
    now: DateTime<Utc>,
) -> IpcResult<Uuid> {
    if req.target.trim().is_empty() {
        return Err(IpcError::invalid("Empfänger der Anfrage fehlt"));
    }
    if let Some(id) = req.account_id {
        if !data.accounts.iter().any(|a| a.id == id) {
            return Err(IpcError::not_found("Account"));
        }
    }
    if let Some(id) = req.subscription_id {
        if !data.subscriptions.iter().any(|s| s.id == id) {
            return Err(IpcError::not_found("Newsletter"));
        }
    }
    if let Some(id) = req.parent_id {
        if !data.requests.iter().any(|r| r.id == id) {
            return Err(IpcError::not_found("Ursprüngliche Anfrage"));
        }
    }
    let id = Uuid::new_v4();
    let (sent_at, status) = if req.sent {
        (Some(now), RequestStatus::Sent)
    } else {
        (None, RequestStatus::Draft)
    };
    data.requests.push(DataRequest {
        id,
        kind: req.kind,
        target: req.target.trim().to_string(),
        domain: req.domain.filter(|d| !d.trim().is_empty()),
        account_id: req.account_id,
        subscription_id: req.subscription_id,
        parent_id: req.parent_id,
        channel: req.channel,
        recipient: req.recipient,
        subject: req.subject,
        body: req.body,
        created_at: now,
        sent_at,
        deadline: sent_at.and_then(|s| deadline_for(req.kind, s)),
        status,
        notes: String::new(),
        message_id: req.message_id,
        mailbox_id: req.mailbox_id,
        attachment: req.attachment,
    });
    Ok(id)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPatch {
    pub id: Uuid,
    pub status: Option<RequestStatus>,
    pub notes: Option<String>,
    pub sent_at: Option<DateTime<Utc>>,
}

pub fn update_request(data: &mut VaultData, patch: RequestPatch) -> IpcResult<()> {
    let r = data
        .requests
        .iter_mut()
        .find(|r| r.id == patch.id)
        .ok_or_else(|| IpcError::not_found("Anfrage"))?;
    if let Some(n) = patch.notes {
        r.notes = n;
    }
    if let Some(sent) = patch.sent_at {
        r.sent_at = Some(sent);
        r.deadline = deadline_for(r.kind, sent);
        if r.status == RequestStatus::Draft && patch.status.is_none() {
            r.status = RequestStatus::Sent;
        }
    }
    if let Some(s) = patch.status {
        r.status = s;
        if s == RequestStatus::Sent && r.sent_at.is_none() {
            let now = Utc::now();
            r.sent_at = Some(now);
            r.deadline = deadline_for(r.kind, now);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ausgeloggt_core::model::{Account, SubscriptionStatus};
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 31, 10, 0, 0).unwrap()
    }

    fn data() -> VaultData {
        let mut d = VaultData::default();
        d.settings.full_name = "Erika Muster".into();
        d.settings.identity.street = "Musterstraße 1".into();
        d.settings.identity.postal_code = "80331".into();
        d.settings.identity.city = "München".into();
        d.settings.identity.email = "erika@example.org".into();
        let mut a = Account::new("spotify.com", "Spotify");
        a.identifiers.push("erika@example.org".into());
        d.accounts.push(a);
        d
    }

    fn args(kind: LetterKind) -> LetterArgs {
        LetterArgs {
            kind,
            account_id: None,
            subscription_id: None,
            request_id: None,
            org_id: None,
            authority_state: None,
            recipient: None,
        }
    }

    fn request(kind: RequestKind) -> DataRequest {
        DataRequest {
            id: Uuid::new_v4(),
            kind,
            target: "Spotify AB".into(),
            domain: Some("spotify.com".into()),
            account_id: None,
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: "privacy@spotify.com".into(),
            subject: "Löschung".into(),
            body: "…".into(),
            created_at: now(),
            sent_at: Some(now()),
            deadline: Some(gdpr::deadline_from(now())),
            status: RequestStatus::Sent,
            notes: String::new(),
            message_id: None,
            mailbox_id: None,
            attachment: None,
        }
    }

    #[test]
    fn erasure_uses_catalog_recipient_and_gdpr_text() {
        let d = data();
        let cat = Catalog::bundled();
        let mut a = args(LetterKind::Erasure);
        a.account_id = Some(d.accounts[0].id);
        let b = build_letter(&d, &cat, &a, now()).unwrap();
        assert!(!b.missing_recipient, "spotify is in the catalog");
        assert!(b.missing_sender_fields.is_empty());
        assert!(b.doc.subject.contains("Art. 17"));
        assert!(b.text.contains("Art. 17"));
        assert!(!b.text.starts_with("Betreff:"));
        assert!(b.text.contains("erika@example.org"));
        assert_eq!(b.doc.sender.name, "Erika Muster");
        assert_eq!(b.doc.salutation, "Sehr geehrte Damen und Herren,");
        assert_eq!(b.doc.closing, "Mit freundlichen Grüßen");
        assert!(b.doc.paragraphs.len() >= 4);
        assert_eq!(b.doc.deadline, Some(gdpr::deadline_from(now())));
        // The PDF renders.
        let pdf = letters::render_pdf(&b.doc).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        b.ensure_complete().unwrap();
        let p = b.preview();
        assert_eq!(p["recipient"]["name"], b.recipient.name);
        assert!(p["deadline"].is_string());
    }

    #[test]
    fn explicit_recipient_wins() {
        let d = data();
        let mut a = args(LetterKind::Erasure);
        a.account_id = Some(d.accounts[0].id);
        a.recipient = Some(PartyDto {
            name: "Eigene GmbH".into(),
            address_lines: vec!["Weg 1".into(), "".into(), "12345 Ort".into()],
            email: Some(" dpo@eigene.de ".into()),
        });
        let b = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert_eq!(b.recipient.name, "Eigene GmbH");
        assert_eq!(b.recipient.address_lines, vec!["Weg 1", "12345 Ort"]);
        assert_eq!(b.recipient.email.as_deref(), Some("dpo@eigene.de"));
    }

    #[test]
    fn missing_sender_fields_are_listed_not_fatal() {
        let mut d = data();
        d.settings.full_name.clear();
        d.settings.identity.city.clear();
        let mut a = args(LetterKind::Erasure);
        a.account_id = Some(d.accounts[0].id);
        let b = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert_eq!(b.missing_sender_fields, vec!["Name", "Ort"]);
        assert!(b.text.contains("[Name]"));
        let e = b.ensure_complete().unwrap_err();
        assert_eq!(e.code, crate::error::ErrorCode::Invalid);
        assert!(e.message.contains("Name, Ort"));
    }

    #[test]
    fn unknown_service_has_missing_recipient_placeholder() {
        let mut d = data();
        d.accounts
            .push(Account::new("kleine-bude-xyz.example", "Kleine Bude XYZ"));
        let mut a = args(LetterKind::Access);
        a.account_id = Some(d.accounts[1].id);
        let b = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert!(b.missing_recipient);
        assert!(b.ensure_complete().is_err());
        assert_eq!(b.preview()["missing_recipient"], true);
    }

    #[test]
    fn credit_bureau_resolves_organisation() {
        let d = data();
        let mut a = args(LetterKind::CreditBureauAccess);
        a.org_id = Some("schufa".into());
        let b = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert!(!b.missing_recipient);
        assert!(b.recipient.name.contains("SCHUFA"));
        // an unknown org id leaves the recipient open
        a.org_id = Some("gibtsnicht".into());
        assert!(
            build_letter(&d, &Catalog::bundled(), &a, now())
                .unwrap()
                .missing_recipient
        );
    }

    #[test]
    fn complaint_uses_authority_for_state_or_postal_code() {
        let mut d = data();
        let req = request(RequestKind::Erasure);
        let rid = req.id;
        d.requests.push(req);
        let mut a = args(LetterKind::Complaint);
        a.request_id = Some(rid);
        // from postal code 80331 -> Bavaria
        let by_plz = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert!(!by_plz.missing_recipient);
        assert!(
            by_plz.recipient.name.contains("Bayer"),
            "{}",
            by_plz.recipient.name
        );
        // explicit state wins
        a.authority_state = Some("DE-BE".into());
        let by_state = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert!(
            by_state.recipient.name.contains("Berlin"),
            "{}",
            by_state.recipient.name
        );
    }

    #[test]
    fn reminder_recipient_comes_from_the_original_request() {
        let mut d = data();
        let req = request(RequestKind::Erasure);
        let rid = req.id;
        d.requests.push(req);
        let mut a = args(LetterKind::Reminder);
        a.request_id = Some(rid);
        let b = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert_eq!(b.recipient.name, "Spotify AB");
        assert_eq!(b.recipient.email.as_deref(), Some("privacy@spotify.com"));
        assert_eq!(b.doc.deadline, Some(now() + Duration::days(14)));
    }

    #[test]
    fn cease_and_desist_needs_subscription() {
        let d = data();
        let a = args(LetterKind::CeaseAndDesist);
        assert_eq!(
            build_letter(&d, &Catalog::bundled(), &a, now())
                .unwrap_err()
                .code,
            crate::error::ErrorCode::Invalid
        );
        let mut a = args(LetterKind::CeaseAndDesist);
        a.subscription_id = Some(Uuid::new_v4());
        assert_eq!(
            build_letter(&d, &Catalog::bundled(), &a, now())
                .unwrap_err()
                .code,
            crate::error::ErrorCode::NotFound
        );
    }

    #[test]
    fn cease_and_desist_for_known_sender() {
        let mut d = data();
        d.subscriptions.push(Subscription {
            id: Uuid::new_v4(),
            mailbox: "erika@example.org".into(),
            list_key: "news@spotify.com".into(),
            domain: "spotify.com".into(),
            name: "Spotify".into(),
            sender: "news@spotify.com".into(),
            unsubscribe: vec![],
            message_count: 4,
            frequency_per_month: 2.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category: Default::default(),
            category_source: Default::default(),
            status: SubscriptionStatus::StillReceiving,
            history: vec![],
            post_unsubscribe_mails: vec![],
            last_message_id: None,
        });
        let mut a = args(LetterKind::CeaseAndDesist);
        a.subscription_id = Some(d.subscriptions[0].id);
        let b = build_letter(&d, &Catalog::bundled(), &a, now()).unwrap();
        assert_eq!(b.doc.deadline, Some(now() + Duration::days(14)));
        assert!(letters::render_pdf(&b.doc).unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn deadlines_by_kind() {
        let sent = now();
        assert_eq!(
            deadline_for(RequestKind::Erasure, sent),
            Some(gdpr::deadline_from(sent))
        );
        // one calendar month, clamped: 31 Jan -> 28 Feb
        assert_eq!(
            deadline_for(RequestKind::Access, sent)
                .unwrap()
                .format("%m-%d")
                .to_string(),
            "02-28"
        );
        assert_eq!(
            deadline_for(RequestKind::CeaseAndDesist, sent),
            Some(sent + Duration::days(14))
        );
        assert_eq!(
            deadline_for(RequestKind::Reminder, sent),
            Some(sent + Duration::days(14))
        );
        assert_eq!(deadline_for(RequestKind::Complaint, sent), None);
    }

    fn new_request(sent: bool) -> NewRequest {
        NewRequest {
            kind: RequestKind::Erasure,
            target: "Spotify AB".into(),
            domain: Some("spotify.com".into()),
            account_id: None,
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: "privacy@spotify.com".into(),
            subject: "Löschung".into(),
            body: "Text".into(),
            sent,
            message_id: None,
            mailbox_id: None,
            attachment: None,
        }
    }

    #[test]
    fn request_create_sent_sets_deadline_draft_does_not() {
        let mut d = data();
        let sent = create_request(&mut d, new_request(true), now()).unwrap();
        let draft = create_request(&mut d, new_request(false), now()).unwrap();
        let s = d.requests.iter().find(|r| r.id == sent).unwrap();
        assert_eq!(s.status, RequestStatus::Sent);
        assert_eq!(s.deadline, Some(gdpr::deadline_from(now())));
        let dr = d.requests.iter().find(|r| r.id == draft).unwrap();
        assert_eq!(dr.status, RequestStatus::Draft);
        assert!(dr.sent_at.is_none() && dr.deadline.is_none());
    }

    #[test]
    fn request_update_sets_deadline_when_marked_sent() {
        let mut d = data();
        let id = create_request(&mut d, new_request(false), now()).unwrap();
        let when = now() + Duration::days(2);
        update_request(
            &mut d,
            RequestPatch {
                id,
                sent_at: Some(when),
                ..Default::default()
            },
        )
        .unwrap();
        let r = &d.requests[0];
        assert_eq!(r.status, RequestStatus::Sent);
        assert_eq!(r.deadline, Some(gdpr::deadline_from(when)));
        update_request(
            &mut d,
            RequestPatch {
                id,
                status: Some(RequestStatus::Done),
                notes: Some("erledigt".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(d.requests[0].status, RequestStatus::Done);
        assert_eq!(d.requests[0].notes, "erledigt");
        assert!(update_request(
            &mut d,
            RequestPatch {
                id: Uuid::new_v4(),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn request_create_validates_references() {
        let mut d = data();
        let mut r = new_request(true);
        r.account_id = Some(Uuid::new_v4());
        assert_eq!(
            create_request(&mut d, r, now()).unwrap_err().code,
            crate::error::ErrorCode::NotFound
        );
        let mut r = new_request(true);
        r.target = "  ".into();
        assert!(create_request(&mut d, r, now()).is_err());
        assert!(d.requests.is_empty());
    }

    #[test]
    fn pdf_kits_render() {
        let mut d = data();
        d.requests.push(request(RequestKind::Erasure));
        assert!(pdf_kit(&d, "legacy", now()).unwrap().starts_with(b"%PDF"));
        assert!(pdf_kit(&d, "report", now()).unwrap().starts_with(b"%PDF"));
        assert!(pdf_kit(&d, "zip", now()).is_err());
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("kit.pdf");
        write_output(&out, b"%PDF-x").unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"%PDF-x");
        assert!(write_output(std::path::Path::new(""), b"x").is_err());
    }

    #[test]
    fn gdpr_conversion_falls_back_on_odd_bodies() {
        let l = gdpr::Letter {
            to: None,
            subject: "S".into(),
            body: "nur ein Absatz".into(),
        };
        let doc = doc_from_gdpr(&l, Party::default(), Party::default(), "x".into(), None);
        assert_eq!(doc.paragraphs, vec!["nur ein Absatz"]);
        assert!(doc.salutation.is_empty());
    }
}
