//! Merge findings into accounts.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::domain::{
    canonical_service_domain, display_name_for, is_infrastructure_domain, registrable_domain,
};
use crate::model::{
    Account, CategorySource, Evidence, EvidenceKind, Finding, PaymentRecord, Subscription,
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

    let mut index = AccountIndex::new(&data.accounts);

    // Accounts created in this run whose name was derived (not explicit): may be upgraded.
    let mut derived_name: HashSet<usize> = HashSet::new();
    let mut created: HashSet<usize> = HashSet::new();
    let mut updated: HashSet<usize> = HashSet::new();
    let mut touched: HashSet<usize> = HashSet::new();
    // Evidence keys per account (built lazily), so dedup is O(1) per finding.
    let mut keys: HashMap<usize, HashSet<EvKey>> = HashMap::new();
    let mut id_keys: HashMap<usize, HashSet<String>> = HashMap::new();
    let now = Utc::now();

    for f in findings {
        report.findings += 1;
        let explicit_name = f
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string);

        let (idx, is_new, derived) = match index.resolve(&f.domain, explicit_name.as_deref()) {
            Resolution::Skip => {
                report.skipped += 1;
                continue;
            }
            Resolution::Existing(i) => (i, false, false),
            Resolution::Adopt(i, key) => {
                index.adopt(data, i, key);
                (i, false, false)
            }
            Resolution::New(key) => {
                let (i, derived) = index.create(data, key, explicit_name.as_deref(), now);
                created.insert(i);
                report.new_accounts += 1;
                (i, true, derived)
            }
        };
        if is_new && derived {
            derived_name.insert(idx);
        }

        let acct = &mut data.accounts[idx];
        let mut changed = false;

        // Name upgrade for accounts created earlier in this very run.
        if let Some(n) = &explicit_name {
            if !is_new && derived_name.remove(&idx) {
                index.by_name.remove(&acct.name.trim().to_lowercase());
                acct.name = n.clone();
                index.by_name.entry(n.to_lowercase()).or_insert(idx);
                changed = true;
            }
        }

        // Evidence (deduplicated).
        let ev_keys = keys
            .entry(idx)
            .or_insert_with(|| acct.evidence.iter().map(ev_key).collect());
        if !ev_keys.insert(ev_key(&f.evidence)) {
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
            let ids = id_keys
                .entry(idx)
                .or_insert_with(|| acct.identifiers.iter().map(|x| x.to_lowercase()).collect());
            if ids.insert(id.to_lowercase()) {
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

        if changed && !is_new {
            acct.updated_at = now;
            updated.insert(idx);
        }
        touched.insert(idx);
    }

    // Confidence is recomputed once per touched account (it only depends on the set of kinds).
    for idx in touched {
        let acct = &mut data.accounts[idx];
        let c = confidence(&acct.evidence);
        if (c - acct.confidence).abs() > f32::EPSILON {
            acct.confidence = c;
            if !created.contains(&idx) && updated.insert(idx) {
                acct.updated_at = now;
            }
        }
    }

    // A new account that was modified again in the same run still counts as new only.
    report.updated_accounts = updated.iter().filter(|i| !created.contains(i)).count() as u64;
    report
}

/// Result of resolving a finding / observation domain to an account.
enum Resolution {
    /// Unparsable or infrastructure domain: ignore.
    Skip,
    Existing(usize),
    /// Existing `unresolved:` account that now gets this real domain.
    Adopt(usize, String),
    /// No account yet; carries the grouping key to create it under.
    New(String),
}

/// Lookup tables shared by all merge functions so that every source resolves
/// accounts the same way (canonical domain, `unresolved:` by service name).
struct AccountIndex {
    by_domain: HashMap<String, usize>,
    by_name: HashMap<String, usize>,
    /// Accounts whose domain is still an `unresolved:` key.
    unresolved: HashSet<usize>,
}

impl AccountIndex {
    fn new(accounts: &[Account]) -> Self {
        let mut by_domain = HashMap::with_capacity(accounts.len());
        let mut by_name = HashMap::new();
        let mut unresolved = HashSet::new();
        for (i, a) in accounts.iter().enumerate() {
            by_domain.entry(a.domain.clone()).or_insert(i);
            by_name.entry(a.name.trim().to_lowercase()).or_insert(i);
            if a.domain.starts_with(UNRESOLVED_PREFIX) {
                unresolved.insert(i);
            }
        }
        Self {
            by_domain,
            by_name,
            unresolved,
        }
    }

    /// Grouping key for a domain: canonical registrable domain or, for
    /// `unresolved:<slug>`, the verbatim key. `None` = skip.
    fn key_for(domain: &str, explicit_name: Option<&str>) -> Option<String> {
        if let Some(rest) = domain.trim().strip_prefix(UNRESOLVED_PREFIX) {
            let rest = rest.trim();
            if rest.is_empty() && explicit_name.is_none() {
                return None;
            }
            return Some(format!("{UNRESOLVED_PREFIX}{}", rest.to_lowercase()));
        }
        let rd = registrable_domain(domain)?;
        if is_infrastructure_domain(&rd) {
            return None;
        }
        let canon = canonical_service_domain(&rd);
        (!is_infrastructure_domain(&canon)).then_some(canon)
    }

    fn resolve(&self, domain: &str, explicit_name: Option<&str>) -> Resolution {
        let Some(key) = Self::key_for(domain, explicit_name) else {
            return Resolution::Skip;
        };
        // The exact key always wins (keeps re-imports stable, whatever the names are).
        if let Some(i) = self.by_domain.get(&key).copied() {
            return Resolution::Existing(i);
        }
        if let Some(rest) = key.strip_prefix(UNRESOLVED_PREFIX) {
            let mut target: Option<usize> = None;
            if let Some(n) = explicit_name {
                target = self.by_name.get(&n.to_lowercase()).copied();
            }
            if target.is_none() {
                target = self.by_name.get(rest).copied();
            }
            return match target {
                Some(i) => Resolution::Existing(i),
                None => Resolution::New(key),
            };
        }
        // A real domain whose name matches an account that so far only exists as
        // `unresolved:<name>` (SSO grant, PayPal / Apple receipt): adopt that account.
        let name = explicit_name
            .map(str::to_lowercase)
            .unwrap_or_else(|| display_name_for(&key).to_lowercase());
        if let Some(&i) = self.by_name.get(name.trim()) {
            if self.unresolved.contains(&i) {
                return Resolution::Adopt(i, key);
            }
        }
        Resolution::New(key)
    }

    /// Give an `unresolved:` account its real domain (the old key keeps resolving to it).
    fn adopt(&mut self, data: &mut VaultData, idx: usize, key: String) {
        data.accounts[idx].domain = key.clone();
        self.by_domain.insert(key, idx);
        self.unresolved.remove(&idx);
    }

    /// Resolve only to an existing account (never creates).
    fn find(&self, domain: &str) -> Option<usize> {
        match self.resolve(domain, None) {
            Resolution::Existing(i) | Resolution::Adopt(i, _) => Some(i),
            _ => None,
        }
    }

    /// Create the account for `key`; returns (index, name_was_derived).
    fn create(
        &mut self,
        data: &mut VaultData,
        key: String,
        explicit_name: Option<&str>,
        now: DateTime<Utc>,
    ) -> (usize, bool) {
        let (name, derived) = match explicit_name {
            Some(n) => (n.to_string(), false),
            None if key.starts_with(UNRESOLVED_PREFIX) => {
                (capitalise(&key[UNRESOLVED_PREFIX.len()..]), true)
            }
            None => (display_name_for(&key), true),
        };
        let i = data.accounts.len();
        let mut acct = Account::new(key.clone(), name.clone());
        acct.created_at = now;
        acct.updated_at = now;
        data.accounts.push(acct);
        if key.starts_with(UNRESOLVED_PREFIX) {
            self.unresolved.insert(i);
        }
        self.by_domain.insert(key, i);
        self.by_name.entry(name.trim().to_lowercase()).or_insert(i);
        (i, derived)
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Identity of an evidence entry for deduplication: (source, source_ref) when it has a
/// reference, else (source, kind, summary, date).
#[derive(Hash, PartialEq, Eq)]
enum EvKey {
    Ref(crate::model::SourceKind, String),
    NoRef(
        crate::model::SourceKind,
        EvidenceKind,
        String,
        Option<DateTime<Utc>>,
    ),
}

fn ev_key(e: &Evidence) -> EvKey {
    match &e.source_ref {
        Some(r) => EvKey::Ref(e.source.clone(), r.clone()),
        None => EvKey::NoRef(e.source.clone(), e.kind, e.summary.clone(), e.date),
    }
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

                // A full rescan reports every mail again: its range covers what we already
                // know and it counts at least as many mails. Replace instead of adding.
                let covers_known = obs.message_count >= sub.message_count
                    && sub.message_count > 0
                    && matches!((obs.first_seen, sub.first_seen), (Some(o), Some(s)) if o <= s)
                    && matches!((obs.last_seen, sub.last_seen), (Some(o), Some(s)) if o >= s);
                if covers_known {
                    sub.message_count = obs.message_count;
                } else {
                    sub.message_count += obs.message_count;
                }
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

                record_post_unsubscribe_mails(sub, &obs.watched_mails);

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
/// - every observation also adds a `Payment` evidence (dedup by source + source_ref, so
///   confidence rises on existing accounts too) and widens first/last_seen
///
/// Returns (records added, accounts created).
pub fn merge_payments(
    data: &mut VaultData,
    observations: Vec<crate::model::PaymentObservation>,
) -> (u64, u64) {
    let mut index = AccountIndex::new(&data.accounts);
    let now = Utc::now();
    let mut added = 0u64;
    let mut created = 0u64;
    for obs in observations {
        let explicit_name = obs
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string);
        let (idx, is_new) = match index.resolve(&obs.domain, explicit_name.as_deref()) {
            Resolution::Skip => continue,
            Resolution::Existing(i) => (i, false),
            Resolution::Adopt(i, key) => {
                index.adopt(data, i, key);
                (i, false)
            }
            Resolution::New(key) => {
                let (i, _) = index.create(data, key, explicit_name.as_deref(), now);
                created += 1;
                (i, true)
            }
        };
        let acct = &mut data.accounts[idx];
        let mut changed = is_new;

        if !acct.payments.iter().any(|p| same_payment(p, &obs.record)) {
            acct.payments.push(obs.record.clone());
            added += 1;
            changed = true;
        }

        // Payment evidence (distinct per source reference).
        let payment_ev = Evidence {
            kind: EvidenceKind::Payment,
            source: obs.source.clone(),
            summary: obs.record.summary.clone(),
            date: obs.record.date,
            source_ref: obs.record.source_ref.clone(),
        };
        let ev_exists = acct.evidence.iter().any(|e| {
            e.kind == EvidenceKind::Payment
                && e.source == payment_ev.source
                && match (&e.source_ref, &payment_ev.source_ref) {
                    (Some(a), Some(b)) => a == b,
                    (None, None) => e.summary == payment_ev.summary && e.date == payment_ev.date,
                    _ => false,
                }
        });
        if !ev_exists {
            if let Some(d) = payment_ev.date {
                acct.first_seen = min_opt(acct.first_seen, Some(d));
                acct.last_seen = max_opt(acct.last_seen, Some(d));
            }
            acct.evidence.push(payment_ev);
            changed = true;
        }

        if let Some(id) = obs
            .identifier
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !acct
                .identifiers
                .iter()
                .any(|x| x.to_lowercase() == id.to_lowercase())
            {
                acct.identifiers.push(id.to_string());
                changed = true;
            }
        }

        if changed {
            acct.confidence = confidence(&acct.evidence);
            if !is_new {
                acct.updated_at = now;
            }
        }
    }
    (added, created)
}

/// Same payment: equal `source_ref` when both have one, else date + amount + summary.
fn same_payment(a: &PaymentRecord, b: &PaymentRecord) -> bool {
    match (&a.source_ref, &b.source_ref) {
        (Some(x), Some(y)) => x == y,
        (None, None) => {
            a.date == b.date && a.amount_minor == b.amount_minor && a.summary == b.summary
        }
        _ => false,
    }
}

/// Apply security observations (password audit) to matching accounts by canonical
/// domain (+ identifier when given). Values from a newer import overwrite older ones;
/// `None` fields don't overwrite. Sets `security.checked_at`. Returns accounts updated.
///
/// Observations for unknown domains are ignored (an audit never creates accounts). A
/// given identifier is added to the account (case-insensitive union); a phone-number
/// identifier sets `security.phone_login`.
pub fn merge_security(
    data: &mut VaultData,
    observations: Vec<crate::model::SecurityObservation>,
) -> u64 {
    let index = AccountIndex::new(&data.accounts);
    let now = Utc::now();
    let mut touched: HashSet<usize> = HashSet::new();
    for obs in observations {
        let Some(idx) = index.find(&obs.domain) else {
            continue;
        };
        let acct = &mut data.accounts[idx];
        if obs.password_reuse_count.is_some() {
            acct.security.password_reuse_count = obs.password_reuse_count;
        }
        if obs.password_pwned.is_some() {
            acct.security.password_pwned = obs.password_pwned;
        }
        if obs.has_2fa.is_some() {
            acct.security.has_2fa = obs.has_2fa;
        }
        if let Some(id) = obs
            .identifier
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !acct
                .identifiers
                .iter()
                .any(|x| x.to_lowercase() == id.to_lowercase())
            {
                acct.identifiers.push(id.to_string());
            }
            if crate::insights::looks_like_phone(id) {
                acct.security.phone_login = true;
            }
        }
        acct.security.checked_at = Some(now);
        acct.updated_at = now;
        touched.insert(idx);
    }
    touched.len() as u64
}

/// Attach domain-level breaches (`(breach_domain, record)`, e.g. from the public HIBP
/// breach list) to accounts with that canonical domain, and identifier-level hits
/// (`confirmed_for_user = true`). Dedup by breach name. Returns accounts updated.
///
/// If a breach is already recorded and the new record is confirmed for the user, the
/// stored one is upgraded to confirmed (counts as an update). Unknown domains are ignored.
pub fn merge_breaches(
    data: &mut VaultData,
    breaches: Vec<(String, crate::model::BreachRecord)>,
) -> u64 {
    let index = AccountIndex::new(&data.accounts);
    let now = Utc::now();
    let mut touched: HashSet<usize> = HashSet::new();
    for (domain, record) in breaches {
        let Some(idx) = index.find(&domain) else {
            continue;
        };
        let acct = &mut data.accounts[idx];
        match acct
            .security
            .breaches
            .iter_mut()
            .find(|b| b.name == record.name)
        {
            Some(existing) => {
                if record.confirmed_for_user && !existing.confirmed_for_user {
                    existing.confirmed_for_user = true;
                } else {
                    continue;
                }
            }
            None => acct.security.breaches.push(record),
        }
        acct.security.checked_at = Some(now);
        acct.updated_at = now;
        touched.insert(idx);
    }
    touched.len() as u64
}

/// Days after the last successful unsubscribe during which mails are not counted.
const UNSUBSCRIBE_GRACE_DAYS: i64 = 2;

/// For subscriptions that are `Unsubscribed` / `StillReceiving`: add observation
/// `watched_mails` dated after the last successful unsubscribe attempt + 2 days grace
/// to `post_unsubscribe_mails` (dedup by Message-ID). Called from `merge_subscriptions`.
///
/// Mails without a date are ignored; mails without a Message-ID are deduplicated by
/// (date, from, subject). The list stays sorted by date. Returns the number added.
pub fn record_post_unsubscribe_mails(
    sub: &mut crate::model::Subscription,
    mails: &[crate::model::MailRef],
) -> usize {
    if !matches!(
        sub.status,
        SubscriptionStatus::Unsubscribed | SubscriptionStatus::StillReceiving
    ) {
        return 0;
    }
    let Some(last_ok) = sub.history.iter().filter(|h| h.ok).map(|h| h.at).max() else {
        return 0;
    };
    let cutoff = last_ok + Duration::days(UNSUBSCRIBE_GRACE_DAYS);
    let mut added = 0;
    for m in mails {
        let Some(date) = m.date else { continue };
        if date <= cutoff {
            continue;
        }
        let dup = sub
            .post_unsubscribe_mails
            .iter()
            .any(|e| match (&e.message_id, &m.message_id) {
                (Some(a), Some(b)) => a.trim() == b.trim(),
                (None, None) => e.date == m.date && e.from == m.from && e.subject == m.subject,
                _ => false,
            });
        if !dup {
            sub.post_unsubscribe_mails.push(m.clone());
            added += 1;
        }
    }
    if added > 0 {
        sub.post_unsubscribe_mails.sort_by_key(|m| m.date);
    }
    added
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
        AccountStatus, MailCategory, SourceKind, SsoProvider, UnsubscribeAttempt, UnsubscribeMethod,
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

    // ---- Wave 2: payments, security, breaches, post-unsubscribe mails ----

    use crate::model::{
        BreachRecord, MailRef, PaymentKind, PaymentObservation, PaymentRecord, SecurityObservation,
    };

    fn pay_obs(domain: &str, r: Option<&str>, amount: i64, d: i64) -> PaymentObservation {
        PaymentObservation {
            domain: domain.into(),
            name: None,
            identifier: None,
            source: SourceKind::Mail {
                mailbox: "me@example.com".into(),
            },
            record: PaymentRecord {
                date: Some(day(d)),
                kind: PaymentKind::Renewal,
                amount_minor: Some(amount),
                currency: Some("EUR".into()),
                plan: None,
                method: None,
                via: None,
                summary: "Rechnung".into(),
                source_ref: r.map(str::to_string),
            },
        }
    }

    #[test]
    fn payments_create_account_with_payment_evidence() {
        let mut d = VaultData::default();
        let (added, created) = merge_payments(
            &mut d,
            vec![pay_obs("billing.spotify.com", Some("m1"), 999, 5)],
        );
        assert_eq!((added, created), (1, 1));
        let a = &d.accounts[0];
        assert_eq!(a.domain, "spotify.com");
        assert_eq!(a.name, "Spotify");
        assert_eq!(a.payments.len(), 1);
        assert_eq!(a.evidence.len(), 1);
        assert_eq!(a.evidence[0].kind, EvidenceKind::Payment);
        assert!((a.confidence - 0.9).abs() < 1e-6);
        assert_eq!(a.first_seen, Some(day(5)));
        assert_eq!(a.last_seen, Some(day(5)));
    }

    #[test]
    fn payments_reimport_is_noop() {
        let mut d = VaultData::default();
        let obs = vec![
            pay_obs("spotify.com", Some("m1"), 999, 5),
            pay_obs("spotify.com", Some("m2"), 999, 35),
        ];
        assert_eq!(merge_payments(&mut d, obs.clone()), (2, 1));
        let before = d.accounts[0].clone();
        assert_eq!(merge_payments(&mut d, obs), (0, 0));
        let after = &d.accounts[0];
        assert_eq!(after.payments, before.payments);
        assert_eq!(after.evidence, before.evidence);
        assert_eq!(after.updated_at, before.updated_at);
    }

    #[test]
    fn payments_raise_confidence_of_existing_account_and_keep_user_state() {
        let mut d = VaultData::default();
        let mut f = finding("netflix.com", EvidenceKind::Newsletter, "n1");
        f.name = Some("Netflix DE".into());
        merge_findings(&mut d, vec![f]);
        d.accounts[0].status = AccountStatus::Keep;
        let c0 = d.accounts[0].confidence;
        assert_eq!(
            merge_payments(
                &mut d,
                vec![pay_obs("www.netflix.com", Some("p1"), 1299, 3)]
            ),
            (1, 0)
        );
        let a = &d.accounts[0];
        assert!(a.confidence > c0);
        assert_eq!(a.name, "Netflix DE");
        assert_eq!(a.status, AccountStatus::Keep);
        assert_eq!(a.evidence.len(), 2);
        assert_eq!(d.accounts.len(), 1);
    }

    #[test]
    fn payments_dedup_by_source_ref_and_fallback() {
        let mut d = VaultData::default();
        // same Message-ID, different amount parsed: still the same record
        merge_payments(&mut d, vec![pay_obs("a.com", Some("m1"), 100, 1)]);
        assert_eq!(
            merge_payments(&mut d, vec![pay_obs("a.com", Some("m1"), 200, 1)]).0,
            0
        );
        // aggregator line items: same mail, different line index
        let (n, _) = merge_payments(
            &mut d,
            vec![
                pay_obs("a.com", Some("m9#0"), 100, 2),
                pay_obs("a.com", Some("m9#1"), 100, 2),
            ],
        );
        assert_eq!(n, 2);
        assert_eq!(
            d.accounts[0].evidence.len(),
            3,
            "one Payment evidence per source_ref"
        );
        // fallback without source_ref
        merge_payments(&mut d, vec![pay_obs("a.com", None, 500, 9)]);
        assert_eq!(
            merge_payments(&mut d, vec![pay_obs("a.com", None, 500, 9)]).0,
            0
        );
        assert_eq!(
            merge_payments(&mut d, vec![pay_obs("a.com", None, 501, 9)]).0,
            1
        );
    }

    #[test]
    fn payments_unresolved_by_name_identifier_and_infrastructure() {
        let mut d = VaultData::default();
        let mut f = finding("notion.so", EvidenceKind::Welcome, "w");
        f.name = Some("Notion".into());
        merge_findings(&mut d, vec![f]);
        let mut o = pay_obs("unresolved:notion", Some("x1"), 800, 4);
        o.name = Some("Notion".into());
        o.identifier = Some("Me@Example.com".into());
        let (added, created) = merge_payments(&mut d, vec![o.clone()]);
        assert_eq!((added, created), (1, 0));
        assert_eq!(d.accounts.len(), 1);
        assert_eq!(
            d.accounts[0].identifiers,
            vec!["Me@Example.com".to_string()]
        );
        o.record.source_ref = Some("x2".into());
        o.identifier = Some("me@example.com".into());
        merge_payments(&mut d, vec![o]);
        assert_eq!(d.accounts[0].identifiers.len(), 1);
        // unknown app stays unresolved and creates an account
        let mut u = pay_obs("unresolved:coolapp", Some("u1"), 100, 1);
        u.name = Some("CoolApp".into());
        assert_eq!(merge_payments(&mut d, vec![u]), (1, 1));
        assert_eq!(d.accounts[1].domain, "unresolved:coolapp");
        // infrastructure / garbage skipped
        assert_eq!(
            merge_payments(
                &mut d,
                vec![
                    pay_obs("gmail.com", Some("g"), 1, 1),
                    pay_obs("???", Some("h"), 1, 1)
                ]
            ),
            (0, 0)
        );
    }

    fn sec(domain: &str) -> SecurityObservation {
        SecurityObservation {
            domain: domain.into(),
            identifier: None,
            password_reuse_count: None,
            password_pwned: None,
            has_2fa: None,
        }
    }

    #[test]
    fn security_overwrites_only_given_values() {
        let mut d = VaultData::default();
        merge_findings(
            &mut d,
            vec![finding("github.com", EvidenceKind::SavedPassword, "s")],
        );
        let mut o = sec("www.github.com");
        o.password_reuse_count = Some(2);
        o.has_2fa = Some(false);
        assert_eq!(merge_security(&mut d, vec![o]), 1);
        let s = &d.accounts[0].security;
        assert_eq!(s.password_reuse_count, Some(2));
        assert_eq!(s.has_2fa, Some(false));
        assert_eq!(s.password_pwned, None);
        assert!(s.checked_at.is_some());
        // newer import: reuse resolved, 2FA unknown (None keeps), pwned set
        let mut o = sec("github.com");
        o.password_reuse_count = Some(0);
        o.password_pwned = Some(true);
        merge_security(&mut d, vec![o]);
        let s = &d.accounts[0].security;
        assert_eq!(s.password_reuse_count, Some(0));
        assert_eq!(s.has_2fa, Some(false));
        assert_eq!(s.password_pwned, Some(true));
    }

    #[test]
    fn security_ignores_unknown_domains_and_counts_accounts_once() {
        let mut d = VaultData::default();
        merge_findings(
            &mut d,
            vec![finding("a.com", EvidenceKind::SavedPassword, "s")],
        );
        let mut o1 = sec("a.com");
        o1.has_2fa = Some(true);
        let mut o2 = sec("login.a.com");
        o2.password_pwned = Some(false);
        assert_eq!(merge_security(&mut d, vec![o1, o2, sec("unknown.org")]), 1);
        assert_eq!(d.accounts.len(), 1, "an audit never creates accounts");
    }

    #[test]
    fn security_identifier_union_and_phone_login() {
        let mut d = VaultData::default();
        merge_findings(
            &mut d,
            vec![finding("signal.org", EvidenceKind::SavedPassword, "s")],
        );
        let mut o = sec("signal.org");
        o.identifier = Some("+491701234567".into());
        merge_security(&mut d, vec![o.clone(), o]);
        assert_eq!(d.accounts[0].identifiers, vec!["+491701234567".to_string()]);
        assert!(d.accounts[0].security.phone_login);
    }

    fn breach(name: &str, confirmed: bool) -> BreachRecord {
        BreachRecord {
            name: name.into(),
            title: name.into(),
            breach_date: Some("2020-01-01".into()),
            data_classes: vec!["Email addresses".into()],
            confirmed_for_user: confirmed,
        }
    }

    #[test]
    fn breaches_attach_dedup_and_upgrade() {
        let mut d = VaultData::default();
        merge_findings(
            &mut d,
            vec![
                finding("adobe.com", EvidenceKind::Welcome, "a"),
                finding("dropbox.com", EvidenceKind::Welcome, "b"),
            ],
        );
        let n = merge_breaches(
            &mut d,
            vec![
                ("adobe.com".into(), breach("Adobe", false)),
                ("www.adobe.com".into(), breach("Adobe", false)),
                ("nowhere.org".into(), breach("Other", false)),
            ],
        );
        assert_eq!(n, 1);
        assert_eq!(d.accounts[0].security.breaches.len(), 1);
        assert!(d.accounts[1].security.breaches.is_empty());
        // idempotent
        assert_eq!(
            merge_breaches(&mut d, vec![("adobe.com".into(), breach("Adobe", false))]),
            0
        );
        // identifier-level hit upgrades the stored record, adds a new one elsewhere
        let n = merge_breaches(
            &mut d,
            vec![
                ("adobe.com".into(), breach("Adobe", true)),
                ("dropbox.com".into(), breach("Dropbox", true)),
            ],
        );
        assert_eq!(n, 2);
        assert!(d.accounts[0].security.breaches[0].confirmed_for_user);
        assert_eq!(d.accounts[0].security.breaches.len(), 1);
        assert_eq!(
            merge_breaches(&mut d, vec![("adobe.com".into(), breach("Adobe", false))]),
            0
        );
        assert!(
            d.accounts[0].security.breaches[0].confirmed_for_user,
            "never downgraded"
        );
    }

    fn mail(id: Option<&str>, d: i64) -> MailRef {
        MailRef {
            date: Some(day(d)),
            from: "news@shop.com".into(),
            subject: format!("Angebot {d}"),
            message_id: id.map(str::to_string),
            raw_headers: String::new(),
        }
    }

    fn unsubscribed_sub(at_day: i64, ok: bool) -> Subscription {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(1, 0, 1, &[])]);
        let mut s = d.subscriptions.remove(0);
        s.status = SubscriptionStatus::Unsubscribed;
        s.history.push(UnsubscribeAttempt {
            at: day(at_day),
            method: UnsubscribeMethod::Link { url: "u".into() },
            ok,
            message: String::new(),
        });
        s
    }

    #[test]
    fn post_unsubscribe_grace_window() {
        let mut s = unsubscribed_sub(10, true);
        let n = record_post_unsubscribe_mails(
            &mut s,
            &[
                mail(Some("<a>"), 9),
                mail(Some("<b>"), 11),
                mail(Some("<c>"), 12), // exactly at the end of the grace period: not counted
                mail(Some("<d>"), 13),
            ],
        );
        assert_eq!(n, 1);
        assert_eq!(
            s.post_unsubscribe_mails[0].message_id.as_deref(),
            Some("<d>")
        );
    }

    #[test]
    fn post_unsubscribe_dedup_by_message_id_is_idempotent() {
        let mut s = unsubscribed_sub(10, true);
        let mails = [
            mail(Some("<a>"), 20),
            mail(Some("<a>"), 21),
            mail(Some("<b>"), 15),
        ];
        assert_eq!(record_post_unsubscribe_mails(&mut s, &mails), 2);
        assert_eq!(record_post_unsubscribe_mails(&mut s, &mails), 0);
        let ids: Vec<_> = s
            .post_unsubscribe_mails
            .iter()
            .map(|m| m.message_id.clone().unwrap())
            .collect();
        assert_eq!(ids, vec!["<b>", "<a>"], "sorted by date");
    }

    #[test]
    fn post_unsubscribe_without_message_id_and_undated() {
        let mut s = unsubscribed_sub(10, true);
        let mut undated = mail(None, 30);
        undated.date = None;
        let m = mail(None, 30);
        assert_eq!(
            record_post_unsubscribe_mails(&mut s, &[m.clone(), m, undated]),
            1
        );
    }

    #[test]
    fn post_unsubscribe_requires_status_and_successful_attempt() {
        let mails = [mail(Some("<a>"), 40)];
        let mut failed = unsubscribed_sub(10, false);
        assert_eq!(record_post_unsubscribe_mails(&mut failed, &mails), 0);
        let mut active = unsubscribed_sub(10, true);
        active.status = SubscriptionStatus::Active;
        assert_eq!(record_post_unsubscribe_mails(&mut active, &mails), 0);
        let mut still = unsubscribed_sub(10, true);
        still.status = SubscriptionStatus::StillReceiving;
        assert_eq!(record_post_unsubscribe_mails(&mut still, &mails), 1);
    }

    #[test]
    fn merge_subscriptions_records_watched_mails() {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(1, 0, 1, &[])]);
        d.subscriptions[0].status = SubscriptionStatus::Unsubscribed;
        d.subscriptions[0].history.push(UnsubscribeAttempt {
            at: day(10),
            method: UnsubscribeMethod::Link { url: "u".into() },
            ok: true,
            message: String::new(),
        });
        let mut o = obs(2, 14, 20, &[]);
        o.watched_mails = vec![mail(Some("<a>"), 14), mail(Some("<b>"), 20)];
        merge_subscriptions(&mut d, vec![o.clone()]);
        assert_eq!(d.subscriptions[0].post_unsubscribe_mails.len(), 2);
        assert_eq!(
            d.subscriptions[0].status,
            SubscriptionStatus::StillReceiving
        );
        // same observation again: mails are not duplicated
        merge_subscriptions(&mut d, vec![o]);
        assert_eq!(d.subscriptions[0].post_unsubscribe_mails.len(), 2);
        // active subscriptions keep no evidence
        let mut d2 = VaultData::default();
        let mut o2 = obs(1, 0, 1, &[]);
        o2.watched_mails = vec![mail(Some("<z>"), 50)];
        merge_subscriptions(&mut d2, vec![o2.clone()]);
        merge_subscriptions(&mut d2, vec![o2]);
        assert!(d2.subscriptions[0].post_unsubscribe_mails.is_empty());
    }

    fn named(domain: &str, name: &str, kind: EvidenceKind, r: &str) -> Finding {
        Finding {
            name: Some(name.into()),
            ..finding(domain, kind, r)
        }
    }

    #[test]
    fn unresolved_account_is_adopted_by_the_real_domain() {
        let mut d = VaultData::default();
        merge_findings(
            &mut d,
            vec![named(
                "unresolved:netflix",
                "Netflix",
                EvidenceKind::SsoGrant,
                "s",
            )],
        );
        merge_findings(
            &mut d,
            vec![named("netflix.com", "Netflix", EvidenceKind::Welcome, "w")],
        );
        assert_eq!(d.accounts.len(), 1);
        assert_eq!(d.accounts[0].domain, "netflix.com");
        assert_eq!(d.accounts[0].evidence.len(), 2);
        // Later unresolved mentions still land on the same account.
        merge_findings(
            &mut d,
            vec![named(
                "unresolved:netflix",
                "Netflix",
                EvidenceKind::SsoGrant,
                "s2",
            )],
        );
        assert_eq!(d.accounts.len(), 1);
        assert_eq!(d.accounts[0].evidence.len(), 3);
        // A different real domain with another name is not adopted.
        merge_findings(
            &mut d,
            vec![named("hulu.com", "Hulu", EvidenceKind::Welcome, "h")],
        );
        assert_eq!(d.accounts.len(), 2);
    }

    #[test]
    fn reimport_is_stable_with_name_collisions() {
        let make = || {
            vec![
                finding("paypal.de", EvidenceKind::Welcome, "r1"),
                finding("unresolved:9", EvidenceKind::SsoGrant, "r29"),
                named("paypal.de", "9", EvidenceKind::Receipt, "r20"),
            ]
        };
        let mut d = VaultData::default();
        merge_findings(&mut d, make());
        let n: usize = d.accounts.iter().map(|a| a.evidence.len()).sum();
        let rep = merge_findings(&mut d, make());
        assert_eq!(
            d.accounts.iter().map(|a| a.evidence.len()).sum::<usize>(),
            n
        );
        assert_eq!(rep.updated_accounts, 0);
    }

    #[test]
    fn many_findings_for_one_account_are_fast() {
        let mut d = VaultData::default();
        let f: Vec<Finding> = (0..20_000)
            .map(|i| {
                finding(
                    "big.example.com",
                    EvidenceKind::SavedPassword,
                    &format!("r{i}"),
                )
            })
            .collect();
        let t = std::time::Instant::now();
        merge_findings(&mut d, f.clone());
        merge_findings(&mut d, f);
        assert_eq!(d.accounts[0].evidence.len(), 20_000);
        assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    }

    #[test]
    fn full_rescan_replaces_counts_but_incremental_adds() {
        let mut d = VaultData::default();
        merge_subscriptions(&mut d, vec![obs(10, 0, 30, &["a"])]);
        // Incremental: newer mails only.
        merge_subscriptions(&mut d, vec![obs(3, 31, 40, &["b"])]);
        assert_eq!(d.subscriptions[0].message_count, 13);
        // Full rescan: all 13 mails again (range covers the known one).
        merge_subscriptions(&mut d, vec![obs(13, 0, 40, &["b"])]);
        assert_eq!(d.subscriptions[0].message_count, 13);
        // Full rescan that also found 2 newer mails.
        merge_subscriptions(&mut d, vec![obs(15, 0, 45, &["c"])]);
        assert_eq!(d.subscriptions[0].message_count, 15);
    }
}
