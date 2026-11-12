//! Curated reference data beyond per-service deletion info:
//! 2FA support (2fa.directory), credit bureaus / address brokers / opt-out lists,
//! German data-protection authorities, search-engine removal links, payment hubs.
//! All bundled (offline); sources + licenses in `data/ATTRIBUTION.md`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TwoFactorInfo {
    /// e.g. ["totp", "sms", "u2f", "email", "custom-software"] as in 2fa.directory.
    pub methods: Vec<String>,
    /// Link to the service's 2FA documentation.
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrgKind {
    /// Auskunftei (SCHUFA, Boniversum/Creditreform, CRIF, Experian, infoscore ...).
    CreditBureau,
    /// Adresshändler / list brokers (Deutsche Post Direkt, AZ Direct, Schober ...).
    AddressBroker,
    /// Opt-out list (Robinsonliste) — sign-up link only.
    OptOutList,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub legal_name: String,
    pub kind: OrgKind,
    pub address_lines: Vec<String>,
    pub email: Option<String>,
    /// Official online form for self-disclosure / objection, if any.
    pub web_form: Option<String>,
    /// German one-liner: what they hold and what to request.
    pub description_de: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Authority {
    /// ISO 3166-2 code (`DE-BY`) or `DE` for the federal BfDI.
    pub state: String,
    pub name: String,
    pub address_lines: Vec<String>,
    pub email: Option<String>,
    /// Online complaint form.
    pub complaint_form: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelpLink {
    pub title_de: String,
    pub url: String,
    pub description_de: String,
}

/// 2FA support for a canonical registrable domain (bundled 2fa.directory snapshot).
pub fn two_factor(domain: &str) -> Option<&'static TwoFactorInfo> {
    let _ = domain;
    todo!("implemented by catalog agent")
}

/// Domains known to support any 2FA method (for `insights::RiskContext`).
pub fn two_factor_domains() -> Vec<String> {
    todo!("implemented by catalog agent")
}

/// Curated credit bureaus, address brokers and opt-out lists (Germany first, verified
/// addresses with source links in ATTRIBUTION.md).
pub fn organizations() -> &'static [Organization] {
    todo!("implemented by catalog agent")
}

/// The 16 state authorities + BfDI with postal address and complaint form.
pub fn authorities() -> &'static [Authority] {
    todo!("implemented by catalog agent")
}

/// Competent authority for a state code (`DE-BY`), or by German postal code
/// (best-effort PLZ range table) when `state` is empty.
pub fn authority_for(state: &str, postal_code: &str) -> Option<&'static Authority> {
    let _ = (state, postal_code);
    todo!("implemented by catalog agent")
}

/// Search engines / people-search removal (Google "Ergebnisse über dich", Google
/// right-to-be-forgotten form, Bing, DuckDuckGo info, ...).
pub fn search_removal_links() -> &'static [HelpLink] {
    todo!("implemented by catalog agent")
}

/// Central places where payment methods / authorisations live: PayPal automatic
/// payments, Apple subscriptions + payment methods, Google Play subscriptions +
/// Google Pay methods, Amazon wallet, Klarna, Shop Pay, Stripe Link, Chrome / Safari
/// saved cards, Microsoft account billing.
pub fn payment_hubs() -> &'static [HelpLink] {
    todo!("implemented by catalog agent")
}

/// Alias services for the alias recommendations (Apple Hide My Email, SimpleLogin,
/// addy.io, Firefox Relay, DuckDuckGo Email Protection) and their relay domains.
pub fn alias_services() -> &'static [(HelpLink, &'static [&'static str])] {
    todo!("implemented by catalog agent")
}
