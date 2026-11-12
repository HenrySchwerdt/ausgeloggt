//! Sending letters by mail from a configured mailbox: single letters, bulk sends with a
//! dry run, and the shared `send_prepared` step that honours the rate limit and records
//! the `DataRequest`. The SMTP part (`Sender`) is injectable for tests.
//!
//! Safety: recipients come only from the catalog or from what the user typed, a letter is
//! only sent when sender fields and the recipient mail address are complete, and an
//! account that already has a non-draft request of the same kind is never contacted again.

use std::time::{Duration, Instant};

use ausgeloggt_catalog::Catalog;
use ausgeloggt_core::model::{
    MailboxConfig, RequestChannel, RequestKind, RequestStatus, VaultData,
};
use ausgeloggt_mail::send::{OutgoingMail, SentMail};
use ausgeloggt_mail::MailError;
use chrono::{DateTime, Local, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::jobs::{JobCtx, JobFail, JobOutcome, JobResult};
use crate::letters::{self, LetterArgs, LetterKind, NewRequest, PartyDto};
use crate::ratelimit::{self, Gate};
use crate::state::AppState;

pub type Sender =
    dyn Fn(&MailboxConfig, &OutgoingMail) -> Result<SentMail, MailError> + Send + Sync;

/// The real SMTP sender (plus the copy in the Sent folder).
pub fn real_sender() -> &'static Sender {
    &ausgeloggt_mail::send::send_mail
}

pub fn request_kind(kind: LetterKind) -> RequestKind {
    match kind {
        LetterKind::Erasure => RequestKind::Erasure,
        LetterKind::Access => RequestKind::Access,
        LetterKind::ObjectionAndAccess => RequestKind::Objection,
        LetterKind::Cancellation => RequestKind::Cancellation,
        LetterKind::CeaseAndDesist => RequestKind::CeaseAndDesist,
        LetterKind::CreditBureauAccess => RequestKind::CreditBureauAccess,
        LetterKind::AddressBrokerObjection => RequestKind::AdvertisingObjection,
        LetterKind::Complaint => RequestKind::Complaint,
        LetterKind::Reminder => RequestKind::Reminder,
    }
}

/// A letter that is ready to go out.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub kind: RequestKind,
    pub target: String,
    pub domain: Option<String>,
    pub account_id: Option<Uuid>,
    pub subscription_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub from_name: String,
    pub pdf: Option<(String, Vec<u8>)>,
}

fn slug(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').chars().take(40).collect()
}

fn plausible_email(s: &str) -> bool {
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.contains('@')
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ',')
}

/// Build everything needed to send `args` from `mailbox_id`. Fails with a German
/// `Invalid` error when anything is missing.
pub fn prepare(
    data: &VaultData,
    catalog: &Catalog,
    args: &LetterArgs,
    mailbox_id: Uuid,
    attach_pdf: bool,
    now: DateTime<Utc>,
) -> IpcResult<(Prepared, MailboxConfig)> {
    let mailbox = data
        .mailboxes
        .iter()
        .find(|m| m.id == mailbox_id)
        .cloned()
        .ok_or_else(|| IpcError::not_found("Postfach"))?;
    if mailbox.password.is_empty() {
        return Err(IpcError::invalid(
            "Für dieses Postfach ist kein Passwort gespeichert",
        ));
    }
    if ausgeloggt_mail::unsubscribe::smtp_for(&mailbox).is_none() {
        return Err(IpcError::invalid(
            "Für dieses Postfach ist kein Mailserver zum Senden bekannt",
        ));
    }
    let built = letters::build_letter(data, catalog, args, now)?;
    built.ensure_complete()?;
    let to = built
        .recipient
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .ok_or_else(|| {
            IpcError::invalid(
                "Für diesen Empfänger ist keine Mailadresse bekannt. Bitte den Brief als PDF speichern und per Post senden",
            )
        })?
        .to_string();
    if !plausible_email(&to) {
        return Err(IpcError::invalid(
            "Die Mailadresse des Empfängers ist ungültig",
        ));
    }
    let kind = request_kind(args.kind);
    let account = args
        .account_id
        .and_then(|id| data.accounts.iter().find(|a| a.id == id));
    let sub = args
        .subscription_id
        .and_then(|id| data.subscriptions.iter().find(|s| s.id == id));
    let domain = account
        .map(|a| a.domain.clone())
        .or_else(|| sub.map(|s| s.domain.clone()))
        .or_else(|| {
            args.request_id
                .and_then(|id| data.requests.iter().find(|r| r.id == id))
                .and_then(|r| r.domain.clone())
        });
    let pdf = if attach_pdf {
        let bytes = ausgeloggt_letters::render_pdf(&built.doc)?;
        let name = format!("{:?}_{}.pdf", args.kind, slug(&built.recipient.name));
        Some((name, bytes))
    } else {
        None
    };
    let prepared = Prepared {
        kind,
        target: built.recipient.name.clone(),
        domain,
        account_id: args.account_id,
        subscription_id: args.subscription_id,
        parent_id: matches!(args.kind, LetterKind::Reminder | LetterKind::Complaint)
            .then_some(args.request_id)
            .flatten(),
        to,
        subject: built.doc.subject.clone(),
        body: built.text.clone(),
        from_name: data.settings.full_name.trim().to_string(),
        pdf,
    };
    Ok((prepared, mailbox))
}

/// Why `send_prepared` did not send.
#[derive(Debug)]
pub enum SendStop {
    Cancelled,
    Cap { cap: u32, sent: u32 },
    Failed(IpcError),
}

impl SendStop {
    pub fn cap_message(cap: u32) -> String {
        format!("Das Tageslimit von {cap} Mails für dieses Postfach ist erreicht. Morgen geht es weiter")
    }
}

impl From<SendStop> for JobFail {
    fn from(s: SendStop) -> Self {
        match s {
            SendStop::Cancelled => JobFail::Cancelled,
            SendStop::Cap { cap, .. } => {
                JobFail::Error(IpcError::invalid(SendStop::cap_message(cap)))
            }
            SendStop::Failed(e) => JobFail::Error(e),
        }
    }
}

pub struct Sent {
    pub request_id: Uuid,
    pub mail: SentMail,
}

fn sleep_cancellable(total: Duration, cancelled: &dyn Fn() -> bool) -> Result<(), SendStop> {
    let end = Instant::now() + total;
    loop {
        if cancelled() {
            return Err(SendStop::Cancelled);
        }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(());
        }
        std::thread::sleep(left.min(Duration::from_millis(200)));
    }
}

/// Wait for the rate limit gap, send, count the mail and record the request.
pub fn send_prepared(
    state: &AppState,
    sender: &Sender,
    mailbox: &MailboxConfig,
    p: &Prepared,
    cancelled: &dyn Fn() -> bool,
) -> Result<Sent, SendStop> {
    loop {
        let gate = state
            .read(|d| ratelimit::gate(d, mailbox, Utc::now(), &Local))
            .map_err(SendStop::Failed)?;
        match gate {
            Gate::Ready => break,
            Gate::Wait(d) => sleep_cancellable(d, cancelled)?,
            Gate::CapReached { cap, sent } => return Err(SendStop::Cap { cap, sent }),
        }
    }
    if cancelled() {
        return Err(SendStop::Cancelled);
    }
    let out = OutgoingMail {
        to: p.to.clone(),
        subject: p.subject.clone(),
        body_text: p.body.clone(),
        attachments: p
            .pdf
            .iter()
            .map(|(n, b)| (n.clone(), "application/pdf".to_string(), b.clone()))
            .collect(),
        reply_to: None,
        from_name: p.from_name.clone(),
    };
    let mail = sender(mailbox, &out).map_err(|e| SendStop::Failed(e.into()))?;
    let now = mail.sent_at;
    let mailbox_id = mailbox.id;
    // The mail is out: count it first, whatever happens next.
    state
        .mutate(|d| ratelimit::record(d, mailbox_id, now, &Local))
        .map_err(SendStop::Failed)?;
    let req = NewRequest {
        kind: p.kind,
        target: p.target.clone(),
        domain: p.domain.clone(),
        account_id: p.account_id,
        subscription_id: p.subscription_id,
        parent_id: p.parent_id,
        channel: RequestChannel::Email,
        recipient: p.to.clone(),
        subject: p.subject.clone(),
        body: p.body.clone(),
        sent: true,
        message_id: Some(mail.message_id.clone()),
        mailbox_id: Some(mailbox_id),
        attachment: p.pdf.as_ref().map(|(n, _)| n.clone()),
    };
    let request_id = state
        .try_mutate(|d| {
            let id = letters::create_request(d, req, now)?;
            if p.kind == RequestKind::Erasure {
                if let Some(a) = d.accounts.iter_mut().find(|a| Some(a.id) == p.account_id) {
                    a.queued_for_deletion = false;
                }
            }
            Ok(id)
        })
        .map_err(|e| {
            SendStop::Failed(IpcError::new(
                e.code,
                "Die Mail wurde gesendet, konnte aber nicht im Tresor vermerkt werden",
            ))
        })?;
    Ok(Sent { request_id, mail })
}

// ---- single letter ----------------------------------------------------------------------

pub fn start_send(
    state: &AppState,
    args: LetterArgs,
    mailbox_id: Uuid,
    attach_pdf: bool,
    sender: &'static Sender,
) -> IpcResult<String> {
    let st = state.clone();
    state.start_job("send", move |ctx| {
        send_job(ctx, &st, &args, mailbox_id, attach_pdf, sender)
    })
}

pub fn send_job(
    ctx: &JobCtx,
    state: &AppState,
    args: &LetterArgs,
    mailbox_id: Uuid,
    attach_pdf: bool,
    sender: &Sender,
) -> JobResult {
    ctx.progress(0, 1, "Brief wird vorbereitet");
    let catalog = state.catalog();
    let (prepared, mailbox) =
        state.read(|d| prepare(d, &catalog, args, mailbox_id, attach_pdf, Utc::now()))??;
    ctx.progress(0, 1, "Mail wird gesendet");
    let sent = send_prepared(state, sender, &mailbox, &prepared, &|| ctx.is_cancelled())?;
    ctx.progress(1, 1, "Gesendet");
    let note = match (&sent.mail.appended_to, &sent.mail.append_error) {
        (Some(f), _) => format!(" Eine Kopie liegt im Ordner {f}."),
        (None, Some(_)) => " Die Kopie im Ordner Gesendet konnte nicht abgelegt werden.".into(),
        (None, None) => String::new(),
    };
    Ok(
        JobOutcome::ok(format!("Mail gesendet.{note}")).with_payload(json!({
            "requestId": sent.request_id,
            "messageId": sent.mail.message_id,
            "sentAt": sent.mail.sent_at.to_rfc3339(),
            "appendedTo": sent.mail.appended_to,
            "appendError": sent.mail.append_error,
        })),
    )
}

// ---- bulk ------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkItem {
    pub kind: LetterKind,
    pub account_id: Uuid,
    #[serde(default)]
    pub recipient: Option<PartyDto>,
}

impl BulkItem {
    pub fn args(&self) -> LetterArgs {
        LetterArgs {
            kind: self.kind,
            account_id: Some(self.account_id),
            subscription_id: None,
            request_id: None,
            org_id: None,
            authority_state: None,
            recipient: self.recipient.clone(),
        }
    }
}

/// Is there a request of this kind for the account that was already sent?
pub fn already_requested(data: &VaultData, kind: RequestKind, account_id: Uuid) -> bool {
    data.requests.iter().any(|r| {
        r.account_id == Some(account_id) && r.kind == kind && r.status != RequestStatus::Draft
    })
}

fn bulk_kind_allowed(kind: LetterKind) -> bool {
    matches!(
        kind,
        LetterKind::Erasure
            | LetterKind::Access
            | LetterKind::ObjectionAndAccess
            | LetterKind::Cancellation
    )
}

/// Account id and either the letter ready to send or the reason why not.
pub type PlannedItem = (Uuid, Result<(Prepared, MailboxConfig), String>);

/// One entry per item: ready to send or the reason why not.
pub fn plan_items(
    data: &VaultData,
    catalog: &Catalog,
    items: &[BulkItem],
    mailbox_id: Uuid,
    attach_pdf: bool,
    now: DateTime<Utc>,
) -> Vec<PlannedItem> {
    let mut seen: Vec<(Uuid, RequestKind)> = Vec::new();
    items
        .iter()
        .map(|item| {
            let kind = request_kind(item.kind);
            let res = if !bulk_kind_allowed(item.kind) {
                Err("Diese Art von Brief ist für den Sammelversand nicht vorgesehen".to_string())
            } else if seen.contains(&(item.account_id, kind)) {
                Err("Dieser Account ist in der Auswahl doppelt enthalten".to_string())
            } else if already_requested(data, kind, item.account_id) {
                Err("Für diesen Account gibt es bereits einen gesendeten Antrag".to_string())
            } else {
                prepare(data, catalog, &item.args(), mailbox_id, attach_pdf, now)
                    .map_err(|e| e.message)
            };
            seen.push((item.account_id, kind));
            (item.account_id, res)
        })
        .collect()
}

/// `letters_send_check`: dry run, nothing is sent.
pub fn check(
    data: &VaultData,
    catalog: &Catalog,
    items: &[BulkItem],
    mailbox_id: Uuid,
    now: DateTime<Utc>,
) -> Value {
    let plan = plan_items(data, catalog, items, mailbox_id, false, now);
    let mut ok = Vec::new();
    let mut problems = Vec::new();
    for (account_id, res) in plan {
        match res {
            Ok((p, _)) => ok.push(json!({
                "accountId": account_id,
                "name": p.target,
                "recipientEmail": p.to,
                "subject": p.subject,
            })),
            Err(reason) => problems.push(json!({ "accountId": account_id, "reason": reason })),
        }
    }
    let mailbox = data.mailboxes.iter().find(|m| m.id == mailbox_id);
    let remaining = mailbox.map_or(0, |m| ratelimit::remaining_today(data, m, now, &Local));
    json!({
        "ok": ok,
        "problems": problems,
        "remainingToday": remaining,
        "dailyCap": mailbox.map_or(0, |m| ratelimit::effective_cap(&data.settings, m)),
        "gapSeconds": ratelimit::gap(&data.settings).as_secs(),
    })
}

pub fn start_bulk(
    state: &AppState,
    items: Vec<BulkItem>,
    mailbox_id: Uuid,
    attach_pdf: bool,
    sender: &'static Sender,
) -> IpcResult<String> {
    if items.is_empty() {
        return Err(IpcError::invalid("Keine Accounts ausgewählt"));
    }
    let st = state.clone();
    state.start_job("send", move |ctx| {
        bulk_job(ctx, &st, &items, mailbox_id, attach_pdf, sender)
    })
}

pub fn bulk_job(
    ctx: &JobCtx,
    state: &AppState,
    items: &[BulkItem],
    mailbox_id: Uuid,
    attach_pdf: bool,
    sender: &Sender,
) -> JobResult {
    let catalog = state.catalog();
    let plan =
        state.read(|d| plan_items(d, &catalog, items, mailbox_id, attach_pdf, Utc::now()))?;
    let total = plan.len() as u64;
    let (mut sent_n, mut failed_n, mut skipped_n) = (0u64, 0u64, 0u64);
    let mut cap_reached = false;
    let mut blocked: Option<String> = None;
    let mut first_error: Option<IpcError> = None;
    let mut results = Vec::new();
    for (i, (account_id, res)) in plan.into_iter().enumerate() {
        ctx.check()?;
        let done = i as u64 + 1;
        let id = account_id.to_string();
        let mut report = |ok: bool, message: String, skipped: bool, reason: Option<&str>| {
            ctx.item_reason(done, total, id.clone(), ok, message.clone(), reason);
            results.push(json!({
                "id": account_id, "ok": ok, "skipped": skipped, "message": message, "reason": reason,
            }));
        };
        let (prepared, mailbox) = match res {
            Ok(x) => x,
            Err(msg) => {
                skipped_n += 1;
                report(false, msg, true, Some("Skipped"));
                continue;
            }
        };
        if let Some(why) = &blocked {
            skipped_n += 1;
            report(false, why.clone(), true, Some("Blocked"));
            continue;
        }
        ctx.progress(done - 1, total, format!("Sende an {}", prepared.target));
        match send_prepared(state, sender, &mailbox, &prepared, &|| ctx.is_cancelled()) {
            Ok(_) => {
                sent_n += 1;
                report(true, "Gesendet".into(), false, None);
            }
            Err(SendStop::Cancelled) => return Err(JobFail::Cancelled),
            Err(SendStop::Cap { cap, .. }) => {
                cap_reached = true;
                let msg = SendStop::cap_message(cap);
                blocked = Some(msg.clone());
                skipped_n += 1;
                report(false, msg, true, Some("CapReached"));
            }
            Err(SendStop::Failed(e)) => {
                failed_n += 1;
                if e.code == ErrorCode::Auth {
                    blocked = Some(
                        "Nicht gesendet, die Anmeldung beim Postfach ist fehlgeschlagen".into(),
                    );
                }
                report(false, e.message.clone(), false, Some("SendFailed"));
                first_error.get_or_insert(e);
            }
        }
    }
    let summary = format!(
        "{sent_n} gesendet, {failed_n} fehlgeschlagen, {skipped_n} übersprungen{}",
        if cap_reached {
            ", Tageslimit erreicht"
        } else {
            ""
        }
    );
    let payload = json!({
        "sent": sent_n, "failed": failed_n, "skipped": skipped_n,
        "capReached": cap_reached, "results": results,
    });
    let outcome = match first_error {
        Some(e) => JobOutcome::failed(summary, e),
        None => JobOutcome::ok(summary),
    };
    Ok(outcome.with_payload(payload))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use ausgeloggt_core::model::Account;
    use std::sync::Mutex;

    pub struct Recorder {
        pub sent: Mutex<Vec<OutgoingMail>>,
        pub fail_for: Mutex<Vec<String>>,
        pub auth_fail: Mutex<bool>,
    }

    impl Recorder {
        pub fn leak() -> &'static Recorder {
            Box::leak(Box::new(Recorder {
                sent: Mutex::new(vec![]),
                fail_for: Mutex::new(vec![]),
                auth_fail: Mutex::new(false),
            }))
        }
        pub fn sender(&'static self) -> &'static Sender {
            Box::leak(Box::new(move |_m: &MailboxConfig, o: &OutgoingMail| {
                if *self.auth_fail.lock().unwrap() {
                    return Err(MailError::Auth);
                }
                if self.fail_for.lock().unwrap().contains(&o.to) {
                    return Err(MailError::Smtp("550 abgelehnt".into()));
                }
                self.sent.lock().unwrap().push(o.clone());
                Ok(SentMail {
                    message_id: format!("<test-{}@example.org>", Uuid::new_v4().simple()),
                    sent_at: Utc::now(),
                    appended_to: Some("Gesendet".into()),
                    append_error: None,
                })
            }))
        }
        pub fn count(&self) -> usize {
            self.sent.lock().unwrap().len()
        }
    }

    pub fn mailbox() -> MailboxConfig {
        MailboxConfig {
            id: Uuid::new_v4(),
            address: "erika@example.org".into(),
            imap_host: "imap.web.de".into(),
            imap_port: 993,
            username: "erika@example.org".into(),
            password: "geheim".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    pub fn party(email: Option<&str>) -> PartyDto {
        PartyDto {
            name: "Beispiel GmbH".into(),
            address_lines: vec!["Weg 1".into(), "10115 Berlin".into()],
            email: email.map(String::from),
        }
    }

    /// Vault with a mailbox, complete sender data, no rate gap.
    pub fn setup(
        f: &crate::state::testing::Fixture,
        accounts: usize,
    ) -> (MailboxConfig, Vec<Uuid>) {
        let mb = mailbox();
        let mut ids = Vec::new();
        f.state
            .mutate(|d| {
                d.mailboxes.push(mb.clone());
                d.settings.full_name = "Erika Müller".into();
                d.settings.identity.street = "Hauptstraße 1".into();
                d.settings.identity.postal_code = "50667".into();
                d.settings.identity.city = "Köln".into();
                d.settings.send_gap_seconds = 0;
                for i in 0..accounts {
                    let mut a = Account::new(format!("dienst{i}.example"), format!("Dienst {i}"));
                    a.identifiers.push("erika@example.org".into());
                    ids.push(a.id);
                    d.accounts.push(a);
                }
            })
            .unwrap();
        (mb, ids)
    }

    pub fn sample_request(
        kind: RequestKind,
        account: Uuid,
        status: RequestStatus,
    ) -> ausgeloggt_core::model::DataRequest {
        let now = Utc::now();
        ausgeloggt_core::model::DataRequest {
            id: Uuid::new_v4(),
            kind,
            target: "Beispiel GmbH".into(),
            domain: None,
            account_id: Some(account),
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: "datenschutz@beispiel.de".into(),
            subject: "Betreff".into(),
            body: "Text".into(),
            created_at: now,
            sent_at: (status != RequestStatus::Draft).then_some(now),
            deadline: None,
            status,
            notes: String::new(),
            message_id: None,
            mailbox_id: None,
            attachment: None,
        }
    }

    fn item(id: Uuid, email: Option<&str>) -> BulkItem {
        BulkItem {
            kind: LetterKind::Erasure,
            account_id: id,
            recipient: Some(party(email)),
        }
    }

    #[test]
    fn single_send_records_request_and_counts() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 1);
        let rec = Recorder::leak();
        let args = item(ids[0], Some("datenschutz@beispiel.de")).args();
        let st = f.state.clone();
        let sender = rec.sender();
        f.state
            .start_job("send", move |ctx| {
                send_job(ctx, &st, &args, mb.id, true, sender)
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(rec.count(), 1);
        let mail = rec.sent.lock().unwrap()[0].clone();
        assert_eq!(mail.to, "datenschutz@beispiel.de");
        assert_eq!(mail.from_name, "Erika Müller");
        assert_eq!(mail.attachments.len(), 1);
        assert!(mail.attachments[0].0.ends_with(".pdf"));
        assert!(mail.attachments[0].2.starts_with(b"%PDF"));
        f.state
            .read(|d| {
                assert_eq!(d.requests.len(), 1);
                let r = &d.requests[0];
                assert_eq!(r.status, RequestStatus::Sent);
                assert_eq!(r.kind, RequestKind::Erasure);
                assert_eq!(r.mailbox_id, Some(d.mailboxes[0].id));
                assert!(r.message_id.as_deref().unwrap().starts_with("<test-"));
                assert_eq!(
                    r.attachment.as_deref(),
                    Some(mail.attachments[0].0.as_str())
                );
                assert!(r.deadline.is_some());
                assert_eq!(r.recipient, "datenschutz@beispiel.de");
                assert_eq!(d.send_counters.len(), 1);
                assert_eq!(d.send_counters[0].count, 1);
            })
            .unwrap();
    }

    #[test]
    fn refuses_incomplete_letters_without_sending() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 1);
        let rec = Recorder::leak();
        let catalog = f.state.catalog();
        let now = Utc::now();
        // no recipient mail address
        let a = item(ids[0], None).args();
        let e = f
            .state
            .read(|d| prepare(d, &catalog, &a, mb.id, false, now))
            .unwrap()
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Invalid);
        assert!(e.message.contains("Mailadresse"));
        // missing sender data
        f.state
            .mutate(|d| d.settings.identity.street.clear())
            .unwrap();
        let a = item(ids[0], Some("a@beispiel.de")).args();
        let e = f
            .state
            .read(|d| prepare(d, &catalog, &a, mb.id, false, now))
            .unwrap()
            .unwrap_err();
        assert!(e.message.contains("Absenderadresse"), "{}", e.message);
        // unknown mailbox, implausible address
        f.state
            .mutate(|d| d.settings.identity.street = "Weg 1".into())
            .unwrap();
        assert!(f
            .state
            .read(|d| prepare(d, &catalog, &a, Uuid::new_v4(), false, now))
            .unwrap()
            .is_err());
        let bad = item(ids[0], Some("kein at")).args();
        assert!(f
            .state
            .read(|d| prepare(d, &catalog, &bad, mb.id, false, now))
            .unwrap()
            .is_err());
        assert_eq!(rec.count(), 0);
    }

    #[test]
    fn check_reports_problems_and_skips_open_requests() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 4);
        let catalog = f.state.catalog();
        // account 1 already has a sent erasure request, account 2 only a draft
        f.state
            .mutate(|d| {
                for (i, status) in [(1usize, RequestStatus::Sent), (2, RequestStatus::Draft)] {
                    d.requests
                        .push(sample_request(RequestKind::Erasure, ids[i], status));
                }
            })
            .unwrap();
        let items = vec![
            item(ids[0], Some("a@beispiel.de")),
            item(ids[1], Some("b@beispiel.de")),
            item(ids[2], Some("c@beispiel.de")),
            item(ids[3], None),
            item(ids[0], Some("a@beispiel.de")),
        ];
        let v = f
            .state
            .read(|d| check(d, &catalog, &items, mb.id, Utc::now()))
            .unwrap();
        let ok: Vec<_> = v["ok"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["accountId"].clone())
            .collect();
        assert_eq!(ok, vec![json!(ids[0]), json!(ids[2])]);
        let problems = v["problems"].as_array().unwrap();
        assert_eq!(problems.len(), 3);
        assert!(problems[0]["reason"].as_str().unwrap().contains("bereits"));
        assert!(problems[1]["reason"]
            .as_str()
            .unwrap()
            .contains("Mailadresse"));
        assert!(problems[2]["reason"].as_str().unwrap().contains("doppelt"));
        assert_eq!(v["remainingToday"], 40);
        assert_eq!(v["gapSeconds"], 0);
    }

    fn run_bulk(
        f: &crate::state::testing::Fixture,
        items: Vec<BulkItem>,
        mb: Uuid,
        sender: &'static Sender,
    ) -> Value {
        let st = f.state.clone();
        f.state
            .start_job("send", move |ctx| {
                bulk_job(ctx, &st, &items, mb, false, sender)
            })
            .unwrap();
        f.host.wait_for("job://done", 20).unwrap()
    }

    #[test]
    fn bulk_sends_each_once_and_never_twice() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 3);
        let rec = Recorder::leak();
        let items: Vec<_> = ids
            .iter()
            .map(|id| item(*id, Some("datenschutz@beispiel.de")))
            .collect();
        let done = run_bulk(&f, items.clone(), mb.id, rec.sender());
        assert_eq!(done["payload"]["sent"], 3, "{done}");
        assert_eq!(rec.count(), 3);
        let items_events = f
            .host
            .named("job://progress")
            .into_iter()
            .filter(|e| e.get("item").is_some())
            .count();
        assert_eq!(items_events, 3);
        // second run: everything is skipped, nothing goes out
        f.host.events.lock().unwrap().clear();
        let done = run_bulk(&f, items, mb.id, rec.sender());
        assert_eq!(done["payload"]["sent"], 0);
        assert_eq!(done["payload"]["skipped"], 3);
        assert_eq!(rec.count(), 3);
        f.state.read(|d| assert_eq!(d.requests.len(), 3)).unwrap();
    }

    #[test]
    fn bulk_stops_at_the_daily_cap() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 4);
        f.state.mutate(|d| d.settings.daily_send_cap = 2).unwrap();
        let rec = Recorder::leak();
        let items: Vec<_> = ids
            .iter()
            .map(|id| item(*id, Some("datenschutz@beispiel.de")))
            .collect();
        let done = run_bulk(&f, items, mb.id, rec.sender());
        assert_eq!(done["payload"]["sent"], 2);
        assert_eq!(done["payload"]["skipped"], 2);
        assert_eq!(done["payload"]["capReached"], true);
        assert_eq!(rec.count(), 2);
        let results = done["payload"]["results"].as_array().unwrap();
        assert_eq!(results[2]["reason"], "CapReached");
        assert_eq!(results[3]["reason"], "Blocked");
        // the counter is persisted in the vault
        f.state
            .read(|d| assert_eq!(d.send_counters[0].count, 2))
            .unwrap();
    }

    #[test]
    fn bulk_continues_after_a_failure_but_stops_on_auth() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 3);
        let rec = Recorder::leak();
        rec.fail_for
            .lock()
            .unwrap()
            .push("kaputt@beispiel.de".into());
        let items = vec![
            item(ids[0], Some("kaputt@beispiel.de")),
            item(ids[1], Some("ok@beispiel.de")),
        ];
        let done = run_bulk(&f, items, mb.id, rec.sender());
        assert_eq!(done["payload"]["sent"], 1);
        assert_eq!(done["payload"]["failed"], 1);
        assert_eq!(done["ok"], false);
        // a failed item leaves no request behind and can be retried
        f.state.read(|d| assert_eq!(d.requests.len(), 1)).unwrap();

        f.host.events.lock().unwrap().clear();
        *rec.auth_fail.lock().unwrap() = true;
        let items = vec![
            item(ids[0], Some("a@beispiel.de")),
            item(ids[2], Some("b@beispiel.de")),
        ];
        let done = run_bulk(&f, items, mb.id, rec.sender());
        assert_eq!(done["payload"]["failed"], 1);
        assert_eq!(done["payload"]["skipped"], 1);
    }

    #[test]
    fn bulk_is_cancellable_and_honours_the_gap() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 2);
        f.state
            .mutate(|d| d.settings.send_gap_seconds = 30)
            .unwrap();
        let rec = Recorder::leak();
        let items: Vec<_> = ids
            .iter()
            .map(|id| item(*id, Some("datenschutz@beispiel.de")))
            .collect();
        let st = f.state.clone();
        let sender = rec.sender();
        let job = f
            .state
            .start_job("send", move |ctx| {
                bulk_job(ctx, &st, &items, mb.id, false, sender)
            })
            .unwrap();
        // the first goes out, the second waits for the gap
        let end = Instant::now() + Duration::from_secs(10);
        while rec.count() < 1 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(rec.count(), 1, "second mail must wait for the gap");
        f.state.cancel_job(&job).unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["summary"], "Abgebrochen");
        assert_eq!(rec.count(), 1);
    }

    #[test]
    fn slug_and_email_checks() {
        assert_eq!(slug("Müller & Söhne GmbH"), "M_ller_S_hne_GmbH");
        assert!(plausible_email("a@b.de"));
        assert!(!plausible_email("a@b"));
        assert!(!plausible_email("a b@c.de"));
        assert!(!plausible_email("a@b.de,c@d.de"));
    }
}
