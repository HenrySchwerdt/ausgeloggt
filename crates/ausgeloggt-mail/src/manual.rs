//! Manual unsubscribe links: what the user can open in the browser when the automatic
//! unsubscribe failed or is not possible.
//!
//! Two sources: the stored `List-Unsubscribe` methods ("header") and, on explicit request
//! only, the body of the newest mail of the list ("mail"). The body is fetched over IMAP
//! (capped at 200 KB), searched for unsubscribe links and dropped. The links themselves
//! are never fetched here.

use std::sync::LazyLock;

use ausgeloggt_core::{MailboxConfig, Subscription, UnsubscribeMethod};
use regex::Regex;
use serde::Serialize;

use crate::imap::Client;
use crate::{connect_and_login, select_folders, MailError};

/// Body size cap for the mail source.
pub const MAX_BODY_BYTES: usize = 200 * 1024;
/// Maximum number of links returned from the mail body.
pub const MAX_MAIL_LINKS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkSource {
    Header,
    Mail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManualLink {
    pub url: String,
    pub source: LinkSource,
    pub label: String,
}

fn is_http(url: &str) -> bool {
    let u = url.trim();
    let lower = u.get(..8).unwrap_or(u).to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !u.chars().any(|c| c.is_control() || c.is_whitespace())
        && u.len() <= 2048
}

/// Links and mailto address from the stored `List-Unsubscribe` methods.
pub fn header_links(sub: &Subscription) -> (Vec<ManualLink>, Option<String>) {
    let mut links: Vec<ManualLink> = Vec::new();
    let mut mailto = None;
    for m in &sub.unsubscribe {
        match m {
            UnsubscribeMethod::OneClick { url } | UnsubscribeMethod::Link { url } => {
                if is_http(url) && !links.iter().any(|l| l.url == url.trim()) {
                    links.push(ManualLink {
                        url: url.trim().to_string(),
                        source: LinkSource::Header,
                        label: "Abmeldelink aus dem Kopf der Mail".into(),
                    });
                }
            }
            UnsubscribeMethod::Mailto { address, .. } => {
                let a = address.trim();
                if mailto.is_none()
                    && !a.is_empty()
                    && a.contains('@')
                    && !a.chars().any(|c| c.is_control() || c.is_whitespace())
                {
                    mailto = Some(a.to_string());
                }
            }
        }
    }
    links.sort_by_key(|l| !l.url.to_ascii_lowercase().starts_with("https://"));
    (links, mailto)
}

// ------------------------------------------------------------------------------------------
// Link extraction from a mail body
// ------------------------------------------------------------------------------------------

/// Words (lowercase) in the anchor text or the text before a bare URL.
const TEXT_WORDS: &[&str] = &[
    "abmelden",
    "abmeldung",
    "abbestellen",
    "austragen",
    "unsubscribe",
    "opt-out",
    "opt out",
    "optout",
    "désinscrire",
    "desinscrire",
    "désinscription",
    "desinscription",
    "se désabonner",
    "darse de baja",
    "cancelar suscripción",
    "afmelden",
    "uitschrijven",
    "cancella iscrizione",
    "annulla iscrizione",
    "disiscriviti",
    "newsletter settings",
    "email preferences",
    "manage subscription",
    "einstellungen",
];

/// Words (lowercase) in the URL itself.
const URL_WORDS: &[&str] = &[
    "unsubscribe",
    "unsub",
    "optout",
    "opt-out",
    "abmelden",
    "abmeldung",
    "abbestellen",
    "austragen",
    "desinscri",
    "d%c3%a9sinscri",
    "darsedebaja",
    "afmelden",
    "uitschrijven",
    "disiscri",
];

static ANCHOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<a\b[^>]*?\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))[^>]*>(.*?)</a\s*>"#)
        .expect("anchor")
});
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").expect("tag"));
static BARE_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>"'\)\]\}]+"#).expect("url"));

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#x3D;", "=")
        .replace("&#61;", "=")
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_word(haystack: &str, words: &[&str]) -> bool {
    let h = haystack.to_lowercase();
    words.iter().any(|w| h.contains(w))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push_str("...");
        t
    }
}

fn clean_url(raw: &str) -> String {
    decode_entities(raw)
        .trim()
        .trim_end_matches(['.', ',', ';', '>'])
        .to_string()
}

/// Extract unsubscribe links from an HTML or plain text body. Deduplicated, https first,
/// at most [`MAX_MAIL_LINKS`]. Nothing is fetched.
pub fn extract_unsubscribe_links(body: &str) -> Vec<ManualLink> {
    let mut found: Vec<ManualLink> = Vec::new();
    let mut push = |url: String, label: String| {
        if is_http(&url) && !found.iter().any(|l| l.url == url) {
            found.push(ManualLink {
                url,
                source: LinkSource::Mail,
                label,
            });
        }
    };
    let mut anchored_ranges: Vec<(usize, usize)> = Vec::new();
    for c in ANCHOR_RE.captures_iter(body) {
        let whole = c.get(0).expect("match");
        anchored_ranges.push((whole.start(), whole.end()));
        let href = c
            .get(1)
            .or_else(|| c.get(2))
            .or_else(|| c.get(3))
            .map_or("", |m| m.as_str());
        let url = clean_url(href);
        let text = collapse(&decode_entities(&TAG_RE.replace_all(&c[4], " ")));
        if has_word(&text, TEXT_WORDS) || has_word(&url, URL_WORDS) {
            let label = if text.is_empty() {
                "Abmeldelink in der Mail".to_string()
            } else {
                truncate(&text, 80)
            };
            push(url, label);
        }
    }
    // Bare URLs (plain text mails, or text parts of HTML mails): judge by the URL and by
    // the text just before it on the same line.
    let plain = body;
    for m in BARE_URL_RE.find_iter(plain) {
        if anchored_ranges
            .iter()
            .any(|(s, e)| m.start() >= *s && m.start() < *e)
        {
            continue;
        }
        let url = clean_url(m.as_str());
        let line_start = plain[..m.start()].rfind('\n').map_or(0, |i| i + 1);
        let before = &plain[line_start..m.start()];
        let before: String = before
            .chars()
            .rev()
            .take(120)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        // Text on the line above counts too ("Zum Abmelden hier klicken:" + URL below).
        let prev_line = plain[..line_start]
            .trim_end()
            .rsplit('\n')
            .next()
            .unwrap_or("");
        let context = format!("{prev_line} {before}");
        if has_word(&url, URL_WORDS)
            || (before.trim().is_empty() && has_word(&context, TEXT_WORDS))
            || has_word(&before, TEXT_WORDS)
        {
            let label = collapse(&decode_entities(&TAG_RE.replace_all(&context, " ")));
            let label = if label.is_empty() {
                "Abmeldelink in der Mail".to_string()
            } else {
                truncate(&label, 80)
            };
            push(url, label);
        }
    }
    found.sort_by_key(|l| !l.url.to_ascii_lowercase().starts_with("https://"));
    found.truncate(MAX_MAIL_LINKS);
    found
}

/// Text of a parsed mail: all `text/html` parts first, then `text/plain`.
fn body_texts(raw: &[u8]) -> String {
    let Ok(parsed) = mailparse::parse_mail(raw) else {
        return String::from_utf8_lossy(raw).into_owned();
    };
    let mut html = String::new();
    let mut plain = String::new();
    let mut stack = vec![&parsed];
    while let Some(p) = stack.pop() {
        if p.subparts.is_empty() {
            let ct = p.ctype.mimetype.to_ascii_lowercase();
            if let Ok(body) = p.get_body() {
                if ct == "text/html" {
                    html.push_str(&body);
                    html.push('\n');
                } else if ct == "text/plain" {
                    plain.push_str(&body);
                    plain.push('\n');
                }
            }
        } else {
            stack.extend(p.subparts.iter().rev());
        }
    }
    format!("{html}\n{plain}")
}

/// Links from raw message bytes (possibly truncated).
pub fn links_from_message(raw: &[u8]) -> Vec<ManualLink> {
    extract_unsubscribe_links(&body_texts(raw))
}

/// Fetch the newest mail of the list (by the stored Message-ID) and extract links.
/// Blocking; call from a worker thread. The body is dropped after extraction.
pub fn fetch_mail_links(
    cfg: &MailboxConfig,
    sub: &Subscription,
) -> Result<Vec<ManualLink>, MailError> {
    let id = sub
        .last_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            MailError::Invalid(
                "Von dieser Liste ist keine Mail gespeichert. Bitte das Postfach erneut scannen"
                    .into(),
            )
        })?;
    if id.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        return Err(MailError::Invalid("Ungültige Nachrichtenkennung".into()));
    }
    let mut client = connect_and_login(cfg)?;
    let result = find_and_read(&mut client, cfg, id);
    client.logout();
    Ok(result?
        .map(|raw| links_from_message(&raw))
        .unwrap_or_default())
}

fn find_and_read(
    client: &mut Client,
    cfg: &MailboxConfig,
    message_id: &str,
) -> Result<Option<Vec<u8>>, MailError> {
    let special_use = client
        .capability()
        .unwrap_or_default()
        .iter()
        .any(|c| c == "SPECIAL-USE");
    let all = client.list_folders(special_use)?;
    for folder in select_folders(cfg, &all) {
        if client.examine(&folder).is_err() {
            continue;
        }
        let uids = client.search_message_id(message_id)?;
        let Some(uid) = uids.last().copied() else {
            continue;
        };
        let items = client.uid_fetch(
            &uid.to_string(),
            &format!("(UID BODY.PEEK[]<0.{MAX_BODY_BYTES}>)"),
        )?;
        if let Some(body) = items.into_iter().find_map(|i| i.body) {
            let mut body = body;
            body.truncate(MAX_BODY_BYTES);
            return Ok(Some(body));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(methods: Vec<UnsubscribeMethod>) -> Subscription {
        use ausgeloggt_core::model::{CategorySource, MailCategory, SubscriptionStatus};
        Subscription {
            id: uuid::Uuid::new_v4(),
            mailbox: "me@example.org".into(),
            list_key: "news@x.example".into(),
            domain: "x.example".into(),
            name: "X".into(),
            sender: "news@x.example".into(),
            unsubscribe: methods,
            message_count: 1,
            frequency_per_month: 1.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category: MailCategory::Newsletter,
            category_source: CategorySource::Rules,
            status: SubscriptionStatus::Failed,
            history: vec![],
            post_unsubscribe_mails: vec![],
            last_message_id: None,
        }
    }

    #[test]
    fn header_links_collect_urls_and_mailto() {
        let s = sub(vec![
            UnsubscribeMethod::Link {
                url: "http://x.example/plain".into(),
            },
            UnsubscribeMethod::OneClick {
                url: "https://x.example/oneclick?a=1".into(),
            },
            UnsubscribeMethod::Link {
                url: "javascript:alert(1)".into(),
            },
            UnsubscribeMethod::Link {
                url: "https://x.example/oneclick?a=1".into(),
            },
            UnsubscribeMethod::Mailto {
                address: "unsub@x.example".into(),
                subject: Some("unsubscribe".into()),
                body: None,
            },
        ]);
        let (links, mailto) = header_links(&s);
        assert_eq!(mailto.as_deref(), Some("unsub@x.example"));
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].url, "https://x.example/oneclick?a=1");
        assert_eq!(links[1].url, "http://x.example/plain");
        assert!(links.iter().all(|l| l.source == LinkSource::Header));
        assert_eq!(header_links(&sub(vec![])), (vec![], None));
    }

    #[test]
    fn html_german_anchor_text() {
        let html = r#"<html><body><p>Hallo</p>
            <a href="https://x.example/shop">Zum Shop</a>
            <a href="https://t.example/c?u=1&amp;x=2" style="color:#999">
               <span>Hier</span>   abmelden </a>
            <a href='https://x.example/prefs'>Einstellungen ändern</a>
            </body></html>"#;
        let l = extract_unsubscribe_links(html);
        assert_eq!(l.len(), 2, "{l:?}");
        assert_eq!(l[0].url, "https://t.example/c?u=1&x=2");
        assert_eq!(l[0].label, "Hier abmelden");
        assert_eq!(l[1].url, "https://x.example/prefs");
        assert!(l.iter().all(|x| x.source == LinkSource::Mail));
    }

    #[test]
    fn html_english_and_url_words() {
        let html = r#"<a href="https://news.example/unsubscribe?id=9"><img src="x.png"></a>
            <a href="https://news.example/a">Click to Unsubscribe</a>
            <a href="http://news.example/optout">Opt-out here</a>
            <a href="mailto:unsub@news.example">Unsubscribe by mail</a>
            <a href="https://news.example/blog">Read our blog</a>"#;
        let l = extract_unsubscribe_links(html);
        let urls: Vec<_> = l.iter().map(|x| x.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://news.example/unsubscribe?id=9",
                "https://news.example/a",
                "http://news.example/optout"
            ]
        );
        assert_eq!(l[0].label, "Abmeldelink in der Mail");
    }

    #[test]
    fn html_french_spanish_dutch_italian() {
        let html = r#"
            <a href="https://f.example/1">Se désabonner</a>
            <a href="https://f.example/2">Pour vous désinscrire cliquez ici</a>
            <a href="https://f.example/3">Darse de baja</a>
            <a href="https://f.example/4">Afmelden voor de nieuwsbrief</a>
            <a href="https://f.example/5">Cancella iscrizione</a>
            <a href="https://f.example/6">Newsletter settings</a>
            <a href="https://f.example/7">Accueil</a>"#;
        let l = extract_unsubscribe_links(html);
        // capped at five, in document order
        let urls: Vec<_> = l.iter().map(|x| x.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://f.example/1",
                "https://f.example/2",
                "https://f.example/3",
                "https://f.example/4",
                "https://f.example/5"
            ]
        );
    }

    #[test]
    fn plain_text_links_with_context() {
        let text = "Hallo Erika,\n\nunser Angebot: https://x.example/angebot\n\n\
            Zum Abmelden hier klicken:\nhttps://x.example/u/abc123\n\n\
            To unsubscribe visit https://y.example/leave?id=1.\n\
            Datenschutz: https://x.example/privacy\n";
        let l = extract_unsubscribe_links(text);
        let urls: Vec<_> = l.iter().map(|x| x.url.as_str()).collect();
        assert_eq!(
            urls,
            vec!["https://x.example/u/abc123", "https://y.example/leave?id=1"]
        );
    }

    #[test]
    fn dedup_and_https_first() {
        let html = r#"<a href="http://a.example/unsubscribe">abmelden</a>
            <a href="https://a.example/unsubscribe">Abmelden</a>
            <a href="https://a.example/unsubscribe">Abmelden</a>"#;
        let l = extract_unsubscribe_links(html);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].url, "https://a.example/unsubscribe");
        assert_eq!(l[1].url, "http://a.example/unsubscribe");
    }

    #[test]
    fn rejects_non_http_and_junk() {
        let html = r#"<a href="javascript:void(0)">Unsubscribe</a>
            <a href="ftp://x.example/unsubscribe">Unsubscribe</a>
            <a href="https://x.example/a b">Unsubscribe</a>"#;
        assert!(extract_unsubscribe_links(html).is_empty());
        assert!(extract_unsubscribe_links("").is_empty());
    }

    #[test]
    fn multipart_message_html_part_is_used() {
        let raw = concat!(
            "From: a@x.example\r\nTo: me@example.org\r\nSubject: Hallo\r\n",
            "MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"B\"\r\n\r\n",
            "--B\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nNur Text\r\n",
            "--B\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\n",
            "<a href=3D\"https://x.example/u?t=3D1\">Abbestellen</a>\r\n--B--\r\n"
        );
        let l = links_from_message(raw.as_bytes());
        assert_eq!(l.len(), 1, "{l:?}");
        assert_eq!(l[0].url, "https://x.example/u?t=1");
        assert_eq!(l[0].label, "Abbestellen");
    }

    #[test]
    fn missing_message_id_is_a_clear_error() {
        let cfg = MailboxConfig {
            id: uuid::Uuid::nil(),
            address: "me@example.org".into(),
            imap_host: "127.0.0.1".into(),
            imap_port: 1,
            username: "u".into(),
            password: "p".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        };
        let e = fetch_mail_links(&cfg, &sub(vec![])).unwrap_err();
        assert!(matches!(e, MailError::Invalid(_)));
    }
}
