//! Derived insights computed from the vault (pure, no I/O): billing / subscriptions,
//! risk ranking, privacy score, payment-method index, alias recommendations,
//! takeover risks. Nothing here is stored; the UI recomputes on demand.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{is_infrastructure_domain, registrable_domain};
use crate::model::{
    Account, AccountStatus, EvidenceKind, MailCategory, PaymentKind, PaymentMethod, PaymentRecord,
    RequestStatus, SubscriptionStatus, VaultData,
};

/// Billing interval detected from payment dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingInterval {
    Weekly,
    Monthly,
    Quarterly,
    HalfYearly,
    Yearly,
}

impl BillingInterval {
    pub fn label_de(self) -> &'static str {
        match self {
            BillingInterval::Weekly => "wöchentlich",
            BillingInterval::Monthly => "monatlich",
            BillingInterval::Quarterly => "vierteljährlich",
            BillingInterval::HalfYearly => "halbjährlich",
            BillingInterval::Yearly => "jährlich",
        }
    }

    /// Nominal length in days (7 / 30 / 91 / 182 / 365).
    fn days(self) -> i64 {
        match self {
            BillingInterval::Weekly => 7,
            BillingInterval::Monthly => 30,
            BillingInterval::Quarterly => 91,
            BillingInterval::HalfYearly => 182,
            BillingInterval::Yearly => 365,
        }
    }

    /// Payments per year.
    fn per_year(self) -> i64 {
        match self {
            BillingInterval::Weekly => 52,
            BillingInterval::Monthly => 12,
            BillingInterval::Quarterly => 4,
            BillingInterval::HalfYearly => 2,
            BillingInterval::Yearly => 1,
        }
    }

    const ALL: [BillingInterval; 5] = [
        BillingInterval::Weekly,
        BillingInterval::Monthly,
        BillingInterval::Quarterly,
        BillingInterval::HalfYearly,
        BillingInterval::Yearly,
    ];

    /// Map a median gap (days) to an interval with ±20 % tolerance.
    fn from_median_days(median: f64) -> Option<BillingInterval> {
        Self::ALL.into_iter().find(|i| {
            let n = i.days() as f64;
            (median - n).abs() <= n * 0.2
        })
    }

    /// Guess the period from free text ("Monatsabo", "annual plan" ...).
    fn from_text(text: &str) -> Option<BillingInterval> {
        let t = text.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
        if has(&[
            "halbjähr",
            "halbjahr",
            "half-year",
            "half year",
            "6 month",
            "6 monat",
            "semi",
        ]) {
            Some(BillingInterval::HalfYearly)
        } else if has(&["vierteljähr", "vierteljahr", "quarter"]) {
            Some(BillingInterval::Quarterly)
        } else if has(&[
            "jähr", "jahr", "annual", "year", "yearly", "12 month", "12 monat",
        ]) {
            Some(BillingInterval::Yearly)
        } else if has(&["monat", "month"]) {
            Some(BillingInterval::Monthly)
        } else if has(&["woche", "week"]) {
            Some(BillingInterval::Weekly)
        } else {
            None
        }
    }
}

/// Subscription / billing summary for one account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BillingSummary {
    pub account_id: Uuid,
    /// Recurring payments detected (≥ 2 charges at a regular interval, or an explicit
    /// renewal mail) and not cancelled afterwards.
    pub active_subscription: bool,
    pub interval: Option<BillingInterval>,
    /// Latest recurring amount in minor units.
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    /// Normalised to a month (yearly / 12 etc.), minor units.
    pub monthly_minor: Option<i64>,
    pub plan: Option<String>,
    pub last_charge: Option<DateTime<Utc>>,
    pub next_charge_estimate: Option<DateTime<Utc>>,
    /// Latest amount higher than the previous one, or a PriceChange record.
    pub price_increased: bool,
    pub cancelled_at: Option<DateTime<Utc>>,
    /// Payment methods seen (newest first, dedup).
    pub methods: Vec<PaymentMethod>,
    /// Billed via an aggregator (apple.com, google.com, paypal.com) -> cancel there.
    pub via: Option<String>,
    /// Paid, but no login / 2FA / password-reset signal in the last 6 months.
    pub possibly_unused: bool,
    /// Sum of all charges in the last 12 months, minor units, per currency.
    pub spent_last_12m: Vec<(String, i64)>,
}

/// Evidence kinds that show the account is still being used.
const USAGE_EVIDENCE: [EvidenceKind; 4] = [
    EvidenceKind::Welcome,
    EvidenceKind::LoginAlert,
    EvidenceKind::PasswordReset,
    EvidenceKind::TwoFactor,
];

/// Window without usage evidence after which a paid account counts as unused.
const UNUSED_DAYS: i64 = 180;

/// Without a known period, a single renewal older than this is considered lapsed.
const LAPSED_DAYS: i64 = 400;

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// Compute the billing summary of one account (None if it has no payment records).
/// Interval detection: median gap between Charge/Renewal dates, tolerance ±20 %.
///
/// Heuristics:
/// - Dated `Charge`/`Renewal` records form the charge series (one per calendar day).
///   The median gap is mapped to 7 / 30 / 91 / 182 / 365 days (±20 %).
/// - An explicit `Renewal` marks the account as recurring even without a detectable
///   interval, if the plan is known or the period can be read from plan / summary text
///   ("Monatsabo", "annual plan"). A period from text acts as the interval.
/// - A `Cancelled` record dated on / after the last charge (or undated) makes the
///   subscription inactive.
/// - `next_charge_estimate = last + interval`; when that is more than one interval in the
///   past the subscription is considered lapsed (inactive, no estimate).
/// - `price_increased`: the newest amount is higher than the previous amount in the same
///   currency (recurring series only), or a dated `PriceChange` follows the last charge.
/// - `possibly_unused`: active, but no Welcome / LoginAlert / PasswordReset / TwoFactor
///   evidence within 180 days.
/// - `spent_last_12m`: charges and renewals of the last 365 days minus refunds, per currency.
pub fn billing_summary(account: &Account, now: DateTime<Utc>) -> Option<BillingSummary> {
    if account.payments.is_empty() {
        return None;
    }
    // Newest first; undated records last.
    let mut recs: Vec<&PaymentRecord> = account.payments.iter().collect();
    recs.sort_by_key(|r| std::cmp::Reverse(r.date));

    // Charge series ascending, one per calendar day.
    let mut charges: Vec<&PaymentRecord> = recs
        .iter()
        .rev()
        .copied()
        .filter(|r| {
            matches!(r.kind, PaymentKind::Charge | PaymentKind::Renewal)
                && r.date.is_some()
                && r.amount_minor.is_none_or(|a| a > 0)
        })
        .collect();
    charges.dedup_by_key(|r| r.date.map(|d| d.date_naive()));

    let last_charge = charges.last().and_then(|r| r.date);
    let has_renewal = recs.iter().any(|r| r.kind == PaymentKind::Renewal);
    let plan = recs.iter().find_map(|r| r.plan.clone());

    // Interval.
    let detected = if charges.len() >= 2 {
        let mut gaps: Vec<f64> = charges
            .windows(2)
            .filter_map(|w| Some((w[1].date? - w[0].date?).num_seconds() as f64 / 86_400.0))
            .collect();
        gaps.sort_by(|a, b| a.total_cmp(b));
        if gaps.is_empty() {
            None
        } else {
            BillingInterval::from_median_days(median(&gaps))
        }
    } else {
        None
    };
    let hinted = if detected.is_none() && has_renewal {
        recs.iter()
            .filter(|r| r.kind == PaymentKind::Renewal)
            .find_map(|r| {
                let text = format!("{} {}", r.plan.as_deref().unwrap_or(""), r.summary);
                BillingInterval::from_text(&text)
            })
            .or_else(|| plan.as_deref().and_then(BillingInterval::from_text))
    } else {
        None
    };
    let interval = detected.or(hinted);
    let recurring = interval.is_some() || (has_renewal && plan.is_some());

    // Cancellation.
    let cancel_dates: Vec<Option<DateTime<Utc>>> = recs
        .iter()
        .filter(|r| r.kind == PaymentKind::Cancelled)
        .map(|r| r.date)
        .collect();
    let cancelled_at = cancel_dates
        .iter()
        .flatten()
        .copied()
        .max()
        .filter(|c| last_charge.is_none_or(|l| *c >= l));
    let cancelled = cancelled_at.is_some() || cancel_dates.iter().any(Option::is_none);

    // Lapsed / next charge.
    let next = match (last_charge, interval) {
        (Some(l), Some(i)) => Some(l + Duration::days(i.days())),
        _ => None,
    };
    let lapsed = match (next, interval, last_charge) {
        (Some(n), Some(i), _) => n + Duration::days(i.days()) < now,
        (None, None, Some(l)) => l + Duration::days(LAPSED_DAYS) < now,
        _ => false,
    };
    let active = recurring && !cancelled && !lapsed;
    let next_charge_estimate = if active { next } else { None };

    // Amount, currency, monthly normalisation.
    let latest = charges.iter().rev().find(|r| r.amount_minor.is_some());
    let amount_minor = latest.and_then(|r| r.amount_minor);
    let currency = latest.and_then(|r| r.currency.clone());
    let monthly_minor = match (amount_minor, interval) {
        (Some(a), Some(i)) => Some((a * i.per_year() + 6) / 12),
        _ => None,
    };

    // Price increase.
    let mut price_increased = false;
    if recurring {
        if let Some(l) = latest {
            let cur = &l.currency;
            let prev = charges
                .iter()
                .rev()
                .filter(|r| r.amount_minor.is_some() && &r.currency == cur)
                .nth(1);
            if let (Some(p), Some(a)) = (prev.and_then(|r| r.amount_minor), l.amount_minor) {
                price_increased = a > p;
            }
        }
    }
    if recs.iter().any(|r| {
        r.kind == PaymentKind::PriceChange
            && r.date.is_some_and(|d| last_charge.is_none_or(|l| d > l))
    }) {
        price_increased = true;
    }

    // Methods (newest first, dedup), aggregator.
    let mut methods: Vec<PaymentMethod> = Vec::new();
    for m in recs.iter().filter_map(|r| r.method.clone()) {
        if !methods.contains(&m) {
            methods.push(m);
        }
    }
    let via = recs.iter().find_map(|r| r.via.clone());

    // Unused?
    let used_recently = account.evidence.iter().any(|e| {
        USAGE_EVIDENCE.contains(&e.kind)
            && e.date
                .is_some_and(|d| d > now - Duration::days(UNUSED_DAYS))
    });
    let possibly_unused = active && !used_recently;

    // Spending in the last 12 months.
    let mut spent: BTreeMap<String, i64> = BTreeMap::new();
    for r in &account.payments {
        let (Some(d), Some(a), Some(c)) = (r.date, r.amount_minor, r.currency.as_ref()) else {
            continue;
        };
        if d <= now - Duration::days(365) || d > now {
            continue;
        }
        let signed = match r.kind {
            PaymentKind::Charge | PaymentKind::Renewal => a,
            PaymentKind::Refund => -a.abs(),
            _ => continue,
        };
        *spent.entry(c.to_uppercase()).or_insert(0) += signed;
    }

    Some(BillingSummary {
        account_id: account.id,
        active_subscription: active,
        interval,
        amount_minor,
        currency,
        monthly_minor,
        plan,
        last_charge,
        next_charge_estimate,
        price_increased,
        cancelled_at,
        methods,
        via,
        possibly_unused,
        spent_last_12m: spent.into_iter().collect(),
    })
}

/// Accounts that count as "open" (still existing and relevant).
fn is_open(a: &Account) -> bool {
    !matches!(
        a.status,
        AccountStatus::Deleted | AccountStatus::NotAnAccount | AccountStatus::Ignored
    )
}

/// Totals over all active subscriptions, per currency: (currency, monthly_minor, yearly_minor, count).
/// Only open accounts with a known interval, amount and currency are counted; sorted by currency.
pub fn subscription_totals(data: &VaultData, now: DateTime<Utc>) -> Vec<(String, i64, i64, usize)> {
    let mut totals: BTreeMap<String, (i64, i64, usize)> = BTreeMap::new();
    for a in data.accounts.iter().filter(|a| is_open(a)) {
        let Some(b) = billing_summary(a, now) else {
            continue;
        };
        if !b.active_subscription {
            continue;
        }
        let (Some(i), Some(amount), Some(cur), Some(monthly)) =
            (b.interval, b.amount_minor, b.currency, b.monthly_minor)
        else {
            continue;
        };
        let e = totals.entry(cur.to_uppercase()).or_insert((0, 0, 0));
        e.0 += monthly;
        e.1 += amount * i.per_year();
        e.2 += 1;
    }
    totals
        .into_iter()
        .map(|(c, (m, y, n))| (c, m, y, n))
        .collect()
}

/// Where each payment method is stored: method -> accounts (sorted by last use).
/// Powers "Karte •••• 1234 ist bei 14 Diensten hinterlegt" and the card-compromise playbook.
///
/// Deleted / not-an-account entries are excluded. Methods are ordered by number of
/// accounts (descending), then by label; accounts by the date of the newest payment
/// made with the method (newest first, undated last).
pub fn payment_method_index(data: &VaultData) -> Vec<(PaymentMethod, Vec<Uuid>)> {
    type Entry = Vec<(Uuid, Option<DateTime<Utc>>)>;
    let mut map: HashMap<PaymentMethod, Entry> = HashMap::new();
    for a in data.accounts.iter().filter(|a| {
        !matches!(
            a.status,
            AccountStatus::Deleted | AccountStatus::NotAnAccount
        )
    }) {
        let mut per_method: HashMap<&PaymentMethod, Option<DateTime<Utc>>> = HashMap::new();
        for r in &a.payments {
            if let Some(m) = &r.method {
                let e = per_method.entry(m).or_insert(None);
                if r.date > *e {
                    *e = r.date;
                }
            }
        }
        for (m, last) in per_method {
            map.entry(m.clone()).or_default().push((a.id, last));
        }
    }
    let mut out: Vec<(PaymentMethod, Entry)> = map.into_iter().collect();
    for (_, accounts) in &mut out {
        accounts.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
    }
    out.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then_with(|| a.0.label_de().cmp(&b.0.label_de()))
    });
    out.into_iter()
        .map(|(m, v)| (m, v.into_iter().map(|(id, _)| id).collect()))
        .collect()
}

/// Accounts that likely have a payment method stored (any payment record, or receipt
/// evidence), even if the method itself is unknown.
///
/// Deleted / not-an-account entries are excluded; order follows `data.accounts`.
pub fn accounts_with_payment_data(data: &VaultData) -> Vec<Uuid> {
    data.accounts
        .iter()
        .filter(|a| {
            !matches!(
                a.status,
                AccountStatus::Deleted | AccountStatus::NotAnAccount
            )
        })
        .filter(|a| {
            !a.payments.is_empty()
                || a.evidence
                    .iter()
                    .any(|e| matches!(e.kind, EvidenceKind::Receipt | EvidenceKind::Payment))
        })
        .map(|a| a.id)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RiskReason {
    PaymentDataStored,
    ActiveSubscription,
    ServiceBreached,
    IdentifierInBreach,
    PasswordPwned,
    PasswordReused,
    No2faButSupported,
    InactiveForYears,
    PhoneLogin,
    /// Identifier on a domain the user doesn't control any more (old employer, expired domain).
    ForeignRecoveryAddress,
    SsoLinked,
}

impl RiskReason {
    pub fn label_de(self) -> &'static str {
        match self {
            RiskReason::PaymentDataStored => "Zahlungsdaten hinterlegt",
            RiskReason::ActiveSubscription => "Aktives Abo",
            RiskReason::ServiceBreached => "Dienst hatte ein Datenleck",
            RiskReason::IdentifierInBreach => "Deine Adresse ist in einem Datenleck",
            RiskReason::PasswordPwned => "Passwort ist geleakt",
            RiskReason::PasswordReused => "Passwort mehrfach verwendet",
            RiskReason::No2faButSupported => "2FA verfügbar, aber nicht aktiv",
            RiskReason::InactiveForYears => "Seit Jahren nicht genutzt",
            RiskReason::PhoneLogin => "Login per Telefonnummer",
            RiskReason::ForeignRecoveryAddress => "Login-Adresse nicht mehr unter deiner Kontrolle",
            RiskReason::SsoLinked => "Mit SSO verknüpft",
        }
    }

    /// Points a reason adds to the 0..=100 risk score (the score is the capped sum).
    ///
    /// | reason | points |
    /// |---|---|
    /// | PasswordPwned | 30 |
    /// | IdentifierInBreach | 25 |
    /// | PaymentDataStored | 20 |
    /// | PasswordReused | 20 |
    /// | ActiveSubscription | 15 |
    /// | ServiceBreached | 15 |
    /// | ForeignRecoveryAddress | 15 |
    /// | No2faButSupported | 12 |
    /// | InactiveForYears | 10 |
    /// | PhoneLogin | 10 |
    /// | SsoLinked | 5 |
    pub fn weight(self) -> u32 {
        match self {
            RiskReason::PasswordPwned => 30,
            RiskReason::IdentifierInBreach => 25,
            RiskReason::PaymentDataStored => 20,
            RiskReason::PasswordReused => 20,
            RiskReason::ActiveSubscription => 15,
            RiskReason::ServiceBreached => 15,
            RiskReason::ForeignRecoveryAddress => 15,
            RiskReason::No2faButSupported => 12,
            RiskReason::InactiveForYears => 10,
            RiskReason::PhoneLogin => 10,
            RiskReason::SsoLinked => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub account_id: Uuid,
    /// 0 (harmless) ..= 100 (act now).
    pub score: u8,
    /// Most important first.
    pub reasons: Vec<RiskReason>,
    /// One recommended next step in German, e.g. "Erst Abo kündigen, dann löschen".
    pub next_step_de: String,
}

/// Context the risk model needs beyond the account itself.
#[derive(Debug, Clone, Default)]
pub struct RiskContext {
    /// Domains whose addresses the user controls (own mailboxes, own domains).
    pub own_mail_domains: Vec<String>,
    /// Domains (canonical) known to support 2FA (from the catalog / 2fa.directory).
    pub supports_2fa: Vec<String>,
}

/// Domains of e-mail relay / alias services. An address on one of these (or a subdomain
/// of it) is an alias, not a main address. Apple "Hide My Email" addresses on
/// `icloud.com` look like normal iCloud addresses and cannot be told apart.
const RELAY_DOMAINS: &[&str] = &[
    "privaterelay.appleid.com",
    "simplelogin.com",
    "simplelogin.co",
    "simplelogin.io",
    "aleeas.com",
    "slmail.me",
    "anonaddy.com",
    "anonaddy.me",
    "addy.io",
    "mozmail.com",
    "duck.com",
];

/// True for the domain part of a relay / alias address (see [`RELAY_DOMAINS`]).
pub fn is_relay_domain(domain: &str) -> bool {
    let d = domain.trim().trim_end_matches('.').to_lowercase();
    RELAY_DOMAINS
        .iter()
        .any(|r| d == *r || d.ends_with(&format!(".{r}")))
}

/// Domain part of an e-mail address (lowercase), None if it isn't one.
fn mail_domain(identifier: &str) -> Option<String> {
    let (local, dom) = identifier.trim().rsplit_once('@')?;
    let dom = dom.trim().trim_end_matches('.').to_lowercase();
    (!local.is_empty() && dom.contains('.')).then_some(dom)
}

/// Heuristic: does this login look like a phone number? Accepts E.164 (`+491701234567`)
/// and national notation (`0170 1234567`, `(030) 123-456`); 7..=15 digits, must start
/// with `+` or `0`, no letters, no `@`.
pub fn looks_like_phone(identifier: &str) -> bool {
    let s = identifier.trim().trim_start_matches('(');
    if !(s.starts_with('+') || s.starts_with('0')) {
        return false;
    }
    let body = s.strip_prefix('+').unwrap_or(s);
    if !body
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '/' | '(' | ')' | '.'))
    {
        return false;
    }
    let digits = body.chars().filter(char::is_ascii_digit).count();
    (7..=15).contains(&digits)
}

fn owns_domain(ctx: &RiskContext, domain: &str) -> bool {
    ctx.own_mail_domains.iter().any(|o| {
        let o = o.trim().trim_start_matches('@').to_lowercase();
        !o.is_empty() && (domain == o || domain.ends_with(&format!(".{o}")))
    })
}

fn is_free_mail(domain: &str) -> bool {
    is_infrastructure_domain(domain)
        || registrable_domain(domain).is_some_and(|r| is_infrastructure_domain(&r))
}

fn has_foreign_recovery_address(a: &Account, ctx: &RiskContext) -> bool {
    a.identifiers
        .iter()
        .filter_map(|i| mail_domain(i))
        .any(|d| !owns_domain(ctx, &d) && !is_free_mail(&d) && !is_relay_domain(&d))
}

fn has_2fa(a: &Account) -> bool {
    a.security.has_2fa == Some(true) || a.evidence.iter().any(|e| e.kind == EvidenceKind::TwoFactor)
}

/// Risk of one account. Accounts with status Deleted / NotAnAccount / Ignored score 0.
///
/// The score is the sum of [`RiskReason::weight`] over all reasons, capped at 100.
/// Reasons are ordered by weight (descending). Reason rules:
/// - `PaymentDataStored`: any payment record other than cancellation / refund
/// - `ActiveSubscription`: [`billing_summary`] says so
/// - `ServiceBreached`: a breach record that is not confirmed for the user;
///   `IdentifierInBreach`: a breach confirmed for the user
/// - `PasswordPwned` / `PasswordReused`: from `security`
/// - `No2faButSupported`: domain in `ctx.supports_2fa` and no 2FA seen
/// - `InactiveForYears`: last evidence older than two years
/// - `PhoneLogin`: `security.phone_login` or a phone-number identifier
/// - `ForeignRecoveryAddress`: an e-mail identifier whose domain is not the user's own,
///   not a free-mail provider and not a relay
/// - `SsoLinked`: at least one SSO provider
pub fn assess_risk(account: &Account, ctx: &RiskContext, now: DateTime<Utc>) -> RiskAssessment {
    let mut out = RiskAssessment {
        account_id: account.id,
        score: 0,
        reasons: Vec::new(),
        next_step_de: "Nichts zu tun".into(),
    };
    if !is_open(account) {
        return out;
    }
    let billing = billing_summary(account, now);
    let sec = &account.security;
    let mut reasons: Vec<RiskReason> = Vec::new();

    if account
        .payments
        .iter()
        .any(|p| !matches!(p.kind, PaymentKind::Cancelled | PaymentKind::Refund))
    {
        reasons.push(RiskReason::PaymentDataStored);
    }
    if billing.as_ref().is_some_and(|b| b.active_subscription) {
        reasons.push(RiskReason::ActiveSubscription);
    }
    if sec.breaches.iter().any(|b| !b.confirmed_for_user) {
        reasons.push(RiskReason::ServiceBreached);
    }
    if sec.breaches.iter().any(|b| b.confirmed_for_user) {
        reasons.push(RiskReason::IdentifierInBreach);
    }
    if sec.password_pwned == Some(true) {
        reasons.push(RiskReason::PasswordPwned);
    }
    if sec.password_reuse_count.is_some_and(|n| n > 0) {
        reasons.push(RiskReason::PasswordReused);
    }
    let domain = account.domain.to_lowercase();
    if ctx
        .supports_2fa
        .iter()
        .any(|d| d.trim().eq_ignore_ascii_case(&domain))
        && !has_2fa(account)
    {
        reasons.push(RiskReason::No2faButSupported);
    }
    let idle_days = account.last_seen.map(|l| (now - l).num_days());
    let inactive_years = idle_days.filter(|d| *d >= 730).map(|d| d / 365);
    if inactive_years.is_some() {
        reasons.push(RiskReason::InactiveForYears);
    }
    if sec.phone_login || account.identifiers.iter().any(|i| looks_like_phone(i)) {
        reasons.push(RiskReason::PhoneLogin);
    }
    if has_foreign_recovery_address(account, ctx) {
        reasons.push(RiskReason::ForeignRecoveryAddress);
    }
    if !account.sso.is_empty() {
        reasons.push(RiskReason::SsoLinked);
    }

    // Stable sort keeps the declaration order for equal weights.
    reasons.sort_by_key(|r| std::cmp::Reverse(r.weight()));
    let total: u32 = reasons.iter().map(|r| r.weight()).sum();
    out.score = total.min(100) as u8;

    let has = |r: RiskReason| reasons.contains(&r);
    out.next_step_de = if account.status == AccountStatus::DeletionPending {
        "Löschung läuft – Antwort abwarten".into()
    } else if has(RiskReason::PasswordPwned) {
        "Passwort ändern und 2FA aktivieren".into()
    } else if has(RiskReason::ActiveSubscription) {
        "Erst Abo kündigen, dann Account löschen".into()
    } else if has(RiskReason::IdentifierInBreach)
        || has(RiskReason::PasswordReused)
        || has(RiskReason::ServiceBreached)
    {
        if has_2fa(account) {
            "Passwort ändern".into()
        } else {
            "Passwort ändern und 2FA aktivieren".into()
        }
    } else if has(RiskReason::ForeignRecoveryAddress) {
        "Login-Adresse auf eine eigene Adresse umstellen".into()
    } else if let Some(y) = inactive_years {
        format!("Account löschen – seit {y} Jahren ungenutzt")
    } else if has(RiskReason::No2faButSupported) {
        "2FA aktivieren".into()
    } else if has(RiskReason::PhoneLogin) {
        "Login auf E-Mail-Adresse und Authenticator umstellen".into()
    } else if has(RiskReason::PaymentDataStored) {
        "Zahlungsdaten im Konto entfernen".into()
    } else if has(RiskReason::SsoLinked) {
        "SSO-Verknüpfung prüfen".into()
    } else {
        "Nichts zu tun".into()
    };
    out.reasons = reasons;
    out
}

/// All open accounts ranked by risk (highest first, ties by account name).
pub fn risk_ranking(
    data: &VaultData,
    ctx: &RiskContext,
    now: DateTime<Utc>,
) -> Vec<RiskAssessment> {
    let mut ranked: Vec<(RiskAssessment, String)> = data
        .accounts
        .iter()
        .filter(|a| is_open(a))
        .map(|a| (assess_risk(a, ctx, now), a.name.to_lowercase()))
        .collect();
    ranked.sort_by(|a, b| b.0.score.cmp(&a.0.score).then_with(|| a.1.cmp(&b.1)));
    ranked.into_iter().map(|(r, _)| r).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivacyScore {
    /// 0..=100, higher is better.
    pub score: u8,
    /// Breakdown with German labels: (label, points achieved, points possible).
    pub parts: Vec<(String, u32, u32)>,
    pub open_accounts: usize,
    pub deleted_accounts: usize,
    pub high_risk_accounts: usize,
    pub pending_requests: usize,
    pub overdue_requests: usize,
}

/// Risk score from which an account counts as "high risk".
const HIGH_RISK: u8 = 60;

/// Points for `share` (0..=1) of `max`, rounded.
fn pts(share: f64, max: u32) -> u32 {
    ((share.clamp(0.0, 1.0) * max as f64).round() as u32).min(max)
}

/// Share of `good` in `total`; a part without any data counts as fully achieved.
fn share(good: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        good as f64 / total as f64
    }
}

/// Overall privacy score: rewards deleted / reviewed accounts, 2FA, unique passwords,
/// unsubscribed marketing, answered requests; penalises high-risk accounts and overdue items.
///
/// Point system (sums to 100; a part without data is awarded in full):
/// - "Accounts aufgeräumt" 25: share of accounts (excl. NotAnAccount / Ignored) that are
///   no longer `Discovered` (confirmed, kept, being deleted or deleted)
/// - "Zwei-Faktor-Schutz" 15: open accounts on 2FA-capable domains that have 2FA
/// - "Einzigartige Passwörter" 15: audited open accounts with a unique, not leaked password
/// - "Wenige Risiko-Accounts" 15: `15 × (1 − high-risk / open)`, high risk = score ≥ 60
/// - "Werbung abgemeldet" 10: marketing / spam lists that are unsubscribed or kept on purpose
/// - "Anfragen beantwortet" 10: sent requests that were answered / done / rejected
/// - "Fristen eingehalten" 10: 10 minus 5 per overdue request
pub fn privacy_score(data: &VaultData, ctx: &RiskContext, now: DateTime<Utc>) -> PrivacyScore {
    let open: Vec<&Account> = data.accounts.iter().filter(|a| is_open(a)).collect();
    let deleted_accounts = data
        .accounts
        .iter()
        .filter(|a| a.status == AccountStatus::Deleted)
        .count();

    // Tidied up.
    let relevant = data
        .accounts
        .iter()
        .filter(|a| {
            !matches!(
                a.status,
                AccountStatus::NotAnAccount | AccountStatus::Ignored
            )
        })
        .count();
    let reviewed = data
        .accounts
        .iter()
        .filter(|a| {
            !matches!(
                a.status,
                AccountStatus::NotAnAccount | AccountStatus::Ignored
            )
        })
        .filter(|a| a.status != AccountStatus::Discovered)
        .count();

    // 2FA.
    let capable: HashSet<String> = ctx
        .supports_2fa
        .iter()
        .map(|d| d.trim().to_lowercase())
        .collect();
    let on_capable: Vec<&&Account> = open
        .iter()
        .filter(|a| capable.contains(&a.domain.to_lowercase()))
        .collect();
    let with_2fa = on_capable.iter().filter(|a| has_2fa(a)).count();

    // Passwords.
    let audited: Vec<&&Account> = open
        .iter()
        .filter(|a| {
            a.security.password_reuse_count.is_some() || a.security.password_pwned.is_some()
        })
        .collect();
    let clean = audited
        .iter()
        .filter(|a| {
            a.security.password_reuse_count.unwrap_or(0) == 0
                && a.security.password_pwned != Some(true)
        })
        .count();

    // Risk.
    let high_risk_accounts = open
        .iter()
        .filter(|a| assess_risk(a, ctx, now).score >= HIGH_RISK)
        .count();

    // Marketing.
    let marketing: Vec<_> = data
        .subscriptions
        .iter()
        .filter(|s| matches!(s.category, MailCategory::Marketing | MailCategory::Spam))
        .collect();
    let handled = marketing
        .iter()
        .filter(|s| {
            matches!(
                s.status,
                SubscriptionStatus::Unsubscribed | SubscriptionStatus::Keep
            )
        })
        .count();

    // Requests.
    let sent: Vec<_> = data
        .requests
        .iter()
        .filter(|r| r.status != RequestStatus::Draft)
        .collect();
    let answered = sent
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                RequestStatus::Answered | RequestStatus::Done | RequestStatus::Rejected
            )
        })
        .count();
    let pending_requests = sent
        .iter()
        .filter(|r| r.status == RequestStatus::Sent)
        .count();
    let overdue = overdue_requests(data, now).len();

    let high_share = if open.is_empty() {
        0.0
    } else {
        high_risk_accounts as f64 / open.len() as f64
    };
    let parts: Vec<(String, u32, u32)> = vec![
        (
            "Accounts aufgeräumt".into(),
            pts(share(reviewed, relevant), 25),
            25,
        ),
        (
            "Zwei-Faktor-Schutz".into(),
            pts(share(with_2fa, on_capable.len()), 15),
            15,
        ),
        (
            "Einzigartige Passwörter".into(),
            pts(share(clean, audited.len()), 15),
            15,
        ),
        (
            "Wenige Risiko-Accounts".into(),
            pts(1.0 - high_share, 15),
            15,
        ),
        (
            "Werbung abgemeldet".into(),
            pts(share(handled, marketing.len()), 10),
            10,
        ),
        (
            "Anfragen beantwortet".into(),
            pts(share(answered, sent.len()), 10),
            10,
        ),
        (
            "Fristen eingehalten".into(),
            10u32.saturating_sub(5 * overdue as u32),
            10,
        ),
    ];
    let total: u32 = parts.iter().map(|p| p.1).sum();

    PrivacyScore {
        score: total.min(100) as u8,
        parts,
        open_accounts: open.len(),
        deleted_accounts,
        high_risk_accounts,
        pending_requests,
        overdue_requests: overdue,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasAdvice {
    pub account_id: Uuid,
    /// The main address currently used at this service.
    pub current_identifier: String,
    /// German reason, e.g. "Behaltener Account nutzt deine Hauptadresse".
    pub reason_de: String,
}

/// Kept / confirmed accounts that still use a main mailbox address (not an alias /
/// relay): candidates for switching to an alias (Apple Hide My Email, SimpleLogin,
/// addy.io, Firefox Relay). Relay/alias domains are recognised and skipped.
///
/// An account qualifies when one of its identifiers equals the address of a configured
/// mailbox (case-insensitive) and none of its identifiers is on a relay domain.
pub fn alias_recommendations(data: &VaultData) -> Vec<AliasAdvice> {
    let mailboxes: HashSet<String> = data
        .mailboxes
        .iter()
        .map(|m| m.address.trim().to_lowercase())
        .filter(|a| !a.is_empty())
        .collect();
    if mailboxes.is_empty() {
        return Vec::new();
    }
    data.accounts
        .iter()
        .filter(|a| matches!(a.status, AccountStatus::Keep | AccountStatus::Confirmed))
        .filter(|a| {
            !a.identifiers
                .iter()
                .any(|i| mail_domain(i).is_some_and(|d| is_relay_domain(&d)))
        })
        .filter_map(|a| {
            let main = a
                .identifiers
                .iter()
                .find(|i| mailboxes.contains(&i.trim().to_lowercase()))?;
            let who = if a.status == AccountStatus::Keep {
                "Behaltener"
            } else {
                "Bestätigter"
            };
            Some(AliasAdvice {
                account_id: a.id,
                current_identifier: main.clone(),
                reason_de: format!("{who} Account nutzt deine Hauptadresse"),
            })
        })
        .collect()
}

/// Requests whose deadline has passed and that are still `Sent`, oldest deadline first.
pub fn overdue_requests(data: &VaultData, now: DateTime<Utc>) -> Vec<Uuid> {
    let mut v: Vec<(DateTime<Utc>, Uuid)> = data
        .requests
        .iter()
        .filter(|r| r.status == RequestStatus::Sent)
        .filter_map(|r| r.deadline.filter(|d| *d < now).map(|d| (d, r.id)))
        .collect();
    v.sort();
    v.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        BreachRecord, DataRequest, EvidenceKind, MailboxConfig, RequestChannel, RequestKind,
        SourceKind, SsoProvider, Subscription,
    };
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()
    }

    /// `d` days before `now()`.
    fn ago(d: i64) -> DateTime<Utc> {
        now() - Duration::days(d)
    }

    fn rec(kind: PaymentKind, days_ago: i64, amount: Option<i64>) -> PaymentRecord {
        PaymentRecord {
            date: Some(ago(days_ago)),
            kind,
            amount_minor: amount,
            currency: amount.map(|_| "EUR".to_string()),
            plan: None,
            method: None,
            via: None,
            summary: String::new(),
            source_ref: None,
        }
    }

    fn acct(domain: &str, payments: Vec<PaymentRecord>) -> Account {
        let mut a = Account::new(domain, domain);
        a.payments = payments;
        a.status = AccountStatus::Confirmed;
        a
    }

    fn monthly_series(n: i64, amount: i64) -> Vec<PaymentRecord> {
        (0..n)
            .map(|i| rec(PaymentKind::Charge, 10 + 30 * i, Some(amount)))
            .collect()
    }

    fn evidence(kind: EvidenceKind, days_ago: i64) -> crate::model::Evidence {
        crate::model::Evidence {
            kind,
            source: SourceKind::Manual,
            summary: String::new(),
            date: Some(ago(days_ago)),
            source_ref: None,
        }
    }

    fn data_with(accounts: Vec<Account>) -> VaultData {
        VaultData {
            accounts,
            ..Default::default()
        }
    }

    #[test]
    fn no_payments_no_summary() {
        assert!(billing_summary(&Account::new("a.com", "A"), now()).is_none());
    }

    #[test]
    fn monthly_interval_detected() {
        let a = acct("spotify.com", monthly_series(4, 999));
        let b = billing_summary(&a, now()).unwrap();
        assert_eq!(b.interval, Some(BillingInterval::Monthly));
        assert!(b.active_subscription);
        assert_eq!(b.amount_minor, Some(999));
        assert_eq!(b.monthly_minor, Some(999));
        assert_eq!(b.last_charge, Some(ago(10)));
        assert_eq!(b.next_charge_estimate, Some(ago(10) + Duration::days(30)));
    }

    #[test]
    fn tolerance_accepts_25_to_36_day_gaps() {
        let p = vec![
            rec(PaymentKind::Charge, 5, Some(500)),
            rec(PaymentKind::Charge, 5 + 35, Some(500)),
            rec(PaymentKind::Charge, 5 + 35 + 26, Some(500)),
        ];
        let b = billing_summary(&acct("x.com", p), now()).unwrap();
        assert_eq!(b.interval, Some(BillingInterval::Monthly));
    }

    #[test]
    fn yearly_interval_and_normalisation() {
        let p = vec![
            rec(PaymentKind::Charge, 100, Some(12_000)),
            rec(PaymentKind::Charge, 100 + 365, Some(12_000)),
        ];
        let b = billing_summary(&acct("adobe.com", p), now()).unwrap();
        assert_eq!(b.interval, Some(BillingInterval::Yearly));
        assert_eq!(b.monthly_minor, Some(1000));
        assert!(b.active_subscription);
    }

    #[test]
    fn quarterly_and_weekly_and_half_yearly() {
        let q = vec![
            rec(PaymentKind::Charge, 20, Some(3000)),
            rec(PaymentKind::Charge, 20 + 91, Some(3000)),
        ];
        assert_eq!(
            billing_summary(&acct("q.com", q), now()).unwrap().interval,
            Some(BillingInterval::Quarterly)
        );
        let w = vec![
            rec(PaymentKind::Charge, 1, Some(100)),
            rec(PaymentKind::Charge, 8, Some(100)),
            rec(PaymentKind::Charge, 15, Some(100)),
        ];
        let b = billing_summary(&acct("w.com", w), now()).unwrap();
        assert_eq!(b.interval, Some(BillingInterval::Weekly));
        assert_eq!(b.monthly_minor, Some(433));
        let h = vec![
            rec(PaymentKind::Charge, 30, Some(6000)),
            rec(PaymentKind::Charge, 30 + 182, Some(6000)),
        ];
        let b = billing_summary(&acct("h.com", h), now()).unwrap();
        assert_eq!(b.interval, Some(BillingInterval::HalfYearly));
        assert_eq!(b.monthly_minor, Some(1000));
    }

    #[test]
    fn irregular_charges_are_not_a_subscription() {
        let p = vec![
            rec(PaymentKind::Charge, 10, Some(2000)),
            rec(PaymentKind::Charge, 60, Some(900)),
            rec(PaymentKind::Charge, 260, Some(4500)),
        ];
        let b = billing_summary(&acct("shop.com", p), now()).unwrap();
        assert_eq!(b.interval, None);
        assert!(!b.active_subscription);
        assert_eq!(b.monthly_minor, None);
        assert_eq!(b.next_charge_estimate, None);
    }

    #[test]
    fn single_charge_is_not_recurring() {
        let b = billing_summary(
            &acct("shop.com", vec![rec(PaymentKind::Charge, 3, Some(2000))]),
            now(),
        )
        .unwrap();
        assert!(!b.active_subscription);
        assert_eq!(b.amount_minor, Some(2000));
    }

    #[test]
    fn single_renewal_with_period_in_plan_is_recurring() {
        let mut r = rec(PaymentKind::Renewal, 12, Some(1299));
        r.plan = Some("Premium Monatsabo".into());
        let b = billing_summary(&acct("netflix.com", vec![r]), now()).unwrap();
        assert!(b.active_subscription);
        assert_eq!(b.interval, Some(BillingInterval::Monthly));
        assert_eq!(b.plan.as_deref(), Some("Premium Monatsabo"));
        assert_eq!(b.next_charge_estimate, Some(ago(12) + Duration::days(30)));
    }

    #[test]
    fn single_renewal_with_plan_but_no_period() {
        let mut r = rec(PaymentKind::Renewal, 12, Some(1299));
        r.plan = Some("Premium".into());
        let b = billing_summary(&acct("x.com", vec![r]), now()).unwrap();
        assert!(b.active_subscription);
        assert_eq!(b.interval, None);
        assert_eq!(b.next_charge_estimate, None);
    }

    #[test]
    fn single_renewal_without_plan_or_period_is_not_enough() {
        let b = billing_summary(
            &acct("x.com", vec![rec(PaymentKind::Renewal, 12, Some(1299))]),
            now(),
        )
        .unwrap();
        assert!(!b.active_subscription);
    }

    #[test]
    fn cancellation_after_last_charge_deactivates() {
        let mut p = monthly_series(3, 799);
        p.push(rec(PaymentKind::Cancelled, 4, None));
        let b = billing_summary(&acct("dazn.com", p), now()).unwrap();
        assert!(!b.active_subscription);
        assert_eq!(b.cancelled_at, Some(ago(4)));
        assert_eq!(b.next_charge_estimate, None);
        assert_eq!(b.interval, Some(BillingInterval::Monthly));
    }

    #[test]
    fn cancellation_before_resubscribe_is_ignored() {
        let mut p = monthly_series(3, 799);
        p.push(rec(PaymentKind::Cancelled, 200, None));
        let b = billing_summary(&acct("dazn.com", p), now()).unwrap();
        assert!(b.active_subscription);
        assert_eq!(b.cancelled_at, None);
    }

    #[test]
    fn lapsed_subscription_becomes_inactive() {
        // last charge 100 days ago, monthly: next was 70 days ago (> 1 interval in the past)
        let p = vec![
            rec(PaymentKind::Charge, 100, Some(500)),
            rec(PaymentKind::Charge, 130, Some(500)),
        ];
        let b = billing_summary(&acct("old.com", p), now()).unwrap();
        assert!(!b.active_subscription);
        assert_eq!(b.next_charge_estimate, None);
        assert_eq!(b.interval, Some(BillingInterval::Monthly));
    }

    #[test]
    fn slightly_overdue_stays_active() {
        // last charge 40 days ago -> estimate 10 days ago, still within one interval
        let p = vec![
            rec(PaymentKind::Charge, 40, Some(500)),
            rec(PaymentKind::Charge, 70, Some(500)),
        ];
        let b = billing_summary(&acct("x.com", p), now()).unwrap();
        assert!(b.active_subscription);
        assert_eq!(b.next_charge_estimate, Some(ago(10)));
    }

    #[test]
    fn price_increase_from_amounts() {
        let p = vec![
            rec(PaymentKind::Charge, 10, Some(1299)),
            rec(PaymentKind::Charge, 40, Some(999)),
            rec(PaymentKind::Charge, 70, Some(999)),
        ];
        let b = billing_summary(&acct("netflix.com", p), now()).unwrap();
        assert!(b.price_increased);
        assert_eq!(b.amount_minor, Some(1299));
    }

    #[test]
    fn no_price_increase_when_equal_or_other_currency() {
        let mut usd = rec(PaymentKind::Charge, 10, Some(1500));
        usd.currency = Some("USD".into());
        let p = vec![
            usd,
            rec(PaymentKind::Charge, 40, Some(999)),
            rec(PaymentKind::Charge, 70, Some(999)),
        ];
        let b = billing_summary(&acct("x.com", p), now()).unwrap();
        // latest is USD with no earlier USD record
        assert!(!b.price_increased);
        let p = monthly_series(3, 999);
        assert!(
            !billing_summary(&acct("y.com", p), now())
                .unwrap()
                .price_increased
        );
    }

    #[test]
    fn price_change_record_after_last_charge() {
        let mut p = monthly_series(3, 999);
        p.push(rec(PaymentKind::PriceChange, 2, None));
        assert!(
            billing_summary(&acct("x.com", p), now())
                .unwrap()
                .price_increased
        );
        // before the last charge: not new any more
        let mut p = monthly_series(3, 999);
        p.push(rec(PaymentKind::PriceChange, 200, None));
        assert!(
            !billing_summary(&acct("x.com", p), now())
                .unwrap()
                .price_increased
        );
    }

    #[test]
    fn methods_via_and_dedup() {
        let mut a = rec(PaymentKind::Charge, 10, Some(500));
        a.method = Some(PaymentMethod::PayPal);
        a.via = Some("paypal.com".into());
        let mut b = rec(PaymentKind::Charge, 40, Some(500));
        b.method = Some(PaymentMethod::Card {
            brand: "Visa".into(),
            last4: "1234".into(),
        });
        let mut c = rec(PaymentKind::Charge, 70, Some(500));
        c.method = Some(PaymentMethod::PayPal);
        let s = billing_summary(&acct("x.com", vec![c, b, a]), now()).unwrap();
        assert_eq!(s.methods.len(), 2);
        assert_eq!(s.methods[0], PaymentMethod::PayPal);
        assert_eq!(s.via.as_deref(), Some("paypal.com"));
    }

    #[test]
    fn possibly_unused_depends_on_recent_usage_evidence() {
        let mut a = acct("x.com", monthly_series(3, 500));
        assert!(billing_summary(&a, now()).unwrap().possibly_unused);
        a.evidence.push(evidence(EvidenceKind::LoginAlert, 30));
        assert!(!billing_summary(&a, now()).unwrap().possibly_unused);
        let mut b = acct("y.com", monthly_series(3, 500));
        b.evidence.push(evidence(EvidenceKind::Welcome, 400));
        b.evidence.push(evidence(EvidenceKind::Receipt, 5));
        assert!(billing_summary(&b, now()).unwrap().possibly_unused);
    }

    #[test]
    fn spent_last_12m_per_currency_with_refunds() {
        let mut usd = rec(PaymentKind::Charge, 20, Some(2000));
        usd.currency = Some("USD".into());
        let mut refund = rec(PaymentKind::Refund, 15, Some(300));
        refund.amount_minor = Some(-300);
        let old = rec(PaymentKind::Charge, 400, Some(9999));
        let p = vec![
            rec(PaymentKind::Charge, 10, Some(1000)),
            rec(PaymentKind::Renewal, 100, Some(1000)),
            refund,
            usd,
            old,
            rec(PaymentKind::PaymentFailed, 5, Some(777)),
        ];
        let b = billing_summary(&acct("x.com", p), now()).unwrap();
        assert_eq!(
            b.spent_last_12m,
            vec![("EUR".to_string(), 1700), ("USD".to_string(), 2000)]
        );
    }

    #[test]
    fn same_day_duplicates_do_not_break_interval() {
        let mut p = monthly_series(3, 999);
        p.push(rec(PaymentKind::Charge, 10, Some(999)));
        let b = billing_summary(&acct("x.com", p), now()).unwrap();
        assert_eq!(b.interval, Some(BillingInterval::Monthly));
    }

    #[test]
    fn totals_mixed_currencies_and_exclusions() {
        let eur_m = acct("a.com", monthly_series(3, 1000));
        let eur_y = acct(
            "b.com",
            vec![
                rec(PaymentKind::Charge, 50, Some(12_000)),
                rec(PaymentKind::Charge, 415, Some(12_000)),
            ],
        );
        let mut usd = monthly_series(3, 500);
        for r in &mut usd {
            r.currency = Some("USD".into());
        }
        let usd = acct("c.com", usd);
        let mut deleted = acct("d.com", monthly_series(3, 5000));
        deleted.status = AccountStatus::Deleted;
        let mut cancelled_p = monthly_series(3, 700);
        cancelled_p.push(rec(PaymentKind::Cancelled, 1, None));
        let cancelled = acct("e.com", cancelled_p);
        let oneoff = acct("f.com", vec![rec(PaymentKind::Charge, 3, Some(900))]);
        let data = data_with(vec![eur_m, eur_y, usd, deleted, cancelled, oneoff]);
        let t = subscription_totals(&data, now());
        assert_eq!(
            t,
            vec![
                ("EUR".to_string(), 2000, 24_000, 2),
                ("USD".to_string(), 500, 6000, 1)
            ]
        );
    }

    #[test]
    fn payment_method_index_dedups_and_sorts() {
        let visa = PaymentMethod::Card {
            brand: "Visa".into(),
            last4: "1234".into(),
        };
        let mk = |domain: &str, m: &PaymentMethod, days: i64| {
            let mut r1 = rec(PaymentKind::Charge, days, Some(100));
            r1.method = Some(m.clone());
            let mut r2 = rec(PaymentKind::Charge, days + 30, Some(100));
            r2.method = Some(m.clone());
            acct(domain, vec![r1, r2])
        };
        let a = mk("a.com", &visa, 50);
        let b = mk("b.com", &visa, 5);
        let c = mk("c.com", &PaymentMethod::PayPal, 1);
        let mut gone = mk("gone.com", &visa, 1);
        gone.status = AccountStatus::Deleted;
        let (ia, ib) = (a.id, b.id);
        let data = data_with(vec![a, b, c, gone]);
        let idx = payment_method_index(&data);
        assert_eq!(idx.len(), 2);
        assert_eq!(idx[0].0, visa);
        assert_eq!(
            idx[0].1,
            vec![ib, ia],
            "newest use first, each account once"
        );
        assert_eq!(idx[1].0, PaymentMethod::PayPal);
    }

    #[test]
    fn accounts_with_payment_data_covers_receipts() {
        let paid = acct("a.com", vec![rec(PaymentKind::Charge, 5, None)]);
        let mut receipt = Account::new("b.com", "B");
        receipt.evidence.push(evidence(EvidenceKind::Receipt, 5));
        let plain = Account::new("c.com", "C");
        let mut deleted = acct("d.com", vec![rec(PaymentKind::Charge, 5, None)]);
        deleted.status = AccountStatus::Deleted;
        let (ia, ib) = (paid.id, receipt.id);
        let data = data_with(vec![paid, receipt, plain, deleted]);
        assert_eq!(accounts_with_payment_data(&data), vec![ia, ib]);
    }

    fn open_account(domain: &str) -> Account {
        let mut a = Account::new(domain, domain);
        a.status = AccountStatus::Confirmed;
        a.last_seen = Some(ago(10));
        a
    }

    fn reasons(a: &Account, ctx: &RiskContext) -> Vec<RiskReason> {
        assess_risk(a, ctx, now()).reasons
    }

    #[test]
    fn harmless_account_scores_zero() {
        let r = assess_risk(&open_account("a.com"), &RiskContext::default(), now());
        assert_eq!(r.score, 0);
        assert!(r.reasons.is_empty());
        assert_eq!(r.next_step_de, "Nichts zu tun");
    }

    #[test]
    fn closed_statuses_score_zero() {
        for st in [
            AccountStatus::Deleted,
            AccountStatus::NotAnAccount,
            AccountStatus::Ignored,
        ] {
            let mut a = open_account("a.com");
            a.status = st;
            a.security.password_pwned = Some(true);
            assert_eq!(assess_risk(&a, &RiskContext::default(), now()).score, 0);
        }
    }

    #[test]
    fn risk_payment_and_subscription() {
        let mut a = open_account("spotify.com");
        a.payments = monthly_series(3, 999);
        let r = assess_risk(&a, &RiskContext::default(), now());
        assert!(r.reasons.contains(&RiskReason::PaymentDataStored));
        assert!(r.reasons.contains(&RiskReason::ActiveSubscription));
        assert_eq!(r.score, 35);
        assert_eq!(r.next_step_de, "Erst Abo kündigen, dann Account löschen");
        // cancelled: payment data remains, subscription reason gone
        a.payments.push(rec(PaymentKind::Cancelled, 1, None));
        let rs = reasons(&a, &RiskContext::default());
        assert_eq!(rs, vec![RiskReason::PaymentDataStored]);
    }

    #[test]
    fn risk_breach_reasons() {
        let mut a = open_account("adobe.com");
        let mk = |confirmed| BreachRecord {
            name: format!("B{confirmed}"),
            title: "T".into(),
            breach_date: None,
            data_classes: vec![],
            confirmed_for_user: confirmed,
        };
        a.security.breaches = vec![mk(false)];
        assert_eq!(
            reasons(&a, &RiskContext::default()),
            vec![RiskReason::ServiceBreached]
        );
        a.security.breaches = vec![mk(true)];
        assert_eq!(
            reasons(&a, &RiskContext::default()),
            vec![RiskReason::IdentifierInBreach]
        );
        let r = assess_risk(&a, &RiskContext::default(), now());
        assert_eq!(r.next_step_de, "Passwort ändern und 2FA aktivieren");
        a.security.has_2fa = Some(true);
        assert_eq!(
            assess_risk(&a, &RiskContext::default(), now()).next_step_de,
            "Passwort ändern"
        );
    }

    #[test]
    fn risk_passwords() {
        let mut a = open_account("a.com");
        a.security.password_pwned = Some(true);
        a.security.password_reuse_count = Some(3);
        let r = assess_risk(&a, &RiskContext::default(), now());
        assert_eq!(
            r.reasons,
            vec![RiskReason::PasswordPwned, RiskReason::PasswordReused]
        );
        assert_eq!(r.score, 50);
        a.security.password_reuse_count = Some(0);
        a.security.password_pwned = Some(false);
        assert!(reasons(&a, &RiskContext::default()).is_empty());
    }

    #[test]
    fn risk_2fa_only_when_supported_and_missing() {
        let ctx = RiskContext {
            supports_2fa: vec!["github.com".into()],
            ..Default::default()
        };
        let mut a = open_account("github.com");
        assert_eq!(reasons(&a, &ctx), vec![RiskReason::No2faButSupported]);
        assert_eq!(assess_risk(&a, &ctx, now()).next_step_de, "2FA aktivieren");
        a.security.has_2fa = Some(true);
        assert!(reasons(&a, &ctx).is_empty());
        a.security.has_2fa = None;
        a.evidence.push(evidence(EvidenceKind::TwoFactor, 20));
        assert!(reasons(&a, &ctx).is_empty());
        assert!(reasons(&open_account("other.com"), &ctx).is_empty());
    }

    #[test]
    fn risk_inactive_for_years_with_next_step() {
        let mut a = open_account("old.com");
        a.last_seen = Some(ago(4 * 365 + 20));
        let r = assess_risk(&a, &RiskContext::default(), now());
        assert_eq!(r.reasons, vec![RiskReason::InactiveForYears]);
        assert_eq!(r.next_step_de, "Account löschen – seit 4 Jahren ungenutzt");
        a.last_seen = Some(ago(400));
        assert!(reasons(&a, &RiskContext::default()).is_empty());
    }

    #[test]
    fn risk_phone_login() {
        let mut a = open_account("signal.org");
        a.identifiers = vec!["+491701234567".into()];
        assert_eq!(
            reasons(&a, &RiskContext::default()),
            vec![RiskReason::PhoneLogin]
        );
        a.identifiers = vec!["0170 1234567".into()];
        assert_eq!(
            reasons(&a, &RiskContext::default()),
            vec![RiskReason::PhoneLogin]
        );
        a.identifiers = vec!["max@gmail.com".into()];
        assert!(reasons(&a, &RiskContext::default()).is_empty());
        a.security.phone_login = true;
        assert_eq!(
            reasons(&a, &RiskContext::default()),
            vec![RiskReason::PhoneLogin]
        );
    }

    #[test]
    fn phone_detection() {
        assert!(looks_like_phone("+49 170 1234567"));
        assert!(looks_like_phone("030/123456"));
        assert!(looks_like_phone("(030) 123-456"));
        assert!(!looks_like_phone("12345678"));
        assert!(!looks_like_phone("max@example.com"));
        assert!(!looks_like_phone("+49"));
        assert!(!looks_like_phone("0abc1234567"));
        assert!(!looks_like_phone("+1234567890123456"));
    }

    #[test]
    fn risk_foreign_recovery_address() {
        let ctx = RiskContext {
            own_mail_domains: vec!["meinedomain.de".into()],
            ..Default::default()
        };
        let mut a = open_account("x.com");
        a.identifiers = vec!["max@firma-alt.de".into()];
        let r = assess_risk(&a, &ctx, now());
        assert_eq!(r.reasons, vec![RiskReason::ForeignRecoveryAddress]);
        assert_eq!(
            r.next_step_de,
            "Login-Adresse auf eine eigene Adresse umstellen"
        );
        for ok in [
            "max@meinedomain.de",
            "max@mail.meinedomain.de",
            "max@gmail.com",
            "max@gmx.de",
            "abc123@privaterelay.appleid.com",
            "foo@simplelogin.com",
            "x.y@mozmail.com",
            "alias@duck.com",
            "a@user.anonaddy.me",
            "just-a-username",
        ] {
            a.identifiers = vec![ok.into()];
            assert!(reasons(&a, &ctx).is_empty(), "{ok}");
        }
    }

    #[test]
    fn risk_sso_and_ordering_and_cap() {
        let mut a = open_account("x.com");
        a.sso = vec![SsoProvider::Google];
        assert_eq!(assess_risk(&a, &RiskContext::default(), now()).score, 5);
        a.security.password_pwned = Some(true);
        a.security.password_reuse_count = Some(2);
        a.payments = monthly_series(3, 100);
        a.security.phone_login = true;
        a.security.breaches = vec![BreachRecord {
            name: "N".into(),
            title: "T".into(),
            breach_date: None,
            data_classes: vec![],
            confirmed_for_user: true,
        }];
        let r = assess_risk(&a, &RiskContext::default(), now());
        assert_eq!(r.score, 100, "capped");
        assert_eq!(r.reasons[0], RiskReason::PasswordPwned);
        let w: Vec<u32> = r.reasons.iter().map(|x| x.weight()).collect();
        assert!(w.windows(2).all(|p| p[0] >= p[1]));
        assert_eq!(*r.reasons.last().unwrap(), RiskReason::SsoLinked);
    }

    #[test]
    fn deletion_pending_next_step() {
        let mut a = open_account("x.com");
        a.status = AccountStatus::DeletionPending;
        a.security.password_pwned = Some(true);
        let r = assess_risk(&a, &RiskContext::default(), now());
        assert_eq!(r.next_step_de, "Löschung läuft – Antwort abwarten");
        assert!(r.score > 0);
    }

    #[test]
    fn ranking_order_and_exclusions() {
        let mut low = open_account("low.com");
        low.name = "Low".into();
        let mut high = open_account("high.com");
        high.name = "High".into();
        high.security.password_pwned = Some(true);
        let mut mid = open_account("mid.com");
        mid.name = "Mid".into();
        mid.sso = vec![SsoProvider::Apple];
        mid.security.phone_login = true;
        let mut deleted = open_account("del.com");
        deleted.status = AccountStatus::Deleted;
        deleted.security.password_pwned = Some(true);
        let mut zero2 = open_account("a-zero.com");
        zero2.name = "A-Zero".into();
        let data = data_with(vec![
            low.clone(),
            high.clone(),
            mid.clone(),
            deleted,
            zero2.clone(),
        ]);
        let rank = risk_ranking(&data, &RiskContext::default(), now());
        let ids: Vec<Uuid> = rank.iter().map(|r| r.account_id).collect();
        assert_eq!(
            ids,
            vec![high.id, mid.id, zero2.id, low.id],
            "ties sorted by name"
        );
    }

    #[test]
    fn overdue_requests_only_sent_and_past() {
        let mk = |status, deadline: Option<DateTime<Utc>>| DataRequest {
            id: Uuid::new_v4(),
            kind: RequestKind::Erasure,
            target: "X".into(),
            domain: None,
            account_id: None,
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: String::new(),
            subject: String::new(),
            body: String::new(),
            created_at: ago(60),
            sent_at: Some(ago(50)),
            deadline,
            status,
            notes: String::new(),
        };
        let late2 = mk(RequestStatus::Sent, Some(ago(2)));
        let late9 = mk(RequestStatus::Sent, Some(ago(9)));
        let data = VaultData {
            requests: vec![
                late2.clone(),
                mk(RequestStatus::Sent, Some(now() + Duration::days(3))),
                mk(RequestStatus::Answered, Some(ago(5))),
                mk(RequestStatus::Draft, Some(ago(5))),
                mk(RequestStatus::Sent, None),
                late9.clone(),
            ],
            ..Default::default()
        };
        assert_eq!(overdue_requests(&data, now()), vec![late9.id, late2.id]);
    }

    fn mailbox(address: &str) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::new_v4(),
            address: address.into(),
            imap_host: String::new(),
            imap_port: 993,
            username: String::new(),
            password: String::new(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    #[test]
    fn alias_advice_for_main_address_only() {
        let mut keep = open_account("a.com");
        keep.status = AccountStatus::Keep;
        keep.identifiers = vec!["Max@Example.com".into()];
        let mut confirmed = open_account("b.com");
        confirmed.identifiers = vec!["max@example.com".into(), "x".into()];
        let mut relay = open_account("c.com");
        relay.identifiers = vec![
            "max@example.com".into(),
            "abc@privaterelay.appleid.com".into(),
        ];
        let mut discovered = open_account("d.com");
        discovered.status = AccountStatus::Discovered;
        discovered.identifiers = vec!["max@example.com".into()];
        let mut other = open_account("e.com");
        other.identifiers = vec!["someone@else.org".into()];
        let (ik, ic) = (keep.id, confirmed.id);
        let data = VaultData {
            accounts: vec![keep, confirmed, relay, discovered, other],
            mailboxes: vec![mailbox("max@example.com")],
            ..Default::default()
        };
        let adv = alias_recommendations(&data);
        assert_eq!(adv.len(), 2);
        assert_eq!(adv[0].account_id, ik);
        assert_eq!(adv[0].current_identifier, "Max@Example.com");
        assert!(adv[0].reason_de.starts_with("Behaltener Account"));
        assert_eq!(adv[1].account_id, ic);
        assert!(adv[1].reason_de.contains("Hauptadresse"));
        // no mailboxes -> no advice
        let mut d2 = data.clone();
        d2.mailboxes.clear();
        assert!(alias_recommendations(&d2).is_empty());
    }

    fn sub(category: MailCategory, status: SubscriptionStatus) -> Subscription {
        Subscription {
            id: Uuid::new_v4(),
            mailbox: "m@x.de".into(),
            list_key: Uuid::new_v4().to_string(),
            domain: "x.com".into(),
            name: "X".into(),
            sender: "n@x.com".into(),
            unsubscribe: vec![],
            message_count: 1,
            frequency_per_month: 0.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category,
            category_source: Default::default(),
            status,
            history: vec![],
            post_unsubscribe_mails: vec![],
        }
    }

    #[test]
    fn privacy_score_bounds_and_parts() {
        let empty = privacy_score(&VaultData::default(), &RiskContext::default(), now());
        assert_eq!(empty.score, 100);
        assert_eq!(empty.parts.iter().map(|p| p.2).sum::<u32>(), 100);
        assert!(empty.parts.iter().all(|p| p.1 <= p.2));

        // Worst case: everything wrong.
        let mut accounts = Vec::new();
        for i in 0..5 {
            let mut a = Account::new(format!("a{i}.com"), format!("A{i}"));
            a.security.password_pwned = Some(true);
            a.security.password_reuse_count = Some(3);
            a.payments = monthly_series(3, 100);
            a.sso = vec![SsoProvider::Google];
            accounts.push(a);
        }
        let data = VaultData {
            accounts,
            subscriptions: vec![sub(MailCategory::Marketing, SubscriptionStatus::Active)],
            ..Default::default()
        };
        let ctx = RiskContext {
            supports_2fa: (0..5).map(|i| format!("a{i}.com")).collect(),
            ..Default::default()
        };
        let s = privacy_score(&data, &ctx, now());
        // only the parts without data (requests, deadlines) are awarded in full
        assert_eq!(s.score, 20);
        assert_eq!(s.high_risk_accounts, 5);
        assert_eq!(s.open_accounts, 5);
    }

    #[test]
    fn privacy_score_rewards_good_behaviour() {
        let mut good = open_account("good.com");
        good.security.has_2fa = Some(true);
        good.security.password_reuse_count = Some(0);
        good.security.password_pwned = Some(false);
        let mut gone = open_account("gone.com");
        gone.status = AccountStatus::Deleted;
        let mut discovered = Account::new("new.com", "New");
        discovered.last_seen = Some(ago(1));
        let data = VaultData {
            accounts: vec![good, gone, discovered],
            subscriptions: vec![
                sub(MailCategory::Marketing, SubscriptionStatus::Unsubscribed),
                sub(MailCategory::Spam, SubscriptionStatus::Active),
                sub(MailCategory::Transactional, SubscriptionStatus::Active),
            ],
            ..Default::default()
        };
        let ctx = RiskContext {
            supports_2fa: vec!["good.com".into()],
            ..Default::default()
        };
        let s = privacy_score(&data, &ctx, now());
        assert_eq!(s.deleted_accounts, 1);
        assert_eq!(s.open_accounts, 2);
        let part = |name: &str| s.parts.iter().find(|p| p.0 == name).unwrap().1;
        assert_eq!(part("Accounts aufgeräumt"), 17); // 2 of 3 reviewed
        assert_eq!(part("Zwei-Faktor-Schutz"), 15);
        assert_eq!(part("Einzigartige Passwörter"), 15);
        assert_eq!(part("Werbung abgemeldet"), 5);
        assert!(s.score <= 100);
    }

    #[test]
    fn privacy_score_penalises_overdue_requests() {
        let mk = |status, deadline| DataRequest {
            id: Uuid::new_v4(),
            kind: RequestKind::Erasure,
            target: "X".into(),
            domain: None,
            account_id: None,
            subscription_id: None,
            parent_id: None,
            channel: RequestChannel::Email,
            recipient: String::new(),
            subject: String::new(),
            body: String::new(),
            created_at: ago(60),
            sent_at: Some(ago(50)),
            deadline,
            status,
            notes: String::new(),
        };
        let data = VaultData {
            requests: vec![
                mk(RequestStatus::Sent, Some(ago(3))),
                mk(RequestStatus::Sent, Some(ago(4))),
                mk(RequestStatus::Done, None),
            ],
            ..Default::default()
        };
        let s = privacy_score(&data, &RiskContext::default(), now());
        assert_eq!(s.overdue_requests, 2);
        assert_eq!(s.pending_requests, 2);
        let part = |name: &str| s.parts.iter().find(|p| p.0 == name).unwrap().1;
        assert_eq!(part("Fristen eingehalten"), 0);
        assert_eq!(part("Anfragen beantwortet"), 3);
    }

    #[test]
    fn relay_domains() {
        assert!(is_relay_domain("privaterelay.appleid.com"));
        assert!(is_relay_domain("Foo.AnonAddy.me"));
        assert!(is_relay_domain("addy.io"));
        assert!(!is_relay_domain("notaddy.io"));
        assert!(!is_relay_domain("gmail.com"));
    }
}
