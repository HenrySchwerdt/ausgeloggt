//! Shared helpers for the integration tests: scan the corpus through the real IMAP code
//! path, merge into a `VaultData`, and compare against ground truth.

use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

use ausgeloggt_core::merge;
use ausgeloggt_core::{Account, VaultData};
use ausgeloggt_mail::{scan_mailbox_with, ScanOptions, ScanOutcome};

use crate::corpus::{self, Corpus, ExpectedAccount, Truth, OWN};
use crate::fake_imap::FakeImap;

pub const MIN_CONFIDENCE: f32 = 0.5;

pub fn scan(cfg: &ausgeloggt_core::MailboxConfig, opts: &ScanOptions) -> ScanOutcome {
    let cancel = AtomicBool::new(false);
    scan_mailbox_with(cfg, opts, &mut |_| {}, &cancel).expect("scan must succeed")
}

pub fn merge_all(data: &mut VaultData, out: &ScanOutcome) {
    merge::merge_findings(data, out.findings.clone());
    merge::merge_subscriptions(data, out.subscriptions.clone());
    merge::merge_payments(data, out.payments.clone());
}

pub fn detected(data: &VaultData) -> Vec<&Account> {
    data.accounts
        .iter()
        .filter(|a| a.confidence >= MIN_CONFIDENCE)
        .collect()
}

pub fn matches(a: &Account, t: &ExpectedAccount) -> bool {
    a.domain == t.key
        || a.domain == format!("unresolved:{}", t.slug)
        || a.name.eq_ignore_ascii_case(&t.name)
}

pub fn find<'a>(data: &'a VaultData, t: &ExpectedAccount) -> Option<&'a Account> {
    data.accounts.iter().find(|a| matches(a, t))
}

#[derive(Debug, Clone)]
pub struct Metrics {
    pub detected: usize,
    pub expected: usize,
    pub true_positives: usize,
    pub precision: f64,
    pub recall: f64,
    pub false_positives: Vec<String>,
    pub missed: Vec<String>,
    pub phishing_hits: Vec<String>,
}

pub fn account_metrics(data: &VaultData, truth: &Truth) -> Metrics {
    let det = detected(data);
    let mut tp = 0;
    let mut missed = Vec::new();
    // De-duplicate expected accounts by key (a service can be listed twice, e.g. SSO + paid).
    let mut seen = std::collections::BTreeSet::new();
    let expected: Vec<&ExpectedAccount> = truth
        .accounts
        .iter()
        .filter(|t| seen.insert(t.key.clone()))
        .collect();
    for t in &expected {
        if det.iter().any(|a| matches(a, t)) {
            tp += 1;
        } else {
            missed.push(format!("{} ({})", t.name, t.key));
        }
    }
    let mut fps = Vec::new();
    let mut phishing = Vec::new();
    for a in &det {
        let is_expected = expected.iter().any(|t| matches(a, t));
        if is_expected || truth.tolerated.contains(&a.domain) {
            continue;
        }
        if truth.phishing.contains(&a.domain) {
            phishing.push(a.domain.clone());
        } else {
            fps.push(format!(
                "{} (conf {:.2}, {} evidence)",
                a.domain,
                a.confidence,
                a.evidence.len()
            ));
        }
    }
    let nd = det.len() - phishing.len();
    Metrics {
        detected: det.len(),
        expected: expected.len(),
        true_positives: tp,
        precision: if nd == 0 {
            1.0
        } else {
            (nd - fps.len()) as f64 / nd as f64
        },
        recall: if expected.is_empty() {
            1.0
        } else {
            tp as f64 / expected.len() as f64
        },
        false_positives: fps,
        missed,
        phishing_hits: phishing,
    }
}

pub fn corpus_cached() -> &'static Corpus {
    static C: OnceLock<Corpus> = OnceLock::new();
    C.get_or_init(|| corpus::generate(20260301))
}

pub struct Scanned {
    pub outcome: ScanOutcome,
    pub data: VaultData,
}

/// Scan the cached corpus once per option set (subject only / with receipt bodies).
pub fn scanned_corpus(read_bodies: bool) -> &'static Scanned {
    static SUBJ: OnceLock<Scanned> = OnceLock::new();
    static BODY: OnceLock<Scanned> = OnceLock::new();
    let cell = if read_bodies { &BODY } else { &SUBJ };
    cell.get_or_init(|| {
        let c = corpus_cached();
        let imap = FakeImap::start(c.mails.clone());
        let cfg = imap.config(OWN);
        let opts = ScanOptions {
            read_receipt_bodies: read_bodies,
            ..Default::default()
        };
        let outcome = scan(&cfg, &opts);
        let mut data = VaultData::default();
        merge_all(&mut data, &outcome);
        Scanned { outcome, data }
    })
}
