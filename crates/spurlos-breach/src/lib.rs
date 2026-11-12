//! Breach checks via Have I Been Pwned.
//!
//! - Pwned Passwords (free, no key): k-anonymity range API. Only the first 5 hex chars
//!   of the SHA-1 are sent (`GET https://api.pwnedpasswords.com/range/{prefix}` with
//!   `Add-Padding: true`); the suffix comparison happens locally.
//! - Public breach catalogue (free, no key): `GET https://haveibeenpwned.com/api/v3/breaches`
//!   -> domain-level "this service had a breach (date, data classes)".
//! - Per-address lookups (needs a paid HIBP API key, opt-in):
//!   `GET /api/v3/breachedaccount/{account}?truncateResponse=false` with `hibp-api-key`.
//!   Rate limits respected (429 + Retry-After).
//!
//! HTTP via `ureq` 3 + rustls (ring), timeouts, a descriptive User-Agent "Spurlos".
//! Results are cached in memory for the run; nothing is logged except counts.

use spurlos_core::BreachRecord;

#[derive(Debug, thiserror::Error)]
pub enum BreachError {
    #[error("Netzwerkfehler: {0}")]
    Network(String),
    #[error("HIBP-API-Key fehlt oder ist ungültig")]
    InvalidKey,
    #[error("Zu viele Anfragen, bitte später erneut versuchen")]
    RateLimited,
    #[error("Ungültige Antwort: {0}")]
    Invalid(String),
}

/// A breach from the public HIBP catalogue.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogBreach {
    /// Registrable domain of the breached service (may be empty upstream; skip those).
    pub domain: String,
    pub record: BreachRecord,
    pub is_verified: bool,
    pub is_sensitive: bool,
}

/// Pwned Passwords client with a per-prefix cache. Implements
/// `spurlos_import::PwnedChecker` semantics (the app glues the trait).
pub struct PwnedPasswords {
    // private: agent, cache prefix -> Vec<(suffix, count)>
}

impl PwnedPasswords {
    pub fn new() -> Self {
        todo!("implemented by breach agent")
    }

    /// How often this SHA-1 appears in Pwned Passwords (0 = not found).
    pub fn count(&self, sha1: &[u8; 20]) -> Result<u64, BreachError> {
        let _ = sha1;
        todo!("implemented by breach agent")
    }
}

impl Default for PwnedPasswords {
    fn default() -> Self {
        Self::new()
    }
}

/// Download the public breach catalogue (no key needed).
pub fn fetch_breach_catalogue() -> Result<Vec<CatalogBreach>, BreachError> {
    todo!("implemented by breach agent")
}

/// Breaches an address appears in (needs API key). Sleeps/retries on 429 per Retry-After (max 2x).
pub fn breaches_for_address(api_key: &str, address: &str) -> Result<Vec<BreachRecord>, BreachError> {
    let _ = (api_key, address);
    todo!("implemented by breach agent")
}

/// Parse the HIBP breach JSON array (pure; used by both endpoints, unit-tested).
pub fn parse_breaches(json: &str) -> Result<Vec<CatalogBreach>, BreachError> {
    let _ = json;
    todo!("implemented by breach agent")
}

/// Parse a range API response body (`SUFFIX:COUNT` lines, padding entries with count 0)
/// and return the count for `suffix` (35 uppercase hex chars).
pub fn parse_range_response(body: &str, suffix: &str) -> u64 {
    let _ = (body, suffix);
    todo!("implemented by breach agent")
}
