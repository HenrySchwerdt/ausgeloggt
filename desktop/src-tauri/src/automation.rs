//! Automation rules: scheduled rescans, erasure requests for queued accounts, reminders
//! for overdue requests and complaint drafts. One rule per mailbox.
//!
//! Safety rules (also covered by tests):
//! - only accounts the user queued (`queued_for_deletion`) get an erasure request, and
//!   never accounts marked "Behalten", deleted or hidden;
//! - an account with a sent request of the same kind is never contacted again;
//! - complaints are only ever prepared as drafts, never sent;
//! - everything goes through the rate limit of the mailbox;
//! - the vault must be unlocked, otherwise nothing runs (a notice is shown once).

use std::time::Duration;

use ausgeloggt_core::model::{
    Account, AccountStatus, AutomationRule, DataRequest, MailboxConfig, RequestChannel,
    RequestKind, RequestStatus, VaultData,
};
use chrono::{DateTime, Days, Local, NaiveDate, TimeZone, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{ErrorCode, IpcError, IpcResult};
use crate::jobs::{JobCtx, JobFail, JobOutcome, JobResult};
use crate::letters::{self, LetterArgs, LetterKind, NewRequest};
use crate::state::{AppState, LockExt};
use crate::work::scan::{self, Scanner};
use crate::work::send::{self, SendStop, Sender};

pub const EVERY_DAYS: [u32; 4] = [1, 7, 14, 30];
pub const CHECK_INTERVAL: Duration = Duration::from_secs(60);
const NEVER_RUN: &str = "Noch nicht ausgeführt";
const NOTHING: &str = "Nichts zu tun";

// ---- schedule math ---------------------------------------------------------------------

/// `hour:00` on `date` in `tz`. A non existent local time (spring forward) moves to the
/// next hour that exists, an ambiguous one (fall back) takes the earlier instant.
pub fn local_at<Tz: TimeZone>(tz: &Tz, date: NaiveDate, hour: u32) -> DateTime<Utc> {
    let hour = hour.min(23);
    let naive = date.and_hms_opt(hour, 0, 0).expect("valid hour");
    match tz.from_local_datetime(&naive) {
        chrono::LocalResult::Single(t) => t.with_timezone(&Utc),
        chrono::LocalResult::Ambiguous(a, b) => a.min(b).with_timezone(&Utc),
        chrono::LocalResult::None => {
            for extra in 1..=3 {
                let later = naive + chrono::Duration::hours(extra);
                if let Some(t) = tz.from_local_datetime(&later).earliest() {
                    return t.with_timezone(&Utc);
                }
            }
            tz.from_utc_datetime(&naive).with_timezone(&Utc)
        }
    }
}

/// When the rule is due next: `every_days` after the local date of the last run, at the
/// local `hour`.
pub fn next_due<Tz: TimeZone>(
    last_run: DateTime<Utc>,
    every_days: u32,
    hour: u8,
    tz: &Tz,
) -> DateTime<Utc> {
    let day = last_run.with_timezone(tz).date_naive();
    let target = day
        .checked_add_days(Days::new(u64::from(every_days.max(1))))
        .unwrap_or(day);
    local_at(tz, target, u32::from(hour))
}

/// Due = enabled, has run (or been armed) before, and the next slot has passed.
pub fn is_due<Tz: TimeZone>(rule: &AutomationRule, now: DateTime<Utc>, tz: &Tz) -> bool {
    rule.enabled
        && rule
            .last_run
            .is_some_and(|l| now >= next_due(l, rule.every_days, rule.hour, tz))
}

// ---- rules ---------------------------------------------------------------------------

/// Create or update a rule from the UI. `last_run` / `last_result` are never taken from
/// the input. A new or newly enabled rule starts counting now, so saving it never fires
/// it immediately (use "Jetzt ausführen" for that).
pub fn upsert_rule(d: &mut VaultData, input: &Value, now: DateTime<Utc>) -> IpcResult<Uuid> {
    let mut obj = input
        .as_object()
        .cloned()
        .ok_or_else(|| IpcError::invalid("Ungültige Automatik"))?;
    let has_id = obj
        .get("id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .is_some_and(|u| !u.is_nil());
    if !has_id {
        obj.insert("id".into(), json!(Uuid::new_v4()));
    }
    let mut rule: AutomationRule = serde_json::from_value(Value::Object(obj))
        .map_err(|e| IpcError::invalid(format!("Ungültige Automatik: {e}")))?;
    if !d.mailboxes.iter().any(|m| m.id == rule.mailbox_id) {
        return Err(IpcError::not_found("Postfach"));
    }
    if !EVERY_DAYS.contains(&rule.every_days) {
        return Err(IpcError::invalid(
            "Das Intervall muss 1, 7, 14 oder 30 Tage betragen",
        ));
    }
    if rule.hour > 23 {
        return Err(IpcError::invalid(
            "Die Uhrzeit muss zwischen 0 und 23 liegen",
        ));
    }
    if d.automations
        .iter()
        .any(|r| r.mailbox_id == rule.mailbox_id && r.id != rule.id)
    {
        return Err(IpcError::invalid(
            "Für dieses Postfach gibt es schon eine Automatik",
        ));
    }
    match d.automations.iter_mut().find(|r| r.id == rule.id) {
        Some(existing) => {
            rule.last_run = existing.last_run;
            rule.last_result = existing.last_result.clone();
            if rule.enabled && !existing.enabled {
                rule.last_run = Some(now);
                rule.last_result = NEVER_RUN.into();
            }
            *existing = rule.clone();
        }
        None => {
            rule.last_run = rule.enabled.then_some(now);
            rule.last_result = NEVER_RUN.into();
            d.automations.push(rule.clone());
        }
    }
    Ok(rule.id)
}

pub fn remove_rule(d: &mut VaultData, id: Uuid) -> IpcResult<()> {
    let before = d.automations.len();
    d.automations.retain(|r| r.id != id);
    if d.automations.len() == before {
        return Err(IpcError::not_found("Automatik"));
    }
    Ok(())
}

pub fn set_queued(d: &mut VaultData, ids: &[Uuid], queued: bool) -> IpcResult<()> {
    let mut found = 0;
    for a in d.accounts.iter_mut().filter(|a| ids.contains(&a.id)) {
        a.queued_for_deletion = queued;
        a.updated_at = Utc::now();
        found += 1;
    }
    if found == 0 && !ids.is_empty() {
        return Err(IpcError::not_found("Account"));
    }
    Ok(())
}

// ---- selection ---------------------------------------------------------------------------

/// Accounts that belong to a mailbox: one of their identifiers is the mailbox address; an
/// account that matches no configured mailbox at all belongs to the only mailbox, if
/// there is exactly one.
pub fn belongs_to_mailbox(d: &VaultData, account: &Account, mailbox: &MailboxConfig) -> bool {
    let mine = |addr: &str| addr.trim().eq_ignore_ascii_case(mailbox.address.trim());
    if account.identifiers.iter().any(|i| mine(i)) {
        return true;
    }
    d.mailboxes.len() == 1
        && !account.identifiers.iter().any(|i| {
            d.mailboxes
                .iter()
                .any(|m| m.address.trim().eq_ignore_ascii_case(i.trim()))
        })
}

fn automatable(status: &AccountStatus) -> bool {
    !matches!(
        status,
        AccountStatus::Keep
            | AccountStatus::Deleted
            | AccountStatus::NotAnAccount
            | AccountStatus::Ignored
    )
}

/// Queued accounts of this mailbox that still need an erasure request.
pub fn queued_candidates(d: &VaultData, mailbox: &MailboxConfig) -> Vec<Uuid> {
    d.accounts
        .iter()
        .filter(|a| a.queued_for_deletion && automatable(&a.status))
        .filter(|a| belongs_to_mailbox(d, a, mailbox))
        .filter(|a| !send::already_requested(d, RequestKind::Erasure, a.id))
        .map(|a| a.id)
        .collect()
}

fn is_overdue(r: &DataRequest, now: DateTime<Utc>) -> bool {
    r.status == RequestStatus::Sent && r.deadline.is_some_and(|dl| dl < now)
}

/// Overdue requests of this mailbox without a reminder yet.
pub fn reminder_targets(d: &VaultData, mailbox: &MailboxConfig, now: DateTime<Utc>) -> Vec<Uuid> {
    d.requests
        .iter()
        .filter(|r| r.mailbox_id == Some(mailbox.id))
        .filter(|r| !matches!(r.kind, RequestKind::Reminder | RequestKind::Complaint))
        .filter(|r| r.channel == RequestChannel::Email && is_overdue(r, now))
        .filter(|r| {
            !d.requests
                .iter()
                .any(|x| x.kind == RequestKind::Reminder && x.parent_id == Some(r.id))
        })
        .map(|r| r.id)
        .collect()
}

/// Original requests whose reminder deadline passed too, without a complaint yet.
pub fn complaint_targets(d: &VaultData, mailbox: &MailboxConfig, now: DateTime<Utc>) -> Vec<Uuid> {
    let mut out: Vec<Uuid> = Vec::new();
    for rem in d.requests.iter().filter(|r| {
        r.kind == RequestKind::Reminder && r.mailbox_id == Some(mailbox.id) && is_overdue(r, now)
    }) {
        let Some(parent_id) = rem.parent_id else {
            continue;
        };
        let parent_open = d
            .requests
            .iter()
            .any(|p| p.id == parent_id && p.status == RequestStatus::Sent);
        let has_complaint = d
            .requests
            .iter()
            .any(|c| c.kind == RequestKind::Complaint && c.parent_id == Some(parent_id));
        if parent_open && !has_complaint && !out.contains(&parent_id) {
            out.push(parent_id);
        }
    }
    out
}

// ---- running ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RuleReport {
    pub rescanned: bool,
    pub scan_failed: bool,
    pub erasures: u32,
    pub reminders: u32,
    pub drafts: u32,
    pub skipped: u32,
    pub failed: u32,
    pub cap_reached: bool,
    pub first_error: Option<IpcError>,
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_string()
    } else {
        many.replace("{n}", &n.to_string())
    }
}

impl RuleReport {
    pub fn text(&self) -> String {
        let mut parts = Vec::new();
        if self.rescanned {
            parts.push("Postfach geprüft".to_string());
        }
        if self.scan_failed {
            parts.push("Prüfung des Postfachs fehlgeschlagen".to_string());
        }
        if self.erasures > 0 {
            parts.push(plural(
                self.erasures,
                "1 Löschantrag gesendet",
                "{n} Löschanträge gesendet",
            ));
        }
        if self.reminders > 0 {
            parts.push(plural(
                self.reminders,
                "1 Erinnerung gesendet",
                "{n} Erinnerungen gesendet",
            ));
        }
        if self.drafts > 0 {
            parts.push(plural(
                self.drafts,
                "1 Beschwerdeentwurf vorbereitet",
                "{n} Beschwerdeentwürfe vorbereitet",
            ));
        }
        if self.skipped > 0 {
            parts.push(format!("{} übersprungen", self.skipped));
        }
        if self.failed > 0 {
            parts.push(format!("{} fehlgeschlagen", self.failed));
        }
        if self.cap_reached {
            parts.push("Tageslimit erreicht".to_string());
        }
        if parts.is_empty() {
            NOTHING.to_string()
        } else {
            parts.join(", ")
        }
    }

    fn did_something(&self) -> bool {
        self.erasures + self.reminders + self.drafts + self.failed > 0
            || self.cap_reached
            || self.scan_failed
    }
}

enum Step {
    Sent,
    Skipped,
    Failed,
    CapReached,
}

/// Send one letter of an automation run; errors are tallied, only cancel aborts.
fn send_step(
    ctx: &JobCtx,
    state: &AppState,
    sender: &Sender,
    mailbox_id: Uuid,
    args: &LetterArgs,
    report: &mut RuleReport,
) -> Result<Step, JobFail> {
    let catalog = state.catalog();
    let prepared =
        state.read(|d| send::prepare(d, &catalog, args, mailbox_id, true, Utc::now()))?;
    let (prepared, mailbox) = match prepared {
        Ok(x) => x,
        Err(e) => {
            report.skipped += 1;
            report.first_error.get_or_insert(e);
            return Ok(Step::Skipped);
        }
    };
    match send::send_prepared(state, sender, &mailbox, &prepared, &|| ctx.is_cancelled()) {
        Ok(_) => Ok(Step::Sent),
        Err(SendStop::Cancelled) => Err(JobFail::Cancelled),
        Err(SendStop::Cap { .. }) => {
            report.cap_reached = true;
            Ok(Step::CapReached)
        }
        Err(SendStop::Failed(e)) => {
            report.failed += 1;
            let auth = e.code == ErrorCode::Auth;
            report.first_error.get_or_insert(e);
            // Wrong credentials: every further try would fail the same way.
            if auth {
                report.cap_reached = false;
                return Ok(Step::CapReached);
            }
            Ok(Step::Failed)
        }
    }
}

/// Prepare a complaint draft for the original request `parent_id` (never sent).
fn prepare_complaint(state: &AppState, mailbox_id: Uuid, parent_id: Uuid) -> IpcResult<bool> {
    let catalog = state.catalog();
    let now = Utc::now();
    let args = LetterArgs {
        kind: LetterKind::Complaint,
        account_id: None,
        subscription_id: None,
        request_id: Some(parent_id),
        org_id: None,
        authority_state: None,
        recipient: None,
    };
    let built = state.read(|d| letters::build_letter(d, &catalog, &args, now))??;
    if !built.missing_sender_fields.is_empty() {
        return Ok(false);
    }
    let (channel, recipient) = match (&built.recipient.email, built.missing_recipient) {
        (Some(e), false) => (RequestChannel::Email, e.clone()),
        (_, false) => (
            RequestChannel::Post,
            built.recipient.address_lines.join("\n"),
        ),
        (_, true) => (RequestChannel::Post, String::new()),
    };
    let target = if built.missing_recipient {
        "Datenschutzbehörde".to_string()
    } else {
        built.recipient.name.clone()
    };
    state.try_mutate(|d| {
        let parent = d
            .requests
            .iter()
            .find(|r| r.id == parent_id)
            .ok_or_else(|| IpcError::not_found("Anfrage"))?;
        let (account_id, domain) = (parent.account_id, parent.domain.clone());
        let id = letters::create_request(
            d,
            NewRequest {
                kind: RequestKind::Complaint,
                target,
                domain,
                account_id,
                subscription_id: None,
                parent_id: Some(parent_id),
                channel,
                recipient,
                subject: built.doc.subject.clone(),
                body: built.text.clone(),
                sent: false,
                message_id: None,
                mailbox_id: Some(mailbox_id),
                attachment: None,
            },
            now,
        )?;
        if let Some(r) = d.requests.iter_mut().find(|r| r.id == id) {
            r.notes = if built.missing_recipient {
                "Von der Automatik als Entwurf vorbereitet. Bitte die zuständige Behörde wählen, prüfen und selbst absenden".into()
            } else {
                "Von der Automatik als Entwurf vorbereitet. Bitte prüfen und selbst absenden".into()
            };
        }
        Ok(())
    })?;
    Ok(true)
}

/// Run one rule. Network and sending happen without holding the vault lock.
pub fn run_rule(
    ctx: &JobCtx,
    state: &AppState,
    rule_id: Uuid,
    sender: &Sender,
    scanner: &Scanner,
) -> Result<RuleReport, JobFail> {
    let (rule, mailbox) = state.read(|d| {
        let rule = d.automations.iter().find(|r| r.id == rule_id).cloned();
        let mb = rule
            .as_ref()
            .and_then(|r| d.mailboxes.iter().find(|m| m.id == r.mailbox_id).cloned());
        (rule, mb)
    })?;
    let rule = rule.ok_or_else(|| IpcError::not_found("Automatik"))?;
    let mailbox = mailbox.ok_or_else(|| IpcError::not_found("Postfach"))?;
    let mut report = RuleReport::default();

    if rule.rescan && !state.job_running("scan") {
        ctx.progress(0, 1, "Postfach wird geprüft");
        match scan::scan_job(ctx, state, &[mailbox.id], false, scanner) {
            Ok(o) => {
                report.rescanned = true;
                if !o.ok {
                    report.scan_failed = true;
                }
            }
            Err(JobFail::Cancelled) => return Err(JobFail::Cancelled),
            Err(JobFail::Error(e)) => {
                report.scan_failed = true;
                report.first_error.get_or_insert(e);
            }
        }
    }

    let mut blocked = false;
    if rule.send_erasure_for_queue {
        let ids = state.read(|d| queued_candidates(d, &mailbox))?;
        for (i, id) in ids.iter().enumerate() {
            ctx.check()?;
            if blocked {
                report.skipped += 1;
                continue;
            }
            ctx.progress(i as u64, ids.len() as u64, "Löschanträge werden gesendet");
            let args = LetterArgs {
                kind: LetterKind::Erasure,
                account_id: Some(*id),
                subscription_id: None,
                request_id: None,
                org_id: None,
                authority_state: None,
                recipient: None,
            };
            match send_step(ctx, state, sender, mailbox.id, &args, &mut report)? {
                Step::Sent => report.erasures += 1,
                Step::CapReached => blocked = true,
                Step::Skipped | Step::Failed => {}
            }
        }
    }

    if rule.send_reminders && !blocked {
        let ids = state.read(|d| reminder_targets(d, &mailbox, Utc::now()))?;
        for (i, id) in ids.iter().enumerate() {
            ctx.check()?;
            if blocked {
                report.skipped += 1;
                continue;
            }
            ctx.progress(i as u64, ids.len() as u64, "Erinnerungen werden gesendet");
            let args = LetterArgs {
                kind: LetterKind::Reminder,
                account_id: None,
                subscription_id: None,
                request_id: Some(*id),
                org_id: None,
                authority_state: None,
                recipient: None,
            };
            match send_step(ctx, state, sender, mailbox.id, &args, &mut report)? {
                Step::Sent => report.reminders += 1,
                Step::CapReached => blocked = true,
                Step::Skipped | Step::Failed => {}
            }
        }
    }

    if rule.prepare_complaints {
        let ids = state.read(|d| complaint_targets(d, &mailbox, Utc::now()))?;
        for id in ids {
            ctx.check()?;
            match prepare_complaint(state, mailbox.id, id) {
                Ok(true) => report.drafts += 1,
                Ok(false) => report.skipped += 1,
                Err(e) => {
                    report.skipped += 1;
                    report.first_error.get_or_insert(e);
                }
            }
        }
    }
    Ok(report)
}

/// Run these rules one after the other (job kind `automation`).
pub fn start_automation(
    state: &AppState,
    rule_ids: Vec<Uuid>,
    sender: &'static Sender,
    scanner: &'static Scanner,
) -> IpcResult<String> {
    if rule_ids.is_empty() {
        return Err(IpcError::invalid("Keine Automatik ausgewählt"));
    }
    let st = state.clone();
    state.start_job("automation", move |ctx| {
        automation_job(ctx, &st, &rule_ids, sender, scanner)
    })
}

pub fn automation_job(
    ctx: &JobCtx,
    state: &AppState,
    rule_ids: &[Uuid],
    sender: &Sender,
    scanner: &Scanner,
) -> JobResult {
    let total = rule_ids.len() as u64;
    let mut runs = Vec::new();
    let mut first_error: Option<IpcError> = None;
    let mut texts = Vec::new();
    let mut notable = false;
    for (i, id) in rule_ids.iter().enumerate() {
        ctx.check()?;
        ctx.progress(i as u64, total, "Automatik läuft");
        let report = run_rule(ctx, state, *id, sender, scanner)?;
        let text = report.text();
        let at = Utc::now();
        let t = text.clone();
        state.mutate(|d| {
            if let Some(r) = d.automations.iter_mut().find(|r| r.id == *id) {
                r.last_run = Some(at);
                r.last_result = t;
            }
        })?;
        ctx.item(
            i as u64 + 1,
            total,
            id.to_string(),
            report.failed == 0,
            text.clone(),
        );
        runs.push(json!({ "ruleId": id, "summary": text, "failed": report.failed }));
        notable |= report.did_something();
        texts.push(text);
        if let Some(e) = report.first_error {
            if report.failed > 0 {
                first_error.get_or_insert(e);
            }
        }
    }
    let summary = texts.join("; ");
    if notable && state.read(|d| d.settings.notifications).unwrap_or(false) {
        state
            .host
            .notify("ausgeloggt", &format!("Automatik: {summary}"));
    }
    let outcome = match first_error {
        Some(e) => JobOutcome::failed(summary, e),
        None => JobOutcome::ok(summary),
    };
    Ok(outcome.with_payload(json!({ "runs": runs })))
}

// ---- scheduler ---------------------------------------------------------------------------

/// One scheduler pass for automations (time and zone injected).
pub fn tick<Tz: TimeZone>(
    state: &AppState,
    now: DateTime<Utc>,
    tz: &Tz,
    sender: &'static Sender,
    scanner: &'static Scanner,
) {
    if !state.is_unlocked() {
        let notify = {
            let mut h = state.automation_hint.lock_safe();
            let go = h.armed && h.notify && !h.paused_notified;
            if go {
                h.paused_notified = true;
            }
            go
        };
        if notify {
            state
                .host
                .notify("ausgeloggt", "Automatik pausiert, Tresor gesperrt");
        }
        return;
    }
    if state.job_running("send") || state.job_running("automation") {
        return;
    }
    // A rule without a last run starts counting now (never fires on its own first).
    let needs_init = state
        .read(|d| {
            d.automations
                .iter()
                .any(|r| r.enabled && r.last_run.is_none())
        })
        .unwrap_or(false);
    if needs_init {
        let _ = state.mutate(|d| {
            for r in d
                .automations
                .iter_mut()
                .filter(|r| r.enabled && r.last_run.is_none())
            {
                r.last_run = Some(now);
                r.last_result = NEVER_RUN.into();
            }
        });
    }
    let due: Vec<Uuid> = state
        .read(|d| {
            d.automations
                .iter()
                .filter(|r| is_due(r, now, tz))
                .filter(|r| d.mailboxes.iter().any(|m| m.id == r.mailbox_id))
                .map(|r| r.id)
                .collect()
        })
        .unwrap_or_default();
    if due.is_empty() {
        return;
    }
    if let Err(e) = start_automation(state, due, sender, scanner) {
        log::debug!("Automatik nicht gestartet: {}", e.message);
    }
}

/// Run every enabled rule right now (tray menu "Jetzt prüfen").
pub fn run_enabled_now(
    state: &AppState,
    sender: &'static Sender,
    scanner: &'static Scanner,
) -> IpcResult<String> {
    let ids: Vec<Uuid> = state.read(|d| {
        d.automations
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.id)
            .collect()
    })?;
    if ids.is_empty() {
        return Err(IpcError::invalid("Es ist keine Automatik aktiv"));
    }
    start_automation(state, ids, sender, scanner)
}

pub fn spawn(state: AppState) {
    let result = std::thread::Builder::new()
        .name("automation".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(10));
            loop {
                tick(
                    &state,
                    Utc::now(),
                    &Local,
                    send::real_sender(),
                    scan::imap_scanner(),
                );
                std::thread::sleep(CHECK_INTERVAL);
            }
        });
    if let Err(e) = result {
        log::error!("Automatik konnte nicht gestartet werden: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{unlocked, Fixture};
    use crate::work::send::tests::{sample_request, setup, Recorder};
    use ausgeloggt_mail::{MailError, ScanEvent, ScanOptions, ScanOutcome};
    use chrono::Duration as D;
    use chrono_tz::Tz;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    static SCANS: AtomicUsize = AtomicUsize::new(0);
    fn noop_scanner() -> &'static Scanner {
        &|_c: &MailboxConfig, _o: &ScanOptions, _e: &mut dyn FnMut(ScanEvent), _x: &AtomicBool| {
            SCANS.fetch_add(1, Ordering::SeqCst);
            Ok::<ScanOutcome, MailError>(ScanOutcome {
                findings: vec![],
                subscriptions: vec![],
                items_scanned: 0,
                progress: vec![],
                payments: vec![],
            })
        }
    }

    fn berlin() -> Tz {
        chrono_tz::Europe::Berlin
    }

    fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    fn rule(mailbox: Uuid) -> AutomationRule {
        AutomationRule {
            id: Uuid::new_v4(),
            mailbox_id: mailbox,
            enabled: true,
            every_days: 1,
            hour: 9,
            rescan: false,
            send_erasure_for_queue: true,
            send_reminders: true,
            prepare_complaints: true,
            last_run: Some(Utc::now() - D::days(2)),
            last_result: String::new(),
        }
    }

    // ---- due computation ------------------------------------------------------------

    #[test]
    fn next_due_winter_and_summer() {
        // 2026-11-12 11:00 Berlin (10:00 UTC), daily at 9: tomorrow 09:00 CET = 08:00 UTC
        assert_eq!(
            next_due(utc(2026, 11, 12, 10, 0), 1, 9, &berlin()),
            utc(2026, 11, 13, 8, 0)
        );
        assert_eq!(
            next_due(utc(2026, 11, 12, 10, 0), 1, 9, &Utc),
            utc(2026, 11, 13, 9, 0)
        );
        assert_eq!(
            next_due(utc(2026, 7, 1, 10, 0), 7, 9, &berlin()),
            utc(2026, 7, 8, 7, 0)
        );
    }

    #[test]
    fn next_due_across_spring_forward() {
        // Saturday 28 March 2026 12:00 CET -> Sunday 09:00 is already CEST (07:00 UTC)
        assert_eq!(
            next_due(utc(2026, 3, 28, 11, 0), 1, 9, &berlin()),
            utc(2026, 3, 29, 7, 0)
        );
        // weekly across the change keeps the local hour
        assert_eq!(
            next_due(utc(2026, 3, 25, 8, 0), 7, 9, &berlin()),
            utc(2026, 4, 1, 7, 0)
        );
    }

    #[test]
    fn nonexistent_local_hour_moves_forward() {
        // 02:30 does not exist on 2026-03-29 in Berlin; hour 2 -> 03:00 CEST = 01:00 UTC
        assert_eq!(
            next_due(utc(2026, 3, 28, 11, 0), 1, 2, &berlin()),
            utc(2026, 3, 29, 1, 0)
        );
    }

    #[test]
    fn ambiguous_local_hour_takes_the_first() {
        // 02:00 happens twice on 2026-10-25; the first one is CEST = 00:00 UTC
        assert_eq!(
            next_due(utc(2026, 10, 24, 11, 0), 1, 2, &berlin()),
            utc(2026, 10, 25, 0, 0)
        );
        // the day after the change: CET again
        assert_eq!(
            next_due(utc(2026, 10, 25, 11, 0), 1, 9, &berlin()),
            utc(2026, 10, 26, 8, 0)
        );
    }

    #[test]
    fn west_of_greenwich_and_midnight_rollover() {
        let ny: Tz = chrono_tz::America::New_York;
        // 23:30 UTC on the 12th is 18:30 on the 12th in New York; daily at 6 -> 13th 06:00 EST
        assert_eq!(
            next_due(utc(2026, 11, 12, 23, 30), 1, 6, &ny),
            utc(2026, 11, 13, 11, 0)
        );
        // 00:30 UTC on the 13th is still the 12th in New York
        assert_eq!(
            next_due(utc(2026, 11, 13, 0, 30), 1, 6, &ny),
            utc(2026, 11, 13, 11, 0)
        );
    }

    #[test]
    fn is_due_conditions() {
        let mb = Uuid::new_v4();
        let mut r = rule(mb);
        r.last_run = Some(utc(2026, 11, 12, 10, 0));
        r.every_days = 1;
        r.hour = 9;
        let due_at = utc(2026, 11, 13, 9, 0);
        assert!(!is_due(&r, due_at - D::seconds(1), &Utc));
        assert!(is_due(&r, due_at, &Utc));
        assert!(is_due(&r, due_at + D::days(40), &Utc), "catch up once");
        r.enabled = false;
        assert!(!is_due(&r, due_at, &Utc));
        r.enabled = true;
        r.last_run = None;
        assert!(!is_due(&r, due_at, &Utc), "never fires before it was armed");
        r.last_run = Some(utc(2026, 11, 12, 10, 0));
        r.every_days = 14;
        assert!(!is_due(&r, due_at, &Utc));
        assert!(is_due(&r, utc(2026, 11, 26, 9, 0), &Utc));
    }

    // ---- rules ---------------------------------------------------------------------

    fn rule_json(mb: Uuid) -> Value {
        json!({
            "mailbox_id": mb, "enabled": true, "every_days": 7, "hour": 8,
            "rescan": true, "send_erasure_for_queue": true, "send_reminders": true,
            "prepare_complaints": false,
            "last_run": "2000-01-01T00:00:00Z", "last_result": "gefälscht",
        })
    }

    #[test]
    fn upsert_validates_and_never_fires_immediately() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        let now = Utc::now();
        let id = f
            .state
            .try_mutate(|d| upsert_rule(d, &rule_json(mb.id), now))
            .unwrap();
        f.state
            .read(|d| {
                let r = &d.automations[0];
                assert_eq!(r.id, id);
                assert_eq!(r.last_run, Some(now), "input last_run is ignored");
                assert_eq!(r.last_result, NEVER_RUN);
                assert!(!is_due(r, now, &Utc));
            })
            .unwrap();
        // second rule for the same mailbox is refused
        assert!(f
            .state
            .try_mutate(|d| upsert_rule(d, &rule_json(mb.id), now))
            .is_err());
        // invalid values
        for (k, v) in [("every_days", json!(3)), ("hour", json!(24))] {
            let mut j = rule_json(mb.id);
            j["id"] = json!(id);
            j[k] = v;
            assert!(
                f.state.try_mutate(|d| upsert_rule(d, &j, now)).is_err(),
                "{k}"
            );
        }
        assert!(f
            .state
            .try_mutate(|d| upsert_rule(d, &rule_json(Uuid::new_v4()), now))
            .is_err());
        // update keeps the run history; re-enabling restarts the clock
        let mut j = rule_json(mb.id);
        j["id"] = json!(id);
        j["enabled"] = json!(false);
        f.state.try_mutate(|d| upsert_rule(d, &j, now)).unwrap();
        f.state
            .mutate(|d| d.automations[0].last_run = Some(now - D::days(90)))
            .unwrap();
        j["enabled"] = json!(true);
        let later = now + D::hours(1);
        f.state.try_mutate(|d| upsert_rule(d, &j, later)).unwrap();
        f.state
            .read(|d| assert_eq!(d.automations[0].last_run, Some(later)))
            .unwrap();
        f.state.try_mutate(|d| remove_rule(d, id)).unwrap();
        assert!(f.state.try_mutate(|d| remove_rule(d, id)).is_err());
    }

    // ---- selection -----------------------------------------------------------------

    #[test]
    fn queue_selection_rules() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 6);
        f.state
            .mutate(|d| {
                for id in &ids[..5] {
                    d.accounts
                        .iter_mut()
                        .find(|a| a.id == *id)
                        .unwrap()
                        .queued_for_deletion = true;
                }
                d.accounts[1].status = AccountStatus::Keep;
                d.accounts[2].identifiers = vec!["anders@example.net".into()];
                d.requests.push(sample_request(
                    RequestKind::Erasure,
                    ids[3],
                    RequestStatus::Sent,
                ));
                d.requests.push(sample_request(
                    RequestKind::Erasure,
                    ids[4],
                    RequestStatus::Draft,
                ));
            })
            .unwrap();
        let got = f.state.read(|d| queued_candidates(d, &mb)).unwrap();
        // 0 ok, 1 keep, 2 other address but only one mailbox exists -> belongs, 3 already sent,
        // 4 only a draft -> still due, 5 not queued
        assert_eq!(got, vec![ids[0], ids[2], ids[4]]);
        // with a second mailbox, an account with a foreign address no longer falls through
        let mut other = crate::work::send::tests::mailbox();
        other.address = "anders@example.net".into();
        f.state.mutate(|d| d.mailboxes.push(other.clone())).unwrap();
        let got = f.state.read(|d| queued_candidates(d, &mb)).unwrap();
        assert_eq!(got, vec![ids[0], ids[4]]);
        let got = f.state.read(|d| queued_candidates(d, &other)).unwrap();
        assert_eq!(got, vec![ids[2]]);
    }

    // ---- full runs -----------------------------------------------------------------

    fn run_now(f: &Fixture, rule_id: Uuid, rec: &'static Recorder) -> Value {
        f.host
            .events
            .lock()
            .unwrap()
            .retain(|(n, _)| n != "job://done");
        start_automation(&f.state, vec![rule_id], rec.sender(), noop_scanner()).unwrap();
        f.host.wait_for("job://done", 30).unwrap()
    }

    /// A queued account whose catalog contact has a mail address.
    fn queued_account(f: &Fixture) -> Uuid {
        let catalog = f.state.catalog();
        let (domain, name) = ["spotify.com", "netflix.com", "amazon.de", "zalando.de"]
            .iter()
            .find_map(|d| {
                catalog
                    .lookup_contact(d)
                    .filter(|c| c.email.is_some())
                    .map(|_| (d.to_string(), d.to_string()))
            })
            .expect("a catalog contact with a mail address");
        let mut a = Account::new(domain, name);
        a.identifiers.push("erika@example.org".into());
        a.queued_for_deletion = true;
        let id = a.id;
        f.state.mutate(|d| d.accounts.push(a)).unwrap();
        id
    }

    #[test]
    fn erasure_only_for_queued_accounts_and_only_once() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 2); // two plain accounts, not queued
        let queued = queued_account(&f);
        let rec = Recorder::leak();
        let r = rule(mb.id);
        let rid = r.id;
        f.state.mutate(|d| d.automations.push(r)).unwrap();
        let done = run_now(&f, rid, rec);
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(rec.count(), 1, "only the queued account is contacted");
        f.state
            .read(|d| {
                assert_eq!(d.requests.len(), 1);
                assert_eq!(d.requests[0].account_id, Some(queued));
                assert_eq!(d.requests[0].kind, RequestKind::Erasure);
                assert_eq!(d.requests[0].status, RequestStatus::Sent);
                assert!(
                    !d.accounts
                        .iter()
                        .find(|a| a.id == queued)
                        .unwrap()
                        .queued_for_deletion
                );
                for id in &ids {
                    assert!(!d.requests.iter().any(|r| r.account_id == Some(*id)));
                }
                assert!(d.automations[0]
                    .last_result
                    .contains("1 Löschantrag gesendet"));
            })
            .unwrap();
        // run again, even with the queue flag forced back on: no double sending
        f.state
            .mutate(|d| {
                d.accounts
                    .iter_mut()
                    .find(|a| a.id == queued)
                    .unwrap()
                    .queued_for_deletion = true
            })
            .unwrap();
        let done = run_now(&f, rid, rec);
        assert_eq!(rec.count(), 1);
        assert_eq!(done["summary"], NOTHING);
        // a notification summarised the first run, none for the empty one
        let notes = f.host.notes.lock().unwrap().clone();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].1.contains("Löschantrag gesendet"));
        assert!(
            !notes[0].1.contains('-') && !notes[0].1.contains('–') && !notes[0].1.contains('—')
        );
    }

    #[test]
    fn rescan_only_when_selected() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        let rec = Recorder::leak();
        let mut r = rule(mb.id);
        r.send_erasure_for_queue = false;
        r.send_reminders = false;
        r.prepare_complaints = false;
        let rid = r.id;
        f.state.mutate(|d| d.automations.push(r)).unwrap();
        let before = SCANS.load(Ordering::SeqCst);
        run_now(&f, rid, rec);
        assert_eq!(SCANS.load(Ordering::SeqCst), before);
        f.state.mutate(|d| d.automations[0].rescan = true).unwrap();
        let done = run_now(&f, rid, rec);
        assert_eq!(SCANS.load(Ordering::SeqCst), before + 1);
        assert!(done["summary"]
            .as_str()
            .unwrap()
            .contains("Postfach geprüft"));
    }

    #[test]
    fn reminder_then_complaint_draft_chain() {
        let f = unlocked();
        let (mb, ids) = setup(&f, 1);
        let rec = Recorder::leak();
        let now = Utc::now();
        let mut orig = sample_request(RequestKind::Erasure, ids[0], RequestStatus::Sent);
        orig.mailbox_id = Some(mb.id);
        orig.sent_at = Some(now - D::days(60));
        orig.deadline = Some(now - D::days(30));
        let orig_id = orig.id;
        // an overdue request from another source (no mailbox) is never touched
        let mut foreign = sample_request(RequestKind::Access, ids[0], RequestStatus::Sent);
        foreign.deadline = Some(now - D::days(30));
        let r = rule(mb.id);
        let rid = r.id;
        f.state
            .mutate(|d| {
                d.requests.extend([orig, foreign]);
                d.automations.push(r);
            })
            .unwrap();

        // 1) reminder goes out, linked to the original
        run_now(&f, rid, rec);
        assert_eq!(rec.count(), 1);
        let rem_id = f
            .state
            .read(|d| {
                let rem = d
                    .requests
                    .iter()
                    .find(|r| r.kind == RequestKind::Reminder)
                    .expect("reminder");
                assert_eq!(rem.parent_id, Some(orig_id));
                assert_eq!(rem.status, RequestStatus::Sent);
                assert_eq!(rem.mailbox_id, Some(mb.id));
                assert!(rem.deadline.unwrap() > now);
                rem.id
            })
            .unwrap();
        // 2) again: reminder deadline not passed, nothing new
        run_now(&f, rid, rec);
        assert_eq!(rec.count(), 1);
        assert_eq!(
            f.state
                .read(|d| d
                    .requests
                    .iter()
                    .filter(|r| r.kind == RequestKind::Reminder)
                    .count())
                .unwrap(),
            1
        );
        // 3) reminder deadline passes: a complaint DRAFT is prepared, nothing is sent
        f.state
            .mutate(|d| {
                d.requests
                    .iter_mut()
                    .find(|r| r.id == rem_id)
                    .unwrap()
                    .deadline = Some(now - D::days(1));
            })
            .unwrap();
        let done = run_now(&f, rid, rec);
        assert_eq!(rec.count(), 1, "complaints are never sent automatically");
        assert!(done["summary"]
            .as_str()
            .unwrap()
            .contains("Beschwerdeentwurf"));
        f.state
            .read(|d| {
                let c: Vec<_> = d
                    .requests
                    .iter()
                    .filter(|r| r.kind == RequestKind::Complaint)
                    .collect();
                assert_eq!(c.len(), 1);
                assert_eq!(c[0].status, RequestStatus::Draft);
                assert_eq!(c[0].parent_id, Some(orig_id));
                assert!(c[0].sent_at.is_none() && c[0].message_id.is_none());
                assert!(c[0].notes.contains("Entwurf"));
            })
            .unwrap();
        // 4) no second draft, no second reminder
        run_now(&f, rid, rec);
        assert_eq!(rec.count(), 1);
        f.state
            .read(|d| {
                assert_eq!(
                    d.requests
                        .iter()
                        .filter(|r| r.kind == RequestKind::Complaint)
                        .count(),
                    1
                );
                assert_eq!(
                    d.requests
                        .iter()
                        .filter(|r| r.kind == RequestKind::Reminder)
                        .count(),
                    1
                );
            })
            .unwrap();
    }

    #[test]
    fn rate_limit_applies_to_automation_runs() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        let queued = queued_account(&f);
        let rec = Recorder::leak();
        f.state.mutate(|d| d.settings.daily_send_cap = 1).unwrap();
        // fill the cap
        f.state.mutate(|d| ratelimit_record(d, mb.id)).unwrap();
        let r = rule(mb.id);
        let rid = r.id;
        f.state.mutate(|d| d.automations.push(r)).unwrap();
        let done = run_now(&f, rid, rec);
        assert_eq!(rec.count(), 0);
        assert!(done["summary"].as_str().unwrap().contains("Tageslimit"));
        // the account stays queued for the next run
        f.state
            .read(|d| {
                assert!(
                    d.accounts
                        .iter()
                        .find(|a| a.id == queued)
                        .unwrap()
                        .queued_for_deletion
                )
            })
            .unwrap();
    }

    fn ratelimit_record(d: &mut VaultData, mb: Uuid) {
        crate::ratelimit::record(d, mb, Utc::now(), &Local);
    }

    // ---- scheduler -----------------------------------------------------------------

    #[test]
    fn tick_runs_due_rules_once() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        let _queued = queued_account(&f);
        let rec = Recorder::leak();
        let now = Utc::now();
        let mut r = rule(mb.id);
        r.last_run = Some(now - D::days(3));
        let rid = r.id;
        f.state.mutate(|d| d.automations.push(r)).unwrap();
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        f.host.wait_for("job://done", 30).unwrap();
        assert_eq!(rec.count(), 1);
        // last_run moved: not due again, a second tick does nothing
        f.state
            .read(|d| assert!(d.automations[0].last_run.unwrap() >= now))
            .unwrap();
        f.host.events.lock().unwrap().clear();
        tick(
            &f.state,
            now + D::minutes(1),
            &Utc,
            rec.sender(),
            noop_scanner(),
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(f.host.named("job://done").is_empty());
        assert_eq!(rec.count(), 1);
        // next day it is due again (nothing to do, but it runs)
        tick(
            &f.state,
            now + D::days(2),
            &Utc,
            rec.sender(),
            noop_scanner(),
        );
        f.host.wait_for("job://done", 30).unwrap();
        let _ = rid;
    }

    #[test]
    fn tick_ignores_disabled_and_arms_rules_without_last_run() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        let rec = Recorder::leak();
        let now = Utc::now();
        let mut disabled = rule(mb.id);
        disabled.enabled = false;
        f.state.mutate(|d| d.automations.push(disabled)).unwrap();
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        assert!(f.host.named("job://done").is_empty());
        f.state
            .mutate(|d| {
                d.automations[0].enabled = true;
                d.automations[0].last_run = None;
            })
            .unwrap();
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(f.host.named("job://done").is_empty(), "armed, not fired");
        f.state
            .read(|d| assert_eq!(d.automations[0].last_run, Some(now)))
            .unwrap();
    }

    #[test]
    fn locked_vault_pauses_with_one_notice_and_runs_after_unlock() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        let _queued = queued_account(&f);
        let rec = Recorder::leak();
        let now = Utc::now();
        let mut r = rule(mb.id);
        r.last_run = Some(now - D::days(3));
        f.state.mutate(|d| d.automations.push(r)).unwrap();
        f.state.lock();
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        let notes = f.host.notes.lock().unwrap().clone();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert_eq!(notes[0].1, "Automatik pausiert, Tresor gesperrt");
        assert_eq!(rec.count(), 0);
        // unlock: due rule runs on the next tick
        f.state.unlock("correct horse battery", None).unwrap();
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        f.host.wait_for("job://done", 30).unwrap();
        assert_eq!(rec.count(), 1);
        // locking again announces again
        f.state.lock();
        tick(&f.state, now, &Utc, rec.sender(), noop_scanner());
        assert_eq!(f.host.notes.lock().unwrap().len(), 3);
    }

    #[test]
    fn hint_drives_tray_and_auto_lock() {
        let f = unlocked();
        let (mb, _) = setup(&f, 0);
        assert!(!f.state.hint().hide_on_close);
        f.state.mutate(|d| d.automations.push(rule(mb.id))).unwrap();
        assert!(f.state.hint().armed && f.state.hint().hide_on_close);
        f.state.mutate(|d| d.automations.clear()).unwrap();
        f.state
            .mutate(|d| d.settings.start_minimized_to_tray = true)
            .unwrap();
        assert!(!f.state.hint().armed && f.state.hint().hide_on_close);
        // auto-lock still applies unless the user opted out explicitly
        f.state
            .mutate(|d| {
                d.settings.start_minimized_to_tray = false;
                d.settings.auto_lock_minutes = 1;
                d.automations.push(rule(mb.id));
            })
            .unwrap();
        let later = std::time::Instant::now() + std::time::Duration::from_secs(120);
        assert!(f.state.auto_lock_due(later));
        f.state
            .mutate(|d| d.settings.keep_unlocked_for_automation = true)
            .unwrap();
        assert!(!f.state.auto_lock_due(later));
        f.state.mutate(|d| d.automations.clear()).unwrap();
        assert!(
            f.state.auto_lock_due(later),
            "no automation enabled, no exemption"
        );
    }
}
