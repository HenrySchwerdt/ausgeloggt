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
//! HTTP via `ureq` 3 + rustls (ring), timeouts, a descriptive User-Agent "ausgeloggt".
//! Results are cached in memory for the run; nothing is logged except counts.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use ausgeloggt_core::BreachRecord;
use serde::Deserialize;

const USER_AGENT: &str = "ausgeloggt";
const RANGE_URL: &str = "https://api.pwnedpasswords.com/range/";
const CATALOGUE_URL: &str = "https://haveibeenpwned.com/api/v3/breaches";
const ACCOUNT_URL: &str = "https://haveibeenpwned.com/api/v3/breachedaccount/";
const MAX_BODY: u64 = 64 * 1024 * 1024;
const MAX_RETRIES: u32 = 2;
/// Upper bound for one Retry-After sleep (seconds).
const MAX_RETRY_WAIT: u64 = 60;

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
/// `ausgeloggt_import::PwnedChecker` semantics (the app glues the trait).
pub struct PwnedPasswords {
    agent: ureq::Agent,
    cache: Mutex<HashMap<String, Vec<(String, u64)>>>,
}

impl PwnedPasswords {
    pub fn new() -> Self {
        Self {
            agent: agent(),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// How often this SHA-1 appears in Pwned Passwords (0 = not found).
    pub fn count(&self, sha1: &[u8; 20]) -> Result<u64, BreachError> {
        let (prefix, suffix) = split_sha1(sha1);
        if let Some(entries) = self.cache.lock().ok().and_then(|c| c.get(&prefix).cloned()) {
            return Ok(lookup(&entries, &suffix));
        }
        let url = format!("{RANGE_URL}{prefix}");
        let mut resp = self
            .agent
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Add-Padding", "true")
            .call()
            .map_err(net_err)?;
        match resp.status().as_u16() {
            200 => {}
            429 => return Err(BreachError::RateLimited),
            s => return Err(BreachError::Network(format!("HTTP {s}"))),
        }
        let body = read_body(&mut resp)?;
        let entries = parse_range_entries(&body);
        let n = lookup(&entries, &suffix);
        if let Ok(mut c) = self.cache.lock() {
            c.insert(prefix, entries);
        }
        Ok(n)
    }
}

impl Default for PwnedPasswords {
    fn default() -> Self {
        Self::new()
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn net_err(e: ureq::Error) -> BreachError {
    BreachError::Network(e.to_string())
}

fn read_body(resp: &mut ureq::http::Response<ureq::Body>) -> Result<String, BreachError> {
    resp.body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_string()
        .map_err(net_err)
}

/// Uppercase hex split of a SHA-1: 5-char prefix (sent) and 35-char suffix (local only).
pub fn split_sha1(sha1: &[u8; 20]) -> (String, String) {
    use std::fmt::Write;
    let mut hex = String::with_capacity(40);
    for b in sha1 {
        let _ = write!(hex, "{b:02X}");
    }
    let suffix = hex.split_off(5);
    (hex, suffix)
}

fn lookup(entries: &[(String, u64)], suffix: &str) -> u64 {
    entries
        .iter()
        .find(|(s, _)| s == suffix)
        .map(|(_, n)| *n)
        .unwrap_or(0)
}

fn parse_range_entries(body: &str) -> Vec<(String, u64)> {
    body.lines()
        .filter_map(|l| {
            let (s, n) = l.trim().split_once(':')?;
            Some((s.trim().to_ascii_uppercase(), n.trim().parse().ok()?))
        })
        .collect()
}

/// Download the public breach catalogue (no key needed).
pub fn fetch_breach_catalogue() -> Result<Vec<CatalogBreach>, BreachError> {
    let mut resp = agent()
        .get(CATALOGUE_URL)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(net_err)?;
    match resp.status().as_u16() {
        200 => {}
        429 => return Err(BreachError::RateLimited),
        s => return Err(BreachError::Network(format!("HTTP {s}"))),
    }
    let body = read_body(&mut resp)?;
    let list = parse_breaches(&body)?;
    log::debug!("{} Einträge im Breach-Katalog", list.len());
    Ok(list)
}

/// Breaches an address appears in (needs API key). Sleeps/retries on 429 per Retry-After (max 2x).
pub fn breaches_for_address(
    api_key: &str,
    address: &str,
) -> Result<Vec<BreachRecord>, BreachError> {
    let url = format!(
        "{ACCOUNT_URL}{}?truncateResponse=false",
        percent_encode(address.trim())
    );
    let agent = agent();
    let mut attempt = 0;
    loop {
        let mut resp = agent
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .header("hibp-api-key", api_key)
            .call()
            .map_err(net_err)?;
        match resp.status().as_u16() {
            200 => {
                let body = read_body(&mut resp)?;
                let list = parse_breaches_as(&body, true)?;
                log::debug!("{} Datenlecks für eine Adresse", list.len());
                return Ok(list.into_iter().map(|b| b.record).collect());
            }
            404 => return Ok(Vec::new()),
            401 => return Err(BreachError::InvalidKey),
            429 => {
                if attempt >= MAX_RETRIES {
                    return Err(BreachError::RateLimited);
                }
                attempt += 1;
                let wait = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .unwrap_or(2)
                    .clamp(1, MAX_RETRY_WAIT);
                std::thread::sleep(Duration::from_secs(wait));
            }
            s => return Err(BreachError::Network(format!("HTTP {s}"))),
        }
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawBreach {
    name: String,
    title: String,
    domain: String,
    breach_date: Option<String>,
    data_classes: Vec<String>,
    is_verified: bool,
    is_sensitive: bool,
}

fn normalise_domain(d: &str) -> String {
    let d = d.trim().to_lowercase();
    d.strip_prefix("www.").unwrap_or(&d).to_string()
}

/// Parse the HIBP breach JSON array (pure; used by both endpoints, unit-tested).
/// Records are marked `confirmed_for_user = false`.
pub fn parse_breaches(json: &str) -> Result<Vec<CatalogBreach>, BreachError> {
    parse_breaches_as(json, false)
}

fn parse_breaches_as(json: &str, confirmed: bool) -> Result<Vec<CatalogBreach>, BreachError> {
    let raw: Vec<RawBreach> =
        serde_json::from_str(json).map_err(|e| BreachError::Invalid(e.to_string()))?;
    Ok(raw
        .into_iter()
        .map(|r| CatalogBreach {
            domain: normalise_domain(&r.domain),
            record: BreachRecord {
                title: if r.title.is_empty() {
                    r.name.clone()
                } else {
                    r.title
                },
                name: r.name,
                breach_date: r.breach_date.filter(|d| !d.is_empty()),
                data_classes: r.data_classes,
                confirmed_for_user: confirmed,
            },
            is_verified: r.is_verified,
            is_sensitive: r.is_sensitive,
        })
        .collect())
}

/// Parse a range API response body (`SUFFIX:COUNT` lines, padding entries with count 0)
/// and return the count for `suffix` (35 uppercase hex chars).
pub fn parse_range_response(body: &str, suffix: &str) -> u64 {
    lookup(&parse_range_entries(body), &suffix.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWORD_SHA1: [u8; 20] = [
        0x5B, 0xAA, 0x61, 0xE4, 0xC9, 0xB9, 0x3F, 0x3F, 0x06, 0x82, 0x25, 0x0B, 0x6C, 0xF8, 0x33,
        0x1B, 0x7E, 0xE6, 0x8F, 0xD8,
    ];

    const FIXTURE: &str = r#"[
      {"Name":"Adobe","Title":"Adobe","Domain":"adobe.com","BreachDate":"2013-10-04","AddedDate":"2013-12-04T00:00:00Z","ModifiedDate":"2022-05-15T23:52:49Z","PwnCount":152445165,"Description":"x","LogoPath":"x.png","DataClasses":["Email addresses","Password hints","Passwords","Usernames"],"IsVerified":true,"IsFabricated":false,"IsSensitive":false,"IsRetired":false,"IsSpamList":false,"IsMalware":false},
      {"Name":"Example","Title":"Example Forum","Domain":"WWW.Forum.Example.org","BreachDate":"2020-01-02","DataClasses":["Email addresses"],"IsVerified":false,"IsSensitive":true},
      {"Name":"NoDomain","Title":"Nothing","Domain":"","BreachDate":"","DataClasses":[]}
    ]"#;

    #[test]
    fn parses_catalogue() {
        let l = parse_breaches(FIXTURE).unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].domain, "adobe.com");
        assert_eq!(l[0].record.name, "Adobe");
        assert_eq!(l[0].record.breach_date.as_deref(), Some("2013-10-04"));
        assert_eq!(l[0].record.data_classes.len(), 4);
        assert!(l[0].is_verified && !l[0].is_sensitive);
        assert!(!l[0].record.confirmed_for_user);
        assert_eq!(l[1].domain, "forum.example.org");
        assert!(l[1].is_sensitive && !l[1].is_verified);
        assert_eq!(l[2].domain, "");
        assert_eq!(l[2].record.breach_date, None);
    }

    #[test]
    fn account_lookup_is_confirmed() {
        let l = parse_breaches_as(FIXTURE, true).unwrap();
        assert!(l.iter().all(|b| b.record.confirmed_for_user));
    }

    #[test]
    fn invalid_json() {
        assert!(matches!(
            parse_breaches("<html>"),
            Err(BreachError::Invalid(_))
        ));
        assert!(parse_breaches("[]").unwrap().is_empty());
    }

    #[test]
    fn sha1_split() {
        let (p, s) = split_sha1(&PASSWORD_SHA1);
        assert_eq!(p, "5BAA6");
        assert_eq!(s, "1E4C9B93F3F0682250B6CF8331B7EE68FD8");
        assert_eq!(s.len(), 35);
    }

    #[test]
    fn range_parsing() {
        let body = "0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n1E4C9B93F3F0682250B6CF8331B7EE68FD8:10434004\r\n00D4F6E8FA6EECAD2A3AA415EEC418D38EC:2\r\nGARBAGE\r\n";
        assert_eq!(
            parse_range_response(body, "1E4C9B93F3F0682250B6CF8331B7EE68FD8"),
            10_434_004
        );
        assert_eq!(
            parse_range_response(body, "0018A45C4D1DEF81644B54AB7F969B88D65"),
            0
        );
        assert_eq!(
            parse_range_response(body, "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"),
            0
        );
        assert_eq!(parse_range_response("", "AB"), 0);
    }

    #[test]
    fn encodes_address() {
        assert_eq!(percent_encode("a+b@x.de"), "a%2Bb%40x.de");
    }

    #[test]
    #[ignore = "live network"]
    fn live_password() {
        assert!(PwnedPasswords::new().count(&PASSWORD_SHA1).unwrap() > 1000);
    }
}
