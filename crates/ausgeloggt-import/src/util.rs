//! Small shared helpers: text decoding, host extraction, hashing, timestamps.

use chrono::{DateTime, NaiveDateTime, Utc};
use sha2::{Digest, Sha256};

/// Decode file bytes to text: handles UTF-8 BOM, UTF-16 BOMs, falls back to lossy UTF-8.
#[allow(unknown_lints, clippy::chunks_exact_to_as_chunks)] // as_chunks needs Rust 1.88, MSRV is 1.85
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    let utf16 = |rest: &[u8], le: bool| -> String {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| {
                if le {
                    u16::from_le_bytes([c[0], c[1]])
                } else {
                    u16::from_be_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, false);
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// Extract a lowercase host name from a URL, bare domain or e-mail address.
/// Returns `None` for non-web schemes (`chrome://`, `android://`, `mailto:` ...),
/// IP addresses, `localhost` and anything that does not look like a DNS name.
pub(crate) fn host_of(raw: &str) -> Option<String> {
    let s = raw
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '<' | '>'));
    if s.is_empty() {
        return None;
    }
    let rest = if let Some(i) = s.find("://") {
        let scheme = s[..i].to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https" | "ftp" | "ftps") {
            return None;
        }
        &s[i + 3..]
    } else if let Some(r) = s.strip_prefix("//") {
        r
    } else {
        // `host:port` is fine, `mailto:x` / `javascript:x` are not.
        let head = s.split(['/', '?', '#']).next().unwrap_or("");
        if let Some((_, after)) = head.split_once(':') {
            if after.is_empty() || !after.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
        }
        s
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    if host_port.starts_with('[') {
        return None; // IPv6
    }
    let host = host_port.split(':').next().unwrap_or("");
    let host = host.trim_matches('.').to_lowercase();
    if host.is_empty() || host.len() > 253 || !host.contains('.') {
        return None;
    }
    if host.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None; // IPv4
    }
    let ok = host.split('.').all(|l| {
        !l.is_empty()
            && l.chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    });
    ok.then_some(host)
}

/// All http(s) hosts mentioned in free text (HTML, CSV, JSON ...), in order of appearance.
pub(crate) fn hosts_in_text(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for (i, _) in text.match_indices("://") {
        let mut start = i;
        while start > 0 && bytes[start - 1].is_ascii_alphabetic() {
            start -= 1;
        }
        let scheme = text[start..i].to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            continue;
        }
        let tail = &text[i + 3..];
        let end = tail
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | '\\' | ',')
            })
            .unwrap_or(tail.len());
        if let Some(h) = host_of(&format!("https://{}", &tail[..end])) {
            out.push(h);
        }
    }
    out
}

/// Stable, password-free row id: truncated SHA-256 over the NUL-joined parts.
pub(crate) fn source_ref(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0u8]);
    }
    let digest = h.finalize();
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Parse epoch seconds/milliseconds or common date-time strings.
pub(crate) fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        let n: i64 = s.parse().ok()?;
        if n <= 0 {
            return None;
        }
        return epoch(n);
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s.trim_end_matches('Z'), fmt) {
            return Some(d.and_utc());
        }
    }
    None
}

/// Seconds (< 1e11) or milliseconds since the epoch.
pub(crate) fn epoch(n: i64) -> Option<DateTime<Utc>> {
    if n < 100_000_000_000 {
        DateTime::from_timestamp(n, 0)
    } else {
        DateTime::from_timestamp_millis(n)
    }
}

/// Undo "UTF-8 bytes decoded as Latin-1" damage (Facebook exports).
pub(crate) fn fix_mojibake(s: &str) -> String {
    if s.is_ascii() || s.chars().any(|c| c as u32 > 0xFF) {
        return s.to_string();
    }
    let bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
    String::from_utf8(bytes).unwrap_or_else(|_| s.to_string())
}

/// Lowercase slug: alphanumerics kept, everything else collapsed to `-`.
pub(crate) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts() {
        assert_eq!(
            host_of("https://Login.Example.com:8443/a?b#c").as_deref(),
            Some("login.example.com")
        );
        assert_eq!(host_of("example.org/x").as_deref(), Some("example.org"));
        assert_eq!(host_of("me@example.net").as_deref(), Some("example.net"));
        assert_eq!(host_of("chrome://FirefoxAccounts"), None);
        assert_eq!(host_of("android://abc@com.app/"), None);
        assert_eq!(host_of("http://192.168.0.1/"), None);
        assert_eq!(host_of("localhost"), None);
        assert_eq!(host_of("mailto:a@b.de"), None);
    }

    #[test]
    fn mojibake() {
        assert_eq!(fix_mojibake("MÃ¼ller"), "Müller");
        assert_eq!(fix_mojibake("Müller"), "Müller");
        assert_eq!(fix_mojibake("plain"), "plain");
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse_ts("1700000000").unwrap().timestamp(), 1_700_000_000);
        assert_eq!(
            parse_ts("1700000000000").unwrap().timestamp(),
            1_700_000_000
        );
        assert!(parse_ts("2023-05-01T10:00:00Z").is_some());
        assert!(parse_ts("nonsense").is_none());
    }

    #[test]
    fn text_hosts() {
        let h = hosts_in_text(
            r#"<a href="https://a.example.com/x">x</a> and http://b.example.org, done"#,
        );
        assert_eq!(h, vec!["a.example.com", "b.example.org"]);
    }
}
