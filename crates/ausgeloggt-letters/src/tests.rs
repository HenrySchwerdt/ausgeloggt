use super::*;
use ausgeloggt_core::insights::PrivacyScore;
use ausgeloggt_core::{
    Account, AccountStatus, DataRequest, MailRef, PaymentKind, PaymentMethod, PaymentRecord,
    RequestChannel, RequestKind, RequestStatus, Settings, SsoProvider, Subscription,
    UnsubscribeAttempt, UnsubscribeMethod, VaultData,
};
use chrono::TimeZone;
use uuid::Uuid;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 12, 10, 0, 0).unwrap()
}

fn settings() -> Settings {
    let mut s = Settings {
        full_name: "Erika Müller".into(),
        ..Settings::default()
    };
    s.identity.street = "Musterstraße 12".into();
    s.identity.postal_code = "10115".into();
    s.identity.city = "Berlin".into();
    s.identity.email = "erika@example.org".into();
    s.identity.birth_date = "1985-03-07".into();
    s.identity.previous_addresses = vec!["Alte Gasse 3, 80331 München".into()];
    s
}

fn recipient() -> Party {
    Party {
        name: "Beispiel Handels GmbH".into(),
        address_lines: vec![
            "Datenschutzbeauftragter".into(),
            "Hafenstraße 1".into(),
            "20457 Hamburg".into(),
        ],
        email: Some("datenschutz@beispiel.de".into()),
    }
}

fn account() -> Account {
    let mut a = Account::new("beispiel.de", "Beispiel Shop");
    a.identifiers = vec![
        "erika@example.org".into(),
        "shop.abc123@relay.example".into(),
    ];
    a
}

fn mail(i: u32) -> MailRef {
    MailRef {
        date: Some(Utc.with_ymd_and_hms(2026, 10, 1, 8, 30, 0).unwrap() + chrono::Duration::hours(i64::from(i) * 29)),
        from: "Beispiel Shop <news@beispiel.de>".into(),
        subject: format!("Nur heute: 20 % Rabatt, Größe M–XL ({i})"),
        message_id: Some(format!("<abc{i}.def@mailer.beispiel.de>")),
        raw_headers: format!(
            "From: Beispiel Shop <news@beispiel.de>\nTo: erika@example.org\nSubject: Nur heute ({i})\nMessage-ID: <abc{i}.def@mailer.beispiel.de>\nDKIM-Signature: v=1; a=rsa-sha256; d=beispiel.de; s=sel; h=from:to:subject; bh=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n\tb=BBBBBBBBBB"
        ),
    }
}

fn subscription(n_mails: u32) -> Subscription {
    Subscription {
        id: Uuid::new_v4(),
        mailbox: "erika@example.org".into(),
        list_key: "news@beispiel.de".into(),
        domain: "beispiel.de".into(),
        name: "Beispiel Newsletter".into(),
        sender: "news@beispiel.de".into(),
        unsubscribe: Vec::new(),
        message_count: 40,
        frequency_per_month: 8.0,
        first_seen: None,
        last_seen: None,
        sample_subjects: Vec::new(),
        category: Default::default(),
        category_source: Default::default(),
        status: Default::default(),
        history: vec![
            UnsubscribeAttempt {
                at: Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap(),
                method: UnsubscribeMethod::OneClick {
                    url: "https://beispiel.de/u".into(),
                },
                ok: true,
                message: "HTTP 200".into(),
            },
            UnsubscribeAttempt {
                at: Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap(),
                method: UnsubscribeMethod::Mailto {
                    address: "unsub@beispiel.de".into(),
                    subject: None,
                    body: None,
                },
                ok: false,
                message: "Zustellung abgelehnt".into(),
            },
        ],
        post_unsubscribe_mails: (0..n_mails).map(mail).collect(),
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
        subject: "Antrag auf Löschung".into(),
        body: "Sehr geehrte Damen und Herren,\n\nbitte löschen Sie meine Daten.\n\nMit freundlichen Grüßen\nErika Müller".into(),
        created_at: Utc.with_ymd_and_hms(2026, 9, 1, 9, 0, 0).unwrap(),
        sent_at: Some(Utc.with_ymd_and_hms(2026, 9, 2, 9, 0, 0).unwrap()),
        deadline: Some(Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap()),
        status: RequestStatus::Sent,
        notes: String::new(),
    message_id: None, mailbox_id: None, attachment: None,
    }
}

fn pdf_page_count(bytes: &[u8]) -> usize {
    let mut warnings = Vec::new();
    printpdf::PdfDocument::parse(bytes, &printpdf::PdfParseOptions::default(), &mut warnings)
        .expect("PDF parses")
        .pages
        .len()
}

fn all_text(d: &LetterDoc) -> String {
    render_text(d)
}

#[test]
fn sender_missing_fields_are_named() {
    let mut s = settings();
    assert!(sender_from(&s).is_ok());
    s.identity.city.clear();
    assert!(matches!(
        sender_from(&s),
        Err(LetterError::MissingSender("Ort"))
    ));
    s.identity.postal_code.clear();
    assert!(matches!(
        sender_from(&s),
        Err(LetterError::MissingSender("PLZ"))
    ));
    s.identity.street.clear();
    assert!(matches!(
        sender_from(&s),
        Err(LetterError::MissingSender("Straße"))
    ));
    s.full_name.clear();
    assert!(matches!(
        sender_from(&s),
        Err(LetterError::MissingSender("Name"))
    ));
}

#[test]
fn sender_party_has_address_and_email() {
    let p = sender_from(&settings()).unwrap();
    assert_eq!(p.name, "Erika Müller");
    assert_eq!(p.address_lines, vec!["Musterstraße 12", "10115 Berlin"]);
    assert_eq!(p.email.as_deref(), Some("erika@example.org"));
    let mut id = settings().identity;
    id.country = "Österreich".into();
    assert_eq!(identity_lines(&id).last().unwrap(), "Österreich");
}

#[test]
fn recipient_from_catalog_prefers_legal_name_and_privacy_mail() {
    let info = ausgeloggt_catalog::DeletionInfo {
        name: "Beispiel".into(),
        domains: vec!["beispiel.de".into()],
        url: None,
        difficulty: ausgeloggt_catalog::Difficulty::Easy,
        notes: None,
        email: Some("info@beispiel.de".into()),
        legal_name: Some("Beispiel Handels GmbH".into()),
        address: Some("Hafenstraße 1\n20457 Hamburg\n".into()),
        privacy_email: Some("datenschutz@beispiel.de".into()),
        slug: None,
        relevant_countries: Vec::new(),
    };
    let p = recipient_from_catalog(&info);
    assert_eq!(p.name, "Beispiel Handels GmbH");
    assert_eq!(p.address_lines, vec!["Hafenstraße 1", "20457 Hamburg"]);
    assert_eq!(p.email.as_deref(), Some("datenschutz@beispiel.de"));
}

#[test]
fn missing_recipient_is_an_error() {
    let r = Party::default();
    assert!(matches!(
        access_request(&settings(), &r, None, now()),
        Err(LetterError::MissingRecipient)
    ));
}

#[test]
fn place_and_date_are_german() {
    let d = access_request(&settings(), &recipient(), None, now()).unwrap();
    assert_eq!(d.place_date, "Berlin, 12.11.2026");
}

#[test]
fn cease_and_desist_content() {
    let d = cease_and_desist(
        &settings(),
        &recipient(),
        &subscription(3),
        Some(&account()),
        now(),
    )
    .unwrap();
    let t = all_text(&d);
    assert!(t.contains("Unterlassungsaufforderung"));
    assert!(t.contains("§ 7 Abs. 2 Nr. 3 UWG"));
    assert!(t.contains("Art. 21"));
    assert!(t.contains("§§ 823 Abs. 1, 1004"));
    assert!(t.contains("26.11.2026"), "14-day deadline");
    assert!(t.contains("Art. 15 Abs. 1 lit. g"));
    assert!(t.contains("keine Kosten"));
    // addresses incl. relay
    assert!(t.contains("erika@example.org"));
    assert!(t.contains("shop.abc123@relay.example"));
    // Hamburger Brauch clause
    assert!(t.contains("billigem Ermessen"));
    assert!(t.contains("zuständigen Gericht"));
    assert!(t.contains("Vertragsstrafe"));
    assert!(t.contains("Hamburger Brauch"));
    assert_eq!(d.deadline, Some(now() + chrono::Duration::days(14)));
    // attachments: declaration, attempts, mails
    assert_eq!(d.attachments.len(), 3);
    match &d.attachments[1] {
        Attachment::EvidenceTable { rows, .. } => {
            assert_eq!(rows.len(), 2);
            assert!(rows[0][1].contains("One-Click"));
            assert!(rows[1][2].contains("Fehlgeschlagen"));
        }
        other => panic!("unexpected {other:?}"),
    }
    match &d.attachments[2] {
        Attachment::EvidenceTable {
            rows, raw_headers, ..
        } => {
            assert_eq!(rows.len(), 3);
            assert_eq!(raw_headers.len(), 3);
            assert!(rows[0][3].contains("<abc0.def@mailer.beispiel.de>"));
            assert_eq!(rows[0][0], "01.10.2026 08:30");
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(t.contains("Message-ID"));
    // the note for the UI must not be in the letter
    assert!(!t.contains("keine Rechtsberatung"));
}

#[test]
fn cease_and_desist_without_evidence_still_works() {
    let mut sub = subscription(0);
    sub.history.clear();
    let d = cease_and_desist(&settings(), &recipient(), &sub, None, now()).unwrap();
    assert_eq!(d.attachments.len(), 1);
    assert!(render_text(&d).contains("Strafbewehrte Unterlassungserklärung"));
}

#[test]
fn objection_and_access_content() {
    let d = objection_and_access(&settings(), &recipient(), &account(), now()).unwrap();
    let t = all_text(&d);
    assert!(t.contains("Art. 21 Abs. 2 und 3 DSGVO"));
    assert!(t.contains("Art. 15"));
    assert!(t.contains("shop.abc123@relay.example"));
    assert!(t.contains("12.12.2026"));
}

#[test]
fn access_request_content() {
    let d = access_request(&settings(), &recipient(), Some(&account()), now()).unwrap();
    let t = all_text(&d);
    for needle in [
        "Art. 15",
        "Verarbeitungszwecke",
        "Empfänger",
        "Speicherdauer",
        "Herkunft",
        "lit. g",
        "Kopie",
        "Art. 15 Abs. 3",
        "unentgeltlich",
        "innerhalb eines Monats",
        "erika@example.org",
    ] {
        assert!(t.contains(needle), "missing {needle}");
    }
}

#[test]
fn cancellation_content() {
    let mut a = account();
    a.payments.push(PaymentRecord {
        date: Some(now()),
        kind: PaymentKind::Renewal,
        amount_minor: Some(999),
        currency: Some("EUR".into()),
        plan: Some("Premium".into()),
        method: Some(PaymentMethod::PayPal),
        via: None,
        summary: String::new(),
        source_ref: None,
    });
    let d = cancellation(&settings(), &recipient(), &a, None, now()).unwrap();
    let t = all_text(&d);
    assert!(t.contains("zum nächstmöglichen Zeitpunkt"));
    assert!(t.contains("Premium"));
    assert!(t.contains("SEPA-Lastschriftmandat"));
    assert!(t.contains("PayPal"));
    assert!(t.contains("Textform"));
    assert!(t.contains("Art. 17 DSGVO"));
    assert!(t.contains("26.11.2026"));
    let d2 = cancellation(&settings(), &recipient(), &a, Some("Family"), now()).unwrap();
    assert!(all_text(&d2).contains("Family"));
}

#[test]
fn credit_bureau_content() {
    let r = Party {
        name: "SCHUFA Holding AG".into(),
        address_lines: vec!["Postfach 10 34 41".into(), "50474 Köln".into()],
        email: None,
    };
    let d = credit_bureau_access(&settings(), &r, now()).unwrap();
    let t = all_text(&d);
    assert!(t.contains("Datenkopie"));
    assert!(t.contains("Geburtsdatum: 07.03.1985"));
    assert!(t.contains("Alte Gasse 3, 80331 München"));
    assert!(t.contains("Scorewerte"));
    assert!(t.contains("12 Monate"));
    assert!(t.contains("Empfänger"));
}

#[test]
fn address_broker_content() {
    let d = address_broker_objection(&settings(), &recipient(), now()).unwrap();
    let t = all_text(&d);
    assert!(t.contains("Art. 21 Abs. 2 und 3 DSGVO"));
    assert!(t.contains("Art. 15 Abs. 1 lit. c"));
    assert!(t.contains("Art. 17"));
    assert!(t.contains("Adresshandel"));
}

#[test]
fn complaint_content() {
    let req = request(RequestKind::Erasure);
    let mut follow = request(RequestKind::Reminder);
    follow.sent_at = Some(Utc.with_ymd_and_hms(2026, 10, 10, 9, 0, 0).unwrap());
    follow.parent_id = Some(req.id);
    let auth = Party {
        name: "Berliner Beauftragte für Datenschutz und Informationsfreiheit".into(),
        address_lines: vec!["Alt-Moabit 59-61".into(), "10555 Berlin".into()],
        email: None,
    };
    let d = complaint(&settings(), &auth, &req, &[follow], now()).unwrap();
    let t = all_text(&d);
    assert!(t.contains("Art. 77 DSGVO"));
    assert!(t.contains("Beispiel Handels GmbH"));
    assert!(t.contains("02.09.2026"));
    assert!(t.contains("10.10.2026"));
    assert!(t.contains("Art. 17 DSGVO"));
    assert!(t.contains("Anlagen:"));
    assert!(d.attachments.len() >= 2);
    assert!(render_pdf(&d).is_ok());
}

#[test]
fn reminder_content() {
    let req = request(RequestKind::Access);
    let d = reminder(&settings(), &recipient(), &req, now()).unwrap();
    let t = all_text(&d);
    assert!(t.contains("26.11.2026"));
    assert!(t.contains("02.09.2026"));
    assert!(t.contains("02.10.2026"));
    assert!(t.contains("Art. 77 DSGVO"));
    assert_eq!(d.deadline, Some(now() + chrono::Duration::days(14)));
}

#[test]
fn text_rendering_layout() {
    let d = cease_and_desist(
        &settings(),
        &recipient(),
        &subscription(2),
        Some(&account()),
        now(),
    )
    .unwrap();
    let t = render_text(&d);
    assert!(t.starts_with("Betreff: "));
    assert!(t.contains("Sehr geehrte Damen und Herren,"));
    assert!(t.contains("Mit freundlichen Grüßen\n\nErika Müller\nMusterstraße 12"));
    assert!(t.contains("Anlage 1: Strafbewehrte Unterlassungserklärung"));
    assert!(t.contains("Anlage 3: Nach der Abmeldung eingegangene Werbe-E-Mails"));
    assert!(t.ends_with('\n'));
}

#[test]
fn pdf_is_valid_and_single_page_for_short_letter() {
    let d = access_request(&settings(), &recipient(), Some(&account()), now()).unwrap();
    let bytes = render_pdf(&d).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert_eq!(pdf_page_count(&bytes), 2.min(pdf_page_count(&bytes)));
    assert!(pdf_page_count(&bytes) >= 1);
}

#[test]
fn pdf_page_count_grows_with_evidence() {
    let small = cease_and_desist(&settings(), &recipient(), &subscription(2), None, now()).unwrap();
    let big = cease_and_desist(&settings(), &recipient(), &subscription(80), None, now()).unwrap();
    let ps = pdf_page_count(&render_pdf(&small).unwrap());
    let pb = pdf_page_count(&render_pdf(&big).unwrap());
    assert!(
        ps >= 4,
        "letter + declaration + 2 evidence attachments, got {ps}"
    );
    assert!(pb > ps + 3, "80 mails need many pages: {ps} vs {pb}");
}

#[test]
fn pdf_survives_exotic_characters() {
    let mut d = access_request(&settings(), &recipient(), None, now()).unwrap();
    d.paragraphs
        .push("Äöüß ÄÖÜ € „Zitat“ – Gedankenstrich … 日本語 😀 http://example.org/aVeryLongTokenWithoutAnySpacesThatMustBeBrokenByTheLayoutEngineBecauseItIsLongerThanTheLine".into());
    let bytes = render_pdf(&d).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}

fn demo_data() -> VaultData {
    let mut data = VaultData {
        settings: settings(),
        ..VaultData::default()
    };
    let mut a = account();
    a.status = AccountStatus::Confirmed;
    a.sso = vec![SsoProvider::Google];
    a.payments.push(PaymentRecord {
        date: Some(now()),
        kind: PaymentKind::Renewal,
        amount_minor: Some(1299),
        currency: Some("EUR".into()),
        plan: Some("Premium Family".into()),
        method: Some(PaymentMethod::Card {
            brand: "Visa".into(),
            last4: "4242".into(),
        }),
        via: None,
        summary: String::new(),
        source_ref: None,
    });
    a.security.password_pwned = Some(true);
    data.accounts.push(a);
    let mut b = Account::new("spotify.com", "Spotify");
    b.identifiers = vec!["erika@example.org".into()];
    b.status = AccountStatus::Keep;
    data.accounts.push(b);
    data.requests.push(request(RequestKind::Erasure));
    data
}

#[test]
fn emergency_kit_renders() {
    let data = demo_data();
    let bytes = emergency_kit_pdf(&settings(), &data, now()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(pdf_page_count(&bytes) >= 2);
    // empty vault works as well
    let empty = emergency_kit_pdf(&settings(), &VaultData::default(), now()).unwrap();
    assert!(empty.starts_with(b"%PDF"));
}

fn demo_score() -> PrivacyScore {
    PrivacyScore {
        score: 62,
        parts: vec![
            ("Konten geprüft".into(), 18, 25),
            ("Zwei-Faktor-Anmeldung".into(), 8, 20),
            ("Eindeutige Passwörter".into(), 20, 20),
            ("Werbung abbestellt".into(), 4, 15),
            ("Anfragen beantwortet".into(), 12, 20),
        ],
        open_accounts: 2,
        deleted_accounts: 5,
        high_risk_accounts: 1,
        pending_requests: 1,
        overdue_requests: 1,
        has_data: true,
    }
}

#[test]
fn privacy_report_renders() {
    let bytes = privacy_report_pdf(&demo_data(), &demo_score(), now()).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    assert!(pdf_page_count(&bytes) >= 1);
}

/// Writes sample PDFs when `AUSGELOGGT_SAMPLE_DIR` is set (visual inspection).
#[test]
fn write_samples_if_requested() {
    let Ok(dir) = std::env::var("AUSGELOGGT_SAMPLE_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let s = settings();
    let rcpt = recipient();
    let acc = account();
    let w = |name: &str, d: &LetterDoc| {
        std::fs::write(dir.join(name), render_pdf(d).unwrap()).unwrap();
    };
    w(
        "cease_and_desist.pdf",
        &cease_and_desist(&s, &rcpt, &subscription(30), Some(&acc), now()).unwrap(),
    );
    w(
        "access_request.pdf",
        &access_request(&s, &rcpt, Some(&acc), now()).unwrap(),
    );
    w(
        "cancellation.pdf",
        &cancellation(&s, &rcpt, &acc, Some("Premium"), now()).unwrap(),
    );
    w(
        "objection_and_access.pdf",
        &objection_and_access(&s, &rcpt, &acc, now()).unwrap(),
    );
    w(
        "credit_bureau.pdf",
        &credit_bureau_access(&s, &rcpt, now()).unwrap(),
    );
    w(
        "address_broker.pdf",
        &address_broker_objection(&s, &rcpt, now()).unwrap(),
    );
    w(
        "complaint.pdf",
        &complaint(
            &s,
            &rcpt,
            &request(RequestKind::Erasure),
            &[request(RequestKind::Reminder)],
            now(),
        )
        .unwrap(),
    );
    w(
        "reminder.pdf",
        &reminder(&s, &rcpt, &request(RequestKind::Erasure), now()).unwrap(),
    );
    std::fs::write(
        dir.join("emergency_kit.pdf"),
        emergency_kit_pdf(&s, &demo_data(), now()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("privacy_report.pdf"),
        privacy_report_pdf(&demo_data(), &demo_score(), now()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        dir.join("cease_and_desist.txt"),
        render_text(&cease_and_desist(&s, &rcpt, &subscription(3), Some(&acc), now()).unwrap()),
    )
    .unwrap();
}
