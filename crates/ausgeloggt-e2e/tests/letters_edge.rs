//! Letters: every kind with extreme inputs; PDFs must be valid; template wording rules.

use std::time::Instant;

use ausgeloggt_catalog::gdpr::deadline_from;
use ausgeloggt_core::insights::{privacy_score, RiskContext};
use ausgeloggt_core::{
    Account, DataRequest, MailRef, PaymentKind, PaymentMethod, PaymentRecord, RequestChannel,
    RequestKind, RequestStatus, Settings, Subscription, UnsubscribeAttempt, UnsubscribeMethod,
    VaultData,
};
use ausgeloggt_letters::*;
use chrono::{DateTime, Duration, TimeZone, Utc};
use uuid::Uuid;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 12, 10, 0, 0).unwrap()
}

fn settings() -> Settings {
    let mut s = Settings {
        full_name: "Erika Mustermann".into(),
        ..Settings::default()
    };
    s.identity.street = "Musterstrasse 12".into();
    s.identity.postal_code = "10115".into();
    s.identity.city = "Berlin".into();
    s.identity.email = "erika@example.org".into();
    s.identity.birth_date = "1985-03-07".into();
    s.identity.previous_addresses = vec!["Alte Gasse 3, 80331 Muenchen".into()];
    s
}

fn extreme_settings() -> Settings {
    let long = "Maximiliane-Charlotte von und zu ".repeat(8);
    let mut s = settings();
    s.full_name = long.clone();
    s.identity.street = format!("{long} Strasse 123456789");
    s.identity.postal_code = "1".repeat(40);
    s.identity.city = long.clone();
    s.identity.country = "Bundesrepublik ".repeat(10);
    s.identity.email = format!("{}@example.org", "a".repeat(200));
    s.identity.previous_addresses = (0..30)
        .map(|i| format!("Alte Adresse {i} {long}"))
        .collect();
    s.identity.birth_date = "not a date \u{0}".into();
    s
}

fn nasty_settings() -> Settings {
    let mut s = settings();
    s.full_name = "Èrïka 日本語 😀 مرحبا \u{202e}Mustermann\n\r\t".into();
    s.identity.street = "Straße\n\nMit Zeilenumbruch 1\u{0}".into();
    s.identity.city = "Zürich 🏔".into();
    s
}

fn recipient() -> Party {
    Party {
        name: "Beispiel Handels GmbH".into(),
        address_lines: vec![
            "Datenschutzbeauftragter".into(),
            "Hafenstrasse 1".into(),
            "20457 Hamburg".into(),
        ],
        email: Some("datenschutz@beispiel.de".into()),
    }
}

fn no_address_recipient() -> Party {
    Party {
        name: "Nur Name".into(),
        address_lines: vec![],
        email: None,
    }
}

fn account() -> Account {
    let mut a = Account::new("beispiel.de", "Beispiel Shop");
    a.identifiers = vec!["erika@example.org".into()];
    a.payments.push(PaymentRecord {
        date: Some(now() - Duration::days(30)),
        kind: PaymentKind::Renewal,
        amount_minor: Some(999),
        currency: Some("EUR".into()),
        plan: Some("Premium".into()),
        method: Some(PaymentMethod::Card {
            brand: "Visa".into(),
            last4: "4242".into(),
        }),
        via: None,
        summary: "Abo".into(),
        source_ref: None,
    });
    a
}

fn mail(subject: &str, i: i64) -> MailRef {
    MailRef {
        date: Some(Utc.with_ymd_and_hms(2026, 10, 1, 8, 30, 0).unwrap() + Duration::hours(i * 7)),
        from: "Beispiel Shop <news@beispiel.de>".into(),
        subject: subject.to_string(),
        message_id: Some(format!("<abc{i}.def@mailer.beispiel.de>")),
        raw_headers: format!(
            "From: Beispiel Shop <news@beispiel.de>\nSubject: {subject}\nMessage-ID: <abc{i}@x>\n"
        ),
    }
}

fn subscription(mails: Vec<MailRef>) -> Subscription {
    Subscription {
        id: Uuid::new_v4(),
        mailbox: "erika@example.org".into(),
        list_key: "news@beispiel.de".into(),
        domain: "beispiel.de".into(),
        name: "Beispiel Newsletter".into(),
        sender: "news@beispiel.de".into(),
        unsubscribe: vec![],
        message_count: 400,
        frequency_per_month: 8.0,
        first_seen: None,
        last_seen: None,
        sample_subjects: vec![],
        category: Default::default(),
        category_source: Default::default(),
        status: Default::default(),
        history: vec![UnsubscribeAttempt {
            at: Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap(),
            method: UnsubscribeMethod::OneClick {
                url: "https://beispiel.de/u".into(),
            },
            ok: true,
            message: "HTTP 200".into(),
        }],
        post_unsubscribe_mails: mails,
        last_message_id: None,
    }
}

fn request(kind: RequestKind) -> DataRequest {
    DataRequest {
        id: Uuid::new_v4(),
        kind,
        target: "Beispiel Handels GmbH".into(),
        domain: Some("beispiel.de".into()),
        account_id: None,
        subscription_id: None,
        parent_id: None,
        channel: RequestChannel::Email,
        recipient: "datenschutz@beispiel.de".into(),
        subject: "Antrag".into(),
        body: "Bitte loeschen.".into(),
        created_at: Utc.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(),
        sent_at: Some(Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap()),
        deadline: Some(Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap()),
        status: RequestStatus::Sent,
        notes: String::new(),
        message_id: None,
        mailbox_id: None,
        attachment: None,
    }
}

fn nasty_subjects() -> Vec<String> {
    vec![
        "Nur heute: 20 % Rabatt, Größe M–XL".into(),
        "🎉🎉🎉 Mega Sale 😀".into(),
        "日本語の件名 セール".into(),
        "عرض خاص اليوم فقط".into(),
        "שלום עולם".into(),
        "x".repeat(5000),
        "".into(),
        "   ".into(),
        "line\nbreak\r\nin subject\ttab".into(),
        "nul\0byte".into(),
        "\u{202e}rtl override".into(),
        "<script>alert(1)</script> & \"quotes\" 'single' %s %d {0}".into(),
        "=?UTF-8?B?bm90IGRlY29kZWQ=?=".into(),
        "A\u{0301}\u{0302}\u{0303}\u{0304} combining".into(),
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} family".into(),
    ]
}

/// A valid PDF: header, trailer, plausible size, and a sane number of pages.
fn check_pdf(bytes: &[u8], max_pages: usize, what: &str) -> usize {
    assert!(bytes.starts_with(b"%PDF-"), "{what}: header");
    let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(64)..]).to_string();
    assert!(tail.contains("%%EOF"), "{what}: trailer missing: {tail:?}");
    assert!(
        bytes.len() > 1000 && bytes.len() < 60 * 1024 * 1024,
        "{what}: {} bytes",
        bytes.len()
    );
    let hay = String::from_utf8_lossy(bytes);
    let pages = hay.matches("/Type /Page\n").count()
        + hay.matches("/Type /Page ").count()
        + hay.matches("/Type /Page>").count()
        + hay.matches("/Type/Page/").count()
        + hay.matches("/Type/Page>").count()
        + hay.matches("/Type/Page ").count()
        + hay.matches("/Type/Page\n").count();
    assert!(
        pages >= 1 && pages <= max_pages,
        "{what}: {pages} pages (max {max_pages})"
    );
    pages
}

/// All letter builders with the given inputs.
fn all_letters(
    s: &Settings,
    rcp: &Party,
    sub: &Subscription,
    acct: &Account,
) -> Vec<(&'static str, Result<LetterDoc, LetterError>)> {
    let req = request(RequestKind::Erasure);
    vec![
        ("access", access_request(s, rcp, Some(acct), now())),
        ("access-no-account", access_request(s, rcp, None, now())),
        (
            "objection+access",
            objection_and_access(s, rcp, acct, now()),
        ),
        (
            "cancellation",
            cancellation(s, rcp, acct, Some("Premium Jahresabo"), now()),
        ),
        (
            "cancellation-no-plan",
            cancellation(s, rcp, acct, None, now()),
        ),
        (
            "cease-and-desist",
            cease_and_desist(s, rcp, sub, Some(acct), now()),
        ),
        (
            "cease-and-desist-no-account",
            cease_and_desist(s, rcp, sub, None, now()),
        ),
        (
            "complaint",
            complaint(s, rcp, &req, &[request(RequestKind::Reminder)], now()),
        ),
        ("reminder", reminder(s, rcp, &req, now())),
        ("credit-bureau", credit_bureau_access(s, rcp, now())),
        ("address-broker", address_broker_objection(s, rcp, now())),
    ]
}

#[test]
fn every_letter_kind_renders_text_and_valid_pdf() {
    let sub = subscription((0..12).map(|i| mail("Rabatt", i)).collect());
    let acct = account();
    for (name, res) in all_letters(&settings(), &recipient(), &sub, &acct) {
        let doc = res.unwrap_or_else(|e| panic!("{name}: {e}"));
        let text = render_text(&doc);
        assert!(text.contains("Erika Mustermann"), "{name}");
        assert!(text.starts_with("Betreff:"), "{name}");
        let pdf = render_pdf(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        let pages = check_pdf(&pdf, 12, name);
        println!(
            "LETTER {name}: {} bytes PDF, {pages} page(s), {} chars text",
            pdf.len(),
            text.len()
        );
    }
}

#[test]
fn missing_sender_fields_are_reported_in_order() {
    let sub = subscription(vec![mail("x", 0)]);
    let acct = account();
    #[allow(clippy::type_complexity)]
    let cases: Vec<(&str, Box<dyn Fn(&mut Settings)>)> = vec![
        (
            "Name",
            Box::new(|s: &mut Settings| s.full_name = "  ".into()),
        ),
        (
            "Straße",
            Box::new(|s: &mut Settings| s.identity.street.clear()),
        ),
        (
            "PLZ",
            Box::new(|s: &mut Settings| s.identity.postal_code = " ".into()),
        ),
        ("Ort", Box::new(|s: &mut Settings| s.identity.city.clear())),
    ];
    for (field, mutate) in cases {
        let mut s = settings();
        mutate(&mut s);
        for (name, res) in all_letters(&s, &recipient(), &sub, &acct) {
            match res {
                Err(LetterError::MissingSender(f)) => assert_eq!(f, field, "{name}"),
                other => panic!("{name} with empty {field}: {:?}", other.map(|d| d.subject)),
            }
        }
        assert!(matches!(sender_from(&s), Err(LetterError::MissingSender(f)) if f == field));
    }
    // Everything empty: the first missing field (Name) wins.
    let empty = Settings::default();
    assert!(matches!(
        sender_from(&empty),
        Err(LetterError::MissingSender("Name"))
    ));
}

#[test]
fn missing_recipient_is_reported() {
    let sub = subscription(vec![mail("x", 0)]);
    let acct = account();
    let nameless = Party::default();
    let blank = Party {
        name: "   ".into(),
        address_lines: vec!["Strasse 1".into()],
        email: Some("a@b.de".into()),
    };
    for rcp in [&nameless, &blank] {
        for (name, res) in all_letters(&settings(), rcp, &sub, &acct) {
            assert!(matches!(res, Err(LetterError::MissingRecipient)), "{name}");
        }
    }
}

#[test]
fn recipient_without_address_lines_still_produces_a_pdf() {
    let sub = subscription(vec![mail("x", 0)]);
    let acct = account();
    for (name, res) in all_letters(&settings(), &no_address_recipient(), &sub, &acct) {
        let doc = res.unwrap();
        let pdf = render_pdf(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        check_pdf(&pdf, 12, name);
        let _ = render_text(&doc);
    }
}

#[test]
fn extreme_identity_and_name_lengths() {
    let sub = subscription((0..5).map(|i| mail("x", i)).collect());
    let acct = account();
    for (label, s) in [("extreme", extreme_settings()), ("nasty", nasty_settings())] {
        for (name, res) in all_letters(&s, &recipient(), &sub, &acct) {
            let doc = res.unwrap_or_else(|e| panic!("{label}/{name}: {e}"));
            let _ = render_text(&doc);
            let pdf = render_pdf(&doc).unwrap_or_else(|e| panic!("{label}/{name}: {e}"));
            check_pdf(&pdf, 30, &format!("{label}/{name}"));
        }
    }
    // Recipient with absurd lengths too.
    let long = "L".repeat(500);
    let rcp = Party {
        name: long.clone(),
        address_lines: (0..40).map(|_| long.clone()).collect(),
        email: Some(format!("{long}@example.org")),
    };
    let doc = access_request(&settings(), &rcp, None, now()).unwrap();
    check_pdf(&render_pdf(&doc).unwrap(), 30, "long recipient");
}

#[test]
fn three_hundred_evidence_mails_with_nasty_subjects() {
    let subjects = nasty_subjects();
    let mails: Vec<MailRef> = (0..300)
        .map(|i| mail(&subjects[i as usize % subjects.len()], i))
        .collect();
    let sub = subscription(mails);
    let acct = account();
    let t = Instant::now();
    let doc = cease_and_desist(&settings(), &recipient(), &sub, Some(&acct), now()).unwrap();
    let text = render_text(&doc);
    let pdf = render_pdf(&doc).unwrap();
    let pages = check_pdf(&pdf, 150, "300 mails");
    println!(
        "LETTER cease-and-desist with 300 evidence mails: {pages} pages, {} KiB, {:?}",
        pdf.len() / 1024,
        t.elapsed()
    );
    assert!(
        text.contains("300") || text.matches("abc").count() >= 100,
        "evidence rows missing in text"
    );
    assert!(t.elapsed().as_secs() < 20);
}

#[test]
fn evidence_mail_with_giant_raw_headers_and_dateless_mails() {
    let mut m = mail("Giant", 0);
    m.raw_headers = "X-Giant: ".to_string() + &"A".repeat(2_000_000);
    let mut dateless = mail("No date", 1);
    dateless.date = None;
    dateless.message_id = None;
    let mut no_from = mail("No from", 2);
    no_from.from = String::new();
    let sub = subscription(vec![m, dateless, no_from]);
    let doc = cease_and_desist(&settings(), &recipient(), &sub, None, now()).unwrap();
    let t = Instant::now();
    let pdf = render_pdf(&doc).unwrap();
    println!(
        "LETTER with 2 MB raw header: {} KiB, {:?}",
        pdf.len() / 1024,
        t.elapsed()
    );
    check_pdf(&pdf, 400, "giant header");
}

#[test]
fn cease_and_desist_without_history_or_mails() {
    let mut sub = subscription(vec![]);
    sub.history.clear();
    let doc = cease_and_desist(&settings(), &recipient(), &sub, None, now()).unwrap();
    check_pdf(&render_pdf(&doc).unwrap(), 10, "empty evidence");
}

#[test]
fn complaint_with_many_followups_and_odd_requests() {
    let mut req = request(RequestKind::Access);
    req.sent_at = None;
    req.deadline = None;
    req.notes = "Notiz äöü 😀\n\n".repeat(200);
    let followups: Vec<DataRequest> = (0..60)
        .map(|i| {
            let mut r = request(if i % 2 == 0 {
                RequestKind::Reminder
            } else {
                RequestKind::Erasure
            });
            r.sent_at = (i % 3 != 0).then(|| now() - Duration::days(i));
            r.subject = "Betreff 日本語 😀".repeat(20);
            r
        })
        .collect();
    let doc = complaint(&settings(), &recipient(), &req, &followups, now()).unwrap();
    check_pdf(&render_pdf(&doc).unwrap(), 150, "complaint");
    for kind in [
        RequestKind::Erasure,
        RequestKind::Access,
        RequestKind::Objection,
        RequestKind::Cancellation,
        RequestKind::CeaseAndDesist,
        RequestKind::Complaint,
        RequestKind::Reminder,
        RequestKind::CreditBureauAccess,
        RequestKind::AdvertisingObjection,
    ] {
        let r = request(kind);
        let d = reminder(&settings(), &recipient(), &r, now()).unwrap();
        check_pdf(&render_pdf(&d).unwrap(), 10, &format!("reminder {kind:?}"));
        let d = complaint(&settings(), &recipient(), &r, &[], now()).unwrap();
        check_pdf(&render_pdf(&d).unwrap(), 10, &format!("complaint {kind:?}"));
    }
}

#[test]
fn our_template_wording_has_no_em_or_en_dashes_in_bodies() {
    // ASCII-only inputs, so any dash comes from our own templates.
    let sub = subscription((0..3).map(|i| mail("Rabatt", i)).collect());
    let acct = account();
    let mut bad = Vec::new();
    for (name, res) in all_letters(&settings(), &recipient(), &sub, &acct) {
        let doc = res.unwrap();
        for p in std::iter::once(&doc.subject)
            .chain(std::iter::once(&doc.salutation))
            .chain(std::iter::once(&doc.closing))
            .chain(doc.paragraphs.iter())
        {
            if p.contains('—') || p.contains('–') {
                bad.push(format!("{name}: {p:.80}"));
            }
        }
    }
    assert!(bad.is_empty(), "dashes in letter bodies: {bad:#?}");
}

#[test]
fn our_template_wording_has_no_dashes_anywhere_in_rendered_text() {
    let sub = subscription(
        (0..3)
            .map(|i| {
                let mut m = mail("Rabatt", i);
                m.message_id = None;
                m
            })
            .collect(),
    );
    let acct = account();
    let mut req = request(RequestKind::Erasure);
    req.deadline = None;
    let mut bad = Vec::new();
    let mut all = all_letters(&settings(), &recipient(), &sub, &acct);
    all.push((
        "complaint-no-deadline",
        complaint(&settings(), &recipient(), &req, &[], now()),
    ));
    for (name, res) in all {
        let text = render_text(&res.unwrap());
        for line in text.lines().filter(|l| l.contains('—') || l.contains('–')) {
            bad.push(format!("{name}: {line:.100}"));
        }
    }
    assert!(
        bad.is_empty(),
        "dashes in rendered letters:\n{}",
        bad.join("\n")
    );
}

#[test]
fn catalog_letters_and_deadlines() {
    // GDPR deadline: one calendar month, month ends and leap years.
    let d = |y, m, dd| Utc.with_ymd_and_hms(y, m, dd, 12, 0, 0).unwrap();
    assert_eq!(deadline_from(d(2026, 1, 31)), d(2026, 2, 28));
    assert_eq!(deadline_from(d(2028, 1, 31)), d(2028, 2, 29));
    assert_eq!(deadline_from(d(2028, 2, 29)), d(2028, 3, 29));
    assert_eq!(deadline_from(d(2027, 2, 28)), d(2027, 3, 28));
    assert_eq!(deadline_from(d(2026, 3, 31)), d(2026, 4, 30));
    assert_eq!(deadline_from(d(2026, 12, 31)), d(2027, 1, 31));
    assert_eq!(deadline_from(d(2026, 5, 31)), d(2026, 6, 30));
    // far future / past never panics
    let far = Utc.with_ymd_and_hms(9999, 12, 31, 0, 0, 0).unwrap();
    let _ = deadline_from(far);

    let acct = account();
    let s = settings();
    let letter =
        ausgeloggt_catalog::gdpr::erasure_request(&acct, &s, Some("datenschutz@beispiel.de"));
    assert!(!letter.body.is_empty());
    let none = ausgeloggt_catalog::gdpr::erasure_request(&acct, &Settings::default(), None);
    let _ = ausgeloggt_catalog::gdpr::mailto_url(&none);
    let nasty = ausgeloggt_catalog::gdpr::erasure_request(
        &acct,
        &nasty_settings(),
        Some("a b@c.de?x=1&y=2"),
    );
    let url = ausgeloggt_catalog::gdpr::mailto_url(&nasty);
    assert!(url.starts_with("mailto:"));
    assert!(
        !url.contains('\n') && !url.contains(' '),
        "unescaped characters in mailto URL: {url}"
    );
}

#[test]
fn kits_with_empty_and_large_vaults() {
    let s = settings();
    let empty = VaultData::default();
    let now = now();
    let pdf = emergency_kit_pdf(&s, &empty, now).unwrap();
    check_pdf(&pdf, 5, "empty emergency kit");
    let score = privacy_score(&empty, &RiskContext::default(), now);
    check_pdf(
        &privacy_report_pdf(&empty, &score, now).unwrap(),
        5,
        "empty report",
    );

    // Large vault: 1,500 accounts, nasty names, payments, SSO.
    let mut data = VaultData::default();
    for i in 0..1500 {
        let mut a = Account::new(
            format!("svc{i}.example.com"),
            format!("{} Dienst {i} 😀 日本語", "N".repeat(i % 120)),
        );
        a.identifiers = vec![format!("user{i}@example.org"), "x".repeat(i % 300)];
        a.payments.push(PaymentRecord {
            date: Some(now - Duration::days(i as i64)),
            kind: PaymentKind::Renewal,
            amount_minor: Some(i as i64 * 7),
            currency: Some("EUR".into()),
            plan: Some("P".repeat(i % 80)),
            method: Some(PaymentMethod::PayPal),
            via: None,
            summary: String::new(),
            source_ref: None,
        });
        data.accounts.push(a);
    }
    let t = Instant::now();
    let pdf = emergency_kit_pdf(&s, &data, now).unwrap();
    let pages = check_pdf(&pdf, 600, "large emergency kit");
    println!(
        "EMERGENCY KIT 1500 accounts: {pages} pages, {} KiB, {:?}",
        pdf.len() / 1024,
        t.elapsed()
    );
    let score = privacy_score(&data, &RiskContext::default(), now);
    let t = Instant::now();
    let pdf = privacy_report_pdf(&data, &score, now).unwrap();
    check_pdf(&pdf, 300, "large report");
    println!(
        "PRIVACY REPORT 1500 accounts: {} KiB, {:?}",
        pdf.len() / 1024,
        t.elapsed()
    );
    // Missing owner name: emergency kit still renders.
    let anon = Settings::default();
    check_pdf(
        &emergency_kit_pdf(&anon, &data, now).unwrap(),
        600,
        "anonymous kit",
    );
}

#[test]
fn identity_lines_edge_cases() {
    let mut id = ausgeloggt_core::Identity::default();
    assert!(identity_lines(&id).is_empty());
    id.country = "Germany".into();
    assert!(identity_lines(&id).is_empty());
    id.country = "Österreich".into();
    assert_eq!(identity_lines(&id), vec!["Österreich".to_string()]);
    id.postal_code = "  ".into();
    id.city = "  Wien ".into();
    assert_eq!(
        identity_lines(&id),
        vec!["Wien".to_string(), "Österreich".to_string()]
    );
}

#[test]
fn extreme_dates_never_panic() {
    let _ = deadline_from(DateTime::<Utc>::MAX_UTC);
    let _ = deadline_from(DateTime::<Utc>::MIN_UTC);
    let acct = account();
    let _ = access_request(
        &settings(),
        &recipient(),
        Some(&acct),
        DateTime::<Utc>::MAX_UTC,
    );
    let _ = reminder(
        &settings(),
        &recipient(),
        &request(RequestKind::Erasure),
        DateTime::<Utc>::MAX_UTC,
    );
}
