//! Unsubscribe job: only automatic methods (RFC 8058 one-click POST, mailto) are executed.
//! Link-only and transactional lists are reported, never executed.

use ausgeloggt_core::model::{
    MailCategory, MailboxConfig, Subscription, SubscriptionStatus, UnsubscribeAttempt,
};
use ausgeloggt_mail::manual::{self, ManualLink};
use ausgeloggt_mail::unsubscribe::{self, UnsubscribeError};
use ausgeloggt_mail::MailError;
use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};
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

pub type LinkFetcher =
    dyn Fn(&MailboxConfig, &Subscription) -> Result<Vec<ManualLink>, MailError> + Send + Sync;

pub fn real_link_fetcher() -> &'static LinkFetcher {
    &manual::fetch_mail_links
}

/// `unsubscribe_manual_links`: links the user can open in the browser. Header links
/// (stored `List-Unsubscribe` methods) always; links found in the newest mail of the list
/// only if `from_mail` (explicit request, one IMAP fetch, body dropped). Nothing is opened
/// or fetched from the links themselves.
pub fn manual_links(
    state: &AppState,
    id: Uuid,
    from_mail: bool,
    fetch: &LinkFetcher,
) -> IpcResult<Value> {
    let (sub, mailbox) = state.read(|d| {
        let sub = d.subscriptions.iter().find(|s| s.id == id).cloned();
        let mb = sub.as_ref().and_then(|s| {
            d.mailboxes
                .iter()
                .find(|m| m.address.eq_ignore_ascii_case(&s.mailbox))
                .cloned()
        });
        (sub, mb)
    })?;
    let sub = sub.ok_or_else(|| IpcError::not_found("Newsletter"))?;
    let (mut links, mailto) = manual::header_links(&sub);
    if from_mail {
        let mailbox = mailbox.ok_or_else(|| {
            IpcError::invalid("Das zugehörige Postfach ist nicht mehr eingerichtet")
        })?;
        for l in fetch(&mailbox, &sub)? {
            if !links.iter().any(|x| x.url == l.url) {
                links.push(l);
            }
        }
    }
    let mut out = json!({ "links": links });
    if let (Some(m), Some(obj)) = (mailto, out.as_object_mut()) {
        obj.insert("mailto".into(), json!(m));
    }
    Ok(out)
}

/// Machine readable failure reason for the UI (which fallback to offer).
pub fn failure_reason(e: &UnsubscribeError) -> &'static str {
    match e {
        UnsubscribeError::NoAutomaticMethod => "NoAutomaticMethod",
        UnsubscribeError::Http(_) => "Http",
        UnsubscribeError::Network(_) => "Network",
        UnsubscribeError::UnsafeTarget => "UnsafeTarget",
        UnsubscribeError::Smtp(_) => "Smtp",
        UnsubscribeError::Refused => "Refused",
    }
}

enum Plan {
    Skip(String, Option<&'static str>),
    Run(Box<Subscription>, Box<MailboxConfig>),
}

fn plan(state: &AppState, id: Uuid) -> Result<Plan, IpcError> {
    state.read(|d| {
        let Some(sub) = d.subscriptions.iter().find(|s| s.id == id) else {
            return Plan::Skip("Newsletter nicht gefunden".into(), None);
        };
        if sub.category == MailCategory::Transactional {
            return Plan::Skip(
                "Wichtige Mails (Rechnungen, Konto Hinweise) werden nicht automatisch abbestellt"
                    .into(),
                Some("Refused"),
            );
        }
        if unsubscribe::best_automatic(sub).is_none() {
            return Plan::Skip(
                "Kein automatischer Weg vorhanden, bitte den Link im Browser öffnen".into(),
                Some("NoAutomaticMethod"),
            );
        }
        match d
            .mailboxes
            .iter()
            .find(|m| m.address.eq_ignore_ascii_case(&sub.mailbox))
        {
            Some(mb) => Plan::Run(Box::new(sub.clone()), Box::new(mb.clone())),
            None => Plan::Skip(
                "Das zugehörige Postfach ist nicht mehr eingerichtet".into(),
                None,
            ),
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
            Plan::Skip(message, reason) => {
                skipped_n += 1;
                ctx.item_reason(done, total, id.to_string(), false, message.clone(), reason);
                results.push(json!({ "id": id, "ok": false, "skipped": true, "message": message, "reason": reason }));
            }
            Plan::Run(sub, mailbox) => {
                ctx.progress(done - 1, total, format!("Melde ab: {}", sub.name));
                // Network without the vault lock.
                let result = run(&sub, &mailbox);
                let (ok, message, reason) = match result {
                    Ok(m) => (true, m, None),
                    Err(e) => {
                        let reason = failure_reason(&e);
                        (false, IpcError::from(e).message, Some(reason))
                    }
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
                ctx.item_reason(done, total, id.to_string(), ok, message.clone(), reason);
                results.push(json!({ "id": id, "ok": ok, "skipped": false, "message": message, "reason": reason }));
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
            last_message_id: None,
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
        let res = done["payload"]["results"].as_array().unwrap();
        assert!(res.iter().any(|r| r["reason"] == "Http"));
        assert!(res.iter().any(|r| r["reason"] == "Refused"));
        assert!(res.iter().any(|r| r["reason"] == "NoAutomaticMethod"));
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

    #[test]
    fn manual_links_header_and_mail_on_request() {
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
        let s = sub(
            "me@example.org",
            "list",
            MailCategory::Marketing,
            vec![
                UnsubscribeMethod::OneClick {
                    url: "https://x.example/u".into(),
                },
                UnsubscribeMethod::Mailto {
                    address: "u@x.example".into(),
                    subject: None,
                    body: None,
                },
            ],
        );
        let id = s.id;
        f.state
            .mutate(|d| {
                d.mailboxes.push(mb);
                d.subscriptions.push(s);
            })
            .unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let calls: &'static _ = Box::leak(Box::new(calls));
        let fetch: &'static LinkFetcher =
            Box::leak(Box::new(move |_: &MailboxConfig, _: &Subscription| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(vec![
                    ManualLink {
                        url: "https://x.example/u".into(),
                        source: manual::LinkSource::Mail,
                        label: "dup".into(),
                    },
                    ManualLink {
                        url: "https://x.example/m".into(),
                        source: manual::LinkSource::Mail,
                        label: "Abmelden".into(),
                    },
                ])
            }));
        let v = manual_links(&f.state, id, false, fetch).unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(v["links"].as_array().unwrap().len(), 1);
        assert_eq!(v["links"][0]["source"], "header");
        assert_eq!(v["mailto"], "u@x.example");
        let v = manual_links(&f.state, id, true, fetch).unwrap();
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let links = v["links"].as_array().unwrap();
        assert_eq!(links.len(), 2);
        assert_eq!(links[1]["source"], "mail");
        assert_eq!(links[1]["url"], "https://x.example/m");
        assert!(manual_links(&f.state, Uuid::new_v4(), false, fetch).is_err());
    }

    #[test]
    fn failure_reasons_are_machine_readable() {
        assert_eq!(failure_reason(&UnsubscribeError::Http(500)), "Http");
        assert_eq!(
            failure_reason(&UnsubscribeError::Network("x".into())),
            "Network"
        );
        assert_eq!(
            failure_reason(&UnsubscribeError::UnsafeTarget),
            "UnsafeTarget"
        );
        assert_eq!(failure_reason(&UnsubscribeError::Smtp("x".into())), "Smtp");
        assert_eq!(failure_reason(&UnsubscribeError::Refused), "Refused");
        assert_eq!(
            failure_reason(&UnsubscribeError::NoAutomaticMethod),
            "NoAutomaticMethod"
        );
    }
}
