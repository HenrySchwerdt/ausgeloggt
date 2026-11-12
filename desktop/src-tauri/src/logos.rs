//! Logo fallback: favicon from the service's *own* domain, cached sealed with the vault key.
//!
//! Privacy rules (docs/ARCHITECTURE.md): never a third-party favicon service; only when
//! `settings.fetch_logos`; the cache is encrypted (`Vault::seal_bytes`, label
//! `logo:<domain>`) and cache file names are salted hashes, so neither content nor file
//! names reveal which services the user has accounts with. Misses are cached for 7 days.

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use ausgeloggt_core::vault::Vault;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::error::IpcResult;
use crate::state::{AppState, LockExt};

pub const MAX_LOGO_BYTES: usize = 256 * 1024;
const NEGATIVE_TTL_DAYS: i64 = 7;
const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REDIRECTS: usize = 3;
const MAX_CONCURRENT_FETCHES: usize = 4;
const MAX_CANDIDATES: usize = 4;

/// Result of fetching: `Ok(Some((mime, bytes)))` found, `Ok(None)` the site has no
/// usable logo (cached as a miss), `Err` transient problem (not cached).
pub type LogoFetcher = dyn Fn(&str) -> Result<Option<(String, Vec<u8>)>, String> + Send + Sync;

// ---- domain validation --------------------------------------------------------------

/// Normalise and validate a domain from the UI. Rejects IPs, single labels, internal
/// names and anything that is not plain ASCII hostname syntax.
pub fn clean_domain(input: &str) -> Option<String> {
    let d = input.trim().trim_end_matches('.').to_ascii_lowercase();
    if d.is_empty() || d.len() > 253 || d.starts_with("unresolved:") {
        return None;
    }
    let labels: Vec<&str> = d.split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    for l in &labels {
        if l.is_empty()
            || l.len() > 63
            || l.starts_with('-')
            || l.ends_with('-')
            || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return None;
        }
    }
    let tld = labels[labels.len() - 1];
    if tld.bytes().all(|b| b.is_ascii_digit()) {
        return None; // IPv4 literal
    }
    const INTERNAL: [&str; 7] = [
        "localhost",
        "local",
        "internal",
        "lan",
        "home",
        "intranet",
        "corp",
    ];
    if INTERNAL.contains(&tld) || d.ends_with(".home.arpa") {
        return None;
    }
    Some(d)
}

// ---- cache --------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum CacheState {
    Hit(String, Vec<u8>),
    /// Known miss, still fresh.
    Miss,
    /// Nothing usable cached (absent, expired, unreadable).
    Stale,
}

fn label(domain: &str) -> String {
    format!("logo:{domain}")
}

/// Per-vault random salt for cache file names, sealed with the vault key. A new salt
/// (and an emptied cache) is created if it cannot be opened, e.g. after a password change.
fn salt(vault: &Vault, dir: &Path) -> Vec<u8> {
    let file = dir.join("logos.salt");
    if let Ok(sealed) = std::fs::read(&file) {
        if let Ok(s) = vault.open_bytes("logo-salt", &sealed) {
            if s.len() == 16 {
                return s;
            }
        }
    }
    // Unreadable: the old logo files are orphaned, remove them.
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("logo-") && name.ends_with(".bin") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let s = uuid::Uuid::new_v4().as_bytes().to_vec();
    if let Ok(sealed) = vault.seal_bytes("logo-salt", &s) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(&file, sealed);
    }
    s
}

fn entry_path(vault: &Vault, domain: &str) -> PathBuf {
    let dir = vault.cache_dir();
    let mut h = Sha256::new();
    h.update(salt(vault, &dir));
    h.update(b"|");
    h.update(domain.as_bytes());
    let hex: String = h.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    dir.join(format!("logo-{hex}.bin"))
}

/// Plaintext layout: `tag (1 = hit, 0 = miss) | unix seconds (i64 LE) | [mime len | mime | bytes]`.
fn encode(entry: Option<(&str, &[u8])>, now: DateTime<Utc>) -> Vec<u8> {
    let mut v = Vec::new();
    v.push(u8::from(entry.is_some()));
    v.extend_from_slice(&now.timestamp().to_le_bytes());
    if let Some((mime, bytes)) = entry {
        v.push(mime.len() as u8);
        v.extend_from_slice(mime.as_bytes());
        v.extend_from_slice(bytes);
    }
    v
}

fn decode(plain: &[u8], now: DateTime<Utc>) -> CacheState {
    if plain.len() < 9 {
        return CacheState::Stale;
    }
    let ts = i64::from_le_bytes(plain[1..9].try_into().expect("8 bytes"));
    match plain[0] {
        0 => {
            if now.timestamp() - ts < NEGATIVE_TTL_DAYS * 86_400 {
                CacheState::Miss
            } else {
                CacheState::Stale
            }
        }
        1 if plain.len() > 10 => {
            let n = plain[9] as usize;
            match plain
                .get(10..10 + n)
                .and_then(|m| std::str::from_utf8(m).ok())
            {
                Some(mime) => CacheState::Hit(mime.to_string(), plain[10 + n..].to_vec()),
                None => CacheState::Stale,
            }
        }
        _ => CacheState::Stale,
    }
}

pub fn cache_lookup(vault: &Vault, domain: &str, now: DateTime<Utc>) -> CacheState {
    let path = entry_path(vault, domain);
    let Ok(sealed) = std::fs::read(&path) else {
        return CacheState::Stale;
    };
    match vault.open_bytes(&label(domain), &sealed) {
        Ok(plain) => decode(&plain, now),
        Err(_) => CacheState::Stale,
    }
}

pub fn cache_store(
    vault: &Vault,
    domain: &str,
    entry: Option<(&str, &[u8])>,
    now: DateTime<Utc>,
) -> IpcResult<()> {
    let path = entry_path(vault, domain);
    let sealed = vault.seal_bytes(&label(domain), &encode(entry, now))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, sealed)?;
    Ok(())
}

// ---- concurrency gate ---------------------------------------------------------------

struct Gate {
    used: Mutex<usize>,
    cv: Condvar,
}

static GATE: Gate = Gate {
    used: Mutex::new(0),
    cv: Condvar::new(),
};

struct Permit;

impl Permit {
    fn acquire() -> Permit {
        let mut used = GATE.used.lock_safe();
        while *used >= MAX_CONCURRENT_FETCHES {
            used = GATE.cv.wait(used).unwrap_or_else(|e| e.into_inner());
        }
        *used += 1;
        Permit
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        *GATE.used.lock_safe() -= 1;
        GATE.cv.notify_one();
    }
}

// ---- command logic ------------------------------------------------------------------

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        ausgeloggt_ai::vision::base64_encode(bytes)
    )
}

/// `logo_get`: data URL or `None`. Fetches only if `settings.fetch_logos`.
pub fn logo_get(
    state: &AppState,
    domain: &str,
    fetch: &LogoFetcher,
    now: DateTime<Utc>,
) -> IpcResult<Option<String>> {
    if !state.read(|d| d.settings.fetch_logos)? {
        return Ok(None);
    }
    let Some(domain) = clean_domain(domain) else {
        return Ok(None);
    };
    match state.with_vault(|v| cache_lookup(v, &domain, now))? {
        CacheState::Hit(mime, bytes) => return Ok(Some(data_url(&mime, &bytes))),
        CacheState::Miss => return Ok(None),
        CacheState::Stale => {}
    }
    let fetched = {
        let _permit = Permit::acquire();
        fetch(&domain)
    };
    match fetched {
        Ok(Some((mime, bytes))) if bytes.len() <= MAX_LOGO_BYTES => {
            let url = data_url(&mime, &bytes);
            // The vault may have been locked meanwhile; the logo is still returned.
            let _ = state.with_vault(|v| cache_store(v, &domain, Some((&mime, &bytes)), now));
            Ok(Some(url))
        }
        Ok(_) => {
            let _ = state.with_vault(|v| cache_store(v, &domain, None, now));
            Ok(None)
        }
        Err(e) => {
            log::debug!("Logo nicht geladen: {e}");
            Ok(None)
        }
    }
}

// ---- HTTP fetch ---------------------------------------------------------------------

/// Image type by magic bytes: png, ico, svg, webp only.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() > MAX_LOGO_BYTES || bytes.len() < 4 {
        return None;
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0, 0, 1, 0]) && bytes.len() > 22 {
        return Some("image/x-icon");
    }
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("<svg")
        || ((head.starts_with("<?xml") || head.starts_with("<!doctype svg"))
            && head.contains("<svg"))
    {
        let all = String::from_utf8_lossy(bytes).to_lowercase();
        if all.contains("<script") || all.contains("javascript:") || all.contains("<foreignobject")
        {
            return None;
        }
        return Some("image/svg+xml");
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconLink {
    pub href: String,
    pub rank: u8,
    pub size: u32,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(name) {
        let start = from + pos;
        let before_ok = start == 0
            || lower.as_bytes()[start - 1].is_ascii_whitespace()
            || lower.as_bytes()[start - 1] == b'"'
            || lower.as_bytes()[start - 1] == b'\'';
        let rest = tag[start + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let v = rest[1..].trim_start();
            let (quote, v) = match v.chars().next() {
                Some(q @ ('"' | '\'')) => (Some(q), &v[1..]),
                _ => (None, v),
            };
            let end = match quote {
                Some(q) => v.find(q)?,
                None => v
                    .find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(v.len()),
            };
            return Some(v[..end].trim().to_string());
        }
        from = start + name.len();
    }
    None
}

/// `<link rel="icon" | "shortcut icon" | "apple-touch-icon">` candidates, best first.
pub fn parse_icon_links(html: &str) -> Vec<IconLink> {
    let lower = html.to_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("<link") {
        let start = from + pos;
        let Some(len) = lower[start..].find('>') else {
            break;
        };
        let tag = &html[start..start + len];
        from = start + len;
        let (Some(rel), Some(href)) = (attr(tag, "rel"), attr(tag, "href")) else {
            continue;
        };
        let rel = rel.to_lowercase();
        if !rel.split_whitespace().any(|r| r.contains("icon")) || rel.contains("mask-icon") {
            continue;
        }
        let ty = attr(tag, "type").unwrap_or_default().to_lowercase();
        let rank = if rel.contains("apple-touch-icon") {
            0
        } else if ty.contains("svg") || ty.contains("png") || href.to_lowercase().ends_with(".png")
        {
            1
        } else {
            2
        };
        let size = attr(tag, "sizes")
            .and_then(|s| {
                s.split(['x', 'X'])
                    .next()
                    .and_then(|n| n.trim().parse().ok())
            })
            .unwrap_or(0);
        out.push(IconLink { href, rank, size });
    }
    out.sort_by(|a, b| a.rank.cmp(&b.rank).then(b.size.cmp(&a.size)));
    out
}

fn host_of(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    if port.is_some_and(|p| p != "443") || host.is_empty() {
        return None;
    }
    Some(host)
}

/// Is `url` an https URL on `domain` or one of its subdomains?
pub fn is_own_https(url: &str, domain: &str) -> bool {
    host_of(url)
        .map(|h| {
            let h = h.to_ascii_lowercase();
            h == domain || h.ends_with(&format!(".{domain}"))
        })
        .unwrap_or(false)
}

/// Resolve `href` against the page URL; `http://` is upgraded to `https://`.
pub fn resolve_href(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with("data:") || href.starts_with("javascript:") {
        return None;
    }
    if let Some(rest) = href.strip_prefix("http://") {
        return Some(format!("https://{rest}"));
    }
    if href.starts_with("https://") {
        return Some(href.to_string());
    }
    if let Some(rest) = href.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    // Rebuild the origin from the host (drops an explicit :443).
    let origin = format!("https://{}", host_of(base)?);
    if href.starts_with('/') {
        return Some(format!("{origin}{href}"));
    }
    let path = base
        .strip_prefix(&origin)
        .unwrap_or("")
        .split(['?', '#'])
        .next()
        .unwrap_or("");
    let dir = match path.rfind('/') {
        Some(i) => &path[..=i],
        None => "/",
    };
    Some(format!("{origin}{dir}{href}"))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .user_agent(concat!("ausgeloggt/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// GET with manual redirects that must stay on the service's own domain (https only).
fn get_bytes(
    agent: &ureq::Agent,
    url: &str,
    domain: &str,
    limit: u64,
) -> Result<(String, Vec<u8>), String> {
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        if !is_own_https(&current, domain) {
            return Err("fremder Host".into());
        }
        let mut resp = agent.get(&current).call().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        if (300..400).contains(&status) {
            let loc = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("Weiterleitung ohne Ziel")?
                .to_string();
            current = resolve_href(&current, &loc).ok_or("ungültige Weiterleitung")?;
            continue;
        }
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        return Ok((current, bytes));
    }
    Err("zu viele Weiterleitungen".into())
}

/// The real fetcher: page -> `<link rel=icon>` candidates -> `/favicon.ico`.
pub fn fetch_from_site(domain: &str) -> Result<Option<(String, Vec<u8>)>, String> {
    let agent = agent();
    let mut page: Option<(String, String)> = None;
    let mut network_error = None;
    for host in [domain.to_string(), format!("www.{domain}")] {
        match get_bytes(
            &agent,
            &format!("https://{host}/"),
            domain,
            MAX_LOGO_BYTES as u64,
        ) {
            Ok((final_url, bytes)) => {
                page = Some((final_url, String::from_utf8_lossy(&bytes).into_owned()));
                break;
            }
            Err(e) => network_error = Some(e),
        }
    }
    let mut candidates: Vec<String> = Vec::new();
    let origin = match &page {
        Some((base, html)) => {
            for l in parse_icon_links(html).into_iter().take(MAX_CANDIDATES) {
                if let Some(u) = resolve_href(base, &l.href) {
                    if is_own_https(&u, domain) && !candidates.contains(&u) {
                        candidates.push(u);
                    }
                }
            }
            host_of(base).map(|h| format!("https://{h}"))
        }
        None => Some(format!("https://{domain}")),
    };
    if let Some(o) = origin {
        candidates.push(format!("{o}/favicon.ico"));
    }
    let mut reached_site = page.is_some();
    for url in candidates {
        match get_bytes(&agent, &url, domain, MAX_LOGO_BYTES as u64) {
            Ok((_, bytes)) => {
                reached_site = true;
                if let Some(mime) = sniff(&bytes) {
                    return Ok(Some((mime.to_string(), bytes)));
                }
            }
            Err(e) => {
                if network_error.is_none() {
                    network_error = Some(e);
                }
            }
        }
    }
    if reached_site {
        Ok(None)
    } else {
        Err(network_error.unwrap_or_else(|| "nicht erreichbar".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::unlocked;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4, 5, 6, 7, 8,
    ];

    fn counting(
        calls: &'static AtomicUsize,
        result: Result<Option<(String, Vec<u8>)>, String>,
    ) -> Box<LogoFetcher> {
        Box::new(move |_d| {
            calls.fetch_add(1, Ordering::SeqCst);
            result.clone()
        })
    }

    #[test]
    fn domain_validation() {
        assert_eq!(
            clean_domain(" Spotify.COM. ").as_deref(),
            Some("spotify.com")
        );
        for bad in [
            "",
            "localhost",
            "intranet",
            "printer.local",
            "127.0.0.1",
            "10.0.0.1",
            "a b.com",
            "unresolved:notion",
            "exa_mple.com",
            "-x.com",
            "http://x.com",
            "x.com/path",
            "user@x.com",
            "nodots",
        ] {
            assert_eq!(clean_domain(bad), None, "{bad}");
        }
        assert!(clean_domain("xn--mnchen-3ya.de").is_some());
    }

    #[test]
    fn fetch_then_cache_hit_is_sealed_on_disk() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let fetch = counting(&CALLS, Ok(Some(("image/png".into(), PNG.to_vec()))));
        let now = Utc::now();
        let a = logo_get(&f.state, "spotify.com", &*fetch, now)
            .unwrap()
            .unwrap();
        assert!(a.starts_with("data:image/png;base64,"));
        let b = logo_get(&f.state, "Spotify.com", &*fetch, now)
            .unwrap()
            .unwrap();
        assert_eq!(a, b);
        assert_eq!(
            CALLS.load(Ordering::SeqCst),
            1,
            "second call served from cache"
        );

        // On disk: salted file names + ciphertext, no domain, no image bytes.
        let dir = f.state.cache_dir();
        let mut files = 0;
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            files += 1;
            let name = e.file_name().to_string_lossy().to_string();
            assert!(!name.contains("spotify"), "{name}");
            let raw = std::fs::read(e.path()).unwrap();
            assert!(!raw.windows(8).any(|w| w == &PNG[..8]));
            assert!(!raw.windows(7).any(|w| w == b"spotify"));
        }
        assert_eq!(files, 2, "salt + one logo");
    }

    #[test]
    fn misses_are_cached_for_seven_days() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let fetch = counting(&CALLS, Ok(None));
        let t0 = Utc::now();
        assert_eq!(
            logo_get(&f.state, "nologo.example", &*fetch, t0).unwrap(),
            None
        );
        assert_eq!(
            logo_get(
                &f.state,
                "nologo.example",
                &*fetch,
                t0 + chrono::Duration::days(6)
            )
            .unwrap(),
            None
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 1, "negative cache honoured");
        assert_eq!(
            logo_get(
                &f.state,
                "nologo.example",
                &*fetch,
                t0 + chrono::Duration::days(8)
            )
            .unwrap(),
            None
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 2, "refetched after expiry");
    }

    #[test]
    fn transient_errors_are_not_cached() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let fetch = counting(&CALLS, Err("offline".into()));
        let now = Utc::now();
        assert_eq!(logo_get(&f.state, "x.example", &*fetch, now).unwrap(), None);
        assert_eq!(logo_get(&f.state, "x.example", &*fetch, now).unwrap(), None);
        assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn nothing_is_fetched_when_disabled_or_invalid() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let fetch = counting(&CALLS, Ok(Some(("image/png".into(), PNG.to_vec()))));
        f.state.mutate(|d| d.settings.fetch_logos = false).unwrap();
        assert_eq!(
            logo_get(&f.state, "spotify.com", &*fetch, Utc::now()).unwrap(),
            None
        );
        f.state.mutate(|d| d.settings.fetch_logos = true).unwrap();
        assert_eq!(
            logo_get(&f.state, "127.0.0.1", &*fetch, Utc::now()).unwrap(),
            None
        );
        assert_eq!(
            logo_get(&f.state, "unresolved:notion", &*fetch, Utc::now()).unwrap(),
            None
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn oversized_result_is_treated_as_miss() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let big = vec![0u8; MAX_LOGO_BYTES + 1];
        let fetch = counting(&CALLS, Ok(Some(("image/png".into(), big))));
        assert_eq!(
            logo_get(&f.state, "big.example", &*fetch, Utc::now()).unwrap(),
            None
        );
        assert_eq!(
            logo_get(&f.state, "big.example", &*fetch, Utc::now()).unwrap(),
            None
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn locked_vault_is_an_error() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        f.state.lock();
        let fetch = counting(&CALLS, Ok(None));
        assert!(logo_get(&f.state, "x.example", &*fetch, Utc::now()).is_err());
    }

    #[test]
    fn password_change_orphans_cache_without_breaking_it() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let f = unlocked();
        let fetch = counting(&CALLS, Ok(Some(("image/png".into(), PNG.to_vec()))));
        let now = Utc::now();
        logo_get(&f.state, "spotify.com", &*fetch, now).unwrap();
        f.state.with_vault(|_| ()).unwrap();
        // simulate a new key: lock + reopen after change_password
        f.state.lock();
        {
            let mut v = Vault::open(&f.path(), "correct horse battery").unwrap();
            v.change_password("a completely new password").unwrap();
        }
        f.state.unlock("a completely new password", None).unwrap();
        assert!(logo_get(&f.state, "spotify.com", &*fetch, now)
            .unwrap()
            .is_some());
        assert_eq!(
            CALLS.load(Ordering::SeqCst),
            2,
            "old entry unreadable -> refetched"
        );
        // stale files were purged: salt + the new logo
        assert_eq!(std::fs::read_dir(f.state.cache_dir()).unwrap().count(), 2);
    }

    #[test]
    fn sniffing_accepts_only_whitelisted_image_types() {
        assert_eq!(sniff(PNG), Some("image/png"));
        let mut ico = vec![0, 0, 1, 0, 1, 0];
        ico.extend(vec![0u8; 30]);
        assert_eq!(sniff(&ico), Some("image/x-icon"));
        let mut webp = b"RIFF\x10\0\0\0WEBPVP8 ".to_vec();
        webp.extend(vec![0u8; 8]);
        assert_eq!(sniff(&webp), Some("image/webp"));
        assert_eq!(
            sniff(b"<svg xmlns='http://www.w3.org/2000/svg'><path/></svg>"),
            Some("image/svg+xml")
        );
        assert_eq!(
            sniff(b"<?xml version='1.0'?><svg></svg>"),
            Some("image/svg+xml")
        );
        assert_eq!(sniff(b"<svg><script>alert(1)</script></svg>"), None);
        assert_eq!(sniff(b"<html><body>no</body></html>"), None);
        assert_eq!(sniff(b"GIF89a....."), None);
        assert_eq!(sniff(&vec![0x89u8; MAX_LOGO_BYTES + 1]), None);
    }

    #[test]
    fn icon_links_are_ranked() {
        let html = r#"<html><head>
            <link rel="stylesheet" href="/s.css">
            <link rel="icon" href="/favicon.ico">
            <link rel='icon' type='image/png' sizes='32x32' href='/f32.png'>
            <link rel="apple-touch-icon" sizes="120x120" href="/a120.png">
            <link rel="apple-touch-icon" sizes="180x180" href="/a180.png">
            <link rel="mask-icon" href="/mask.svg">
            <LINK REL="shortcut icon" HREF="//static.example.com/x.ico">
        </head></html>"#;
        let links = parse_icon_links(html);
        let hrefs: Vec<&str> = links.iter().map(|l| l.href.as_str()).collect();
        assert_eq!(
            hrefs,
            vec![
                "/a180.png",
                "/a120.png",
                "/f32.png",
                "/favicon.ico",
                "//static.example.com/x.ico"
            ]
        );
    }

    #[test]
    fn href_resolution_and_host_checks() {
        let base = "https://www.example.com/de/index.html?x=1";
        assert_eq!(
            resolve_href(base, "/a.png").as_deref(),
            Some("https://www.example.com/a.png")
        );
        assert_eq!(
            resolve_href(base, "a.png").as_deref(),
            Some("https://www.example.com/de/a.png")
        );
        assert_eq!(
            resolve_href(base, "//cdn.example.com/a.png").as_deref(),
            Some("https://cdn.example.com/a.png")
        );
        assert_eq!(
            resolve_href(base, "http://example.com/a.png").as_deref(),
            Some("https://example.com/a.png")
        );
        assert_eq!(resolve_href(base, "data:image/png;base64,AAAA"), None);
        assert_eq!(
            resolve_href("https://example.com", "a.png").as_deref(),
            Some("https://example.com/a.png")
        );

        assert!(is_own_https("https://example.com/x", "example.com"));
        assert!(is_own_https("https://static.example.com/x", "example.com"));
        assert!(is_own_https("https://example.com:443/x", "example.com"));
        assert!(!is_own_https("https://evilexample.com/x", "example.com"));
        assert!(!is_own_https(
            "https://example.com.evil.net/x",
            "example.com"
        ));
        assert!(!is_own_https("http://example.com/x", "example.com"));
        assert!(!is_own_https("https://example.com:8443/x", "example.com"));
        assert!(!is_own_https("https://user@example.com/x", "example.com"));
        assert!(!is_own_https("https://127.0.0.1/x", "example.com"));
    }
}
