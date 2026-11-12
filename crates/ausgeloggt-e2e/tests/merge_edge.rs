//! Merge semantics across sources and over time (order independence, aliases, naming).

use ausgeloggt_core::merge::{merge_findings, merge_payments};
use ausgeloggt_core::{
    Evidence, EvidenceKind, Finding, PaymentKind, PaymentObservation, PaymentRecord, SourceKind,
    SsoProvider, VaultData,
};
use chrono::{TimeZone, Utc};

fn finding(domain: &str, name: Option<&str>, kind: EvidenceKind, r: &str) -> Finding {
    Finding {
        domain: domain.into(),
        name: name.map(Into::into),
        identifier: Some("me@example.org".into()),
        sso: None,
        evidence: Evidence {
            kind,
            source: SourceKind::Mail {
                mailbox: "me@example.org".into(),
            },
            summary: r.into(),
            date: Some(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()),
            source_ref: Some(r.into()),
        },
    }
}

fn payment(domain: &str, name: &str, r: &str) -> PaymentObservation {
    PaymentObservation {
        domain: domain.into(),
        name: Some(name.into()),
        identifier: None,
        source: SourceKind::Mail {
            mailbox: "me@example.org".into(),
        },
        record: PaymentRecord {
            date: Some(Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap()),
            kind: PaymentKind::Renewal,
            amount_minor: Some(1399),
            currency: Some("EUR".into()),
            plan: None,
            method: None,
            via: Some("paypal.com".into()),
            summary: "x".into(),
            source_ref: Some(r.into()),
        },
    }
}

fn evidence_total(d: &VaultData) -> usize {
    d.accounts.iter().map(|a| a.evidence.len()).sum()
}

#[test]
fn domain_variants_and_aliases_share_one_account() {
    let mut d = VaultData::default();
    let f = vec![
        finding("spotify.com", None, EvidenceKind::Welcome, "a"),
        finding("mail.spotify.com", None, EvidenceKind::Receipt, "b"),
        finding("WWW.Spotify.COM.", None, EvidenceKind::Newsletter, "c"),
        finding(
            "no-reply@accounts.spotify.com",
            None,
            EvidenceKind::Verification,
            "d",
        ),
        finding(
            "https://open.spotify.com/x?y=1",
            None,
            EvidenceKind::SavedPassword,
            "e",
        ),
        finding("spotifymail.com", None, EvidenceKind::OtherMail, "f"),
        finding(
            "Spotify <no-reply@spotify.com>",
            None,
            EvidenceKind::OtherMail,
            "g",
        ),
    ];
    let rep = merge_findings(&mut d, f);
    assert_eq!(
        d.accounts.len(),
        1,
        "{:?}",
        d.accounts.iter().map(|a| &a.domain).collect::<Vec<_>>()
    );
    assert_eq!(rep.new_accounts, 1);
    assert_eq!(d.accounts[0].domain, "spotify.com");
    assert_eq!(d.accounts[0].evidence.len(), 7);
    // Infrastructure / unparsable domains are dropped, never create accounts.
    let mut d = VaultData::default();
    let rep = merge_findings(
        &mut d,
        vec![
            finding("mcsv.net", None, EvidenceKind::Newsletter, "1"),
            finding("gmail.com", None, EvidenceKind::Welcome, "2"),
            finding("em123.sendgrid.net", None, EvidenceKind::Welcome, "3"),
            finding("localhost", None, EvidenceKind::Welcome, "5"),
            finding("127.0.0.1", None, EvidenceKind::Welcome, "6"),
            finding("", None, EvidenceKind::Welcome, "7"),
            finding("unresolved:", None, EvidenceKind::Welcome, "8"),
        ],
    );
    assert!(
        d.accounts.is_empty(),
        "{:?}",
        d.accounts.iter().map(|a| &a.domain).collect::<Vec<_>>()
    );
    assert_eq!(rep.skipped, 7);
}

#[test]
fn merge_is_order_independent_for_ordinary_findings() {
    let make = || {
        vec![
            finding("a.example.com", Some("A"), EvidenceKind::Welcome, "1"),
            finding("b.example.com", Some("B"), EvidenceKind::Receipt, "2"),
            finding("a.example.com", None, EvidenceKind::Verification, "3"),
            finding("c.example.com", None, EvidenceKind::Payment, "4"),
            finding("b.example.com", None, EvidenceKind::Payment, "5"),
        ]
    };
    let summary = |d: &VaultData| {
        let mut v: Vec<_> = d
            .accounts
            .iter()
            .map(|a| {
                (
                    a.domain.clone(),
                    a.evidence.len(),
                    (a.confidence * 100.0) as i32,
                )
            })
            .collect();
        v.sort();
        v
    };
    let mut forward = VaultData::default();
    merge_findings(&mut forward, make());
    let mut reversed = VaultData::default();
    let mut m = make();
    m.reverse();
    merge_findings(&mut reversed, m);
    assert_eq!(summary(&forward), summary(&reversed));
    // One finding per call gives the same result as one batch.
    let mut single = VaultData::default();
    for f in make() {
        merge_findings(&mut single, vec![f]);
    }
    assert_eq!(summary(&forward), summary(&single));
}

#[test]
fn sso_grant_attaches_to_existing_account_by_name() {
    let mut d = VaultData::default();
    merge_findings(
        &mut d,
        vec![finding(
            "notion.so",
            Some("Notion"),
            EvidenceKind::Welcome,
            "w",
        )],
    );
    let mut sso = finding(
        "unresolved:notion",
        Some("Notion"),
        EvidenceKind::SsoGrant,
        "s",
    );
    sso.sso = Some(SsoProvider::Google);
    merge_findings(&mut d, vec![sso]);
    assert_eq!(d.accounts.len(), 1);
    assert!(d.accounts[0].sso.contains(&SsoProvider::Google));
}

#[test]
fn unresolved_account_is_adopted_when_the_real_domain_appears_later() {
    let mut d = VaultData::default();
    // First scan: only an aggregator receipt (PayPal) mentions Netflix.
    merge_payments(&mut d, vec![payment("unresolved:netflix", "Netflix", "p1")]);
    assert_eq!(d.accounts.len(), 1);
    // Later scan: Netflix's own welcome mail arrives.
    merge_findings(
        &mut d,
        vec![finding(
            "netflix.com",
            Some("Netflix"),
            EvidenceKind::Welcome,
            "w1",
        )],
    );
    assert_eq!(
        d.accounts.len(),
        1,
        "duplicate accounts: {:?}",
        d.accounts
            .iter()
            .map(|a| (&a.domain, &a.name))
            .collect::<Vec<_>>()
    );
}

#[test]
fn reimport_is_a_noop_even_with_name_collisions() {
    let make = || {
        vec![
            finding("paypal.de", None, EvidenceKind::Welcome, "r1"),
            finding("unresolved:9", None, EvidenceKind::SsoGrant, "r29"),
            finding("paypal.de", Some("9"), EvidenceKind::Receipt, "r20"),
        ]
    };
    let mut d = VaultData::default();
    merge_findings(&mut d, make());
    let before = evidence_total(&d);
    merge_findings(&mut d, make());
    assert_eq!(evidence_total(&d), before, "re-import added evidence");
}

#[test]
fn apple_relay_sender_domain_never_becomes_an_account() {
    let mut d = VaultData::default();
    merge_findings(
        &mut d,
        vec![finding(
            "privaterelay.appleid.com",
            None,
            EvidenceKind::Welcome,
            "relay",
        )],
    );
    assert!(
        d.accounts.is_empty(),
        "{:?}",
        d.accounts.iter().map(|a| &a.domain).collect::<Vec<_>>()
    );
}
