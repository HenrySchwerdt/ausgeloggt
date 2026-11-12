//! Mailbox scan job. The network part (`Scanner`) is injectable for tests; the vault
//! lock is only taken to read the config before and to merge after each mailbox.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use ausgeloggt_core::merge;
use ausgeloggt_core::model::{MailboxConfig, SourceKind, SourceRun, SubscriptionStatus, VaultData};
use ausgeloggt_mail::{MailError, ScanEvent, ScanOptions, ScanOutcome};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::error::IpcError;
use crate::jobs::{JobCtx, JobFail, JobOutcome, JobResult};
use crate::state::AppState;

pub type Scanner = dyn Fn(
        &MailboxConfig,
        &ScanOptions,
        &mut dyn FnMut(ScanEvent),
        &AtomicBool,
    ) -> Result<ScanOutcome, MailError>
    + Send
    + Sync;

/// The real IMAP scanner.
pub fn imap_scanner() -> &'static Scanner {
    &ausgeloggt_mail::scan_mailbox_with
}

/// Start a scan job for these mailboxes (all of them if the list is empty).
pub fn start_scan(
    state: &AppState,
    mailbox_ids: Vec<Uuid>,
    full: bool,
    scanner: &'static Scanner,
) -> Result<String, IpcError> {
    let st = state.clone();
    state.start_job("scan", move |ctx| {
        scan_job(ctx, &st, &mailbox_ids, full, scanner)
    })
}

/// Lowercase list keys of lists the user unsubscribed from (the proof for objections).
fn watch_lists(data: &VaultData, address: &str) -> Vec<String> {
    data.subscriptions
        .iter()
        .filter(|s| s.mailbox.eq_ignore_ascii_case(address))
        .filter(|s| {
            matches!(
                s.status,
                SubscriptionStatus::Unsubscribed | SubscriptionStatus::StillReceiving
            )
        })
        .map(|s| s.list_key.to_lowercase())
        .collect()
}

fn still_receiving(data: &VaultData) -> HashSet<Uuid> {
    data.subscriptions
        .iter()
        .filter(|s| s.status == SubscriptionStatus::StillReceiving)
        .map(|s| s.id)
        .collect()
}

pub fn scan_job(
    ctx: &JobCtx,
    state: &AppState,
    mailbox_ids: &[Uuid],
    full: bool,
    scanner: &Scanner,
) -> JobResult {
    let (configs, read_bodies, notifications) = state.read(|d| {
        let configs: Vec<MailboxConfig> = d
            .mailboxes
            .iter()
            .filter(|m| mailbox_ids.is_empty() || mailbox_ids.contains(&m.id))
            .cloned()
            .collect();
        (
            configs,
            d.settings.read_receipt_bodies,
            d.settings.notifications,
        )
    })?;
    if configs.is_empty() {
        return Err(IpcError::invalid("Kein Postfach zum Scannen ausgewählt").into());
    }
    let total = configs.len() as u64;
    let before_still = state.read(still_receiving)?;

    let mut new_accounts = 0u64;
    let mut findings = 0u64;
    let mut subs_new = 0u64;
    let mut payments = 0u64;
    let mut mails = 0u64;
    let mut first_error: Option<IpcError> = None;
    let mut failed = 0u64;

    for (i, cfg) in configs.iter().enumerate() {
        ctx.check()?;
        let done = i as u64;
        ctx.progress(done, total, format!("{}: Verbindung", cfg.address));
        let started = Utc::now();
        let options = ScanOptions {
            full_rescan: full,
            watch_lists: state.read(|d| watch_lists(d, &cfg.address))?,
            read_receipt_bodies: read_bodies,
        };
        let mut on_event = |ev: ScanEvent| match ev {
            ScanEvent::Connected => {
                ctx.progress(done, total, format!("{}: verbunden", cfg.address))
            }
            ScanEvent::FolderStarted { folder, total: n } => ctx.progress(
                done,
                total,
                format!("{}: {folder} ({n} Mails)", cfg.address),
            ),
            ScanEvent::Progress {
                folder,
                done: d,
                total: n,
                ..
            } => ctx.progress(done, total, format!("{}: {folder} {d}/{n}", cfg.address)),
            ScanEvent::FolderFinished { .. } => {}
        };
        match scanner(cfg, &options, &mut on_event, &ctx.cancel) {
            Ok(outcome) => {
                let summary = state.mutate(|d| merge_outcome(d, cfg, outcome, started))?;
                new_accounts += summary.new_accounts;
                findings += summary.findings;
                subs_new += summary.subscriptions_new;
                payments += summary.payments;
                mails += summary.items;
                ctx.item(
                    done + 1,
                    total,
                    cfg.id.to_string(),
                    true,
                    format!("{}: {} Mails gelesen", cfg.address, summary.items),
                );
            }
            Err(MailError::Cancelled) => return Err(JobFail::Cancelled),
            Err(e) => {
                failed += 1;
                let err = IpcError::from(e);
                let message = err.message.clone();
                // Record the failed run (no secrets in the message).
                let _ = state.mutate(|d| {
                    d.runs.push(SourceRun {
                        id: Uuid::new_v4(),
                        source: SourceKind::Mail {
                            mailbox: cfg.address.clone(),
                        },
                        started_at: started,
                        finished_at: Some(Utc::now()),
                        items_scanned: 0,
                        findings: 0,
                        new_accounts: 0,
                        error: Some(message.clone()),
                    });
                });
                ctx.item(done + 1, total, cfg.id.to_string(), false, message);
                first_error.get_or_insert(err);
            }
        }
    }

    // Notifications: new accounts, mails despite unsubscribe.
    if notifications {
        if new_accounts > 0 {
            state.host.notify(
                "ausgeloggt",
                &format!("{new_accounts} neue Accounts gefunden"),
            );
        }
        let after_still = state.read(still_receiving)?;
        let newly = after_still.difference(&before_still).count();
        if newly > 0 {
            state.host.notify(
                "ausgeloggt",
                &format!("{newly} abgemeldete Newsletter kommen trotzdem weiter"),
            );
        }
    }

    let payload = json!({
        "mailboxes": total, "failed": failed, "mails": mails, "findings": findings,
        "newAccounts": new_accounts, "newSubscriptions": subs_new, "payments": payments,
    });
    let summary = format!(
        "{} von {} Postfächern gescannt, {} Mails, {} neue Accounts",
        total - failed,
        total,
        mails,
        new_accounts
    );
    match first_error {
        None => Ok(JobOutcome::ok(summary).with_payload(payload)),
        Some(e) => {
            let mut o = JobOutcome::failed(summary, e);
            o.payload = Some(payload);
            Ok(o)
        }
    }
}

struct MergeSummary {
    items: u64,
    findings: u64,
    new_accounts: u64,
    subscriptions_new: u64,
    payments: u64,
}

/// Fold one mailbox's scan result into the vault (called under the vault lock).
fn merge_outcome(
    d: &mut VaultData,
    cfg: &MailboxConfig,
    outcome: ScanOutcome,
    started: chrono::DateTime<Utc>,
) -> MergeSummary {
    let report = merge::merge_findings(d, outcome.findings);
    let (subs_new, _subs_updated) = merge::merge_subscriptions(d, outcome.subscriptions);
    let (payments_added, payment_accounts_new) = merge::merge_payments(d, outcome.payments);
    let new_accounts = report.new_accounts + payment_accounts_new;
    let now = Utc::now();
    // The mailbox may have been removed or edited while scanning.
    if let Some(mb) = d.mailboxes.iter_mut().find(|m| m.id == cfg.id) {
        mb.progress = outcome.progress;
        mb.last_scan = Some(now);
    }
    d.runs.push(SourceRun {
        id: Uuid::new_v4(),
        source: SourceKind::Mail {
            mailbox: cfg.address.clone(),
        },
        started_at: started,
        finished_at: Some(now),
        items_scanned: outcome.items_scanned,
        findings: report.findings,
        new_accounts,
        error: None,
    });
    MergeSummary {
        items: outcome.items_scanned,
        findings: report.findings,
        new_accounts,
        subscriptions_new: subs_new,
        payments: payments_added,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use ausgeloggt_core::model::{
        Evidence, EvidenceKind, Finding, FolderProgress, MailCategory, Subscription,
        SubscriptionObservation, UnsubscribeAttempt, UnsubscribeMethod,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn mailbox(address: &str) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::new_v4(),
            address: address.into(),
            imap_host: "imap.example.org".into(),
            imap_port: 993,
            username: address.into(),
            password: "pw".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    fn finding(domain: &str) -> Finding {
        Finding {
            domain: domain.into(),
            name: None,
            identifier: Some("me@example.org".into()),
            sso: None,
            evidence: Evidence {
                kind: EvidenceKind::Welcome,
                source: SourceKind::Mail {
                    mailbox: "me@example.org".into(),
                },
                summary: "Willkommen".into(),
                date: Some(Utc::now()),
                source_ref: Some(format!("<{domain}@x>")),
            },
        }
    }

    fn observation(list_key: &str) -> SubscriptionObservation {
        SubscriptionObservation {
            mailbox: "me@example.org".into(),
            list_key: list_key.into(),
            sender: list_key.into(),
            sender_name: Some("News".into()),
            domain: "shop.example".into(),
            unsubscribe: vec![],
            message_count: 3,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category: MailCategory::Marketing,
            watched_mails: vec![],
        }
    }

    fn unsubscribed_sub(mailbox: &str) -> Subscription {
        Subscription {
            id: Uuid::new_v4(),
            mailbox: mailbox.into(),
            list_key: "News@Shop.Example".into(),
            domain: "shop.example".into(),
            name: "Shop".into(),
            sender: "news@shop.example".into(),
            unsubscribe: vec![],
            message_count: 1,
            frequency_per_month: 0.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category: MailCategory::Marketing,
            category_source: Default::default(),
            status: SubscriptionStatus::Unsubscribed,
            history: vec![UnsubscribeAttempt {
                at: Utc::now() - chrono::Duration::days(30),
                method: UnsubscribeMethod::Link {
                    url: "https://x".into(),
                },
                ok: true,
                message: "ok".into(),
            }],
            post_unsubscribe_mails: vec![],
        }
    }

    /// Scanner stub returning one account + one list, recording the options it got.
    fn fake(
        seen: &'static Mutex<Vec<ScanOptions>>,
        calls: &'static AtomicUsize,
    ) -> impl Fn(
        &MailboxConfig,
        &ScanOptions,
        &mut dyn FnMut(ScanEvent),
        &AtomicBool,
    ) -> Result<ScanOutcome, MailError>
           + Send
           + Sync {
        move |_cfg, opts, on_event, _cancel| {
            calls.fetch_add(1, Ordering::SeqCst);
            seen.lock().unwrap().push(opts.clone());
            on_event(ScanEvent::Connected);
            on_event(ScanEvent::FolderStarted {
                folder: "INBOX".into(),
                total: 2,
            });
            Ok(ScanOutcome {
                findings: vec![finding("spotify.com")],
                subscriptions: vec![observation("news@shop.example")],
                items_scanned: 2,
                progress: vec![FolderProgress {
                    folder: "INBOX".into(),
                    uid_validity: 7,
                    last_uid: 99,
                }],
                payments: vec![],
            })
        }
    }

    #[test]
    fn scan_merges_everything_and_records_run() {
        static SEEN: Mutex<Vec<ScanOptions>> = Mutex::new(Vec::new());
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let mb = mailbox("me@example.org");
        let mb_id = mb.id;
        f.state
            .mutate(|d| {
                d.mailboxes.push(mb);
                d.settings.read_receipt_bodies = true;
                d.subscriptions.push(unsubscribed_sub("ME@example.org"));
            })
            .unwrap();
        let scanner = Box::leak(Box::new(fake(&SEEN, &CALLS)));
        let st = f.state.clone();
        f.state
            .start_job("scan", move |ctx| {
                scan_job(ctx, &st, &[mb_id], true, &*scanner)
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["ok"], true, "{done}");
        assert_eq!(done["payload"]["newAccounts"], 1);

        // options passed through
        let opts = SEEN.lock().unwrap()[0].clone();
        assert!(opts.full_rescan && opts.read_receipt_bodies);
        assert_eq!(opts.watch_lists, vec!["news@shop.example"]);

        f.state
            .read(|d| {
                assert_eq!(d.accounts.len(), 1);
                assert_eq!(d.subscriptions.len(), 1, "merged into the existing list");
                let m = &d.mailboxes[0];
                assert_eq!(m.progress[0].last_uid, 99);
                assert!(m.last_scan.is_some());
                assert_eq!(d.runs.len(), 1);
                assert_eq!(d.runs[0].items_scanned, 2);
                assert_eq!(d.runs[0].new_accounts, 1);
                assert!(d.runs[0].error.is_none());
            })
            .unwrap();
        let progress = f.host.named("job://progress");
        assert!(progress
            .iter()
            .any(|p| p["message"].as_str().unwrap().contains("INBOX")));
        assert!(progress.iter().any(|p| p["item"]["ok"] == true));
        let notes = f.host.notes.lock().unwrap();
        assert!(notes.iter().any(|(_, b)| b.contains("neue Accounts")));
    }

    #[test]
    fn failing_mailbox_is_reported_others_continue() {
        let f = unlocked();
        let good = mailbox("good@example.org");
        let bad = mailbox("bad@example.org");
        f.state
            .mutate(|d| {
                d.mailboxes.push(bad);
                d.mailboxes.push(good);
            })
            .unwrap();
        let st = f.state.clone();
        let scanner = |cfg: &MailboxConfig,
                       _o: &ScanOptions,
                       _e: &mut dyn FnMut(ScanEvent),
                       _c: &AtomicBool| {
            if cfg.address.starts_with("bad") {
                Err(MailError::Auth)
            } else {
                Ok(ScanOutcome {
                    findings: vec![finding("netflix.com")],
                    subscriptions: vec![],
                    items_scanned: 1,
                    progress: vec![],
                    payments: vec![],
                })
            }
        };
        let st2 = st.clone();
        let outcome = {
            // run synchronously through a job so ctx exists
            f.state
                .start_job("scan", move |ctx| scan_job(ctx, &st2, &[], false, &scanner))
                .unwrap();
            f.host.wait_for("job://done", 10).unwrap()
        };
        assert_eq!(outcome["ok"], false);
        assert_eq!(outcome["error"]["code"], "Auth");
        assert_eq!(outcome["payload"]["failed"], 1);
        st.read(|d| {
            assert_eq!(d.accounts.len(), 1);
            assert_eq!(d.runs.len(), 2);
            assert!(d.runs.iter().any(|r| r.error.is_some()));
            assert!(d.runs.iter().all(|r| !format!("{:?}", r).contains("pw")));
        })
        .unwrap();
    }

    #[test]
    fn cancelled_scan_merges_nothing() {
        let f = unlocked();
        let mb = mailbox("me@example.org");
        f.state.mutate(|d| d.mailboxes.push(mb)).unwrap();
        let st = f.state.clone();
        let scanner = |_c: &MailboxConfig,
                       _o: &ScanOptions,
                       _e: &mut dyn FnMut(ScanEvent),
                       _cancel: &AtomicBool| Err(MailError::Cancelled);
        f.state
            .start_job("scan", move |ctx| scan_job(ctx, &st, &[], false, &scanner))
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["summary"], "Abgebrochen");
        f.state.read(|d| assert!(d.runs.is_empty())).unwrap();
    }

    #[test]
    fn no_mailbox_is_an_error() {
        let f = unlocked();
        let st = f.state.clone();
        f.state
            .start_job("scan", move |ctx| {
                scan_job(ctx, &st, &[], false, imap_scanner())
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["error"]["code"], "Invalid");
    }
}
