//! Unsubscribe parsing and safety rules (no network access needed).

use ausgeloggt_core::{
    CategorySource, MailCategory, MailboxConfig, Subscription, SubscriptionStatus,
    UnsubscribeMethod,
};
use ausgeloggt_mail::unsubscribe::{
    best_automatic, parse_list_unsubscribe, smtp_for, unsubscribe, UnsubscribeError,
};
use uuid::Uuid;

const OC: &str = "List-Unsubscribe=One-Click";

fn sub(methods: Vec<UnsubscribeMethod>, category: MailCategory) -> Subscription {
    Subscription {
        id: Uuid::nil(),
        mailbox: "me@example.org".into(),
        list_key: "news@shop.example".into(),
        domain: "shop.example".into(),
        name: "Shop".into(),
        sender: "news@shop.example".into(),
        unsubscribe: methods,
        message_count: 3,
        frequency_per_month: 1.0,
        first_seen: None,
        last_seen: None,
        sample_subjects: vec![],
        category,
        category_source: CategorySource::Rules,
        status: SubscriptionStatus::Active,
        history: vec![],
        post_unsubscribe_mails: vec![],
    }
}

fn cfg(address: &str) -> MailboxConfig {
    MailboxConfig {
        id: Uuid::nil(),
        address: address.into(),
        imap_host: "imap.firma.example".into(),
        imap_port: 993,
        username: address.into(),
        password: "x".into(),
        folders: vec![],
        progress: vec![],
        last_scan: None,
    }
}

#[test]
fn header_forms() {
    let m = parse_list_unsubscribe(
        "<https://a.example/u?id=1>, <mailto:unsub@a.example?subject=stop>",
        Some(OC),
    );
    assert!(
        matches!(&m[0], UnsubscribeMethod::OneClick { url } if url == "https://a.example/u?id=1")
    );
    assert!(
        matches!(&m[1], UnsubscribeMethod::Mailto { address, subject: Some(s), .. } if address == "unsub@a.example" && s == "stop")
    );
    // Without the Post header an https link is never one-click.
    let m = parse_list_unsubscribe("<https://a.example/u>", None);
    assert!(matches!(&m[0], UnsubscribeMethod::Link { .. }));
    // One-click needs https: http stays a plain link even with the Post header.
    let m = parse_list_unsubscribe("<http://a.example/u>", Some(OC));
    assert!(matches!(&m[0], UnsubscribeMethod::Link { .. }));
    // Upper-case scheme, whitespace inside brackets, folded header.
    let m = parse_list_unsubscribe(
        "< HTTPS://A.example/u >\r\n , <MAILTO:x@a.example>",
        Some("list-unsubscribe=one-click"),
    );
    assert!(matches!(&m[0], UnsubscribeMethod::OneClick { .. }), "{m:?}");
    assert!(matches!(&m[1], UnsubscribeMethod::Mailto { .. }), "{m:?}");
    // Bare (non-compliant) comma separated URIs.
    let m = parse_list_unsubscribe("https://a.example/u, mailto:x@a.example", None);
    assert_eq!(m.len(), 2);
    // Hostile schemes are dropped.
    for h in [
        "<javascript:alert(1)>",
        "<file:///etc/passwd>",
        "<data:text/html,x>",
        "<ftp://a.example/x>",
        "<https://>",
        "<mailto:>",
        "<mailto:a@b@c>",
        "",
        "<>",
        "<<<>>>",
    ] {
        assert!(
            parse_list_unsubscribe(h, Some(OC)).is_empty(),
            "{h:?} produced methods"
        );
    }
    // Duplicates collapse; only one OneClick is produced.
    let m = parse_list_unsubscribe(
        "<https://a.example/u>, <https://a.example/u>, <https://b.example/u>",
        Some(OC),
    );
    assert_eq!(
        m.iter()
            .filter(|x| matches!(x, UnsubscribeMethod::OneClick { .. }))
            .count(),
        1
    );
    assert_eq!(m.len(), 2);
}

#[test]
fn mailto_header_injection_is_neutralised() {
    let m = parse_list_unsubscribe(
        "<mailto:x@a.example?subject=stop%0d%0aBcc:%20evil@e.example&body=hi%0d%0a%0d%0abye>",
        None,
    );
    match &m[0] {
        UnsubscribeMethod::Mailto { subject, .. } => {
            let s = subject.as_deref().unwrap();
            assert!(!s.contains('\r') && !s.contains('\n'), "{s:?}");
        }
        other => panic!("{other:?}"),
    }
    // CR/LF inside the address part is rejected or cleaned.
    for m in parse_list_unsubscribe("<mailto:x%0d%0aBcc:e@e.example@a.example>", None) {
        if let UnsubscribeMethod::Mailto { address, .. } = m {
            assert!(!address.contains('\n') && !address.contains('\r'));
        }
    }
}

#[test]
fn many_uris_are_cheap() {
    let h = "<mailto:a@b.de>,".repeat(50_000);
    let t = std::time::Instant::now();
    let m = parse_list_unsubscribe(&h, None);
    assert_eq!(m.len(), 1);
    assert!(t.elapsed().as_secs() < 5);
}

#[test]
fn automatic_method_priority_and_refusals() {
    let link = UnsubscribeMethod::Link {
        url: "https://a.example/u".into(),
    };
    let mailto = UnsubscribeMethod::Mailto {
        address: "u@a.example".into(),
        subject: None,
        body: None,
    };
    let one = UnsubscribeMethod::OneClick {
        url: "https://a.example/oc".into(),
    };
    assert_eq!(
        best_automatic(&sub(
            vec![link.clone(), mailto.clone(), one.clone()],
            MailCategory::Marketing
        )),
        Some(&one)
    );
    assert_eq!(
        best_automatic(&sub(
            vec![link.clone(), mailto.clone()],
            MailCategory::Marketing
        )),
        Some(&mailto)
    );
    assert_eq!(
        best_automatic(&sub(vec![link.clone()], MailCategory::Marketing)),
        None
    );
    assert_eq!(best_automatic(&sub(vec![], MailCategory::Marketing)), None);

    let mb = cfg("me@example.org");
    assert!(matches!(
        unsubscribe(&sub(vec![one.clone()], MailCategory::Transactional), &mb),
        Err(UnsubscribeError::Refused)
    ));
    assert!(matches!(
        unsubscribe(&sub(vec![link], MailCategory::Marketing), &mb),
        Err(UnsubscribeError::NoAutomaticMethod)
    ));
    assert!(matches!(
        unsubscribe(&sub(vec![], MailCategory::Spam), &mb),
        Err(UnsubscribeError::NoAutomaticMethod)
    ));
    // One-click over plain http (only possible through hand-edited vault data) is refused offline.
    let http = UnsubscribeMethod::OneClick {
        url: "http://a.example/oc".into(),
    };
    assert!(matches!(
        unsubscribe(&sub(vec![http], MailCategory::Marketing), &mb),
        Err(UnsubscribeError::NoAutomaticMethod)
    ));
    // Invalid mailto targets fail before any connection.
    let bad = UnsubscribeMethod::Mailto {
        address: "not an address".into(),
        subject: None,
        body: None,
    };
    assert!(matches!(
        unsubscribe(&sub(vec![bad], MailCategory::Marketing), &mb),
        Err(UnsubscribeError::Smtp(_))
    ));
}

#[test]
fn smtp_settings_guessing_never_panics() {
    for a in [
        "me@example.org",
        "x@gmail.com",
        "x@gmx.de",
        "x@proton.me",
        "",
        "@",
        "x@",
        "x@localhost",
        "x@a.b.c.d",
        "ÄÖ@ÜÖ.de",
        "x@y@z.de",
    ] {
        let _ = smtp_for(&cfg(a));
    }
    assert_eq!(
        smtp_for(&cfg("me@firma.example")).unwrap().host,
        "smtp.firma.example"
    );
    assert!(smtp_for(&cfg("no-at-sign")).is_none());
}

#[test]
fn one_click_refuses_loopback_and_private_targets() {
    let mb = cfg("me@example.org");
    for url in [
        "https://127.0.0.1:1/x",
        "https://localhost:1/x",
        "https://[::1]:1/x",
        "https://192.168.0.1:1/x",
        "https://169.254.169.254/latest/meta-data",
    ] {
        let r = unsubscribe(
            &sub(
                vec![UnsubscribeMethod::OneClick { url: url.into() }],
                MailCategory::Marketing,
            ),
            &mb,
        );
        assert!(
            !matches!(
                r,
                Err(UnsubscribeError::Network(_)) | Err(UnsubscribeError::Http(_)) | Ok(_)
            ),
            "{url}: a request was attempted ({r:?})"
        );
    }
}
