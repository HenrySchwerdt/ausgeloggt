//! Derived insights computed from the vault (pure, no I/O): billing / subscriptions,
//! risk ranking, privacy score, payment-method index, alias recommendations,
//! takeover risks. Nothing here is stored; the UI recomputes on demand.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{Account, PaymentMethod, VaultData};

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

/// Compute the billing summary of one account (None if it has no payment records).
/// Interval detection: median gap between Charge/Renewal dates, tolerance ±20 %.
pub fn billing_summary(account: &Account, now: DateTime<Utc>) -> Option<BillingSummary> {
    let _ = (account, now);
    todo!("implemented by core agent")
}

/// Totals over all active subscriptions, per currency: (currency, monthly_minor, yearly_minor, count).
pub fn subscription_totals(data: &VaultData, now: DateTime<Utc>) -> Vec<(String, i64, i64, usize)> {
    let _ = (data, now);
    todo!("implemented by core agent")
}

/// Where each payment method is stored: method -> accounts (sorted by last use).
/// Powers "Karte •••• 1234 ist bei 14 Diensten hinterlegt" and the card-compromise playbook.
pub fn payment_method_index(data: &VaultData) -> Vec<(PaymentMethod, Vec<Uuid>)> {
    let _ = data;
    todo!("implemented by core agent")
}

/// Accounts that likely have a payment method stored (any payment record, or receipt
/// evidence), even if the method itself is unknown.
pub fn accounts_with_payment_data(data: &VaultData) -> Vec<Uuid> {
    let _ = data;
    todo!("implemented by core agent")
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

/// Risk of one account. Accounts with status Deleted / NotAnAccount / Ignored score 0.
pub fn assess_risk(account: &Account, ctx: &RiskContext, now: DateTime<Utc>) -> RiskAssessment {
    let _ = (account, ctx, now);
    todo!("implemented by core agent")
}

/// All open accounts ranked by risk (highest first).
pub fn risk_ranking(data: &VaultData, ctx: &RiskContext, now: DateTime<Utc>) -> Vec<RiskAssessment> {
    let _ = (data, ctx, now);
    todo!("implemented by core agent")
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

/// Overall privacy score: rewards deleted / reviewed accounts, 2FA, unique passwords,
/// unsubscribed marketing, answered requests; penalises high-risk accounts and overdue items.
pub fn privacy_score(data: &VaultData, ctx: &RiskContext, now: DateTime<Utc>) -> PrivacyScore {
    let _ = (data, ctx, now);
    todo!("implemented by core agent")
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
pub fn alias_recommendations(data: &VaultData) -> Vec<AliasAdvice> {
    let _ = data;
    todo!("implemented by core agent")
}

/// Requests whose deadline has passed and that are still `Sent`.
pub fn overdue_requests(data: &VaultData, now: DateTime<Utc>) -> Vec<Uuid> {
    let _ = (data, now);
    todo!("implemented by core agent")
}
