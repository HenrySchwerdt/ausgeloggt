//! Fuzz-style "no panic" tests over every parser / classifier entry point.
//!
//! A deterministic random generator feeds arbitrary and mutated inputs. Panics are caught,
//! de-duplicated by message and reported together with a minimal example input, so one run
//! lists every crash instead of stopping at the first. Set `E2E_FUZZ_ITERS` to scale.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::AtomicBool;
use std::sync::Once;
use std::time::Instant;

use ausgeloggt_core::domain::{canonical_service_domain, display_name_for, registrable_domain};
use ausgeloggt_core::merge::{merge_findings, merge_payments, merge_subscriptions};
use ausgeloggt_core::vault::Vault;
use ausgeloggt_core::{
    Evidence, EvidenceKind, Finding, MailCategory, PaymentKind, PaymentObservation, PaymentRecord,
    SourceKind, SsoProvider, SubscriptionObservation, VaultData,
};
use ausgeloggt_e2e::corpus::OWN;
use ausgeloggt_e2e::fake_imap::FakeImap;
use ausgeloggt_e2e::rng::Rng;
use ausgeloggt_mail::classify::{self, MailHeader};
use ausgeloggt_mail::{receipts, scan_mailbox_with, unsubscribe, ScanOptions};

thread_local! {
    static QUIET: Cell<bool> = const { Cell::new(false) };
}
static HOOK: Once = Once::new();

fn install_hook() {
    HOOK.call_once(|| {
        let prev = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if !QUIET.with(Cell::get) {
                prev(info);
            }
        }));
    });
}

fn iters(default: usize) -> usize {
    std::env::var("E2E_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Default)]
struct Crashes(BTreeMap<String, (usize, String)>);

impl Crashes {
    fn run<F: FnOnce()>(&mut self, name: &str, input: &dyn Fn() -> String, f: F) {
        install_hook();
        QUIET.with(|q| q.set(true));
        let r = panic::catch_unwind(AssertUnwindSafe(f));
        QUIET.with(|q| q.set(false));
        if let Err(e) = r {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "?".into());
            let key = format!("{name}: {}", msg.lines().next().unwrap_or(""));
            let ex = self
                .0
                .entry(key)
                .or_insert((0, input().chars().take(300).collect()));
            ex.0 += 1;
        }
    }

    fn report(self, what: &str) {
        if !self.0.is_empty() {
            let mut s = format!("{what}: {} distinct panic(s)\n", self.0.len());
            for (k, (n, ex)) in &self.0 {
                s.push_str(&format!("  [{n}x] {k}\n      e.g. input {ex:?}\n"));
            }
            panic!("{s}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------------------------

const DOMAINS: &[&str] = &[
    "spotify.com",
    "paypal.de",
    "accounts.google.com",
    "privaterelay.appleid.com",
    "email.apple.com",
    "github.com",
    "amazon.de",
    "mail.notion.so",
    "gmail.com",
    "gmx.de",
    "xn--mller-kva.de",
    "co.uk",
    "a.b.c.d.e.f.example.co.uk",
    "localhost",
    "127.0.0.1",
    "[::1]",
    "",
    ".",
    "..",
    "-",
    "ÄÖÜ.de",
];

const SUBJECTS: &[&str] = &[
    "Willkommen bei Spotify",
    "Welcome to GitHub",
    "Deine Rechnung über 9,99 €",
    "Your receipt from Apple – €10.99",
    "Sicherheitswarnung",
    "Security alert",
    "A third-party OAuth application has been added to your account",
    "App connected to your Microsoft account",
    "Sie haben eine automatische Zahlung an Netflix gesendet",
    "Preisanpassung für dein Abo",
    "Kündigungsbestätigung",
    "Erstattung über 9,99 €",
    "Your free trial has started",
    "Reset your password",
    "Dein Bestätigungscode: 123456",
    "20% Rabatt auf alles",
    "=?UTF-8?B?V2lsbGtvbW1lbg==?=",
    "=?UTF-8?Q?Willkommen_bei_Foo?=",
    "=?utf-8?q?=C3=A4=C3=B6=C3=BC?= =?utf-8?b?4pyT?=",
];

fn pick_nasty(rng: &mut Rng) -> String {
    let n = rng.below(12) as usize;
    rng.nasty_string(n)
}

fn random_text(rng: &mut Rng) -> String {
    match rng.below(5) {
        0 => pick_nasty(rng),
        1 => (*rng.pick(SUBJECTS)).to_string(),
        2 => format!("{} {}", rng.pick(SUBJECTS), pick_nasty(rng)),
        3 => {
            let n = rng.below(200) as usize;
            String::from_utf8_lossy(&rng.bytes(n)).into_owned()
        }
        _ => format!("{}{}", pick_nasty(rng), rng.pick(SUBJECTS)),
    }
}

fn random_addr(rng: &mut Rng) -> String {
    let d = if rng.chance(0.7) {
        (*rng.pick(DOMAINS)).to_string()
    } else {
        pick_nasty(rng)
    };
    let l = if rng.chance(0.6) {
        (*rng.pick(&["noreply", "no-reply", "info", "billing", "x", "a.b+c", ""])).to_string()
    } else {
        pick_nasty(rng)
    };
    match rng.below(7) {
        0 => format!("{l}@{d}"),
        1 => format!("<{l}@{d}>"),
        2 => format!("\"{}\" <{l}@{d}>", pick_nasty(rng)),
        3 => format!("{} <{l}@{d}>", pick_nasty(rng)),
        4 => format!("{l}@{d}, {l}@{d}"),
        5 => pick_nasty(rng),
        _ => format!("{}<{l}@{d}>{}", pick_nasty(rng), pick_nasty(rng)),
    }
}

fn random_header(rng: &mut Rng) -> MailHeader {
    let opt = |rng: &mut Rng, f: &dyn Fn(&mut Rng) -> String| rng.chance(0.5).then(|| f(rng));
    let lu = opt(rng, &|r| match r.below(6) {
        0 => format!("<https://{}/u?x=1>", r.pick(DOMAINS)),
        1 => format!("<mailto:{}?subject={}>", random_addr(r), pick_nasty(r)),
        2 => format!("<https://x.example>, <mailto:{}>", random_addr(r)),
        3 => "<<>>, <>, ,".into(),
        4 => format!("<mailto:{}>,<mailto:{}>", pick_nasty(r), pick_nasty(r)),
        _ => pick_nasty(r),
    });
    MailHeader {
        from: random_addr(rng),
        subject: random_text(rng),
        date: rng.chance(0.7).then(|| {
            chrono::DateTime::from_timestamp(rng.range(-62_135_596_800, 253_402_300_799), 0)
                .unwrap_or_default()
        }),
        message_id: opt(rng, &|r| format!("<{}>", pick_nasty(r))),
        to: random_addr(rng),
        has_list_unsubscribe: lu.is_some(),
        list_unsubscribe: lu,
        list_unsubscribe_post: opt(rng, &|r| {
            if r.chance(0.5) {
                "List-Unsubscribe=One-Click".into()
            } else {
                pick_nasty(r)
            }
        }),
        list_id: opt(rng, &|r| pick_nasty(r)),
        precedence: opt(rng, &|r| {
            (*r.pick(&["bulk", "list", "junk", "x"])).to_string()
        }),
        sender: opt(rng, &random_addr),
    }
}

fn random_body(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(25) {
        match rng.below(12) {
            0 => s.push_str("Gesamt 9,99 €\n"),
            1 => s.push_str("Bezahlt mit Visa ending in 4242\n"),
            2 => {
                s.push_str("Notion was granted access to your Google Account\nhttps://notion.so\n")
            }
            3 => s.push_str("IBAN: DE89 3704 0044 0532 0130 00\n"),
            4 => s.push_str("<table><tr><td>Plan</td><td>Pro</td></tr></table>"),
            5 => s.push_str("Verlängert sich am 12.04.2025\nProblem melden\n"),
            6 => s.push_str("ABGERECHNET AN\tVisa •••• 1234\n"),
            7 => s.push_str("Application: Foo\n"),
            8 => s.push_str("&euro;&#8364;&#xZZ;&bogus;"),
            _ => s.push_str(&pick_nasty(rng)),
        }
    }
    s
}

/// Byte-level mutations of a seed.
fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut v = seed.to_vec();
    for _ in 0..rng.range(1, 6) {
        if v.is_empty() {
            v.push(rng.next_u64() as u8);
            continue;
        }
        let i = rng.below(v.len() as u64) as usize;
        match rng.below(7) {
            0 => v[i] ^= 1 << rng.below(8),
            1 => v[i] = rng.next_u64() as u8,
            2 => {
                let n = rng.below(20) as usize;
                let ins = rng.bytes(n);
                v.splice(i..i, ins);
            }
            3 => {
                let j = (i + rng.below(30) as usize).min(v.len());
                v.drain(i..j);
            }
            4 => {
                let j = (i + rng.below(60) as usize).min(v.len());
                let dup = v[i..j].to_vec();
                v.splice(i..i, dup);
            }
            5 => v.truncate(i),
            _ => v.insert(i, *rng.pick(b"\"',;\r\n\t\0{}[]<>=?@:/\\")),
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[test]
fn mail_classification_and_receipts_never_panic() {
    let mut rng = Rng::new(1);
    let mut c = Crashes::default();
    for _ in 0..iters(6000) {
        let h = random_header(&mut rng);
        let body = random_body(&mut rng);
        let hd = || format!("{h:?} / body {body:.100}");
        c.run("classify_for", &hd, || {
            let _ = classify::classify_for(&h, Some(OWN));
            let _ = classify::classify(&h);
        });
        c.run("categorize/is_bulk/needs_body", &hd, || {
            let _ = classify::categorize(&h);
            let _ = classify::is_bulk(&h);
            let _ = classify::needs_body(&h);
        });
        c.run("classify_sso", &hd, || {
            let _ = classify::classify_sso(&h, &body);
            let _ = classify::parse_sso_body(&h, &body);
        });
        c.run("payment_kind/subject", &hd, || {
            let _ = receipts::payment_kind(&h);
            let _ = receipts::extract_from_subject(&h);
        });
        c.run("extract_from_body", &hd, || {
            let _ = receipts::extract_from_body(&h, &body);
        });
        c.run("parse_list_unsubscribe", &hd, || {
            if let Some(lu) = &h.list_unsubscribe {
                let _ = unsubscribe::parse_list_unsubscribe(lu, h.list_unsubscribe_post.as_deref());
            }
        });
        let s = random_text(&mut rng);
        c.run("text helpers", &|| s.clone(), || {
            let _ = classify::fold(&s);
            let _ = classify::decode_header(&s);
            let _ = classify::parse_address(&s);
            let _ = receipts::parse_amount(&s);
        });
    }
    c.report("mail classification");
}

#[test]
fn domain_helpers_never_panic() {
    let mut rng = Rng::new(2);
    let mut c = Crashes::default();
    for _ in 0..iters(20_000) {
        let s = match rng.below(4) {
            0 => pick_nasty(&mut rng),
            1 => random_addr(&mut rng),
            2 => format!("https://{}/{}", rng.pick(DOMAINS), pick_nasty(&mut rng)),
            _ => format!("android://{}@{}/", pick_nasty(&mut rng), rng.pick(DOMAINS)),
        };
        c.run("domain", &|| s.clone(), || {
            if let Some(r) = registrable_domain(&s) {
                let _ = canonical_service_domain(&r);
                let _ = display_name_for(&r);
            }
            let _ = canonical_service_domain(&s);
            let _ = display_name_for(&s);
        });
    }
    c.report("domain helpers");
}

fn sample_csv() -> String {
    "name,url,username,password,note\nSpotify,https://spotify.com/login,max@example.net,pw1,\"a note\"\nGitHub,github.com,max,pw2,\nAndroid,android://hash@com.foo.bar/,x,pw,\n".into()
}

fn sample_bitwarden() -> String {
    r#"{"encrypted":false,"folders":[],"items":[{"type":1,"name":"Spotify","login":{"username":"me","password":"p","uris":[{"uri":"https://spotify.com"}],"totp":"x"},"revisionDate":"2024-02-02T10:00:00.000Z"},{"type":2,"name":"n"}]}"#.into()
}

fn sample_zip() -> Vec<u8> {
    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (n, b) in [
        (
            "fb/apps_and_websites_off_of_facebook/connected_apps_and_websites.json",
            &br#"{"installed_apps_v2":[{"name":"Spotify","added_timestamp":1600000000},{"name":"Mueller"}]}"#[..],
        ),
        (
            "Takeout/Drittanbieter/a.json",
            &br#"[{"titleUrl":"https://notion.so/x","time":"2024-05-01T10:00:00Z"}]"#[..],
        ),
    ] {
        zw.start_file(n, opts).unwrap();
        zw.write_all(b).unwrap();
    }
    zw.finish().unwrap().into_inner()
}

#[test]
fn importers_never_panic() {
    let mut rng = Rng::new(3);
    let mut c = Crashes::default();
    let csv = sample_csv().into_bytes();
    let bw = sample_bitwarden().into_bytes();
    let zip = sample_zip();
    let dir = tempfile::tempdir().unwrap();
    for i in 0..iters(2500) {
        // CSV: mutated sample or random text.
        let text = if rng.chance(0.8) {
            String::from_utf8_lossy(&mutate(&mut rng, &csv)).into_owned()
        } else {
            random_text(&mut rng)
        };
        c.run("import_password_csv", &|| text.clone(), || {
            let _ = ausgeloggt_import::import_password_csv(&text);
        });
        let js = String::from_utf8_lossy(&mutate(&mut rng, &bw)).into_owned();
        c.run("import_bitwarden_json", &|| js.clone(), || {
            let _ = ausgeloggt_import::import_bitwarden_json(&js);
        });
        let z = mutate(&mut rng, &zip);
        c.run("import_zip_bytes", &|| format!("{} bytes", z.len()), || {
            let _ = ausgeloggt_import::import_zip_bytes(z.clone());
        });
        // File based: random extension, random content (text-ish or binary).
        if i % 3 == 0 {
            let ext = *rng.pick(&["csv", "txt", "json", "zip", "html", ""]);
            let bytes = match rng.below(4) {
                0 => mutate(&mut rng, &csv),
                1 => mutate(&mut rng, &bw),
                2 => mutate(&mut rng, &zip),
                _ => {
                    let n = rng.below(500) as usize;
                    rng.bytes(n)
                }
            };
            let p = dir.path().join(format!("f{i}.{ext}"));
            std::fs::write(&p, &bytes).unwrap();
            c.run(
                "import_path/detect",
                &|| format!("{ext}: {bytes:?}"),
                || {
                    let _ = ausgeloggt_import::detect(&p);
                    let _ = ausgeloggt_import::import_path(&p);
                    let _ = ausgeloggt_import::import_path_with(&p, &Default::default());
                },
            );
        }
    }
    c.report("importers");
}

fn random_finding(rng: &mut Rng) -> Finding {
    let kinds = EvidenceKind::ALL;
    Finding {
        domain: match rng.below(4) {
            0 => format!("unresolved:{}", pick_nasty(rng)),
            1 => (*rng.pick(DOMAINS)).to_string(),
            2 => random_addr(rng),
            _ => pick_nasty(rng),
        },
        name: rng.chance(0.5).then(|| pick_nasty(rng)),
        identifier: rng.chance(0.5).then(|| random_addr(rng)),
        sso: rng.chance(0.3).then(|| SsoProvider::Other(pick_nasty(rng))),
        evidence: Evidence {
            kind: *rng.pick(&kinds),
            source: SourceKind::Manual,
            summary: pick_nasty(rng),
            date: rng.chance(0.7).then(|| {
                chrono::DateTime::from_timestamp(rng.range(-5_000_000_000, 5_000_000_000), 0)
                    .unwrap_or_default()
            }),
            source_ref: rng.chance(0.5).then(|| format!("r{}", rng.below(30))),
        },
    }
}

#[test]
fn merge_functions_never_panic_and_stay_idempotent() {
    let mut rng = Rng::new(4);
    let mut c = Crashes::default();
    for round in 0..iters(150) {
        let findings: Vec<Finding> = (0..rng.range(1, 60))
            .map(|_| random_finding(&mut rng))
            .collect();
        let obs: Vec<SubscriptionObservation> = (0..rng.range(0, 10))
            .map(|_| SubscriptionObservation {
                mailbox: pick_nasty(&mut rng),
                list_key: pick_nasty(&mut rng),
                sender: random_addr(&mut rng),
                sender_name: rng.chance(0.5).then(|| pick_nasty(&mut rng)),
                domain: pick_nasty(&mut rng),
                unsubscribe: vec![],
                message_count: rng.below(100),
                first_seen: None,
                last_seen: chrono::DateTime::from_timestamp(rng.range(0, 2_000_000_000), 0),
                sample_subjects: vec![pick_nasty(&mut rng)],
                category: MailCategory::Marketing,
                watched_mails: vec![],
            })
            .collect();
        let pays: Vec<PaymentObservation> = (0..rng.range(0, 10))
            .map(|_| PaymentObservation {
                domain: random_finding(&mut rng).domain,
                name: rng.chance(0.5).then(|| pick_nasty(&mut rng)),
                identifier: None,
                source: SourceKind::Manual,
                record: PaymentRecord {
                    date: chrono::DateTime::from_timestamp(rng.range(0, 2_000_000_000), 0),
                    kind: PaymentKind::Renewal,
                    amount_minor: Some(rng.range(-100, 100_000)),
                    currency: Some("EUR".into()),
                    plan: None,
                    method: None,
                    via: None,
                    summary: pick_nasty(&mut rng),
                    source_ref: rng.chance(0.5).then(|| format!("p{}", rng.below(10))),
                },
            })
            .collect();
        let mut data = VaultData::default();
        c.run("merge", &|| format!("round {round}"), || {
            merge_findings(&mut data, findings.clone());
            merge_subscriptions(&mut data, obs.clone());
            merge_payments(&mut data, pays.clone());
            // Second identical merge must not add evidence (except with `unresolved:` keys,
            // a known merge bug covered by merge_edge.rs).
            let unique = !findings.iter().any(|f| f.domain.starts_with("unresolved:"))
                && !pays.iter().any(|p| p.domain.starts_with("unresolved:"));
            let n: usize = data.accounts.iter().map(|a| a.evidence.len()).sum();
            merge_findings(&mut data, findings.clone());
            let n2: usize = data.accounts.iter().map(|a| a.evidence.len()).sum();
            assert!(!unique || n == n2, "re-merge added evidence");
            for a in &data.accounts {
                assert!(
                    (0.0..=1.0).contains(&a.confidence),
                    "confidence {}",
                    a.confidence
                );
            }
        });
    }
    c.report("merge");
}

#[test]
fn scanner_survives_random_header_blocks() {
    let mut rng = Rng::new(5);
    let mut c = Crashes::default();
    // 1. Random header blocks as raw bytes: exercises the private header parser.
    let mut mails = Vec::new();
    for uid in 1..=400u32 {
        let h = random_header(&mut rng);
        let mut block: Vec<u8> = Vec::new();
        let mut put = |name: &str, v: &[u8]| {
            block.extend_from_slice(name.as_bytes());
            block.extend_from_slice(b": ");
            block.extend_from_slice(v);
            block.extend_from_slice(b"\r\n");
        };
        if rng.chance(0.9) {
            put("From", h.from.as_bytes());
        }
        put("Subject", &mutate(&mut rng, h.subject.as_bytes()));
        put("To", h.to.as_bytes());
        if let Some(d) = h.date {
            put(
                "Date",
                ausgeloggt_e2e::corpus::rfc2822(d, rng.range(-720, 840) as i32).as_bytes(),
            );
        } else if rng.chance(0.5) {
            put("Date", &rng.bytes(20));
        }
        if let Some(m) = &h.message_id {
            put("Message-ID", m.as_bytes());
        }
        if let Some(l) = &h.list_unsubscribe {
            put("List-Unsubscribe", l.as_bytes());
        }
        if let Some(l) = &h.list_id {
            put("List-Id", l.as_bytes());
        }
        if rng.chance(0.2) {
            block.extend_from_slice(&rng.bytes(40)); // garbage line without CRLF
            block.extend_from_slice(b"\r\n");
        }
        block.extend_from_slice(b"\r\n");
        mails.push(ausgeloggt_e2e::corpus::RawMail {
            uid,
            header: block,
            body: Some(random_body(&mut rng).into_bytes()),
            body_headers: "Content-Type: text/plain".into(),
            internal_date: "01-Mar-2026 10:00:00 +0000".into(),
        });
    }
    let imap = FakeImap::start(mails);
    let cfg = imap.config(OWN);
    let cancel = AtomicBool::new(false);
    let opts = ScanOptions {
        read_receipt_bodies: true,
        watch_lists: vec!["x".into()],
        ..Default::default()
    };
    let started = Instant::now();
    c.run(
        "scan random headers",
        &|| "400 random header blocks".into(),
        || {
            let out = scan_mailbox_with(&cfg, &opts, &mut |_| {}, &cancel);
            let out = out.expect("scan with random headers must succeed");
            let mut data = VaultData::default();
            merge_findings(&mut data, out.findings);
            merge_subscriptions(&mut data, out.subscriptions);
            merge_payments(&mut data, out.payments);
        },
    );
    println!(
        "FUZZ scanner: 400 random header blocks in {:?}",
        started.elapsed()
    );
    c.report("scanner (random headers)");
}

#[test]
fn hostile_imap_server_lines_do_not_panic_the_scanner() {
    let mut rng = Rng::new(8);
    let _ = &mut rng;
    let mut c = Crashes::default();
    for seed in 0..iters(60) as u64 {
        let imap = FakeImap::start(vec![ausgeloggt_e2e::corpus::RawMail {
            uid: 5,
            header: b"From: A <hello@foo-service.de>\r\nSubject: Willkommen\r\nMessage-ID: <1@x>\r\n\r\n".to_vec(),
            ..Default::default()
        }]);
        imap.state.lock().unwrap().chaos = Some(seed);
        let cfg = imap.config(OWN);
        c.run(
            "hostile IMAP server",
            &|| format!("chaos seed {seed}"),
            || {
                let _ = scan_mailbox_with(
                    &cfg,
                    &ScanOptions::default(),
                    &mut |_| {},
                    &AtomicBool::new(false),
                );
            },
        );
    }
    c.report("scanner (hostile server)");
}

#[test]
fn vault_open_on_random_bytes_never_panics() {
    let mut rng = Rng::new(6);
    let mut c = Crashes::default();
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("v.ausgeloggt");
    for i in 0..iters(400) {
        let mut bytes = match rng.below(3) {
            0 => {
                let n = rng.below(200) as usize;
                rng.bytes(n)
            }
            1 => {
                let mut v = b"AUSGLOGT\x01".to_vec();
                let n = rng.below(200) as usize;
                v.extend(rng.bytes(n));
                v
            }
            _ => {
                let mut v = b"AUSGLOGT\x01".to_vec();
                v.extend_from_slice(&8u32.to_le_bytes()); // tiny, valid KDF cost: fast
                v.extend_from_slice(&1u32.to_le_bytes());
                v.extend_from_slice(&1u32.to_le_bytes());
                let n = rng.below(150) as usize;
                v.extend(rng.bytes(n));
                v
            }
        };
        bytes.truncate(4096);
        std::fs::write(&p, &bytes).unwrap();
        c.run(
            "Vault::open",
            &|| format!("{i}: {:?}", &bytes[..bytes.len().min(40)]),
            || {
                assert!(Vault::open(&p, "pw").is_err());
            },
        );
    }
    c.report("vault");
}

#[test]
fn letters_with_random_strings_never_panic() {
    use ausgeloggt_letters::*;
    let mut rng = Rng::new(7);
    let mut c = Crashes::default();
    for _ in 0..iters(25) {
        let party = |rng: &mut Rng| Party {
            name: random_text(rng),
            address_lines: (0..rng.below(5)).map(|_| random_text(rng)).collect(),
            email: rng.chance(0.5).then(|| random_text(rng)),
        };
        let doc = LetterDoc {
            sender: party(&mut rng),
            recipient: party(&mut rng),
            place_date: random_text(&mut rng),
            subject: random_text(&mut rng),
            reference: rng.chance(0.5).then(|| random_text(&mut rng)),
            salutation: random_text(&mut rng),
            paragraphs: (0..rng.below(8)).map(|_| random_text(&mut rng)).collect(),
            closing: random_text(&mut rng),
            attachments: vec![
                Attachment::EvidenceTable {
                    title: random_text(&mut rng),
                    columns: (0..rng.below(6)).map(|_| random_text(&mut rng)).collect(),
                    rows: (0..rng.below(10))
                        .map(|_| (0..rng.below(7)).map(|_| random_text(&mut rng)).collect())
                        .collect(),
                    raw_headers: (0..rng.below(3)).map(|_| random_text(&mut rng)).collect(),
                },
                Attachment::Section {
                    title: random_text(&mut rng),
                    paragraphs: (0..rng.below(4)).map(|_| random_text(&mut rng)).collect(),
                },
                Attachment::CeaseAndDesistDeclaration {
                    sender: party(&mut rng),
                    recipient: party(&mut rng),
                    scope: random_text(&mut rng),
                },
            ],
            deadline: chrono::DateTime::from_timestamp(rng.range(0, 4_000_000_000), 0),
        };
        c.run("letters", &|| format!("{:?}", doc.subject), || {
            let _ = render_text(&doc);
            let pdf = render_pdf(&doc).expect("pdf");
            assert!(pdf.starts_with(b"%PDF"));
        });
    }
    c.report("letters");
}

#[test]
fn huge_inputs_are_processed_in_bounded_time() {
    let big_subject = "Willkommen Rabatt 9,99 € ".repeat(80_000); // ~2 MB
    let big_body = "Gesamt 9,99 € Visa ending in 4242 Notion was granted access to your Google Account\nhttps://a.example.com\n".repeat(30_000); // ~3 MB
    let h = MailHeader {
        from: "A <hello@foo-service.de>".into(),
        subject: big_subject.clone(),
        to: OWN.into(),
        ..Default::default()
    };
    for (name, f) in [
        (
            "classify",
            Box::new(|| drop(classify::classify_for(&h, Some(OWN)))) as Box<dyn Fn()>,
        ),
        (
            "categorize",
            Box::new(|| {
                let _ = classify::categorize(&h);
            }),
        ),
        (
            "payment subject",
            Box::new(|| drop(receipts::extract_from_subject(&h))),
        ),
        (
            "payment body",
            Box::new(|| drop(receipts::extract_from_body(&h, &big_body))),
        ),
        (
            "sso body",
            Box::new(|| drop(classify::classify_sso(&h, &big_body))),
        ),
        (
            "parse_amount",
            Box::new(|| drop(receipts::parse_amount(&big_body))),
        ),
        (
            "decode_header",
            Box::new(|| {
                drop(classify::decode_header(&format!(
                    "=?UTF-8?B?{}?=",
                    "QUJD".repeat(500_000)
                )))
            }),
        ),
        (
            "domain",
            Box::new(|| drop(registrable_domain(&"a.".repeat(100_000)))),
        ),
        (
            "list-unsubscribe",
            Box::new(|| {
                drop(unsubscribe::parse_list_unsubscribe(
                    &"<mailto:a@b.de>,".repeat(100_000),
                    None,
                ))
            }),
        ),
    ] {
        let t = Instant::now();
        f();
        let dt = t.elapsed();
        println!("HUGE INPUT {name}: {dt:?}");
        assert!(dt.as_secs() < 15, "{name} took {dt:?}");
    }
}
