//! Merge findings into accounts.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::domain::{
    canonical_service_domain, display_name_for, is_infrastructure_domain, registrable_domain,
};
use crate::model::{
    Account, AccountStatus, CategorySource, Evidence, EvidenceKind, Finding, Subscription,
    SubscriptionObservation, SubscriptionStatus, VaultData, SCHEMA_VERSION,
};

/// Prefix for findings whose domain is not known, only a service name.
pub const UNRESOLVED_PREFIX: &str = "unresolved:";

/// Maximum number of sample subjects kept per subscription.
const MAX_SUBJECTS: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub findings: u64,
    pub new_accounts: u64,
    pub updated_accounts: u64,
    /// Findings dropped (unparsable domain, infrastructure domain, duplicate evidence).
    pub skipped: u64,
}

/// Fold findings into `data.accounts`:
/// - group by `canonical_service_domain(registrable_domain(f.domain))`; drop
///   unparsable and infrastructure domains (count as skipped)
/// - special case: `f.domain` starting with [`UNRESOLVED_PREFIX`] (e.g. an SSO grant
///   mail only names the app, `unresolved:notion`) is kept verbatim as the key; if an
///   existing account's name matches `f.name` case-insensitively, attach to that instead
/// - dedupe evidence by (source, source_ref); without source_ref by (source, kind, summary, date).
///   Re-importing the same file must be a no-op.
/// - union identifiers (case-insensitive) and sso providers
/// - name: prefer an explicit finding name over `display_name_for`, never overwrite a
///   name on an existing account
/// - update first_seen / last_seen from evidence dates
/// - recompute confidence via [`confidence`]
/// - never change a status the user has set (anything but `Discovered`)
/// - set `updated_at` only on accounts that actually changed
pub fn merge_findings(data: &mut VaultData, findings: Vec<Finding>) -> MergeReport {
    let mut report = MergeReport::default();
    if data.schema == 0 {
        data.schema = SCHEMA_VERSION;
    }

    let mut by_domain: HashMap<String, usize> = HashMap::with_capacity(data.accounts.len());
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for (i, a) in data.accounts.iter().enumerate() {
        by_domain.entry(a.domain.clone()).or_insert(i);
        by_name.entry(a.name.trim().to_lowercase()).or_insert(i);
    }

    // Accounts created in this run whose name was derived (not explicit): may be upgraded.
    let mut derived_name: HashSet<usize> = HashSet::new();
    let mut created: HashSet<usize> = HashSet::new();
    let mut updated: HashSet<usize> = HashSet::new();
    let now = Utc::now();

    for f in findings {
        report.findings += 1;
        let explicit_name = f
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string);

        // Resolve the grouping key.
        let key: String;
        let mut target: Option<usize> = None;
        if let Some(rest) = f.domain.trim().strip_prefix(UNRESOLVED_PREFIX) {
            let rest = rest.trim();
            if rest.is_empty() && explicit_name.is_none() {
                report.skipped += 1;
                continue;
            }
            key = format!("{UNRESOLVED_PREFIX}{}", rest.to_lowercase());
            if let Some(n) = &explicit_name {
                target = by_name.get(&n.to_lowercase()).copied();
            }
            if target.is_none() {
                target = by_name.get(&rest.to_lowercase()).copied();
            }
        } else {
            let Some(rd) = registrable_domain(&f.domain) else {
                report.skipped += 1;
                continue;
            };
            if is_infrastructure_domain(&rd) {
                report.skipped += 1;
                continue;
            }
            let canon = canonical_service_domain(&rd);
            if is_infrastructure_domain(&canon) {
                report.skipped += 1;
                continue;
            }
            key = canon;
        }
        if target.is_none() {
            target = by_domain.get(&key).copied();
        }

        let (idx, is_new) = match target {
            Some(i) => (i, false),
            None => {
                let (name, derived) = match &explicit_name {
                    Some(n) => (n.clone(), false),
                    None if key.starts_with(UNRESOLVED_PREFIX) => {
                        (capitalise(&key[UNRESOLVED_PREFIX.len()..]), true)
                    }
                    None => (display_name_for(&key), true),
                };
                let i = data.accounts.len();
                data.accounts.push(Account {
                    id: Uuid::new_v4(),
                    domain: key.clone(),
                    name: name.clone(),
                    identifiers: Vec::new(),
                    sso: Vec::new(),
                    evidence: Vec::new(),
                    status: AccountStatus::Discovered,
                    confidence: 0.0,
                    first_seen: None,
                    last_seen: None,
                    tags: Vec::new(),
                    notes: String::new(),
                    deletion: Vec::new(),
                    created_at: now,
                    updated_at: now,
                    payments: Vec::new(),
                    security: Default::default(),
                });
                by_domain.insert(key.clone(), i);
                by_name.entry(name.trim().to_lowercase()).or_insert(i);
                created.insert(i);
                if derived {
                    derived_name.insert(i);
                }
                report.new_accounts += 1;
                (i, true)
            }
        };

        let acct = &mut data.accounts[idx];
        let mut changed = false;

        // Name upgrade for accounts created earlier in this very run.
        if let Some(n) = &explicit_name {
            if !is_new && derived_name.remove(&idx) {
                by_name.remove(&acct.name.trim().to_lowercase());
                acct.name = n.clone();
                by_name.entry(n.to_lowercase()).or_insert(idx);
                changed = true;
            }
        }

        // Evidence (deduplicated).
        if evidence_exists(&acct.evidence, &f.evidence) {
            report.skipped += 1;
        } else {
            if let Some(d) = f.evidence.date {
                if acct.first_seen.is_none_or(|x| d < x) {
                    acct.first_seen = Some(d);
                }
                if acct.last_seen.is_none_or(|x| d > x) {
                    acct.last_seen = Some(d);
                }
            }
            acct.evidence.push(f.evidence);
            changed = true;
        }

        // Identifier (case-insensitive union).
        if let Some(id) = f
            .identifier
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !acct
                .identifiers
                .iter()
                .any(|x| x.eq_ignore_ascii_case(id) || x.to_lowercase() == id.to_lowercase())
            {
                acct.identifiers.push(id.to_string());
                changed = true;
            }
        }

        // SSO provider.
        if let Some(p) = f.sso {
            if !acct.sso.contains(&p) {
                acct.sso.push(p);
                changed = true;
            }
        }

        if changed {
            let c = confidence(&acct.evidence);
            acct.confidence = c;
            if !is_new {
                acct.updated_at = now;
                updated.insert(idx);
            }
        } else if !is_new {
            // Keep stored confidence consistent without touching updated_at semantics.
            let c = confidence(&acct.evidence);
            if (c - acct.confidence).abs() > f32::EPSILON {
                acct.confidence = c;
                acct.updated_at = now;
                updated.insert(idx);
            }
        }
    }

    // A new account that was modified again in the same run still counts as new only.
    report.updated_accounts = updated.iter().filter(|i| !created.contains(i)).count() as u64;
    report
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn evidence_exists(existing: &[Evidence], new: &Evidence) -> bool {
    existing.iter().any(|e| {
        if e.source != new.source {
            return false;
        }
        match (&e.source_ref, &new.source_ref) {
            (Some(a), Some(b)) => a == b,
            (None, None) => e.kind == new.kind && e.summary == new.summary && e.date == new.date,
            _ => false,
        }
    })
}

/// Fold scan observations into `data.subscriptions`, keyed by (mailbox, list_key):
/// - message_count adds up only for new mails (the scan is incremental; observations
///   carry counts for *new* mails only)
/// - first/last_seen widen; sample_subjects: newest first, dedup, max 5
/// - unsubscribe methods: replaced by the newest observation's if non-empty
/// - category: rules may update only if `category_source == Rules`
/// - frequency_per_month recomputed from count and first/last_seen
///   (`(count - 1)` intervals over the observed span, scaled to 30 days; 0 for fewer than 2 mails)
/// - status: if `Unsubscribed` and a mail with date > last unsubscribe attempt + 7 days
///   arrives -> `StillReceiving`
///
/// Returns (new, updated) counts.
pub fn merge_subscriptions(
    data: &mut VaultData,
    observations: Vec<SubscriptionObservation>,
) -> (u64, u64) {
    let mut index: HashMap<(String, String), usize> =
        HashMap::with_capacity(data.subscriptions.len());
    for (i, s) in data.subscriptions.iter().enumerate() {
        index
            .entry((s.mailbox.to_lowercase(), s.list_key.to_lowercase()))
            .or_insert(i);
    }
    let mut created: HashSet<usize> = HashSet::new();
    let mut updated: HashSet<usize> = HashSet::new();

    for obs in observations {
        let k = (obs.mailbox.to_lowercase(), obs.list_key.to_lowercase());
        let subjects = dedup_subjects(obs.sample_subjects.iter().cloned());
        match index.get(&k).copied() {
            None => {
                let domain = obs_domain(&obs);
                let name = obs
                    .sender_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| display_name_for(&domain));
                let mut sub = Subscription {
                    id: Uuid::new_v4(),
                    mailbox: obs.mailbox,
                    list_key: obs.list_key.to_lowercase(),
                    domain,
                    name,
                    sender: obs.sender,
                    unsubscribe: obs.unsubscribe,
                    message_count: obs.message_count,
                    frequency_per_month: 0.0,
                    first_seen: obs.first_seen,
                    last_seen: obs.last_seen,
                    sample_subjects: subjects,
                    category: obs.category,
                    category_source: CategorySource::Rules,
                    status: SubscriptionStatus::Active,
                    history: Vec::new(),
                    post_unsubscribe_mails: Vec::new(),
                };
                sub.frequency_per_month =
                    frequency(sub.message_count, sub.first_seen, sub.last_seen);
                index.insert(k, data.subscriptions.len());
                created.insert(data.subscriptions.len());
                data.subscriptions.push(sub);
            }
            Some(i) => {
                let sub = &mut data.subscriptions[i];
                let before = sub.clone();

                sub.message_count += obs.message_count;
                sub.first_seen = min_opt(sub.first_seen, obs.first_seen);
                sub.last_seen = max_opt(sub.last_seen, obs.last_seen);
                if !subjects.is_empty() {
                    sub.sample_subjects = dedup_subjects(
                        subjects
                            .into_iter()
                            .chain(sub.sample_subjects.iter().cloned()),
                    );
                }
                if !obs.unsubscribe.is_empty() {
                    sub.unsubscribe = obs.unsubscribe;
                }
                if sub.category_source == CategorySource::Rules {
                    sub.category = obs.category;
                }
                if sub.name.trim().is_empty() {
                    if let Some(n) = obs
                        .sender_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                    {
                        sub.name = n.to_string();
                    }
                }
                sub.frequency_per_month =
                    frequency(sub.message_count, sub.first_seen, sub.last_seen);

                if obs.message_count > 0 && sub.status == SubscriptionStatus::Unsubscribed {
                    let last_attempt = sub.history.iter().map(|h| h.at).max();
                    if let (Some(at), Some(newest)) = (last_attempt, obs.last_seen) {
                        if newest > at + Duration::days(7) {
                            sub.status = SubscriptionStatus::StillReceiving;
                        }
                    }
                }

                if *sub != before {
                    updated.insert(i);
                }
            }
        }
    }
    (created.len() as u64, updated.len() as u64)
}

fn obs_domain(obs: &SubscriptionObservation) -> String {
    registrable_domain(&obs.domain)
        .or_else(|| registrable_domain(&obs.sender))
        .map(|d| canonical_service_domain(&d))
        .unwrap_or_else(|| obs.domain.trim().to_lowercase())
}

fn dedup_subjects(subjects: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(MAX_SUBJECTS);
    for s in subjects {
        if out.len() >= MAX_SUBJECTS {
            break;
        }
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn min_opt(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, None) | (None, x) => x,
    }
}

fn max_opt(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (x, None) | (None, x) => x,
    }
}

fn frequency(count: u64, first: Option<DateTime<Utc>>, last: Option<DateTime<Utc>>) -> f32 {
    let (Some(f), Some(l)) = (first, last) else {
        return 0.0;
    };
    if count < 2 {
        return 0.0;
    }
    let span_days = ((l - f).num_seconds() as f64 / 86_400.0).max(1.0);
    (((count - 1) as f64 / span_days) * 30.0) as f32
}

/// Fold payment observations into `Account.payments`:
/// - resolve the account like `merge_findings` does (canonical domain / `unresolved:` by name);
///   create the account if missing (with a `Payment` evidence entry, so it shows up)
/// - dedupe by `record.source_ref` (fallback: date + amount + summary)
/// - union identifier
/// Returns (records added, accounts created).
pub fn merge_payments(
    data: &mut VaultData,
    observations: Vec<crate::model::PaymentObservation>,
) -> (u64, u64) {
    let _ = (data, observations);
    todo!("implemented by core agent")
}

/// Apply security observations (password audit) to matching accounts by canonical
/// domain (+ identifier when given). Values from a newer import overwrite older ones;
/// `None` fields don't overwrite. Sets `security.checked_at`. Returns accounts updated.
pub fn merge_security(
    data: &mut VaultData,
    observations: Vec<crate::model::SecurityObservation>,
) -> u64 {
    let _ = (data, observations);
    todo!("implemented by core agent")
}

/// Attach domain-level breaches (`(breach_domain, record)`, e.g. from the public HIBP
/// breach list) to accounts with that canonical domain, and identifier-level hits
/// (`confirmed_for_user = true`). Dedup by breach name. Returns accounts updated.
pub fn merge_breaches(
    data: &mut VaultData,
    breaches: Vec<(String, crate::model::BreachRecord)>,
) -> u64 {
    let _ = (data, breaches);
    todo!("implemented by core agent")
}

/// For subscriptions that are `Unsubscribed` / `StillReceiving`: add observation
/// `watched_mails` dated after the last successful unsubscribe attempt + 2 days grace
/// to `post_unsubscribe_mails` (dedup by Message-ID). Called from `merge_subscriptions`.
pub fn record_post_unsubscribe_mails(
    sub: &mut crate::model::Subscription,
    mails: &[crate::model::MailRef],
) -> usize {
    let _ = (sub, mails);
    todo!("implemented by core agent")
}

/// Confidence from evidence: `1 - Π(1 - weight)` over the *distinct* evidence kinds.
pub fn confidence(evidence: &[Evidence]) -> f32 {
    let mut seen: HashSet<EvidenceKind> = HashSet::new();
    let mut miss = 1.0f32;
    for e in evidence {
        if seen.insert(e.kind) {
            miss *= 1.0 - e.kind.weight();
        }
    }
    (1.0 - miss).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        MailCategory, SourceKind, SsoProvider, UnsubscribeAttempt, UnsubscribeMethod,
    };
    use chrono::TimeZone;

    fn day(d: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap() + Duration::days(d)
    }

    fn ev(kind: EvidenceKind, r: Option<&str>, date: Option<DateTime<Utc>>) -> Evidence {
        Evidence {
            kind,
            source: SourceKind::GenericList,
            summary: format!("{kind:?}"),
            date,
            source_ref: r.map(str::to_string),
        }
    }

    fn finding(domain: &str, kind: EvidenceKind, r: &str) -> Finding {
        Finding {
            domain: domain.into(),
            name: None,
            identifier: None,
            sso: None,
            evidence: ev(kind, Some(r), None),
        }
    }

    #[test]
    fn confidence_math() {
        assert_eq!(confidence(&[]), 0.0);
        let one = confidence(&[ev(EvidenceKind::Welcome, None, None)]);
        assert!((one - 0.9).abs() < 1e-6);
        let two = confidence(&[
            ev(EvidenceKind::Welcome, Some("a"), None),
            ev(EvidenceKind::Receipt, Some("b"), None),
        ]);
        assert!((two - (1.0 - 0.1 * 0.65)).abs() < 1e-6);
        // duplicates of one kind do not add up
        let dup = confidence(&[
            ev(EvidenceKind::Newsletter, Some("a"), None),
            ev(EvidenceKind::Newsletter, Some("b"), None),
            ev(EvidenceKind::Newsletter, Some("c"), None),
        ]);
        assert!((dup - 0.15).abs() < 1e-6);
        assert_eq!(confidence(&[ev(EvidenceKind::Manual, None, None)]), 1.0);
    }

    #[test]
    fn groups_by_registrable_domain_and_aliases() {
        let mut d = VaultData::default();
        let r = merge_findings(
            &mut d,
            vec![
                finding(
                    "https://accounts.spotify.com/login",
                    EvidenceKind::SavedPassword,
                    "1",
                ),
                finding("no-reply@mail.spotify.com", EvidenceKind::Welcome, "2"),
                finding("spotifymail.com", EvidenceKind::Receipt, "3"),
                finding("twitter.com", EvidenceKind::Welcome, "4"),
            ],
        );
        assert_eq!(d.accounts.len(), 2);
        assert_eq!(r.findings, 4);
        assert_eq!(r.new_accounts, 2);
        assert_eq!(r.updated_accounts, 0);
        assert_eq!(r.skipped, 0);
        let sp = d
            .accounts
            .iter()
            .find(|a| a.domain == "spotify.com")
            .unwrap();
        assert_eq!(sp.name, "Spotify");
        assert_eq!(sp.evidence.len(), 3);
        assert!(d.accounts.iter().any(|a| a.domain == "x.com"));
        assert_eq!(d.schema, SCHEMA_VERSION);
    }

    #[test]
    fn skips_infrastructure_and_garbage() {
        let mut d = VaultData::default();
        let r = merge_findings(
            &mut d,
            vec![
                finding("bounce@sendgrid.net", EvidenceKind::OtherMail, "1"),
                finding("someone@gmail.com", EvidenceKind::OtherMail, "2"),
                finding("localhost", EvidenceKind::SavedPassword, "3"),
                finding("", EvidenceKind::SavedPassword, "4"),
                finding("192.168.0.1", EvidenceKind::SavedPassword, "5"),
            ],
        );
        assert!(d.accounts.is_empty());
        assert_eq!(r.skipped, 5);
        assert_eq!(r.findings, 5);
    }

    #[test]
    fn reimport_is_noop() {
        let mk = || {
            vec![
                finding("spotify.com", EvidenceKind::SavedPassword, "row1"),
                Finding {
                    identifier: Some("Me@Example.com".into()),
                    sso: Some(SsoProvider::Google),
                    ..finding("netflix.com", EvidenceKind::Welcome, "row2")
                },
                Finding {
                    evidence: ev(EvidenceKind::Receipt, None, Some(day(3))),
                    ..finding("otto.de", EvidenceKind::Receipt, "x")
                },
            ]
        };
        let mut d = VaultData::default();
        merge_findings(&mut d, mk());
        let snapshot = serde_json::to_string(&d).unwrap();
        let r = merge_findings(&mut d, mk());
        assert_eq!(serde_json::to_string(&d).unwrap(), snapshot);
        assert_eq!(r.new_accounts, 0);
        assert_eq!(r.updated_accounts, 0);
        assert_eq!(r.skipped, 3);
    }

    #[test]
    fn dedup_without_source_ref_uses_kind_summary_date() {
        let mut d = VaultData::default();
        let mut f = finding("a-shop.com", EvidenceKind::Receipt, "x");
        f.evidence.source_ref = None;
        f.evidence.date = Some(day(1));
        merge_findings(&mut d, vec![f.clone()]);
        merge_findings(&mut d, vec![f.clone()]);
        assert_eq!(d.accounts[0].evidence.len(), 1);
        f.evidence.date = Some(day(2));
        merge_findings(&mut d, vec![f]);
        assert_eq!(d.accounts[0].evidence.len(), 2);
    }

    #[test]
    fn status_and_name_preserved() {
        let mut d = VaultData::default();
        merge_findings(
            &mut d,
            vec![finding("spotify.com", EvidenceKind::Welcome, "1")],
        );
        d.accounts[0].status = AccountStatus::Keep;
        d.accounts[0].name = "Mein Spotify".into();
        let before = d.accounts[0].updated_at;
        let mut f = finding("spotify.com", EvidenceKind::Receipt, "2");
        f.name = Some("Spotify AB".into());
        let r = merge_findings(&mut d, vec![f]);
        assert_eq!(d.accounts[0].status, AccountStatus::Keep);
        assert_eq!(d.accounts[0].name, "Mein Spotify");
        assert_eq!(r.updated_accounts, 1);
        assert!(d.accounts[0].updated_at >= before);
    }

    #[test]
    fn explicit_name_preferred_on_create_and_upgrades_within_run() {
        let mut d = VaultData::default();
        let mut named = finding("zalando.de", EvidenceKind::SavedPassword, "1");
        named.name = Some("Zalando Shop".into());
        merge_findings(&mut d, vec![named]);
        assert_eq!(d.accounts[0].name, "Zalando Shop");

        let mut d = VaultData::default();
        let mut named = finding("foo.example", EvidenceKind::SavedPassword, "2");
        named.name = Some("Foo Deluxe".into());
        merge_findings(
            &mut d,
            vec![finding("foo.example", EvidenceKind::Welcome, "1"), named],
        );
        assert_eq!(d.accounts[0].name, "Foo Deluxe");
    }

    #[test]
    fn identifiers_union_case_insensitive() {
        let mut d = VaultData::default();
        let mut a = finding("spotify.com", EvidenceKind::Welcome, "1");
        a.identifier = Some("Max@Example.com".into());
        let mut b = finding("spotify.com", EvidenceKind::Welcome, "2");
        b.identifier = Some("max@example.COM".into());
        let mut c = finding("spotify.com", EvidenceKind::Welcome, "3");
        c.identifier = Some("maxi".into());
        c.sso = Some(SsoProvider::Apple);
        let mut e = finding("spotify.com", EvidenceKind::Welcome, "4");
        e.sso = Some(SsoProvider::Apple);
        merge_findings(&mut d, vec![a, b, c, e]);
        assert_eq!(d.accounts[0].identifiers, vec!["Max@Example.com", "maxi"]);
        assert_eq!(d.accounts[0].sso, vec![SsoProvider::Apple]);
    }

    #[test]
    fn seen_dates_and_confidence() {
        let mut d = VaultData::default();
        let mut a = finding("spotify.com", EvidenceKind::Welcome, "1");
        a.evidence.date = Some(day(10));
        let mut b = finding("spotify.com", EvidenceKind::Receipt, "2");
        b.evidence.date = Some(day(2));
        let mut c = finding("spotify.com", EvidenceKind::Newsletter, "3");
        c.evidence.date = Some(day(30));
        merge_findings(&mut d, vec![a, b, c]);
        let acc = &d.accounts[0];
        assert_eq!(acc.first_seen, Some(day(2)));
        assert_eq!(acc.last_seen, Some(day(30)));
        assert!((acc.confidence - confidence(&acc.evidence)).abs() < 1e-9);
        assert!(acc.confidence > 0.9);
    }

    #[test]
    fn unresolved_handling() {
        let mut d = VaultData::default();
        // no matching account: kept verbatim
        let mut f = finding("unresolved:notion", EvidenceKind::SsoGrant, "1");
        f.sso = Some(SsoProvider::Google);
        merge_findings(&mut d, vec![f]);
        assert_eq!(d.accounts.len(), 1);
        assert_eq!(d.accounts[0].domain, "unresolved:notion");
        assert_eq!(d.accounts[0].name, "Notion");

        // repeat attaches to the same unresolved account
        merge_findings(
            &mut d,
            vec![finding("unresolved:notion", EvidenceKind::SsoGrant, "2")],
        );
        assert_eq!(d.accounts.len(), 1);
        assert_eq!(d.accounts[0].evidence.len(), 2);

        // existing account with matching name wins
        let mut known = finding("slack.com", EvidenceKind::Welcome, "3");
        known.name = Some("Slack".into());
        merge_findings(&mut d, vec![known]);
        let mut f = finding("unresolved:slack", EvidenceKind::SsoGrant, "4");
        f.name = Some("slack".into());
        let r = merge_findings(&mut d, vec![f]);
        assert_eq!(d.accounts.len(), 2);
        let slack = d.accounts.iter().find(|a| a.domain == "slack.com").unwrap();
        assert_eq!(slack.evidence.len(), 2);
        assert_eq!(r.new_accounts, 0);
        assert_eq!(r.updated_accounts, 1);
    }

    fn obs(count: u64, first: i64, last: i64, subjects: &[&str]) -> SubscriptionObservation {
        SubscriptionObservation {
            mailbox: "me@example.com".into(),
            list_key: "news.shop.com".into(),
            sender: "news@shop.com".into(),
            sender_name: Some("Shop Newsletter".into()),
            domain: "mail.shop.com".into(),
            unsubscribe: vec![UnsubscribeMethod::Link {
                url: "https://shop.com/u".into(),
            }],
            message_count: count,
            first_seen: Some(day(first)),
            last_seen: Some(day(last)),
            sample_subjects: subjects.iter().map(|s| s.to_string()).collect(),
            category: MailCategory::Marketing,
            watched_mails: Vec::new(),
        }
    }

    #[test]
    fn subscriptions_new_and_incremental() {
        let mut d = VaultData::default();
        let (n, u) = merge_subscriptions(&mut d, vec![obs(4, 0, 30, &["a", "b"])]);
        assert_eq!((n, u), (1, 0));
        let s = &d.subscriptions[0];
        assert_eq!(s.domain, "shop.com");
        assert_eq!(s.name, "Shop Newsletter");
        assert_eq!(s.message_count, 4);
        assert!((s.frequency_per_month - 3.0).abs() < 0.01);

        let (n, u) = merge_subscriptions(&mut d, vec![obs(2, 31, 40, &["c", "a"])]);
        assert_eq!((n, u), (0, 1));
        let s = &d.subscriptions[0];
        assert_eq!(d.subscriptions.len(), 1);
        assert_eq!(s.message_count, 6);
        assert_eq!(s.first_seen, Some(day(0)));
        assert_eq!(s.last_seen, Some(day(40)));
        assert_eq!(s.sample_subjects, vec!["c", "a", "b"]);
        assert!((s.frequency_per_month - 5.0 / 40.0 * 30.0).abs() < 0.01);
    }

    #[test]
    fn subscriptions_noop_observation_is_not_an_update() {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(3, 0, 10, &["a"])]);
        let mut empty = obs(0, 0, 10, &[]);
        empty.unsubscribe.clear();
        let (n, u) = merge_subscriptions(&mut d, vec![empty]);
        assert_eq!((n, u), (0, 0));
    }

    #[test]
    fn subscriptions_subject_cap_and_unsubscribe_replacement() {
        let mut d = VaultData::default();
        merge_subscriptions(
            &mut d,
            vec![obs(1, 0, 1, &["1", "2", "3", "4", "5", "6", "7"])],
        );
        assert_eq!(d.subscriptions[0].sample_subjects.len(), 5);
        let mut o = obs(1, 2, 3, &["x", "y", "z"]);
        o.unsubscribe = vec![UnsubscribeMethod::OneClick {
            url: "https://shop.com/oc".into(),
        }];
        merge_subscriptions(&mut d, vec![o]);
        assert_eq!(
            d.subscriptions[0].sample_subjects,
            vec!["x", "y", "z", "1", "2"]
        );
        assert_eq!(
            d.subscriptions[0].unsubscribe,
            vec![UnsubscribeMethod::OneClick {
                url: "https://shop.com/oc".into()
            }]
        );
        let mut o = obs(1, 4, 5, &[]);
        o.unsubscribe.clear();
        merge_subscriptions(&mut d, vec![o]);
        assert_eq!(
            d.subscriptions[0].unsubscribe.len(),
            1,
            "empty list keeps old methods"
        );
    }

    #[test]
    fn subscriptions_category_only_updated_by_rules() {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(1, 0, 1, &[])]);
        let mut o = obs(1, 2, 3, &[]);
        o.category = MailCategory::Spam;
        merge_subscriptions(&mut d, vec![o.clone()]);
        assert_eq!(d.subscriptions[0].category, MailCategory::Spam);
        d.subscriptions[0].category = MailCategory::Newsletter;
        d.subscriptions[0].category_source = CategorySource::Ai;
        merge_subscriptions(&mut d, vec![o]);
        assert_eq!(d.subscriptions[0].category, MailCategory::Newsletter);
    }

    #[test]
    fn subscriptions_still_receiving() {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(1, 0, 10, &[])]);
        d.subscriptions[0].status = SubscriptionStatus::Unsubscribed;
        d.subscriptions[0].history.push(UnsubscribeAttempt {
            at: day(11),
            method: UnsubscribeMethod::Link {
                url: "https://shop.com/u".into(),
            },
            ok: true,
            message: String::new(),
        });
        // within 7 days: still Unsubscribed
        merge_subscriptions(&mut d, vec![obs(1, 15, 18, &[])]);
        assert_eq!(d.subscriptions[0].status, SubscriptionStatus::Unsubscribed);
        // beyond 7 days
        merge_subscriptions(&mut d, vec![obs(1, 19, 19, &[])]);
        assert_eq!(
            d.subscriptions[0].status,
            SubscriptionStatus::StillReceiving
        );
        // Keep stays Keep
        d.subscriptions[0].status = SubscriptionStatus::Keep;
        merge_subscriptions(&mut d, vec![obs(1, 30, 30, &[])]);
        assert_eq!(d.subscriptions[0].status, SubscriptionStatus::Keep);
    }

    #[test]
    fn subscriptions_unsubscribed_without_new_mail_stays() {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(1, 0, 10, &[])]);
        d.subscriptions[0].status = SubscriptionStatus::Unsubscribed;
        d.subscriptions[0].history.push(UnsubscribeAttempt {
            at: day(11),
            method: UnsubscribeMethod::Link { url: "u".into() },
            ok: true,
            message: String::new(),
        });
        merge_subscriptions(&mut d, vec![obs(0, 0, 50, &[])]);
        assert_eq!(d.subscriptions[0].status, SubscriptionStatus::Unsubscribed);
    }
}
