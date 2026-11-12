//! Unsubscribe job: only automatic methods (RFC 8058 one-click POST, mailto) are executed.
//! Link-only and transactional lists are reported, never executed.

use ausgeloggt_core::model::{
    MailCategory, MailboxConfig, Subscription, SubscriptionStatus, UnsubscribeAttempt,
};
use ausgeloggt_mail::unsubscribe::{self, UnsubscribeError};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::error::IpcError;
use crate::jobs::{JobCtx, JobOutcome, JobResult};
use crate::state::AppState;

pub type Unsubscriber =
    dyn Fn(&Subscription, &MailboxConfig) -> Result<String, UnsubscribeError> + Send + Sync;

pub fn real_unsubscriber() -> &'static Unsubscriber {
    &unsubscribe::unsubscribe
}

pub fn start_unsubscribe(
    state: &AppState,
    ids: Vec<Uuid>,
    run: &'static Unsubscriber,
) -> Result<String, IpcError> {
    if ids.is_empty() {
        return Err(IpcError::invalid("Keine Newsletter ausgewählt"));
    }
    let st = state.clone();
    state.start_job("unsubscribe", move |ctx| {
        unsubscribe_job(ctx, &st, &ids, run)
    })
}

enum Plan {
    Skip(String),
    Run(Box<Subscription>, Box<MailboxConfig>),
}

fn plan(state: &AppState, id: Uuid) -> Result<Plan, IpcError> {
    state.read(|d| {
        let Some(sub) = d.subscriptions.iter().find(|s| s.id == id) else {
            return Plan::Skip("Newsletter nicht gefunden".into());
        };
        if sub.category == MailCategory::Transactional {
            return Plan::Skip(
                "Wichtige Mails (Rechnungen, Konto Hinweise) werden nicht automatisch abbestellt"
                    .into(),
            );
        }
        if unsubscribe::best_automatic(sub).is_none() {
            return Plan::Skip(
                "Kein automatischer Weg vorhanden, bitte den Link im Browser öffnen".into(),
            );
        }
        match d
            .mailboxes
            .iter()
            .find(|m| m.address.eq_ignore_ascii_case(&sub.mailbox))
        {
            Some(mb) => Plan::Run(Box::new(sub.clone()), Box::new(mb.clone())),
            None => Plan::Skip("Das zugehörige Postfach ist nicht mehr eingerichtet".into()),
        }
    })
}

pub fn unsubscribe_job(
    ctx: &JobCtx,
    state: &AppState,
    ids: &[Uuid],
    run: &Unsubscriber,
) -> JobResult {
    let total = ids.len() as u64;
    let (mut ok_n, mut failed_n, mut skipped_n) = (0u64, 0u64, 0u64);
    let mut results = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        ctx.check()?;
        let done = i as u64 + 1;
        match plan(state, *id)? {
            Plan::Skip(reason) => {
                skipped_n += 1;
                ctx.item(done, total, id.to_string(), false, reason.clone());
                results.push(json!({ "id": id, "ok": false, "skipped": true, "message": reason }));
            }
            Plan::Run(sub, mailbox) => {
                ctx.progress(done - 1, total, format!("Melde ab: {}", sub.name));
                // Network without the vault lock.
                let result = run(&sub, &mailbox);
                let (ok, message) = match result {
                    Ok(m) => (true, m),
                    Err(e) => (false, IpcError::from(e).message),
                };
                let method = unsubscribe::best_automatic(&sub).cloned();
                let sub_id = *id;
                let msg = message.clone();
                state.mutate(|d| {
                    if let (Some(s), Some(method)) =
                        (d.subscriptions.iter_mut().find(|s| s.id == sub_id), method)
                    {
                        s.history.push(UnsubscribeAttempt {
                            at: Utc::now(),
                            method,
                            ok,
                            message: msg,
                        });
                        s.status = if ok {
                            SubscriptionStatus::Unsubscribed
                        } else {
                            SubscriptionStatus::Failed
                        };
                    }
                })?;
                if ok {
                    ok_n += 1;
                } else {
                    failed_n += 1;
                }
                ctx.item(done, total, id.to_string(), ok, message.clone());
                results.push(json!({ "id": id, "ok": ok, "skipped": false, "message": message }));
            }
        }
    }
    let summary = format!("{ok_n} abgemeldet, {failed_n} fehlgeschlagen, {skipped_n} übersprungen");
    let payload =
        json!({ "ok": ok_n, "failed": failed_n, "skipped": skipped_n, "results": results });
    Ok(JobOutcome::ok(summary).with_payload(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use ausgeloggt_core::model::UnsubscribeMethod;

    fn sub(
        mailbox: &str,
        name: &str,
        category: MailCategory,
        methods: Vec<UnsubscribeMethod>,
    ) -> Subscription {
        Subscription {
            id: Uuid::new_v4(),
            mailbox: mailbox.into(),
            list_key: format!("{name}@x.example"),
            domain: "x.example".into(),
            name: name.into(),
            sender: format!("{name}@x.example"),
            unsubscribe: methods,
            message_count: 5,
            frequency_per_month: 4.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category,
            category_source: Default::default(),
            status: SubscriptionStatus::Active,
            history: vec![],
            post_unsubscribe_mails: vec![],
        }
    }

    fn one_click() -> Vec<UnsubscribeMethod> {
        vec![UnsubscribeMethod::OneClick {
            url: "https://x.example/u".into(),
        }]
    }

    #[test]
    fn executes_only_automatic_methods_and_records_attempts() {
        let f = unlocked();
        let mb = MailboxConfig {
            id: Uuid::new_v4(),
            address: "me@example.org".into(),
            imap_host: "h".into(),
            imap_port: 993,
            username: "u".into(),
            password: "p".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        };
        let auto = sub(
            "ME@example.org",
            "auto",
            MailCategory::Marketing,
            one_click(),
        );
        let link = sub(
            "me@example.org",
            "link",
            MailCategory::Marketing,
            vec![UnsubscribeMethod::Link {
                url: "https://x.example/l".into(),
            }],
        );
        let trans = sub(
            "me@example.org",
            "trans",
            MailCategory::Transactional,
            one_click(),
        );
        let failing = sub(
            "me@example.org",
            "fail",
            MailCategory::Marketing,
            one_click(),
        );
        let orphan = sub(
            "other@example.org",
            "orphan",
            MailCategory::Marketing,
            one_click(),
        );
        let ids = vec![auto.id, link.id, trans.id, failing.id, orphan.id];
        f.state
            .mutate(|d| {
                d.mailboxes.push(mb);
                d.subscriptions.extend([auto, link, trans, failing, orphan]);
            })
            .unwrap();
        let calls = std::sync::Mutex::new(Vec::<String>::new());
        let calls: &'static _ = Box::leak(Box::new(calls));
        let runner: &'static Unsubscriber =
            Box::leak(Box::new(move |s: &Subscription, _m: &MailboxConfig| {
                calls.lock().unwrap().push(s.name.clone());
                if s.name == "fail" {
                    Err(UnsubscribeError::Http(500))
                } else {
                    Ok("Abmeldung gesendet".to_string())
                }
            }));
        let st = f.state.clone();
        let ids2 = ids.clone();
        f.state
            .start_job("unsubscribe", move |ctx| {
                unsubscribe_job(ctx, &st, &ids2, runner)
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 10).unwrap();
        assert_eq!(done["payload"]["ok"], 1);
        assert_eq!(done["payload"]["failed"], 1);
        assert_eq!(done["payload"]["skipped"], 3);
        assert_eq!(*calls.lock().unwrap(), vec!["auto", "fail"]);
        f.state
            .read(|d| {
                let by = |n: &str| d.subscriptions.iter().find(|s| s.name == n).unwrap();
                assert_eq!(by("auto").status, SubscriptionStatus::Unsubscribed);
                assert!(by("auto").history[0].ok);
                assert_eq!(by("fail").status, SubscriptionStatus::Failed);
                assert!(!by("fail").history[0].ok);
                for n in ["link", "trans", "orphan"] {
                    assert_eq!(by(n).status, SubscriptionStatus::Active);
                    assert!(by(n).history.is_empty(), "{n} must not be touched");
                }
            })
            .unwrap();
    }

    #[test]
    fn empty_selection_is_invalid() {
        let f = unlocked();
        assert!(start_unsubscribe(&f.state, vec![], real_unsubscriber()).is_err());
    }
}
