//! Insights: billing intervals, currency mixes, refunds, time zones, leap years, risk and
//! privacy score bounds.

use ausgeloggt_core::insights::*;
use ausgeloggt_core::{
    Account, AccountStatus, BreachRecord, DataRequest, Evidence, EvidenceKind, MailCategory,
    MailboxConfig, PaymentKind, PaymentMethod, PaymentRecord, RequestChannel, RequestKind,
    RequestStatus, SourceKind, SsoProvider, Subscription, SubscriptionStatus, VaultData,
};
use ausgeloggt_e2e::rng::Rng;
use chrono::{DateTime, Duration, FixedOffset, Months, TimeZone, Utc};
use uuid::Uuid;

fn utc(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
}

fn now() -> DateTime<Utc> {
    utc(2026, 3, 1, 12, 0)
}

fn rec(
    kind: PaymentKind,
    date: Option<DateTime<Utc>>,
    amount: Option<i64>,
    cur: &str,
) -> PaymentRecord {
    PaymentRecord {
        date,
        kind,
        amount_minor: amount,
        currency: Some(cur.to_string()),
        plan: None,
        method: None,
        via: None,
        summary: String::new(),
        source_ref: None,
    }
}

fn account_with(records: Vec<PaymentRecord>) -> Account {
    let mut a = Account::new("shop.example.com", "Shop");
    a.payments = records;
    a
}

fn series(dates: &[DateTime<Utc>], amount: i64, cur: &str) -> Vec<PaymentRecord> {
    dates
        .iter()
        .enumerate()
        .map(|(i, d)| {
            rec(
                if i == 0 {
                    PaymentKind::Charge
                } else {
                    PaymentKind::Renewal
                },
                Some(*d),
                Some(amount),
                cur,
            )
        })
        .collect()
}

fn monthly_dates(first: DateTime<Utc>, n: u32) -> Vec<DateTime<Utc>> {
    (0..n).map(|k| first + Months::new(k)).collect()
}

fn interval_of(records: Vec<PaymentRecord>, at: DateTime<Utc>) -> Option<BillingInterval> {
    billing_summary(&account_with(records), at).and_then(|b| b.interval)
}

// ---------------------------------------------------------------------------------------------
// Billing interval detection
// ---------------------------------------------------------------------------------------------

#[test]
fn month_end_billing_is_monthly() {
    // 31st of each month: gaps 31, 28/29, 31, 30, 31, 30 ...; `Months` clamps to month ends.
    let d = monthly_dates(utc(2025, 1, 31, 23, 59), 12);
    let s = billing_summary(&account_with(series(&d, 999, "EUR")), now()).unwrap();
    assert_eq!(s.interval, Some(BillingInterval::Monthly));
    assert!(s.active_subscription);
    assert_eq!(s.amount_minor, Some(999));
    assert_eq!(s.monthly_minor, Some(999));
}

#[test]
fn leap_year_feb_29_series() {
    // Yearly on Feb 29 (2024, then clamped Feb 28 2025, 2026 ...).
    let d: Vec<_> = (0..3)
        .map(|k| utc(2024, 2, 29, 8, 0) + Months::new(12 * k))
        .collect();
    assert_eq!(d[1].format("%m-%d").to_string(), "02-28");
    let s = billing_summary(
        &account_with(series(&d, 5999, "EUR")),
        utc(2026, 4, 1, 0, 0),
    )
    .unwrap();
    assert_eq!(s.interval, Some(BillingInterval::Yearly));
    assert!(s.active_subscription);
    assert_eq!(s.monthly_minor, Some(500)); // 5999 / 12 rounded
                                            // The estimated next charge (last + 365 days) lies within +-2 days of the real date.
    let next = s.next_charge_estimate.unwrap();
    let real = utc(2027, 2, 28, 8, 0);
    assert!((next - real).num_days().abs() <= 2, "{next} vs {real}");
}

#[test]
fn irregular_dates_with_jitter_still_detect_the_interval() {
    let mut rng = Rng::new(5);
    let mut d = Vec::new();
    let mut t = utc(2025, 1, 3, 10, 0);
    for _ in 0..10 {
        d.push(t);
        t += Duration::days(rng.range(26, 34)); // +-4 days around a month
    }
    assert_eq!(
        interval_of(series(&d, 1299, "EUR"), d[9] + Duration::days(5)),
        Some(BillingInterval::Monthly)
    );
    // One skipped month (a gap of ~60 days) does not break the median.
    let mut d2 = d.clone();
    d2.remove(4);
    assert_eq!(
        interval_of(series(&d2, 1299, "EUR"), d2[8] + Duration::days(5)),
        Some(BillingInterval::Monthly)
    );
    // Duplicate charge on the same day collapses.
    let mut d3 = d.clone();
    d3.insert(3, d[2]);
    assert_eq!(
        interval_of(series(&d3, 1299, "EUR"), d3[10] + Duration::days(5)),
        Some(BillingInterval::Monthly)
    );
}

#[test]
fn all_supported_intervals() {
    let cases = [
        (7i64, BillingInterval::Weekly),
        (30, BillingInterval::Monthly),
        (91, BillingInterval::Quarterly),
        (182, BillingInterval::HalfYearly),
        (365, BillingInterval::Yearly),
    ];
    for (days, expect) in cases {
        let first = utc(2024, 1, 10, 9, 0);
        let d: Vec<_> = (0..5).map(|k| first + Duration::days(days * k)).collect();
        let at = d[4] + Duration::days(days / 2);
        assert_eq!(
            interval_of(series(&d, 1000, "EUR"), at),
            Some(expect),
            "{days} days"
        );
    }
}

#[test]
fn biweekly_bimonthly_and_four_monthly_subscriptions_are_recognised() {
    for days in [14i64, 61, 120] {
        let first = utc(2025, 1, 10, 9, 0);
        let d: Vec<_> = (0..6).map(|k| first + Duration::days(days * k)).collect();
        let at = d[5] + Duration::days(3);
        let s = billing_summary(&account_with(series(&d, 1000, "EUR")), at).unwrap();
        assert!(
            s.interval.is_some() || s.active_subscription,
            "{days} day cycle not recognised"
        );
    }
}

#[test]
fn timezone_boundaries_do_not_split_or_merge_charges_wrongly() {
    // The same charge expressed at 23:30 UTC and 00:30 local next day is one day in UTC.
    let tz = FixedOffset::east_opt(2 * 3600).unwrap();
    let late = utc(2026, 1, 31, 23, 30);
    let local = late.with_timezone(&tz);
    assert_eq!(local.format("%d").to_string(), "01");
    let mut d = monthly_dates(utc(2025, 8, 31, 23, 30), 6);
    d.push(late);
    d.sort();
    d.dedup();
    let s = billing_summary(&account_with(series(&d, 700, "EUR")), now()).unwrap();
    assert_eq!(s.interval, Some(BillingInterval::Monthly));
    // 00:00:00 exactly and 23:59:59 on consecutive days both count as distinct days.
    let two = vec![
        rec(
            PaymentKind::Charge,
            Some(utc(2026, 1, 1, 0, 0)),
            Some(100),
            "EUR",
        ),
        rec(
            PaymentKind::Charge,
            Some(utc(2026, 1, 1, 23, 59)),
            Some(100),
            "EUR",
        ),
        rec(
            PaymentKind::Charge,
            Some(utc(2026, 1, 2, 0, 0)),
            Some(100),
            "EUR",
        ),
    ];
    let s = billing_summary(&account_with(two), now()).unwrap();
    assert!(!s.active_subscription);
}

#[test]
fn lapsed_cancelled_and_single_payments() {
    // Monthly, last charge 3 months ago -> lapsed.
    let d = monthly_dates(utc(2025, 3, 5, 10, 0), 6); // last: 2025-08-05
    let s = billing_summary(&account_with(series(&d, 999, "EUR")), now()).unwrap();
    assert_eq!(s.interval, Some(BillingInterval::Monthly));
    assert!(!s.active_subscription);
    assert!(s.next_charge_estimate.is_none());

    // Cancelled after the last charge.
    let d = monthly_dates(utc(2025, 8, 5, 10, 0), 7);
    let mut r = series(&d, 999, "EUR");
    let last = *d.last().unwrap();
    r.push(rec(
        PaymentKind::Cancelled,
        Some(last + Duration::days(2)),
        None,
        "EUR",
    ));
    let s = billing_summary(&account_with(r.clone()), now()).unwrap();
    assert!(!s.active_subscription);
    assert_eq!(s.cancelled_at, Some(last + Duration::days(2)));
    // A cancellation BEFORE the last charge (re-subscribed) does not cancel.
    r.pop();
    r.push(rec(
        PaymentKind::Cancelled,
        Some(d[2] + Duration::days(1)),
        None,
        "EUR",
    ));
    let s = billing_summary(&account_with(r), now()).unwrap();
    assert!(s.active_subscription);
    // Undated cancellation cancels.
    let mut r = series(&d, 999, "EUR");
    r.push(rec(PaymentKind::Cancelled, None, None, "EUR"));
    assert!(
        !billing_summary(&account_with(r), now())
            .unwrap()
            .active_subscription
    );

    // Single charge: not a subscription.
    let one = vec![rec(
        PaymentKind::Charge,
        Some(now() - Duration::days(3)),
        Some(4999),
        "EUR",
    )];
    let s = billing_summary(&account_with(one), now()).unwrap();
    assert!(!s.active_subscription && s.interval.is_none());
    // Empty / undated records.
    assert!(billing_summary(&account_with(vec![]), now()).is_none());
    let undated = vec![rec(PaymentKind::Renewal, None, Some(100), "EUR")];
    let _ = billing_summary(&account_with(undated), now());
}

#[test]
fn currency_mixes_refunds_and_zero_or_negative_amounts() {
    let d = monthly_dates(utc(2025, 4, 10, 10, 0), 11); // up to 2026-02-10
    let mut r = series(&d[..5], 999, "EUR");
    r.extend(series(&d[5..], 1099, "USD").into_iter().map(|mut x| {
        x.kind = PaymentKind::Renewal;
        x
    }));
    // Refund of one EUR charge, a zero-amount trial, a negative "charge".
    r.push(rec(
        PaymentKind::Refund,
        Some(d[2] + Duration::days(1)),
        Some(999),
        "EUR",
    ));
    r.push(rec(
        PaymentKind::TrialStarted,
        Some(d[0] - Duration::days(14)),
        Some(0),
        "EUR",
    ));
    r.push(rec(
        PaymentKind::Charge,
        Some(d[3] + Duration::days(2)),
        Some(-500),
        "EUR",
    ));
    let s = billing_summary(&account_with(r), now()).unwrap();
    assert_eq!(s.interval, Some(BillingInterval::Monthly));
    assert_eq!(
        s.currency.as_deref(),
        Some("USD"),
        "latest amount decides the currency"
    );
    assert_eq!(s.amount_minor, Some(1099));
    // 12-month sums per currency; refund deducted from EUR.
    let spent: std::collections::BTreeMap<_, _> = s.spent_last_12m.iter().cloned().collect();
    assert_eq!(spent["USD"], 6 * 1099);
    let eur_charges: i64 = d[..5]
        .iter()
        .filter(|x| **x > now() - Duration::days(365))
        .count() as i64
        * 999;
    assert_eq!(spent["EUR"], eur_charges - 999 - 500);
    // A price "increase" across currencies is NOT a price increase (EUR 9.99 -> USD 10.99).
    assert!(
        !s.price_increased,
        "currency switch flagged as price increase"
    );
}

#[test]
fn price_changes() {
    let d = monthly_dates(utc(2025, 6, 1, 10, 0), 9);
    let mut amounts: Vec<PaymentRecord> = series(&d, 1000, "EUR");
    amounts.last_mut().unwrap().amount_minor = Some(1200);
    assert!(
        billing_summary(&account_with(amounts), now())
            .unwrap()
            .price_increased
    );
    // Price decrease.
    let mut dec: Vec<PaymentRecord> = series(&d, 1000, "EUR");
    dec.last_mut().unwrap().amount_minor = Some(800);
    assert!(
        !billing_summary(&account_with(dec), now())
            .unwrap()
            .price_increased
    );
    // A PriceChange notice after the last charge flags it.
    let mut notice: Vec<PaymentRecord> = series(&d, 1000, "EUR");
    notice.push(rec(
        PaymentKind::PriceChange,
        Some(*d.last().unwrap() + Duration::days(3)),
        Some(1500),
        "EUR",
    ));
    assert!(
        billing_summary(&account_with(notice), now())
            .unwrap()
            .price_increased
    );
}

#[test]
fn dated_in_the_future_or_the_distant_past_never_panics() {
    for year in [1, 1970, 1999, 2099, 9999] {
        let base = Utc.with_ymd_and_hms(year, 6, 15, 0, 0, 0).unwrap();
        let d: Vec<_> = (0..4).map(|k| base + Duration::days(30 * k)).collect();
        let a = account_with(series(&d, 999, "EUR"));
        let s = billing_summary(&a, now()).unwrap();
        let _ = (s.active_subscription, s.next_charge_estimate);
        let mut data = VaultData::default();
        data.accounts.push(a);
        let _ = subscription_totals(&data, now());
        let _ = privacy_score(&data, &RiskContext::default(), now());
    }
}

#[test]
fn future_dated_charges_do_not_make_a_subscription_active() {
    let d = monthly_dates(utc(2031, 1, 1, 0, 0), 4);
    let s = billing_summary(&account_with(series(&d, 999, "EUR")), now()).unwrap();
    assert!(
        !s.active_subscription,
        "charges dated 5 years in the future count as active"
    );
}

#[test]
fn huge_amounts_do_not_overflow() {
    let d = monthly_dates(utc(2025, 4, 10, 10, 0), 11);
    let a = account_with(series(&d, i64::MAX / 2, "EUR"));
    let _ = billing_summary(&a, now());
    let mut data = VaultData::default();
    data.accounts.push(a);
    let _ = subscription_totals(&data, now());
}

#[test]
fn payment_method_index_and_payment_data_sorting() {
    let mut data = VaultData::default();
    let visa = PaymentMethod::Card {
        brand: "Visa".into(),
        last4: "4242".into(),
    };
    for (i, status) in [
        AccountStatus::Discovered,
        AccountStatus::Deleted,
        AccountStatus::NotAnAccount,
        AccountStatus::Keep,
    ]
    .into_iter()
    .enumerate()
    {
        let mut a = Account::new(format!("s{i}.example.com"), format!("S{i}"));
        a.status = status;
        let mut r = rec(
            PaymentKind::Charge,
            Some(now() - Duration::days(i as i64)),
            Some(100),
            "EUR",
        );
        r.method = Some(visa.clone());
        a.payments.push(r);
        data.accounts.push(a);
    }
    let idx = payment_method_index(&data);
    assert_eq!(idx.len(), 1);
    assert_eq!(
        idx[0].1.len(),
        2,
        "deleted and not-an-account entries are excluded"
    );
    assert_eq!(accounts_with_payment_data(&data).len(), 2);
}

// ---------------------------------------------------------------------------------------------
// Risk and privacy score
// ---------------------------------------------------------------------------------------------

fn risky_account() -> Account {
    let mut a = Account::new("bank.example.com", "Bank");
    a.identifiers = vec!["+491701234567".into(), "old@ex-employer.example.org".into()];
    a.sso = vec![SsoProvider::Google];
    a.security.password_pwned = Some(true);
    a.security.password_reuse_count = Some(5);
    a.security.phone_login = true;
    a.security.breaches = vec![
        BreachRecord {
            name: "X".into(),
            title: "X".into(),
            breach_date: None,
            data_classes: vec![],
            confirmed_for_user: true,
        },
        BreachRecord {
            name: "Y".into(),
            title: "Y".into(),
            breach_date: None,
            data_classes: vec![],
            confirmed_for_user: false,
        },
    ];
    a.last_seen = Some(now() - Duration::days(365 * 5));
    let d = monthly_dates(now() - Duration::days(200), 6);
    a.payments = series(&d, 999, "EUR");
    a
}

#[test]
fn risk_score_is_bounded_and_ordered() {
    let a = risky_account();
    let ctx = RiskContext {
        own_mail_domains: vec![],
        supports_2fa: vec!["bank.example.com".into()],
    };
    let r = assess_risk(&a, &ctx, now());
    assert_eq!(
        r.score, 100,
        "sum of weights exceeds 100 and must be capped"
    );
    assert!(r.reasons.windows(2).all(|w| w[0].weight() >= w[1].weight()));
    assert_eq!(r.reasons[0], RiskReason::PasswordPwned);
    // Closed accounts score 0.
    for st in [
        AccountStatus::Deleted,
        AccountStatus::NotAnAccount,
        AccountStatus::Ignored,
    ] {
        let mut b = risky_account();
        b.status = st;
        assert_eq!(assess_risk(&b, &ctx, now()).score, 0);
    }
    // Plain account scores 0.
    assert_eq!(
        assess_risk(&Account::new("a.example.com", "A"), &ctx, now()).score,
        0
    );
}

#[test]
fn risk_and_score_fuzz_stay_in_bounds() {
    let mut rng = Rng::new(99);
    let kinds = EvidenceKind::ALL;
    let statuses = [
        AccountStatus::Discovered,
        AccountStatus::Confirmed,
        AccountStatus::Keep,
        AccountStatus::DeletionPending,
        AccountStatus::Deleted,
        AccountStatus::NotAnAccount,
        AccountStatus::Ignored,
    ];
    let mut data = VaultData::default();
    for i in 0..600 {
        let mut a = Account::new(
            format!(
                "{}.example.com",
                rng.nasty_string(2)
                    .replace(['\u{0}', ' ', '\n', '\r', '\t'], "x")
            ),
            rng.nasty_string(5),
        );
        a.status = rng.pick(&statuses).clone();
        a.confidence = rng.below(300) as f32 / 100.0 - 1.0;
        a.last_seen = rng
            .chance(0.7)
            .then(|| now() - Duration::days(rng.range(-400, 4000)));
        for _ in 0..rng.below(4) {
            a.identifiers.push(rng.nasty_string(4));
        }
        for _ in 0..rng.below(5) {
            a.evidence.push(Evidence {
                kind: *rng.pick(&kinds),
                source: SourceKind::Manual,
                summary: rng.nasty_string(3),
                date: rng
                    .chance(0.6)
                    .then(|| now() - Duration::days(rng.range(-100, 3000))),
                source_ref: None,
            });
        }
        for _ in 0..rng.below(8) {
            let kind = match rng.below(7) {
                0 => PaymentKind::Charge,
                1 => PaymentKind::Renewal,
                2 => PaymentKind::Refund,
                3 => PaymentKind::Cancelled,
                4 => PaymentKind::PriceChange,
                5 => PaymentKind::TrialStarted,
                _ => PaymentKind::PaymentFailed,
            };
            a.payments.push(rec(
                kind,
                rng.chance(0.8)
                    .then(|| now() - Duration::days(rng.range(-30, 900))),
                rng.chance(0.8).then(|| rng.range(-1000, 100_000)),
                rng.pick(&["EUR", "USD", "eur", "", "XXX"]),
            ));
        }
        a.security.password_reuse_count = rng.chance(0.3).then(|| rng.below(10) as u32);
        a.security.password_pwned = rng.chance(0.3).then(|| rng.chance(0.5));
        a.security.has_2fa = rng.chance(0.3).then(|| rng.chance(0.5));
        data.accounts.push(a);
        if i % 3 == 0 {
            data.subscriptions.push(Subscription {
                id: Uuid::new_v4(),
                mailbox: "m".into(),
                list_key: format!("k{i}"),
                domain: "d.example.com".into(),
                name: "n".into(),
                sender: "s".into(),
                unsubscribe: vec![],
                message_count: rng.below(1000),
                frequency_per_month: 1.0,
                first_seen: None,
                last_seen: None,
                sample_subjects: vec![],
                category: if rng.chance(0.5) {
                    MailCategory::Marketing
                } else {
                    MailCategory::Spam
                },
                category_source: Default::default(),
                status: if rng.chance(0.5) {
                    SubscriptionStatus::Keep
                } else {
                    SubscriptionStatus::Active
                },
                history: vec![],
                post_unsubscribe_mails: vec![],
            });
        }
    }
    for i in 0..40 {
        data.requests.push(DataRequest {
            id: Uuid::new_v4(),
            kind: RequestKind::Erasure,
            target: "t".into(),
            domain: None,
            account_id: None,
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: String::new(),
            subject: String::new(),
            body: String::new(),
            created_at: now(),
            sent_at: None,
            deadline: Some(now() + Duration::days(i - 20)),
            status: match i % 4 {
                0 => RequestStatus::Draft,
                1 => RequestStatus::Sent,
                2 => RequestStatus::Answered,
                _ => RequestStatus::Done,
            },
            notes: String::new(),
        });
    }
    let ctx = RiskContext {
        own_mail_domains: vec!["example.org".into()],
        supports_2fa: data
            .accounts
            .iter()
            .take(100)
            .map(|a| a.domain.clone())
            .collect(),
    };
    for a in &data.accounts {
        let r = assess_risk(a, &ctx, now());
        assert!(r.score <= 100);
        assert!(r.reasons.windows(2).all(|w| w[0].weight() >= w[1].weight()));
        let _ = billing_summary(a, now());
    }
    let ranking = risk_ranking(&data, &ctx, now());
    assert!(ranking.windows(2).all(|w| w[0].score >= w[1].score));
    let sc = privacy_score(&data, &ctx, now());
    assert!(sc.score <= 100);
    assert_eq!(sc.parts.iter().map(|p| p.2).sum::<u32>(), 100);
    assert!(sc.parts.iter().all(|p| p.1 <= p.2));
    assert_eq!(
        sc.parts.iter().map(|p| p.1).sum::<u32>().min(100) as u8,
        sc.score
    );
    let _ = subscription_totals(&data, now());
    let _ = payment_method_index(&data);
    let _ = alias_recommendations(&data);
    let overdue = overdue_requests(&data, now());
    assert!(overdue.len() <= data.requests.len());
}

#[test]
fn overdue_boundaries() {
    let mut data = VaultData::default();
    let mk = |deadline: Option<DateTime<Utc>>, status| DataRequest {
        id: Uuid::new_v4(),
        kind: RequestKind::Access,
        target: "t".into(),
        domain: None,
        account_id: None,
        subscription_id: None,
        parent_id: None,
        channel: RequestChannel::Post,
        recipient: String::new(),
        subject: String::new(),
        body: String::new(),
        created_at: now(),
        sent_at: Some(now()),
        deadline,
        status,
        notes: String::new(),
    };
    data.requests.push(mk(Some(now()), RequestStatus::Sent)); // exactly now: not overdue yet
    data.requests
        .push(mk(Some(now() - Duration::seconds(1)), RequestStatus::Sent));
    data.requests
        .push(mk(Some(now() - Duration::days(9)), RequestStatus::Sent));
    data.requests.push(mk(None, RequestStatus::Sent));
    data.requests
        .push(mk(Some(now() - Duration::days(30)), RequestStatus::Done));
    let o = overdue_requests(&data, now());
    assert_eq!(o.len(), 2);
    assert_eq!(o[0], data.requests[2].id, "oldest deadline first");
}

#[test]
fn privacy_score_with_an_empty_vault_is_within_bounds() {
    let s = privacy_score(&VaultData::default(), &RiskContext::default(), now());
    assert!(s.score <= 100);
    assert_eq!(s.open_accounts, 0);
    assert!(!s.has_data);
}

#[test]
fn privacy_score_of_an_empty_vault_is_not_perfect() {
    let s = privacy_score(&VaultData::default(), &RiskContext::default(), now());
    assert!(s.score < 100, "empty vault scores {}", s.score);
}

#[test]
fn alias_and_relay_helpers() {
    assert!(is_relay_domain("privaterelay.appleid.com"));
    assert!(is_relay_domain("X.SimpleLogin.com."));
    assert!(!is_relay_domain("notprivaterelay.appleid.com.evil.com"));
    assert!(!is_relay_domain("evilsimplelogin.com"));
    for (s, expect) in [
        ("+491701234567", true),
        ("0170 1234567", true),
        ("(030) 123-456", true),
        ("12345", false),
        ("0000", false),
        ("user@example.org", false),
        ("+49 abc", false),
        ("", false),
        ("+", false),
        ("0123456789012345678", false),
    ] {
        assert_eq!(looks_like_phone(s), expect, "{s:?}");
    }
    let mut data = VaultData::default();
    data.mailboxes.push(MailboxConfig {
        id: Uuid::nil(),
        address: "Me@Example.org".into(),
        imap_host: String::new(),
        imap_port: 993,
        username: String::new(),
        password: String::new(),
        folders: vec![],
        progress: vec![],
        last_scan: None,
    });
    let mut a = Account::new("a.example.com", "A");
    a.status = AccountStatus::Keep;
    a.identifiers = vec!["me@example.org".into()];
    let mut b = Account::new("b.example.com", "B");
    b.status = AccountStatus::Confirmed;
    b.identifiers = vec!["me@example.org".into(), "x@privaterelay.appleid.com".into()];
    data.accounts = vec![a, b];
    assert_eq!(alias_recommendations(&data).len(), 1);
}
