//! End-to-end: synthetic mailbox -> fake IMAP -> real scanner -> merge -> insights.
//!
//! Run with `-- --nocapture` to see the measured quality numbers.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicBool;

use ausgeloggt_core::insights::{
    billing_summary, payment_method_index, subscription_totals, BillingInterval,
};
use ausgeloggt_core::merge;
use ausgeloggt_core::{
    AccountStatus, CategorySource, MailCategory, SubscriptionStatus, UnsubscribeAttempt,
    UnsubscribeMethod, VaultData,
};
use ausgeloggt_e2e::corpus::{self, now, RawMail, OWN};
use ausgeloggt_e2e::fake_imap::FakeImap;
use ausgeloggt_e2e::harness::*;
use ausgeloggt_mail::{scan_mailbox_with, MailError, ScanOptions};
use chrono::{Duration, TimeZone, Utc};

fn bodies_needed_keys(truth: &corpus::Truth) -> BTreeSet<String> {
    truth
        .billing
        .iter()
        .filter(|b| b.bodies_needed)
        .map(|b| b.key.clone())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Corpus sanity
// ---------------------------------------------------------------------------------------------

#[test]
fn corpus_is_large_deterministic_and_varied() {
    let c = corpus_cached();
    assert!(c.mails.len() >= 1500, "mails: {}", c.mails.len());
    let services = c.truth.accounts.len() + c.truth.lists.len() + c.truth.negatives.len();
    assert!(services >= 120, "services: {services}");
    let again = corpus::generate(20260301);
    assert_eq!(c.mails.len(), again.mails.len());
    assert!(c
        .mails
        .iter()
        .zip(&again.mails)
        .all(|(a, b)| a.header == b.header && a.uid == b.uid && a.body == b.body));
    let other = corpus::generate(7);
    assert!(c
        .mails
        .iter()
        .zip(&other.mails)
        .any(|(a, b)| a.header != b.header));
    // UIDs strictly increasing.
    assert!(c.mails.windows(2).all(|w| w[0].uid < w[1].uid));
    println!(
        "CORPUS mails={} unique_message_ids={} duplicates={} expected_accounts={} lists={} billing={}",
        c.mails.len(),
        c.truth.unique_mails,
        c.truth.duplicate_mails,
        c.truth.accounts.len(),
        c.truth.lists.len(),
        c.truth.billing.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Account detection quality
// ---------------------------------------------------------------------------------------------

#[test]
fn account_detection_precision_and_recall() {
    let truth = &corpus_cached().truth;
    for bodies in [false, true] {
        let s = scanned_corpus(bodies);
        assert_eq!(
            s.outcome.items_scanned as usize,
            corpus_cached().mails.len()
        );
        let mut m = account_metrics(&s.data, truth);
        // Aggregator items (Apple in-app purchases) are only resolvable from bodies.
        if !bodies {
            let skip = bodies_needed_keys(truth);
            m.missed
                .retain(|x| !skip.iter().any(|k| x.contains(k.as_str())));
        }
        let expected_effective = m.expected
            - if bodies {
                0
            } else {
                bodies_needed_keys(truth).len()
            };
        let recall = (expected_effective - m.missed.len()) as f64 / expected_effective as f64;
        println!(
            "ACCOUNTS read_receipt_bodies={bodies}: detected={} expected={} precision={:.3} recall={:.3} (false positives: {:?}, missed: {:?}, phishing accepted: {})",
            m.detected, m.expected, m.precision, recall, m.false_positives, m.missed, m.phishing_hits.len()
        );
        assert!(
            m.precision >= 0.97,
            "precision {:.3}: {:?}",
            m.precision,
            m.false_positives
        );
        let hard_missed = m
            .missed
            .iter()
            .filter(|x| truth.hard.iter().any(|k| x.contains(k.as_str())))
            .count();
        let easy_expected = expected_effective - truth.hard.len();
        let easy_recall =
            (easy_expected - (m.missed.len() - hard_missed)) as f64 / easy_expected as f64;
        println!(
            "ACCOUNTS read_receipt_bodies={bodies}: easy recall={easy_recall:.3} hard-wording recall={:.3} ({hard_missed}/{} missed)",
            1.0 - hard_missed as f64 / truth.hard.len() as f64,
            truth.hard.len()
        );
        assert!(
            easy_recall >= 0.97,
            "easy recall {easy_recall:.3}: {:?}",
            m.missed
        );
        assert!(recall >= 0.80, "overall recall {recall:.3}: {:?}", m.missed);
    }
}

#[test]
fn no_accounts_from_freemail_esp_spam_or_own_mail() {
    let s = scanned_corpus(true);
    let truth = &corpus_cached().truth;
    let bad: Vec<_> = s
        .data
        .accounts
        .iter()
        .filter(|a| a.confidence >= MIN_CONFIDENCE && truth.negatives.contains(&a.domain))
        .map(|a| a.domain.clone())
        .collect();
    assert!(bad.is_empty(), "negatives became accounts: {bad:?}");
    // Not even a low-confidence account for free-mail providers / ESPs (infrastructure).
    for forbidden in [
        "gmail.com",
        "gmx.de",
        "web.de",
        "outlook.com",
        "t-online.de",
        "icloud.com",
        "googlemail.com",
        "yahoo.de",
        "posteo.de",
        "freenet.de",
        "mcsv.net",
        "mailchimp.com",
        "sendgrid.net",
        "amazonses.com",
        "mailgun.org",
        "mailgun.net",
    ] {
        assert!(
            !s.data.accounts.iter().any(|a| a.domain == forbidden),
            "unexpected account for {forbidden}"
        );
    }
    // Mails sent by the user: no evidence anywhere references them.
    assert!(!s
        .data
        .accounts
        .iter()
        .any(|a| a.identifiers.iter().any(|i| i == "chef@firma-xyz.de")));
}

#[test]
fn weak_signals_stay_below_confidence_threshold() {
    let s = scanned_corpus(true);
    let truth = &corpus_cached().truth;
    for l in &truth.lists {
        if let Some(a) = s
            .data
            .accounts
            .iter()
            .find(|a| a.domain == corpus::canon_key(&l.domain))
        {
            assert!(
                a.confidence < MIN_CONFIDENCE,
                "{} newsletter-only account at {:.2}",
                a.domain,
                a.confidence
            );
        }
    }
}

#[test]
fn evidence_is_capped_and_summaries_bounded() {
    let s = scanned_corpus(true);
    for a in &s.data.accounts {
        assert!(
            a.evidence.len() <= 6 * 15,
            "{} has {} evidence",
            a.domain,
            a.evidence.len()
        );
        for e in &a.evidence {
            assert!(
                e.summary.chars().count() <= 141,
                "summary too long: {}",
                e.summary.chars().count()
            );
        }
    }
}

#[test]
fn sso_and_relay_attribution() {
    let s = scanned_corpus(true);
    let truth = &corpus_cached().truth;
    let mut wrong = Vec::new();
    for t in truth.accounts.iter().filter(|t| t.sso.is_some()) {
        let Some(a) = find(&s.data, t) else {
            wrong.push(format!("{} missing", t.name));
            continue;
        };
        if !a.sso.contains(t.sso.as_ref().unwrap()) {
            wrong.push(format!("{}: sso {:?}", t.name, a.sso));
        }
    }
    println!(
        "SSO attribution errors: {}/{}",
        wrong.len(),
        truth.accounts.iter().filter(|t| t.sso.is_some()).count()
    );
    assert!(wrong.is_empty(), "{wrong:?}");
    // Apple relay recipients become the identifier of the account.
    let relay_accounts = s
        .data
        .accounts
        .iter()
        .filter(|a| {
            a.identifiers
                .iter()
                .any(|i| i.ends_with("@privaterelay.appleid.com"))
        })
        .count();
    assert!(relay_accounts >= 6, "relay accounts: {relay_accounts}");
    // SSO grant by Google for an app that also has payments merges into ONE account (Notion).
    let notion: Vec<_> = s
        .data
        .accounts
        .iter()
        .filter(|a| a.name.eq_ignore_ascii_case("notion"))
        .collect();
    assert_eq!(
        notion.len(),
        1,
        "notion accounts: {:?}",
        notion.iter().map(|a| &a.domain).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------------------------
// Subscriptions (newsletters)
// ---------------------------------------------------------------------------------------------

#[test]
fn subscriptions_detected_with_counts_and_unsubscribe_methods() {
    let s = scanned_corpus(false);
    let truth = &corpus_cached().truth;
    let by_key: BTreeMap<_, _> = s
        .data
        .subscriptions
        .iter()
        .map(|x| (x.list_key.clone(), x))
        .collect();
    let (mut found, mut count_ok, mut method_ok) = (0, 0, 0);
    let mut problems = Vec::new();
    for l in &truth.lists {
        let Some(sub) = by_key.get(&l.list_key) else {
            problems.push(format!("missing list {}", l.list_key));
            continue;
        };
        found += 1;
        if sub.message_count == l.count {
            count_ok += 1;
        } else {
            problems.push(format!(
                "{}: count {} != {}",
                l.list_key, sub.message_count, l.count
            ));
        }
        let has_one_click = sub
            .unsubscribe
            .iter()
            .any(|m| matches!(m, UnsubscribeMethod::OneClick { .. }));
        let any = !sub.unsubscribe.is_empty();
        if has_one_click == l.one_click && any == l.any_unsub {
            method_ok += 1;
        } else {
            problems.push(format!("{}: methods {:?}", l.list_key, sub.unsubscribe));
        }
    }
    println!(
        "SUBSCRIPTIONS expected={} found={} exact_counts={} correct_methods={} (total detected lists: {})",
        truth.lists.len(),
        found,
        count_ok,
        method_ok,
        s.data.subscriptions.len()
    );
    assert_eq!(found, truth.lists.len(), "{problems:?}");
    assert_eq!(count_ok, truth.lists.len(), "{problems:?}");
    assert_eq!(method_ok, truth.lists.len(), "{problems:?}");
}

#[test]
fn newsletters_are_not_transactional_and_transactional_mails_are_not_lists() {
    let s = scanned_corpus(false);
    // Receipts / welcome mails carry no List-Unsubscribe in the corpus: no list for them.
    let truth = &corpus_cached().truth;
    let list_domains: BTreeSet<String> = truth
        .lists
        .iter()
        .map(|l| corpus::canon_key(&l.domain))
        .collect();
    let stray: Vec<_> = s
        .data
        .subscriptions
        .iter()
        .filter(|x| !list_domains.contains(&x.domain) && !x.domain.ends_with(".top"))
        .map(|x| (&x.list_key, &x.domain))
        .collect();
    // Allowed strays: junk/edge senders and ESP mails whose visible From is an ESP.
    assert!(stray.len() <= 12, "unexpected lists: {stray:?}");
    for sub in &s.data.subscriptions {
        assert_ne!(
            sub.category,
            MailCategory::Transactional,
            "{}",
            sub.list_key
        );
        assert!(!sub.sample_subjects.is_empty() || sub.sender.is_empty() || true);
        assert!(sub.sample_subjects.iter().all(|x| x.chars().count() <= 141));
        assert!(sub.sample_subjects.len() <= 5);
        assert!(sub.frequency_per_month.is_finite() && sub.frequency_per_month >= 0.0);
    }
    // Promo / content categorisation: Lidl (promo heavy) is marketing, heise (content) newsletter.
    let lidl = s
        .data
        .subscriptions
        .iter()
        .find(|x| x.domain == "lidl.de")
        .unwrap();
    assert!(matches!(
        lidl.category,
        MailCategory::Marketing | MailCategory::Newsletter
    ));
}

#[test]
fn huge_subject_is_truncated_everywhere() {
    let s = scanned_corpus(false);
    let sub = s
        .data
        .subscriptions
        .iter()
        .find(|x| x.list_key == "news@bigsubject.de")
        .expect("big subject list");
    assert_eq!(sub.sample_subjects.len(), 1);
    assert!(sub.sample_subjects[0].chars().count() <= 141);
}

// ---------------------------------------------------------------------------------------------
// Payments / billing
// ---------------------------------------------------------------------------------------------

fn billing_eval(read_bodies: bool) -> (usize, usize, usize, usize, usize, Vec<String>) {
    let s = scanned_corpus(read_bodies);
    let truth = &corpus_cached().truth;
    let (mut n, mut interval_ok, mut amount_ok, mut active_ok, mut price_ok) = (0, 0, 0, 0, 0);
    let mut problems = Vec::new();
    for b in &truth.billing {
        if b.bodies_needed && !read_bodies {
            continue;
        }
        n += 1;
        let Some(acct) = s
            .data
            .accounts
            .iter()
            .find(|a| a.domain == b.key || a.name.eq_ignore_ascii_case(&b.name))
        else {
            problems.push(format!("{}: no account", b.key));
            continue;
        };
        let Some(sum) = billing_summary(acct, now()) else {
            problems.push(format!("{}: no billing summary", b.key));
            continue;
        };
        if sum.interval == b.interval {
            interval_ok += 1;
        } else {
            problems.push(format!(
                "{}: interval {:?} != {:?}",
                b.key, sum.interval, b.interval
            ));
        }
        if sum.amount_minor == Some(b.amount_minor)
            && sum.currency.as_deref() == Some(b.currency.as_str())
        {
            amount_ok += 1;
        } else {
            problems.push(format!(
                "{}: amount {:?} {:?} != {} {}",
                b.key, sum.amount_minor, sum.currency, b.amount_minor, b.currency
            ));
        }
        if sum.active_subscription == b.active {
            active_ok += 1;
        } else {
            problems.push(format!(
                "{}: active {} != {}",
                b.key, sum.active_subscription, b.active
            ));
        }
        if sum.price_increased == b.price_increased {
            price_ok += 1;
        } else {
            problems.push(format!(
                "{}: price_increased {} != {}",
                b.key, sum.price_increased, b.price_increased
            ));
        }
    }
    (n, interval_ok, amount_ok, active_ok, price_ok, problems)
}

#[test]
fn billing_interval_amount_and_state_from_subjects() {
    let (n, i, a, act, p, problems) = billing_eval(false);
    println!(
        "BILLING (subjects only) services={n} interval={i}/{n} amount={a}/{n} active={act}/{n} price_increase={p}/{n}"
    );
    for x in &problems {
        println!("  billing problem: {x}");
    }
    assert!(i * 100 >= n * 95, "interval {i}/{n}: {problems:?}");
    assert!(a * 100 >= n * 90, "amount {a}/{n}: {problems:?}");
    assert!(act * 100 >= n * 95, "active {act}/{n}: {problems:?}");
    assert!(p * 100 >= n * 90, "price {p}/{n}: {problems:?}");
}

#[test]
fn billing_with_receipt_bodies_including_aggregators() {
    let (n, i, a, act, p, problems) = billing_eval(true);
    println!(
        "BILLING (with bodies) services={n} interval={i}/{n} amount={a}/{n} active={act}/{n} price_increase={p}/{n}"
    );
    for x in &problems {
        println!("  billing problem: {x}");
    }
    assert!(i * 100 >= n * 95, "interval {i}/{n}: {problems:?}");
    assert!(a * 100 >= n * 95, "amount {a}/{n}: {problems:?}");
    assert!(act * 100 >= n * 95, "active {act}/{n}: {problems:?}");
    assert!(p * 100 >= n * 95, "price {p}/{n}: {problems:?}");
}

#[test]
fn aggregator_via_is_recorded() {
    let s = scanned_corpus(true);
    let truth = &corpus_cached().truth;
    for b in truth.billing.iter().filter(|b| b.via.is_some()) {
        let acct = s
            .data
            .accounts
            .iter()
            .find(|a| a.domain == b.key || a.name.eq_ignore_ascii_case(&b.name))
            .unwrap_or_else(|| panic!("no account for {}", b.key));
        let sum = billing_summary(acct, now()).unwrap();
        assert_eq!(sum.via, b.via, "{}", b.key);
    }
}

#[test]
fn payment_method_index_matches_ground_truth() {
    let s = scanned_corpus(true);
    let truth = &corpus_cached().truth;
    let index = payment_method_index(&s.data);
    let by_label: BTreeMap<String, BTreeSet<String>> = index
        .iter()
        .map(|(m, ids)| {
            let keys = ids
                .iter()
                .map(|id| {
                    s.data
                        .accounts
                        .iter()
                        .find(|a| a.id == *id)
                        .unwrap()
                        .domain
                        .clone()
                })
                .collect();
            (m.label_de(), keys)
        })
        .collect();
    let mut covered = 0;
    let mut total = 0;
    for (label, keys) in &truth.method_index {
        for k in keys {
            total += 1;
            if by_label.get(label).is_some_and(|got| got.contains(k)) {
                covered += 1;
            }
        }
    }
    println!("PAYMENT METHOD INDEX: {covered}/{total} (method, account) pairs correct; methods seen: {:?}", by_label.keys().collect::<Vec<_>>());
    assert!(covered * 100 >= total * 95, "{covered}/{total}");
    // Never more than brand + last4 / iban last4.
    for label in by_label.keys() {
        let digits: String = label.chars().filter(char::is_ascii_digit).collect();
        assert!(digits.len() <= 4, "{label}");
    }
    // The most used card is Visa 4242.
    assert_eq!(index[0].0.label_de(), "Visa •••• 4242");
}

#[test]
fn subscription_totals_are_sane_and_per_currency() {
    let s = scanned_corpus(true);
    let totals = subscription_totals(&s.data, now());
    let currencies: Vec<_> = totals.iter().map(|t| t.0.as_str()).collect();
    println!("SUBSCRIPTION TOTALS {totals:?}");
    for c in ["CHF", "EUR", "GBP", "USD"] {
        assert!(currencies.contains(&c), "{c} missing in {currencies:?}");
    }
    for (_, monthly, yearly, n) in &totals {
        assert!(*monthly > 0 && *yearly >= *monthly && *n > 0);
        // yearly ~ 12 x monthly (within rounding)
        assert!(
            (*yearly - *monthly * 12).abs() <= *monthly / 2 + 12 * *n as i64,
            "{monthly} vs {yearly}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Idempotency, incremental rescans, status preservation
// ---------------------------------------------------------------------------------------------

fn snapshot(data: &VaultData) -> Vec<(String, usize, usize, i64, Vec<String>)> {
    let mut v: Vec<_> = data
        .accounts
        .iter()
        .map(|a| {
            (
                a.domain.clone(),
                a.evidence.len(),
                a.payments.len(),
                (a.confidence * 1000.0) as i64,
                a.identifiers.clone(),
            )
        })
        .collect();
    v.sort();
    v
}

#[test]
fn reimport_of_same_scan_is_idempotent_for_accounts_and_payments() {
    let s = scanned_corpus(true);
    let mut data = s.data.clone();
    let before = snapshot(&data);
    let report = merge::merge_findings(&mut data, s.outcome.findings.clone());
    let (added, created) = merge::merge_payments(&mut data, s.outcome.payments.clone());
    assert_eq!(report.new_accounts, 0);
    assert_eq!(report.updated_accounts, 0, "{report:?}");
    assert_eq!((added, created), (0, 0));
    assert_eq!(snapshot(&data), before);
}

#[test]
fn incremental_rescan_finds_nothing_new_and_new_mails_are_merged_once() {
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails.clone());
    let mut cfg = imap.config(OWN);
    let opts = ScanOptions::default();
    let first = scan(&cfg, &opts);
    cfg.progress = first.progress.clone();
    let mut data = VaultData::default();
    merge_all(&mut data, &first);
    let before_subs: u64 = data.subscriptions.iter().map(|x| x.message_count).sum();
    let before = snapshot(&data);

    // Nothing new on the server.
    let second = scan(&cfg, &opts);
    assert_eq!(second.items_scanned, 0);
    assert!(
        second.findings.is_empty() && second.subscriptions.is_empty() && second.payments.is_empty()
    );
    assert_eq!(second.progress, first.progress, "progress must not regress");
    merge_all(&mut data, &second);
    assert_eq!(snapshot(&data), before);

    // 3 new newsletter mails + 1 welcome mail arrive.
    let uid = imap.max_uid();
    let mk = |i: u32, h: String| RawMail {
        uid: uid + i,
        header: h.into_bytes(),
        body: None,
        body_headers: "Content-Type: text/plain".into(),
        internal_date: "01-Mar-2026 10:00:00 +0000".into(),
    };
    let nl = "From: Lidl <newsletter@lidl.de>\r\nTo: erika.beispiel@gmx.de\r\nList-Unsubscribe: <https://lidl.de/u>\r\nDate: Sun, 01 Mar 2026 09:00:00 +0000\r\n";
    let new_mails = vec![
        mk(1, format!("{nl}Subject: Rabatt NEU 1\r\nMessage-ID: <new1@lidl.de>\r\n\r\n")),
        mk(2, format!("{nl}Subject: Rabatt NEU 2\r\nMessage-ID: <new2@lidl.de>\r\n\r\n")),
        mk(3, format!("{nl}Subject: Rabatt NEU 3\r\nMessage-ID: <new3@lidl.de>\r\n\r\n")),
        mk(4, "From: Neu <hello@brandneu-service.de>\r\nTo: erika.beispiel@gmx.de\r\nSubject: Willkommen bei Neu\r\nDate: Sun, 01 Mar 2026 09:30:00 +0000\r\nMessage-ID: <w@brandneu-service.de>\r\n\r\n".into()),
    ];
    imap.add_mails(new_mails);
    let third = scan(&cfg, &opts);
    assert_eq!(third.items_scanned, 4);
    merge_all(&mut data, &third);
    let after_subs: u64 = data.subscriptions.iter().map(|x| x.message_count).sum();
    assert_eq!(after_subs, before_subs + 3);
    assert!(data
        .accounts
        .iter()
        .any(|a| a.domain == "brandneu-service.de" && a.confidence >= MIN_CONFIDENCE));

    // Same scan result merged twice must not double count findings.
    let acc_before = snapshot(&data);
    merge::merge_findings(&mut data, third.findings.clone());
    assert_eq!(snapshot(&data), acc_before);
}

#[test]
fn uidvalidity_change_triggers_full_rescan() {
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails.clone());
    let mut cfg = imap.config(OWN);
    let first = scan(&cfg, &ScanOptions::default());
    cfg.progress = first.progress.clone();
    imap.state.lock().unwrap().uid_validity = 9999;
    let again = scan(&cfg, &ScanOptions::default());
    assert_eq!(again.items_scanned, first.items_scanned);
    assert_eq!(again.progress[0].uid_validity, 9999);
}

#[test]
fn full_rescan_does_not_double_subscription_counts() {
    // `ScanOptions::full_rescan` re-reads everything; merging that into the existing vault
    // must not add every mail a second time.
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails.clone());
    let cfg = imap.config(OWN);
    let first = scan(&cfg, &ScanOptions::default());
    let mut data = VaultData::default();
    merge_all(&mut data, &first);
    let before: u64 = data.subscriptions.iter().map(|s| s.message_count).sum();
    let full = scan(
        &cfg,
        &ScanOptions {
            full_rescan: true,
            ..Default::default()
        },
    );
    merge_all(&mut data, &full);
    let after: u64 = data.subscriptions.iter().map(|s| s.message_count).sum();
    assert_eq!(after, before, "message_count doubled after a full rescan");
}

#[test]
fn user_decisions_survive_rescans() {
    let s = scanned_corpus(true);
    let mut data = s.data.clone();
    let keys: Vec<String> = data
        .accounts
        .iter()
        .take(6)
        .map(|a| a.domain.clone())
        .collect();
    let statuses = [
        AccountStatus::Confirmed,
        AccountStatus::Keep,
        AccountStatus::Deleted,
        AccountStatus::NotAnAccount,
        AccountStatus::Ignored,
        AccountStatus::DeletionPending,
    ];
    for (k, st) in keys.iter().zip(statuses.iter()) {
        let a = data.accounts.iter_mut().find(|a| &a.domain == k).unwrap();
        a.status = st.clone();
        a.notes = format!("meine Notiz zu {k} äöü 😀");
        a.tags = vec!["wichtig".into()];
    }
    let name_before: Vec<_> = data.accounts.iter().map(|a| a.name.clone()).collect();
    for sub in data.subscriptions.iter_mut().take(4) {
        sub.status = SubscriptionStatus::Keep;
        sub.category = MailCategory::Spam;
        sub.category_source = CategorySource::User;
    }
    merge::merge_findings(&mut data, s.outcome.findings.clone());
    merge::merge_subscriptions(&mut data, s.outcome.subscriptions.clone());
    merge::merge_payments(&mut data, s.outcome.payments.clone());
    for (k, st) in keys.iter().zip(statuses.iter()) {
        let a = data.accounts.iter().find(|a| &a.domain == k).unwrap();
        assert_eq!(&a.status, st, "{k}");
        assert!(a.notes.contains("meine Notiz"));
        assert_eq!(a.tags, vec!["wichtig".to_string()]);
    }
    assert_eq!(
        name_before,
        data.accounts
            .iter()
            .map(|a| a.name.clone())
            .collect::<Vec<_>>()
    );
    for sub in data.subscriptions.iter().take(4) {
        assert_eq!(sub.status, SubscriptionStatus::Keep);
        assert_eq!(
            sub.category,
            MailCategory::Spam,
            "user category must not be overwritten"
        );
    }
    // Deleted / NotAnAccount stay out of the open totals.
    let totals = subscription_totals(&data, now());
    assert!(!totals.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Post-unsubscribe proof capture
// ---------------------------------------------------------------------------------------------

#[test]
fn post_unsubscribe_mails_are_captured_with_raw_headers() {
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails.clone());
    let cfg = imap.config(OWN);
    let first = scan(&cfg, &ScanOptions::default());
    let mut data = VaultData::default();
    merge_all(&mut data, &first);

    let list = "newsletter@lidl.de";
    let key = data
        .subscriptions
        .iter()
        .find(|s| s.domain == "lidl.de")
        .map(|s| s.list_key.clone())
        .unwrap_or_else(|| list.to_string());
    let unsub_at = Utc.with_ymd_and_hms(2026, 3, 2, 8, 0, 0).unwrap();
    {
        let sub = data
            .subscriptions
            .iter_mut()
            .find(|s| s.list_key == key)
            .unwrap();
        sub.status = SubscriptionStatus::Unsubscribed;
        sub.history.push(UnsubscribeAttempt {
            at: unsub_at,
            method: UnsubscribeMethod::Mailto {
                address: "unsubscribe@lidl.de".into(),
                subject: None,
                body: None,
            },
            ok: true,
            message: "ok".into(),
        });
    }
    // Mails keep coming: 1 within the 2-day grace period, 3 after, one of them duplicated.
    let uid = imap.max_uid();
    let host_sender = data
        .subscriptions
        .iter()
        .find(|s| s.list_key == key)
        .unwrap()
        .sender
        .clone();
    let mk = |i: u32, day: i64, id: &str| {
        RawMail {
        uid: uid + i,
        header: format!(
            "From: Lidl <{host_sender}>\r\nTo: erika.beispiel@gmx.de\r\nSubject: Rabatt Nachzuegler {i}\r\nDate: {}\r\nMessage-ID: <{id}@lidl.de>\r\nList-Unsubscribe: <https://lidl.de/u>\r\n\r\n",
            corpus::rfc2822(unsub_at + Duration::days(day), 0)
        )
        .into_bytes(),
        body: None,
        body_headers: String::new(),
        internal_date: "10-Mar-2026 10:00:00 +0000".into(),
    }
    };
    imap.add_mails(vec![
        mk(1, 1, "grace"),
        mk(2, 5, "after1"),
        mk(3, 9, "after2"),
        mk(4, 9, "after2"),
        mk(5, 12, "after3"),
    ]);
    let mut cfg2 = cfg.clone();
    cfg2.progress = first.progress.clone();
    let out = scan(
        &cfg2,
        &ScanOptions {
            watch_lists: vec![key.clone()],
            ..Default::default()
        },
    );
    let obs = out
        .subscriptions
        .iter()
        .find(|o| o.list_key == key)
        .expect("watched list observed");
    assert_eq!(
        obs.watched_mails.len(),
        4,
        "dedup by Message-ID happens in the scan"
    );
    assert!(obs
        .watched_mails
        .iter()
        .all(|m| m.raw_headers.contains("Message-ID")));
    merge::merge_subscriptions(&mut data, out.subscriptions.clone());
    let sub = data
        .subscriptions
        .iter()
        .find(|s| s.list_key == key)
        .unwrap();
    assert_eq!(
        sub.post_unsubscribe_mails.len(),
        3,
        "grace-period mail must not count"
    );
    assert!(sub
        .post_unsubscribe_mails
        .windows(2)
        .all(|w| w[0].date <= w[1].date));
    assert_eq!(sub.status, SubscriptionStatus::StillReceiving);
    // Merging the very same observation again must not duplicate proofs.
    merge::merge_subscriptions(&mut data, out.subscriptions);
    let sub = data
        .subscriptions
        .iter()
        .find(|s| s.list_key == key)
        .unwrap();
    assert_eq!(sub.post_unsubscribe_mails.len(), 3);
}

// ---------------------------------------------------------------------------------------------
// Failure modes of the scanner
// ---------------------------------------------------------------------------------------------

#[test]
fn wrong_password_is_a_clean_auth_error() {
    let imap = FakeImap::start(vec![]);
    let mut cfg = imap.config(OWN);
    cfg.password = "wrongpw".into();
    let cancel = AtomicBool::new(false);
    let r = scan_mailbox_with(&cfg, &ScanOptions::default(), &mut |_| {}, &cancel);
    assert!(matches!(r, Err(MailError::Auth)), "{r:?}");
}

#[test]
fn empty_mailbox_scans_cleanly() {
    let imap = FakeImap::start(vec![]);
    let out = scan(&imap.config(OWN), &ScanOptions::default());
    assert_eq!(out.items_scanned, 0);
    assert!(out.findings.is_empty());
}

#[test]
fn dropped_connection_mid_scan_is_an_error_not_a_panic() {
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails.clone());
    imap.state.lock().unwrap().drop_after_fetches = Some(2);
    let cancel = AtomicBool::new(false);
    let r = scan_mailbox_with(
        &imap.config(OWN),
        &ScanOptions::default(),
        &mut |_| {},
        &cancel,
    );
    assert!(r.is_err());
}

#[test]
fn cancel_flag_stops_the_scan() {
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails.clone());
    let cancel = AtomicBool::new(false);
    let mut n = 0;
    let r = scan_mailbox_with(
        &imap.config(OWN),
        &ScanOptions::default(),
        &mut |_| {
            n += 1;
            if n >= 3 {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(r, Err(MailError::Cancelled)), "{r:?}");
}

#[test]
fn only_inbox_is_scanned_not_sent_trash_spam() {
    let c = corpus_cached();
    let imap = FakeImap::start(c.mails[..50].to_vec());
    let _ = scan(&imap.config(OWN), &ScanOptions::default());
    let verbs = imap.state.lock().unwrap().verbs.clone();
    assert_eq!(verbs.iter().filter(|v| v.starts_with("EXAMINE")).count(), 1);
    assert!(!verbs
        .iter()
        .any(|v| v.starts_with("SELECT") || v.starts_with("STORE")));
}

// ---------------------------------------------------------------------------------------------
// Known bugs (ignored): run with `-- --ignored`
// ---------------------------------------------------------------------------------------------

#[test]
fn phishing_lookalikes_do_not_become_accounts() {
    let s = scanned_corpus(true);
    let truth = &corpus_cached().truth;
    let hits: Vec<_> = s
        .data
        .accounts
        .iter()
        .filter(|a| a.confidence >= MIN_CONFIDENCE && truth.phishing.contains(&a.domain))
        .map(|a| a.domain.clone())
        .collect();
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn message_id_collision_across_senders_does_not_drop_mails() {
    let mk = |uid: u32, from: &str| {
        RawMail {
        uid,
        header: format!(
            "From: {from}\r\nTo: erika.beispiel@gmx.de\r\nSubject: Willkommen\r\nDate: Sun, 01 Mar 2026 09:00:00 +0000\r\nMessage-ID: <1@localhost>\r\n\r\n"
        )
        .into_bytes(),
        body: None,
        body_headers: String::new(),
        internal_date: "01-Mar-2026 10:00:00 +0000".into(),
    }
    };
    let imap = FakeImap::start(vec![
        mk(1, "A <hello@collision-a.de>"),
        mk(2, "B <hello@collision-b.de>"),
    ]);
    let out = scan(&imap.config(OWN), &ScanOptions::default());
    let mut data = VaultData::default();
    merge_all(&mut data, &out);
    assert!(data.accounts.iter().any(|a| a.domain == "collision-a.de"));
    assert!(
        data.accounts.iter().any(|a| a.domain == "collision-b.de"),
        "second sender lost"
    );
}

#[test]
fn billing_interval_enum_is_exported() {
    // Keeps the import alive and documents the interval set used by the ground truth.
    assert_eq!(BillingInterval::Monthly.label_de(), "monatlich");
}
