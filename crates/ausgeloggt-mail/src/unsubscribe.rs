//! Unsubscribing from newsletters.
//!
//! Safety rules:
//! - Automated only for RFC 8058 one-click (HTTPS POST `List-Unsubscribe=One-Click`,
//!   no cookies, no redirects followed (a 3xx answer counts as accepted), 15 s timeout) and
//!   `mailto:` (sent via the mailbox's own SMTP server).
//! - Plain `Link` methods are NEVER fetched automatically: they may confirm the
//!   address to spammers. The UI opens them in the browser on explicit click.
//! - Never unsubscribe from `MailCategory::Transactional`.

use std::time::Duration;

use ausgeloggt_core::{MailCategory, MailboxConfig, Subscription, UnsubscribeMethod};
use percent_encoding::percent_decode_str;

#[derive(Debug, thiserror::Error)]
pub enum UnsubscribeError {
    #[error("Kein automatischer Abmeldeweg vorhanden, bitte Link im Browser öffnen")]
    NoAutomaticMethod,
    #[error("Abmeldung abgelehnt: HTTP {0}")]
    Http(u16),
    #[error("Netzwerkfehler: {0}")]
    Network(String),
    #[error("Mailversand fehlgeschlagen: {0}")]
    Smtp(String),
    /// The subscription is categorised as transactional (receipts, account mails).
    #[error("Wichtige Mails (Rechnungen, Konto-Hinweise) werden nicht automatisch abbestellt")]
    Refused,
}

/// SMTP settings for a mailbox (derived from the provider preset; same credentials as IMAP).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtpSettings {
    pub host: String,
    pub port: u16,
    /// true = implicit TLS (465), false = STARTTLS (587).
    pub implicit_tls: bool,
}

/// (preset id, host, port, implicit TLS)
const SMTP_TABLE: &[(&str, &str, u16, bool)] = &[
    ("gmail", "smtp.gmail.com", 465, true),
    ("outlook", "smtp-mail.outlook.com", 587, false),
    ("icloud", "smtp.mail.me.com", 587, false),
    ("gmx", "mail.gmx.net", 465, true),
    ("webde", "smtp.web.de", 587, false),
    ("t-online", "securesmtp.t-online.de", 465, true),
    ("posteo", "posteo.de", 465, true),
    ("mailbox-org", "smtp.mailbox.org", 465, true),
    ("yahoo", "smtp.mail.yahoo.com", 465, true),
    ("fastmail", "smtp.fastmail.com", 465, true),
    ("aol", "smtp.aol.com", 465, true),
    ("freenet", "mx.freenet.de", 465, true),
    ("proton", "127.0.0.1", 1025, false),
];

/// SMTP settings for a configured mailbox, via provider preset or `smtp.` + domain guess.
pub fn smtp_for(cfg: &MailboxConfig) -> Option<SmtpSettings> {
    let preset = crate::presets()
        .iter()
        .find(|p| p.imap_host.eq_ignore_ascii_case(cfg.imap_host.trim()))
        .or_else(|| crate::preset_for_address(&cfg.address));
    if let Some(p) = preset {
        if let Some((_, host, port, implicit_tls)) = SMTP_TABLE.iter().find(|(id, ..)| *id == p.id)
        {
            return Some(SmtpSettings {
                host: (*host).to_string(),
                port: *port,
                implicit_tls: *implicit_tls,
            });
        }
    }
    let domain = cfg.address.rsplit_once('@')?.1.trim().to_ascii_lowercase();
    if domain.is_empty() || !domain.contains('.') {
        return None;
    }
    Some(SmtpSettings {
        host: format!("smtp.{domain}"),
        port: 465,
        implicit_tls: true,
    })
}

/// The best automatic method: OneClick > Mailto. `None` if only links exist.
pub fn best_automatic(sub: &Subscription) -> Option<&UnsubscribeMethod> {
    sub.unsubscribe
        .iter()
        .find(|m| matches!(m, UnsubscribeMethod::OneClick { .. }))
        .or_else(|| {
            sub.unsubscribe
                .iter()
                .find(|m| matches!(m, UnsubscribeMethod::Mailto { .. }))
        })
}

/// Perform an automatic unsubscribe. Blocking; call from a worker thread.
/// Returns a short German success message for the history.
pub fn unsubscribe(
    sub: &Subscription,
    mailbox: &MailboxConfig,
) -> Result<String, UnsubscribeError> {
    if sub.category == MailCategory::Transactional {
        return Err(UnsubscribeError::Refused);
    }
    match best_automatic(sub).ok_or(UnsubscribeError::NoAutomaticMethod)? {
        UnsubscribeMethod::OneClick { url } => one_click(url),
        UnsubscribeMethod::Mailto {
            address,
            subject,
            body,
        } => send_mailto(mailbox, address, subject.as_deref(), body.as_deref()),
        UnsubscribeMethod::Link { .. } => Err(UnsubscribeError::NoAutomaticMethod),
    }
}

fn one_click(url: &str) -> Result<String, UnsubscribeError> {
    if !starts_with_ci(url, "https://") {
        return Err(UnsubscribeError::NoAutomaticMethod);
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .user_agent("ausgeloggt")
        .build();
    let agent: ureq::Agent = config.into();
    let resp = agent
        .post(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send("List-Unsubscribe=One-Click")
        .map_err(|e| UnsubscribeError::Network(e.to_string()))?;
    let status = resp.status().as_u16();
    if (200..400).contains(&status) {
        Ok("Per One-Click abgemeldet (Server hat die Abmeldung akzeptiert)".to_string())
    } else {
        Err(UnsubscribeError::Http(status))
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn clean_line(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string()
}

fn send_mailto(
    cfg: &MailboxConfig,
    to: &str,
    subject: Option<&str>,
    body: Option<&str>,
) -> Result<String, UnsubscribeError> {
    use lettre::message::Mailbox;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::{Message, SmtpTransport, Transport};

    let smtp = smtp_for(cfg)
        .ok_or_else(|| UnsubscribeError::Smtp("Kein SMTP-Server bekannt".to_string()))?;
    let from: Mailbox = cfg
        .address
        .parse()
        .map_err(|_| UnsubscribeError::Smtp("Ungültige eigene Adresse".to_string()))?;
    let to_mb: Mailbox = clean_line(to)
        .parse()
        .map_err(|_| UnsubscribeError::Smtp("Ungültige Abmelde-Adresse".to_string()))?;
    let subject = subject
        .map(clean_line)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unsubscribe".to_string());
    let body = body
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("unsubscribe")
        .to_string();
    let message = Message::builder()
        .from(from)
        .to(to_mb)
        .subject(subject)
        .body(body)
        .map_err(|e| UnsubscribeError::Smtp(e.to_string()))?;

    let builder = if is_loopback(&smtp.host) {
        // Proton Bridge: local, self-signed certificate.
        let params = TlsParameters::builder(smtp.host.clone())
            .dangerous_accept_invalid_certs(true)
            .build()
            .map_err(|e| UnsubscribeError::Smtp(e.to_string()))?;
        SmtpTransport::builder_dangerous(smtp.host.as_str()).tls(Tls::Opportunistic(params))
    } else if smtp.implicit_tls {
        SmtpTransport::relay(&smtp.host).map_err(|e| UnsubscribeError::Smtp(e.to_string()))?
    } else {
        SmtpTransport::starttls_relay(&smtp.host)
            .map_err(|e| UnsubscribeError::Smtp(e.to_string()))?
    };
    let mailer = builder
        .port(smtp.port)
        .credentials(Credentials::new(cfg.username.clone(), cfg.password.clone()))
        .timeout(Some(Duration::from_secs(30)))
        .build();
    mailer.send(&message).map_err(|e| {
        let msg = e.to_string();
        if e.is_permanent() || e.is_transient() || e.is_response() {
            UnsubscribeError::Smtp(msg)
        } else {
            UnsubscribeError::Network(msg)
        }
    })?;
    Ok("Abmelde-Mail gesendet".to_string())
}

fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// Parse `List-Unsubscribe` (+ `List-Unsubscribe-Post`) header values into methods.
/// `<https://x/u?id=1>, <mailto:unsub@x.com?subject=unsubscribe>` with
/// `List-Unsubscribe-Post: List-Unsubscribe=One-Click` -> [OneClick, Mailto].
pub fn parse_list_unsubscribe(header: &str, post_header: Option<&str>) -> Vec<UnsubscribeMethod> {
    let one_click_allowed = post_header
        .map(|p| {
            p.to_ascii_lowercase()
                .replace(char::is_whitespace, "")
                .contains("list-unsubscribe=one-click")
        })
        .unwrap_or(false);

    let mut uris: Vec<String> = Vec::new();
    let mut rest = header;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else { break };
        // Unfold / strip whitespace inside the brackets.
        let uri: String = after[..end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if !uri.is_empty() {
            uris.push(uri);
        }
        rest = &after[end + 1..];
    }
    if uris.is_empty() && !header.contains('<') {
        // Non-compliant: bare, comma separated URIs.
        for part in header.split(',') {
            let uri: String = part.chars().filter(|c| !c.is_whitespace()).collect();
            if starts_with_ci(&uri, "mailto:")
                || starts_with_ci(&uri, "https://")
                || starts_with_ci(&uri, "http://")
            {
                uris.push(uri);
            }
        }
    }

    let mut out: Vec<UnsubscribeMethod> = Vec::new();
    let mut have_one_click = false;
    for uri in uris {
        let method = if starts_with_ci(&uri, "mailto:") {
            parse_mailto(&uri[7..])
        } else if starts_with_ci(&uri, "https://") && uri.len() > 8 {
            if one_click_allowed && !have_one_click {
                have_one_click = true;
                Some(UnsubscribeMethod::OneClick { url: uri })
            } else {
                Some(UnsubscribeMethod::Link { url: uri })
            }
        } else if starts_with_ci(&uri, "http://") && uri.len() > 7 {
            Some(UnsubscribeMethod::Link { url: uri })
        } else {
            None
        };
        if let Some(m) = method {
            if !out.contains(&m) {
                out.push(m);
            }
        }
    }
    out
}

fn decode(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

fn parse_mailto(rest: &str) -> Option<UnsubscribeMethod> {
    let (addr_part, query) = match rest.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (rest, None),
    };
    // Several recipients are possible; use the first.
    let address = clean_line(&decode(addr_part.split(',').next().unwrap_or("")));
    if address.is_empty()
        || address.matches('@').count() != 1
        || address.starts_with('@')
        || address.ends_with('@')
        || address.chars().any(char::is_whitespace)
    {
        return None;
    }
    let (mut subject, mut body) = (None, None);
    for pair in query.unwrap_or("").split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let value = decode(v);
        match k.to_ascii_lowercase().as_str() {
            "subject" if subject.is_none() => subject = Some(clean_line(&value)),
            "body" if body.is_none() => body = Some(value.trim().to_string()),
            _ => {}
        }
    }
    Some(UnsubscribeMethod::Mailto {
        address,
        subject: subject.filter(|s| !s.is_empty()),
        body: body.filter(|s| !s.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ausgeloggt_core::{CategorySource, SubscriptionStatus};
    use uuid::Uuid;

    const ONE_CLICK: &str = "List-Unsubscribe=One-Click";

    fn sub(methods: Vec<UnsubscribeMethod>, category: MailCategory) -> Subscription {
        Subscription {
            id: Uuid::nil(),
            mailbox: "me@example.org".into(),
            list_key: "news.shop.example".into(),
            domain: "shop.example".into(),
            name: "Shop".into(),
            sender: "news@shop.example".into(),
            unsubscribe: methods,
            message_count: 3,
            frequency_per_month: 1.0,
            first_seen: None,
            last_seen: None,
            sample_subjects: vec![],
            category,
            category_source: CategorySource::Rules,
            status: SubscriptionStatus::Active,
            history: vec![],
            post_unsubscribe_mails: Vec::new(),
        }
    }

    fn cfg(address: &str, host: &str) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::nil(),
            address: address.into(),
            imap_host: host.into(),
            imap_port: 993,
            username: address.into(),
            password: "x".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    #[test]
    fn one_click_and_mailto() {
        let m = parse_list_unsubscribe(
            "<https://x.example/u?id=1>, <mailto:unsub@x.example?subject=unsubscribe>",
            Some(ONE_CLICK),
        );
        assert_eq!(
            m,
            vec![
                UnsubscribeMethod::OneClick {
                    url: "https://x.example/u?id=1".into()
                },
                UnsubscribeMethod::Mailto {
                    address: "unsub@x.example".into(),
                    subject: Some("unsubscribe".into()),
                    body: None
                },
            ]
        );
    }

    #[test]
    fn https_without_post_header_is_link() {
        let m = parse_list_unsubscribe("<https://x.example/u>", None);
        assert_eq!(
            m,
            vec![UnsubscribeMethod::Link {
                url: "https://x.example/u".into()
            }]
        );
    }

    #[test]
    fn post_header_other_value_is_not_one_click() {
        let m = parse_list_unsubscribe("<https://x.example/u>", Some("something-else"));
        assert!(matches!(m[0], UnsubscribeMethod::Link { .. }));
    }

    #[test]
    fn http_never_one_click() {
        let m = parse_list_unsubscribe("<http://x.example/u>", Some(ONE_CLICK));
        assert_eq!(
            m,
            vec![UnsubscribeMethod::Link {
                url: "http://x.example/u".into()
            }]
        );
    }

    #[test]
    fn post_header_case_and_spacing() {
        let m = parse_list_unsubscribe(
            "<https://x.example/u>",
            Some("  list-unsubscribe = one-click "),
        );
        assert!(matches!(m[0], UnsubscribeMethod::OneClick { .. }));
    }

    #[test]
    fn only_first_https_is_one_click() {
        let m = parse_list_unsubscribe(
            "<https://a.example/1>, <https://b.example/2>",
            Some(ONE_CLICK),
        );
        assert!(matches!(m[0], UnsubscribeMethod::OneClick { .. }));
        assert!(matches!(m[1], UnsubscribeMethod::Link { .. }));
    }

    #[test]
    fn folded_header() {
        let h = "<mailto:u@x.example>,\r\n <https://x.example/\r\n unsub?a=1>";
        let m = parse_list_unsubscribe(h, None);
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[1],
            UnsubscribeMethod::Link {
                url: "https://x.example/unsub?a=1".into()
            }
        );
    }

    #[test]
    fn mailto_subject_and_body_percent_decoded() {
        let m = parse_list_unsubscribe(
            "<mailto:list-request@x.example?subject=Unsubscribe%20me&body=please%20remove%0Aadd>",
            None,
        );
        assert_eq!(
            m,
            vec![UnsubscribeMethod::Mailto {
                address: "list-request@x.example".into(),
                subject: Some("Unsubscribe me".into()),
                body: Some("please remove\nadd".into()),
            }]
        );
    }

    #[test]
    fn mailto_without_query() {
        let m = parse_list_unsubscribe("<mailto:leave@x.example>", None);
        assert_eq!(
            m,
            vec![UnsubscribeMethod::Mailto {
                address: "leave@x.example".into(),
                subject: None,
                body: None
            }]
        );
    }

    #[test]
    fn mailto_uppercase_scheme_and_params() {
        let m = parse_list_unsubscribe("<MAILTO:leave@x.example?Subject=Bye>", None);
        assert_eq!(
            m,
            vec![UnsubscribeMethod::Mailto {
                address: "leave@x.example".into(),
                subject: Some("Bye".into()),
                body: None
            }]
        );
    }

    #[test]
    fn mailto_subject_newline_injection_stripped() {
        let m = parse_list_unsubscribe(
            "<mailto:a@x.example?subject=hi%0D%0ABcc:evil@x.example>",
            None,
        );
        match &m[0] {
            UnsubscribeMethod::Mailto {
                subject: Some(s), ..
            } => {
                assert!(!s.contains('\n') && !s.contains('\r'))
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn invalid_mailto_ignored() {
        assert!(parse_list_unsubscribe("<mailto:notanaddress>", None).is_empty());
        assert!(parse_list_unsubscribe("<mailto:>", None).is_empty());
        assert!(parse_list_unsubscribe("<mailto:a@b@c.example>", None).is_empty());
    }

    #[test]
    fn unsupported_schemes_ignored() {
        assert!(
            parse_list_unsubscribe("<ftp://x.example/u>, <javascript:alert(1)>", None).is_empty()
        );
    }

    #[test]
    fn empty_and_garbage() {
        assert!(parse_list_unsubscribe("", None).is_empty());
        assert!(parse_list_unsubscribe("<>", None).is_empty());
        assert!(parse_list_unsubscribe("<https://x.example/u", None).is_empty());
        assert!(parse_list_unsubscribe("no brackets here", None).is_empty());
    }

    #[test]
    fn bare_uris_fallback() {
        let m = parse_list_unsubscribe("https://x.example/u, mailto:a@x.example", None);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn duplicates_removed() {
        let m = parse_list_unsubscribe("<mailto:a@x.example>, <mailto:a@x.example>", None);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn comments_between_uris() {
        let m = parse_list_unsubscribe(
            "(Unsub) <mailto:a@x.example>, (Web) <https://x.example/u>",
            None,
        );
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn best_automatic_prefers_one_click() {
        let s = sub(
            vec![
                UnsubscribeMethod::Link {
                    url: "https://l".into(),
                },
                UnsubscribeMethod::Mailto {
                    address: "a@b.c".into(),
                    subject: None,
                    body: None,
                },
                UnsubscribeMethod::OneClick {
                    url: "https://o".into(),
                },
            ],
            MailCategory::Marketing,
        );
        assert!(matches!(
            best_automatic(&s),
            Some(UnsubscribeMethod::OneClick { .. })
        ));
        let s = sub(
            vec![UnsubscribeMethod::Link {
                url: "https://l".into(),
            }],
            MailCategory::Marketing,
        );
        assert!(best_automatic(&s).is_none());
    }

    #[test]
    fn transactional_refused() {
        let s = sub(
            vec![UnsubscribeMethod::OneClick {
                url: "https://x.invalid/u".into(),
            }],
            MailCategory::Transactional,
        );
        assert!(matches!(
            unsubscribe(&s, &cfg("me@gmail.com", "imap.gmail.com")),
            Err(UnsubscribeError::Refused)
        ));
    }

    #[test]
    fn link_only_has_no_automatic_method() {
        let s = sub(
            vec![UnsubscribeMethod::Link {
                url: "https://x.invalid/u".into(),
            }],
            MailCategory::Marketing,
        );
        assert!(matches!(
            unsubscribe(&s, &cfg("me@gmail.com", "imap.gmail.com")),
            Err(UnsubscribeError::NoAutomaticMethod)
        ));
    }

    #[test]
    fn non_https_one_click_not_posted() {
        assert!(matches!(
            one_click("http://x.invalid/u"),
            Err(UnsubscribeError::NoAutomaticMethod)
        ));
    }

    #[test]
    fn one_click_unreachable_is_network_error() {
        // Port 1 on loopback: connection refused immediately.
        let r = one_click("https://127.0.0.1:1/unsub");
        assert!(matches!(r, Err(UnsubscribeError::Network(_))), "{r:?}");
    }

    #[test]
    fn smtp_presets() {
        let g = smtp_for(&cfg("a@gmail.com", "imap.gmail.com")).unwrap();
        assert_eq!(
            g,
            SmtpSettings {
                host: "smtp.gmail.com".into(),
                port: 465,
                implicit_tls: true
            }
        );
        let o = smtp_for(&cfg("a@outlook.com", "outlook.office365.com")).unwrap();
        assert_eq!(
            (o.host.as_str(), o.port, o.implicit_tls),
            ("smtp-mail.outlook.com", 587, false)
        );
        let w = smtp_for(&cfg("a@web.de", "imap.web.de")).unwrap();
        assert_eq!((w.host.as_str(), w.port), ("smtp.web.de", 587));
        let p = smtp_for(&cfg("a@proton.me", "127.0.0.1")).unwrap();
        assert_eq!((p.host.as_str(), p.port), ("127.0.0.1", 1025));
    }

    #[test]
    fn smtp_every_preset_has_entry() {
        for p in crate::presets() {
            assert!(
                SMTP_TABLE.iter().any(|(id, ..)| *id == p.id),
                "no SMTP for {}",
                p.id
            );
        }
    }

    #[test]
    fn smtp_custom_domain_on_gmail_host() {
        let s = smtp_for(&cfg("me@firma.example", "imap.gmail.com")).unwrap();
        assert_eq!(s.host, "smtp.gmail.com");
    }

    #[test]
    fn smtp_guess_for_unknown() {
        let s = smtp_for(&cfg("me@firma.example", "mail.firma.example")).unwrap();
        assert_eq!(
            s,
            SmtpSettings {
                host: "smtp.firma.example".into(),
                port: 465,
                implicit_tls: true
            }
        );
        assert!(smtp_for(&cfg("broken", "mail.firma.example")).is_none());
    }
}
