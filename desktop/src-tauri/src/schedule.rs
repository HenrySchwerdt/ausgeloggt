//! Periodic housekeeping: auto-lock, auto-rescan and desktop notifications.
//! `tick` is a plain function (time injected) so it is testable; `spawn` runs it every
//! 20 seconds on a background thread.

use std::time::{Duration, Instant};

use ausgeloggt_core::insights;
use ausgeloggt_core::model::VaultData;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::state::{AppState, LockExt};
use crate::work::scan::{self, Scanner};

/// After a failed automatic rescan, wait this long before trying the mailbox again.
const RESCAN_BACKOFF_HOURS: i64 = 1;
const RENEWAL_WINDOW_DAYS: i64 = 3;
pub const TICK_INTERVAL: Duration = Duration::from_secs(20);

pub fn spawn(state: AppState) {
    let result = std::thread::Builder::new()
        .name("scheduler".into())
        .spawn(move || loop {
            std::thread::sleep(TICK_INTERVAL);
            tick(&state, Instant::now(), Utc::now(), scan::imap_scanner());
        });
    if let Err(e) = result {
        log::error!("Zeitplaner konnte nicht gestartet werden: {e}");
    }
}

/// One scheduler pass. Returns early after locking.
pub fn tick(state: &AppState, now_i: Instant, now: DateTime<Utc>, scanner: &'static Scanner) {
    if state.tick_auto_lock(now_i) || !state.is_unlocked() {
        return;
    }
    auto_rescan(state, now, scanner);
    notify_due(state, now);
}

fn due_mailboxes(state: &AppState, now: DateTime<Utc>) -> Vec<Uuid> {
    let days = state
        .read(|d| i64::from(d.settings.auto_rescan_days))
        .unwrap_or(0);
    if days == 0 {
        return Vec::new();
    }
    let attempts = state.rescan_attempts.lock_safe().clone();
    state
        .read(|d| {
            d.mailboxes
                .iter()
                // Never scanned mailboxes are left to the user's first manual scan.
                .filter(|m| {
                    m.last_scan
                        .is_some_and(|t| now - t >= ChronoDuration::days(days))
                })
                .filter(|m| {
                    attempts
                        .get(&m.id)
                        .is_none_or(|t| now - *t >= ChronoDuration::hours(RESCAN_BACKOFF_HOURS))
                })
                .map(|m| m.id)
                .collect()
        })
        .unwrap_or_default()
}

fn auto_rescan(state: &AppState, now: DateTime<Utc>, scanner: &'static Scanner) {
    if state.job_running("scan") {
        return;
    }
    let due = due_mailboxes(state, now);
    if due.is_empty() {
        return;
    }
    {
        let mut attempts = state.rescan_attempts.lock_safe();
        for id in &due {
            attempts.insert(*id, now);
        }
    }
    state.emit("schedule://rescan", json!({ "mailboxIds": due }));
    if let Err(e) = scan::start_scan(state, due, false, scanner) {
        log::debug!("Automatischer Scan nicht gestartet: {}", e.message);
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        one.to_string()
    } else {
        many.replace("{n}", &n.to_string())
    }
}

/// Notify about overdue deadlines and renewals within 3 days, each item once per session.
fn notify_due(state: &AppState, now: DateTime<Utc>) {
    let Ok((enabled, overdue, renewals)) = state.read(|d| {
        (
            d.settings.notifications,
            insights::overdue_requests(d, now),
            upcoming_renewals(d, now),
        )
    }) else {
        return;
    };
    if !enabled {
        return;
    }
    let new_overdue: Vec<String> = {
        let mut seen = state.notified.lock_safe();
        overdue
            .iter()
            .map(|id| format!("overdue:{id}"))
            .filter(|k| seen.insert(k.clone()))
            .collect()
    };
    if !new_overdue.is_empty() {
        state.host.notify(
            "ausgeloggt",
            &plural(
                new_overdue.len(),
                "Eine Anfrage ist überfällig. Zeit für eine Erinnerung",
                "{n} Anfragen sind überfällig. Zeit für eine Erinnerung",
            ),
        );
    }
    let new_renewals: Vec<String> = {
        let mut seen = state.notified.lock_safe();
        renewals
            .into_iter()
            .filter(|k| seen.insert(k.clone()))
            .collect()
    };
    if !new_renewals.is_empty() {
        state.host.notify(
            "ausgeloggt",
            &plural(
                new_renewals.len(),
                "Ein Abo verlängert sich in den nächsten 3 Tagen",
                "{n} Abos verlängern sich in den nächsten 3 Tagen",
            ),
        );
    }
}

/// Dedup keys (`renew:<account>:<date>`) of subscriptions renewing within 3 days.
fn upcoming_renewals(d: &VaultData, now: DateTime<Utc>) -> Vec<String> {
    let horizon = now + ChronoDuration::days(RENEWAL_WINDOW_DAYS);
    d.accounts
        .iter()
        .filter_map(|a| insights::billing_summary(a, now).map(|b| (a.id, b)))
        .filter(|(_, b)| b.active_subscription)
        .filter_map(|(id, b)| {
            let next = b.next_charge_estimate?;
            (next > now && next <= horizon)
                .then(|| format!("renew:{id}:{}", next.format("%Y-%m-%d")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use ausgeloggt_core::model::{
        Account, DataRequest, MailboxConfig, PaymentKind, PaymentRecord, RequestChannel,
        RequestKind, RequestStatus,
    };
    use ausgeloggt_mail::{MailError, ScanEvent, ScanOptions, ScanOutcome};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn overdue_request(now: DateTime<Utc>) -> DataRequest {
        DataRequest {
            id: Uuid::new_v4(),
            kind: RequestKind::Erasure,
            target: "X".into(),
            domain: None,
            account_id: None,
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: "a@b.de".into(),
            subject: "s".into(),
            body: "b".into(),
            created_at: now - ChronoDuration::days(40),
            sent_at: Some(now - ChronoDuration::days(40)),
            deadline: Some(now - ChronoDuration::days(9)),
            status: RequestStatus::Sent,
            notes: String::new(),
            message_id: None,
            mailbox_id: None,
            attachment: None,
        }
    }

    static NOOP_CALLS: AtomicUsize = AtomicUsize::new(0);
    fn noop_scanner() -> &'static Scanner {
        &|_c: &MailboxConfig, _o: &ScanOptions, _e: &mut dyn FnMut(ScanEvent), _x: &AtomicBool| {
            NOOP_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok::<ScanOutcome, MailError>(ScanOutcome {
                findings: vec![],
                subscriptions: vec![],
                items_scanned: 0,
                progress: vec![],
                payments: vec![],
            })
        }
    }

    fn mailbox(last_scan: Option<DateTime<Utc>>) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::new_v4(),
            address: "me@example.org".into(),
            imap_host: "h".into(),
            imap_port: 993,
            username: "u".into(),
            password: "p".into(),
            folders: vec![],
            progress: vec![],
            last_scan,
        }
    }

    #[test]
    fn overdue_notification_fires_once_per_request() {
        let f = unlocked();
        let now = Utc::now();
        f.state
            .mutate(|d| {
                d.requests.push(overdue_request(now));
                d.settings.auto_rescan_days = 0;
            })
            .unwrap();
        tick(&f.state, Instant::now(), now, noop_scanner());
        tick(&f.state, Instant::now(), now, noop_scanner());
        let notes = f.host.notes.lock().unwrap().clone();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].1.contains("überfällig"));
        assert!(!notes[0].1.contains('-') && !notes[0].1.contains('–'));
        // a second overdue request notifies again, plural form
        f.state
            .mutate(|d| d.requests.push(overdue_request(now)))
            .unwrap();
        f.state
            .mutate(|d| d.requests.push(overdue_request(now)))
            .unwrap();
        tick(&f.state, Instant::now(), now, noop_scanner());
        let notes = f.host.notes.lock().unwrap().clone();
        assert_eq!(notes.len(), 2);
        assert!(notes[1].1.starts_with("2 Anfragen"));
    }

    #[test]
    fn notifications_respect_the_setting() {
        let f = unlocked();
        let now = Utc::now();
        f.state
            .mutate(|d| {
                d.requests.push(overdue_request(now));
                d.settings.notifications = false;
                d.settings.auto_rescan_days = 0;
            })
            .unwrap();
        tick(&f.state, Instant::now(), now, noop_scanner());
        assert!(f.host.notes.lock().unwrap().is_empty());
    }

    #[test]
    fn renewal_within_three_days_is_announced() {
        let f = unlocked();
        let now = Utc::now();
        let mut acct = Account::new("netflix.com", "Netflix");
        for months in [3, 2, 1] {
            // monthly charges; the last one 28 days ago -> next in ~2 days
            let date = now - ChronoDuration::days(28 * (months - 1) + 28);
            acct.payments.push(PaymentRecord {
                date: Some(date),
                kind: PaymentKind::Renewal,
                amount_minor: Some(1299),
                currency: Some("EUR".into()),
                plan: Some("Standard".into()),
                method: None,
                via: None,
                summary: "Rechnung".into(),
                source_ref: Some(format!("<{months}@x>")),
            });
        }
        f.state
            .mutate(|d| {
                d.accounts.push(acct);
                d.settings.auto_rescan_days = 0;
            })
            .unwrap();
        let soon = f.state.read(|d| upcoming_renewals(d, now)).unwrap();
        assert_eq!(soon.len(), 1, "fixture must renew within 3 days");
        tick(&f.state, Instant::now(), now, noop_scanner());
        let notes = f.host.notes.lock().unwrap().clone();
        assert!(notes.iter().any(|(_, b)| b.contains("Abo verlängert sich")));
        tick(&f.state, Instant::now(), now, noop_scanner());
        assert_eq!(f.host.notes.lock().unwrap().len(), notes.len());
    }

    #[test]
    fn auto_rescan_starts_for_due_mailboxes_only() {
        let f = unlocked();
        let now = Utc::now();
        let old = mailbox(Some(now - ChronoDuration::days(8)));
        let fresh = mailbox(Some(now - ChronoDuration::days(1)));
        let never = mailbox(None);
        let old_id = old.id;
        f.state
            .mutate(|d| {
                d.settings.auto_rescan_days = 7;
                d.mailboxes.extend([old, fresh, never]);
            })
            .unwrap();
        tick(&f.state, Instant::now(), now, noop_scanner());
        let ev = f.host.named("schedule://rescan");
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0]["mailboxIds"], json!([old_id]));
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["kind"], "scan");
        // the scan updated last_scan, so nothing is due any more
        f.state
            .read(|d| {
                assert!(
                    d.mailboxes
                        .iter()
                        .find(|m| m.id == old_id)
                        .unwrap()
                        .last_scan
                        .unwrap()
                        > now - ChronoDuration::minutes(1)
                )
            })
            .unwrap();
    }

    #[test]
    fn auto_rescan_off_or_backing_off() {
        let f = unlocked();
        let now = Utc::now();
        let old = mailbox(Some(now - ChronoDuration::days(30)));
        let id = old.id;
        f.state
            .mutate(|d| {
                d.settings.auto_rescan_days = 0;
                d.mailboxes.push(old);
            })
            .unwrap();
        tick(&f.state, Instant::now(), now, noop_scanner());
        assert!(f.host.named("schedule://rescan").is_empty());
        // enabled, but an attempt just happened: backoff
        f.state.mutate(|d| d.settings.auto_rescan_days = 7).unwrap();
        f.state
            .rescan_attempts
            .lock_safe()
            .insert(id, now - ChronoDuration::minutes(10));
        tick(&f.state, Instant::now(), now, noop_scanner());
        assert!(f.host.named("schedule://rescan").is_empty());
        // after the backoff it tries again
        tick(
            &f.state,
            Instant::now(),
            now + ChronoDuration::hours(2),
            noop_scanner(),
        );
        assert_eq!(f.host.named("schedule://rescan").len(), 1);
        f.host.wait_for("job://done", 10).unwrap();
    }

    #[test]
    fn tick_locks_after_idle_timeout_and_then_stops() {
        let f = unlocked();
        f.state
            .mutate(|d| d.settings.auto_lock_minutes = 1)
            .unwrap();
        let later = Instant::now() + Duration::from_secs(120);
        tick(&f.state, later, Utc::now(), noop_scanner());
        assert!(!f.state.is_unlocked());
        // locked: ticking again is a harmless no-op
        tick(&f.state, later, Utc::now(), noop_scanner());
    }
}
