//! Sending mail from a configured mailbox (SMTP via lettre, rustls) and keeping a copy
//! in the IMAP "Sent" folder as proof.
//!
//! Message bodies, recipients and credentials are never logged.

use std::time::{Duration, SystemTime};

use ausgeloggt_core::MailboxConfig;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::imap::{is_loopback, Folder};
use crate::unsubscribe::{smtp_for, SmtpSettings};
use crate::{connect_and_login, has_attr, MailError};

/// A mail to send. `attachments` are `(file name, MIME type, bytes)`.
#[derive(Debug, Clone, Default)]
pub struct OutgoingMail {
    pub to: String,
    pub subject: String,
    pub body_text: String,
    pub attachments: Vec<(String, String, Vec<u8>)>,
    pub reply_to: Option<String>,
    /// Display name for the From header (from the settings); empty = address only.
    pub from_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMail {
    /// Message-ID including angle brackets, as it appears in the header.
    pub message_id: String,
    pub sent_at: DateTime<Utc>,
    /// Folder the copy was appended to (`None` when the provider stores it itself, e.g.
    /// Gmail, or no Sent folder was found).
    pub appended_to: Option<String>,
    /// Why the copy could not be stored (the mail itself was sent).
    pub append_error: Option<String>,
}

const SMTP_TIMEOUT: Duration = Duration::from_secs(30);

/// Names of Sent folders, lowercase, used when the server offers no SPECIAL-USE `\Sent`.
const SENT_NAMES: &[&str] = &[
    "sent",
    "gesendet",
    "gesendete objekte",
    "gesendete elemente",
    "gesendete nachrichten",
    "sent items",
    "sent messages",
    "[gmail]/sent mail",
    "[google mail]/gesendet",
    "inbox.sent",
    "inbox/sent",
];

fn clean_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn domain_of(address: &str) -> Option<&str> {
    address
        .rsplit_once('@')
        .map(|(_, d)| d.trim())
        .filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_graphic()))
}

/// Generate `<uuid@sender-domain>`.
pub fn new_message_id(sender_address: &str) -> String {
    let domain = domain_of(sender_address).unwrap_or("localhost.localdomain");
    format!("<{}@{}>", Uuid::new_v4().simple(), domain)
}

/// A built message: header-complete RFC 5322 bytes plus the envelope pieces.
pub struct Built {
    pub message: lettre::Message,
    pub raw: Vec<u8>,
    pub message_id: String,
}

/// Build the message without any network access.
pub fn build_message(
    from_address: &str,
    msg: &OutgoingMail,
    now: DateTime<Utc>,
    message_id: &str,
) -> Result<Built, MailError> {
    use lettre::message::header::ContentType;
    use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
    use lettre::Address;

    let invalid = |m: &str| MailError::Invalid(m.to_string());
    let from_addr: Address = from_address
        .trim()
        .parse()
        .map_err(|_| invalid("Die eigene Mailadresse ist ungültig"))?;
    let name = clean_line(&msg.from_name);
    let from = Mailbox::new((!name.is_empty()).then_some(name), from_addr);
    let to_raw = msg.to.trim();
    if to_raw.is_empty() || to_raw.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid("Die Empfängeradresse ist ungültig"));
    }
    let to_addr: Address = to_raw
        .parse()
        .map_err(|_| invalid("Die Empfängeradresse ist ungültig"))?;
    let subject = clean_line(&msg.subject);
    if subject.is_empty() {
        return Err(invalid("Der Betreff fehlt"));
    }
    let system_time = SystemTime::UNIX_EPOCH
        + Duration::from_secs(u64::try_from(now.timestamp().max(0)).unwrap_or(0));
    let mut builder = lettre::Message::builder()
        .from(from)
        .to(Mailbox::new(None, to_addr))
        .subject(subject)
        .date(system_time)
        .message_id(Some(message_id.to_string()));
    if let Some(r) = msg
        .reply_to
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        let addr: Address = r
            .parse()
            .map_err(|_| invalid("Die Antwortadresse ist ungültig"))?;
        builder = builder.reply_to(Mailbox::new(None, addr));
    }
    let text = SinglePart::plain(msg.body_text.clone());
    let message = if msg.attachments.is_empty() {
        builder.singlepart(text)
    } else {
        let mut multi = MultiPart::mixed().singlepart(text);
        for (file, mime, bytes) in &msg.attachments {
            let ct = ContentType::parse(mime)
                .map_err(|_| invalid("Der Dateityp des Anhangs ist ungültig"))?;
            multi = multi.singlepart(Attachment::new(clean_line(file)).body(bytes.clone(), ct));
        }
        builder.multipart(multi)
    }
    .map_err(|e| MailError::Invalid(format!("Die Mail konnte nicht aufgebaut werden: {e}")))?;
    let raw = message.formatted();
    Ok(Built {
        message,
        raw,
        message_id: message_id.to_string(),
    })
}

/// Pick the Sent folder: SPECIAL-USE `\Sent` first, then well known names.
pub(crate) fn pick_sent_folder(folders: &[Folder]) -> Option<String> {
    let selectable = |f: &&Folder| !has_attr(f, "\\noselect") && !has_attr(f, "\\nonexistent");
    if let Some(f) = folders
        .iter()
        .filter(selectable)
        .find(|f| has_attr(f, "\\sent"))
    {
        return Some(f.name.clone());
    }
    for wanted in SENT_NAMES {
        if let Some(f) = folders
            .iter()
            .filter(selectable)
            .find(|f| f.name.to_lowercase() == *wanted)
        {
            return Some(f.name.clone());
        }
    }
    // Hierarchical names like "INBOX.Gesendet" or "Mail/Sent": match the last part.
    folders.iter().filter(selectable).find_map(|f| {
        let last = f.name.rsplit(['/', '.']).next().unwrap_or(&f.name);
        SENT_NAMES
            .contains(&last.to_lowercase().as_str())
            .then(|| f.name.clone())
    })
}

fn is_gmail(cfg: &MailboxConfig) -> bool {
    cfg.imap_host.trim().eq_ignore_ascii_case("imap.gmail.com")
}

/// Copy `raw` into the Sent folder. `Ok(None)` = nothing to do / no folder found.
fn append_to_sent(cfg: &MailboxConfig, raw: &[u8]) -> Result<Option<String>, MailError> {
    if is_gmail(cfg) {
        return Ok(None);
    }
    let mut client = connect_and_login(cfg)?;
    let special_use = client
        .capability()
        .unwrap_or_default()
        .iter()
        .any(|c| c == "SPECIAL-USE");
    let folders = client.list_folders(special_use);
    let result = match folders {
        Ok(folders) => match pick_sent_folder(&folders) {
            Some(folder) => client.append(&folder, "\\Seen", raw).map(|()| Some(folder)),
            None => Ok(None),
        },
        Err(e) => Err(e),
    };
    client.logout();
    result
}

fn transport(cfg: &MailboxConfig, smtp: &SmtpSettings) -> Result<lettre::SmtpTransport, MailError> {
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::transport::smtp::client::{Tls, TlsParameters};
    use lettre::SmtpTransport;

    let err = |e: lettre::transport::smtp::Error| MailError::Smtp(e.to_string());
    let builder = if is_loopback(&smtp.host) {
        // Proton Bridge: local, self-signed certificate. Never used for remote hosts.
        let params = TlsParameters::builder(smtp.host.clone())
            .dangerous_accept_invalid_certs(true)
            .build()
            .map_err(err)?;
        SmtpTransport::builder_dangerous(smtp.host.as_str()).tls(Tls::Opportunistic(params))
    } else if smtp.implicit_tls {
        SmtpTransport::relay(&smtp.host).map_err(err)?
    } else {
        SmtpTransport::starttls_relay(&smtp.host).map_err(err)?
    };
    Ok(builder
        .port(smtp.port)
        .credentials(Credentials::new(cfg.username.clone(), cfg.password.clone()))
        .timeout(Some(SMTP_TIMEOUT))
        .build())
}

/// Send `msg` through the mailbox's own SMTP server and store a copy in its Sent folder.
/// Blocking; call from a worker thread. A failure to store the copy does not fail the
/// send (see [`SentMail::append_error`]).
pub fn send_mail(cfg: &MailboxConfig, msg: &OutgoingMail) -> Result<SentMail, MailError> {
    let smtp = smtp_for(cfg).ok_or_else(|| MailError::Smtp("Kein SMTP Server bekannt".into()))?;
    send_mail_via(cfg, &smtp, msg, Utc::now())
}

pub(crate) fn send_mail_via(
    cfg: &MailboxConfig,
    smtp: &SmtpSettings,
    msg: &OutgoingMail,
    now: DateTime<Utc>,
) -> Result<SentMail, MailError> {
    use lettre::Transport;

    let message_id = new_message_id(&cfg.address);
    let built = build_message(&cfg.address, msg, now, &message_id)?;
    let mailer = transport(cfg, smtp)?;
    mailer
        .send_raw(built.message.envelope(), &built.raw)
        .map_err(|e| {
            let status = e.status().map(|c| c.to_string()).unwrap_or_default();
            if matches!(status.as_str(), "530" | "534" | "535" | "538") {
                MailError::Auth
            } else {
                MailError::Smtp(e.to_string())
            }
        })?;
    let (appended_to, append_error) = match append_to_sent(cfg, &built.raw) {
        Ok(folder) => (folder, None),
        Err(e) => (None, Some(e.to_string())),
    };
    Ok(SentMail {
        message_id,
        sent_at: now,
        appended_to,
        append_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    use chrono::TimeZone;

    fn cfg(imap_port: u16) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::nil(),
            address: "erika@example.org".into(),
            imap_host: "127.0.0.1".into(),
            imap_port,
            username: "erika".into(),
            password: "geheim".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 12, 9, 30, 0).unwrap()
    }

    fn mail() -> OutgoingMail {
        OutgoingMail {
            to: "datenschutz@beispiel.de".into(),
            subject: "Antrag auf Löschung nach Art. 17 DSGVO".into(),
            body_text:
                "Sehr geehrte Damen und Herren,\n\nbitte löschen Sie meine Daten. Grüße aus Köln.\n"
                    .into(),
            attachments: vec![],
            reply_to: None,
            from_name: "Erika Müller".into(),
        }
    }

    fn text(raw: &[u8]) -> String {
        String::from_utf8_lossy(raw).into_owned()
    }

    #[test]
    fn message_id_uses_sender_domain() {
        let id = new_message_id("erika@Example.org");
        assert!(id.starts_with('<') && id.ends_with("@Example.org>"), "{id}");
        assert_ne!(id, new_message_id("erika@Example.org"));
        assert!(new_message_id("kaputt").ends_with("@localhost.localdomain>"));
    }

    #[test]
    fn headers_are_complete_and_utf8_safe() {
        let b = build_message("erika@example.org", &mail(), now(), "<abc@example.org>").unwrap();
        let t = text(&b.raw);
        assert!(t.contains("Message-ID: <abc@example.org>"), "{t}");
        assert!(t.contains("Date: Thu, 12 Nov 2026 09:30:00 +0000"), "{t}");
        assert!(t.contains("To: datenschutz@beispiel.de"), "{t}");
        assert!(t.contains("MIME-Version: 1.0"));
        // Non ASCII header values are encoded (RFC 2047), the raw bytes stay ASCII.
        let head = t.split("\r\n\r\n").next().unwrap();
        assert!(head.is_ascii(), "{head}");
        assert!(head.contains("=?utf-8?"), "{head}");
        assert!(head.contains("erika@example.org"));
        // The parsed subject and display name survive the round trip.
        let parsed = mailparse::parse_mail(&b.raw).unwrap();
        use mailparse::MailHeaderMap;
        assert_eq!(
            parsed.headers.get_first_value("Subject").unwrap(),
            "Antrag auf Löschung nach Art. 17 DSGVO"
        );
        let from = parsed.headers.get_first_value("From").unwrap();
        assert!(
            from.contains("Erika Müller") && from.contains("erika@example.org"),
            "{from}"
        );
        assert!(parsed.get_body().unwrap().contains("Köln"));
    }

    #[test]
    fn envelope_matches_headers() {
        let b = build_message("erika@example.org", &mail(), now(), "<a@b.de>").unwrap();
        let env = b.message.envelope();
        assert_eq!(env.from().unwrap().to_string(), "erika@example.org");
        assert_eq!(env.to().len(), 1);
        assert_eq!(env.to()[0].to_string(), "datenschutz@beispiel.de");
    }

    #[test]
    fn attachment_is_base64_with_name_and_type() {
        let mut m = mail();
        let pdf = b"%PDF-1.4\n\xe4\xf6\xfc binary \x00\x01".to_vec();
        m.attachments.push((
            "Loeschantrag.pdf".into(),
            "application/pdf".into(),
            pdf.clone(),
        ));
        let b = build_message("erika@example.org", &m, now(), "<a@b.de>").unwrap();
        let parsed = mailparse::parse_mail(&b.raw).unwrap();
        assert!(parsed.ctype.mimetype.starts_with("multipart/mixed"));
        assert_eq!(parsed.subparts.len(), 2);
        let att = &parsed.subparts[1];
        assert_eq!(att.ctype.mimetype, "application/pdf");
        assert_eq!(att.get_body_raw().unwrap(), pdf);
        let t = text(&b.raw);
        assert!(t.contains("filename=\"Loeschantrag.pdf\""), "{t}");
        assert!(t
            .to_lowercase()
            .contains("content-transfer-encoding: base64"));
        assert!(parsed.subparts[0].get_body().unwrap().contains("löschen"));
    }

    #[test]
    fn reply_to_and_validation() {
        let mut m = mail();
        m.reply_to = Some("antwort@example.org".into());
        let b = build_message("erika@example.org", &m, now(), "<a@b.de>").unwrap();
        assert!(text(&b.raw).contains("Reply-To: antwort@example.org"));

        for bad in ["", "kein at", "a@b.de, c@d.de", "a@b.de\r\nBcc: x@y.de"] {
            let mut m = mail();
            m.to = bad.into();
            assert!(
                matches!(
                    build_message("erika@example.org", &m, now(), "<a@b.de>"),
                    Err(MailError::Invalid(_))
                ),
                "{bad:?}"
            );
        }
        let mut m = mail();
        m.subject = "  \r\n ".into();
        assert!(build_message("erika@example.org", &m, now(), "<a@b.de>").is_err());
        assert!(build_message("kaputt", &mail(), now(), "<a@b.de>").is_err());
    }

    #[test]
    fn header_injection_in_subject_and_name_is_neutralised() {
        let mut m = mail();
        m.subject = "Hallo\r\nBcc: boese@example.net".into();
        m.from_name = "Erika\r\nBcc: boese@example.net".into();
        let b = build_message("erika@example.org", &m, now(), "<a@b.de>").unwrap();
        let head = text(&b.raw);
        let head = head.split("\r\n\r\n").next().unwrap();
        assert!(
            !head.lines().any(|l| l.to_lowercase().starts_with("bcc:")),
            "{head}"
        );
        assert_eq!(b.message.envelope().to().len(), 1);
    }

    fn folder(name: &str, attrs: &[&str]) -> Folder {
        Folder {
            name: name.into(),
            attrs: attrs.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    #[test]
    fn sent_folder_selection() {
        let f = [
            folder("INBOX", &[]),
            folder("Gesendete Objekte", &[]),
            folder("Postausgang", &["\\sent"]),
        ];
        assert_eq!(pick_sent_folder(&f).as_deref(), Some("Postausgang"));
        let f = [folder("INBOX", &[]), folder("Gesendete Objekte", &[])];
        assert_eq!(pick_sent_folder(&f).as_deref(), Some("Gesendete Objekte"));
        let f = [folder("INBOX", &[]), folder("[Gmail]/Sent Mail", &[])];
        assert_eq!(pick_sent_folder(&f).as_deref(), Some("[Gmail]/Sent Mail"));
        let f = [folder("INBOX", &[]), folder("INBOX.Gesendet", &[])];
        assert_eq!(pick_sent_folder(&f).as_deref(), Some("INBOX.Gesendet"));
        let f = [folder("Sent", &["\\noselect"]), folder("INBOX", &[])];
        assert_eq!(pick_sent_folder(&f), None);
        assert_eq!(pick_sent_folder(&[]), None);
    }

    // ---- fake servers ---------------------------------------------------------------

    #[derive(Default)]
    struct Seen {
        appended: Vec<(String, Vec<u8>)>,
        commands: Vec<String>,
        smtp_data: Vec<u8>,
        smtp_rcpt: Vec<String>,
        smtp_from: String,
    }

    /// Minimal IMAP server: LIST (with a Sent folder), APPEND with literal.
    fn fake_imap(
        seen: Arc<Mutex<Seen>>,
        folders: &'static [&'static str],
        refuse_append: bool,
    ) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let mut out = stream.try_clone().unwrap();
                    let mut rd = BufReader::new(stream);
                    out.write_all(b"* OK ready\r\n").unwrap();
                    loop {
                        let mut line = String::new();
                        if rd.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let line = line.trim_end().to_string();
                        let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
                        let up = cmd.to_ascii_uppercase();
                        seen.lock()
                            .unwrap()
                            .commands
                            .push(up.split_whitespace().next().unwrap_or("").to_string());
                        if up == "CAPABILITY" {
                            write!(
                                out,
                                "* CAPABILITY IMAP4rev1 SPECIAL-USE\r\n{tag} OK done\r\n"
                            )
                            .unwrap();
                        } else if up.starts_with("LOGIN") {
                            write!(out, "{tag} OK logged in\r\n").unwrap();
                        } else if up.starts_with("LIST") {
                            for f in folders {
                                let attr = if f.starts_with("Sent") { "\\Sent" } else { "" };
                                write!(out, "* LIST ({attr}) \"/\" \"{f}\"\r\n").unwrap();
                            }
                            write!(out, "{tag} OK listed\r\n").unwrap();
                        } else if up.starts_with("APPEND") {
                            if refuse_append {
                                write!(out, "{tag} NO [OVERQUOTA] full\r\n").unwrap();
                                continue;
                            }
                            let name = cmd.split('"').nth(1).unwrap_or("").to_string();
                            let n: usize = cmd
                                .rsplit('{')
                                .next()
                                .unwrap()
                                .trim_end_matches('}')
                                .parse()
                                .unwrap();
                            write!(out, "+ go ahead\r\n").unwrap();
                            let mut buf = vec![0u8; n];
                            rd.read_exact(&mut buf).unwrap();
                            let mut crlf = [0u8; 2];
                            rd.read_exact(&mut crlf).unwrap();
                            seen.lock().unwrap().appended.push((name, buf));
                            write!(out, "{tag} OK [APPENDUID 1 7] appended\r\n").unwrap();
                        } else if up == "LOGOUT" {
                            write!(out, "* BYE\r\n{tag} OK bye\r\n").unwrap();
                            return;
                        } else {
                            write!(out, "{tag} BAD unknown\r\n").unwrap();
                        }
                    }
                });
            }
        });
        port
    }

    /// Minimal SMTP server with AUTH PLAIN; records the DATA section.
    fn fake_smtp(seen: Arc<Mutex<Seen>>, reject_auth: bool) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let seen = seen.clone();
                std::thread::spawn(move || {
                    let mut out = stream.try_clone().unwrap();
                    let mut rd = BufReader::new(stream);
                    out.write_all(b"220 fake ESMTP\r\n").unwrap();
                    loop {
                        let mut line = String::new();
                        if rd.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let up = line.trim_end().to_ascii_uppercase();
                        if up.starts_with("EHLO") || up.starts_with("HELO") {
                            out.write_all(b"250-fake\r\n250 AUTH PLAIN LOGIN\r\n")
                                .unwrap();
                        } else if up.starts_with("AUTH") {
                            if reject_auth {
                                out.write_all(b"535 5.7.8 Authentication failed\r\n")
                                    .unwrap();
                            } else {
                                out.write_all(b"235 2.7.0 ok\r\n").unwrap();
                            }
                        } else if up.starts_with("MAIL FROM:") {
                            seen.lock().unwrap().smtp_from = line.trim_end().to_string();
                            out.write_all(b"250 ok\r\n").unwrap();
                        } else if up.starts_with("RCPT TO:") {
                            seen.lock()
                                .unwrap()
                                .smtp_rcpt
                                .push(line.trim_end().to_string());
                            out.write_all(b"250 ok\r\n").unwrap();
                        } else if up == "DATA" {
                            out.write_all(b"354 go\r\n").unwrap();
                            let mut data = Vec::new();
                            loop {
                                let mut l = Vec::new();
                                if rd.read_until(b'\n', &mut l).unwrap_or(0) == 0 {
                                    return;
                                }
                                if l == b".\r\n" {
                                    break;
                                }
                                data.extend_from_slice(&l);
                            }
                            seen.lock().unwrap().smtp_data = data;
                            out.write_all(b"250 queued\r\n").unwrap();
                        } else if up == "QUIT" {
                            out.write_all(b"221 bye\r\n").unwrap();
                            return;
                        } else {
                            out.write_all(b"250 ok\r\n").unwrap();
                        }
                    }
                });
            }
        });
        port
    }

    fn smtp_settings(port: u16) -> SmtpSettings {
        SmtpSettings {
            host: "127.0.0.1".into(),
            port,
            implicit_tls: false,
        }
    }

    #[test]
    fn append_over_imap_uploads_the_exact_bytes() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let port = fake_imap(seen.clone(), &["INBOX", "Sent"], false);
        let raw = b"From: a@b.de\r\nSubject: =?utf-8?b?w6Q=?=\r\n\r\nK\xc3\xb6rper\r\n".to_vec();
        let folder = append_to_sent(&cfg(port), &raw).unwrap();
        assert_eq!(folder.as_deref(), Some("Sent"));
        let s = seen.lock().unwrap();
        assert_eq!(s.appended.len(), 1);
        assert_eq!(s.appended[0].0, "Sent");
        assert_eq!(s.appended[0].1, raw);
    }

    #[test]
    fn append_refusal_and_missing_folder() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let port = fake_imap(seen.clone(), &["INBOX", "Sent"], true);
        assert!(append_to_sent(&cfg(port), b"x").is_err());
        let port = fake_imap(seen.clone(), &["INBOX", "Archiv"], false);
        assert_eq!(append_to_sent(&cfg(port), b"x").unwrap(), None);
        assert!(seen.lock().unwrap().appended.is_empty());
    }

    #[test]
    fn gmail_skips_the_append() {
        let mut c = cfg(1);
        c.imap_host = "imap.gmail.com".into();
        // Would fail to connect if it tried.
        assert_eq!(append_to_sent(&c, b"x").unwrap(), None);
    }

    #[test]
    fn send_via_fake_smtp_then_append() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let imap = fake_imap(seen.clone(), &["INBOX", "Gesendet"], false);
        let smtp = fake_smtp(seen.clone(), false);
        let mut m = mail();
        m.attachments
            .push(("a.pdf".into(), "application/pdf".into(), vec![1, 2, 3, 4]));
        let sent = send_mail_via(&cfg(imap), &smtp_settings(smtp), &m, now()).unwrap();
        assert!(sent.message_id.ends_with("@example.org>"));
        assert_eq!(sent.sent_at, now());
        assert_eq!(sent.appended_to.as_deref(), Some("Gesendet"));
        assert!(sent.append_error.is_none());
        let s = seen.lock().unwrap();
        assert!(s.smtp_from.contains("erika@example.org"));
        assert_eq!(s.smtp_rcpt.len(), 1);
        assert!(s.smtp_rcpt[0].contains("datenschutz@beispiel.de"));
        let data = text(&s.smtp_data);
        assert!(
            data.contains(&format!("Message-ID: {}", sent.message_id)),
            "{data}"
        );
        // The stored copy is byte identical to what went over SMTP (modulo dot stuffing).
        assert_eq!(s.appended.len(), 1);
        assert_eq!(
            s.appended[0].1.trim_ascii_end(),
            s.smtp_data.trim_ascii_end()
        );
    }

    #[test]
    fn append_failure_does_not_fail_the_send() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let imap = fake_imap(seen.clone(), &["INBOX", "Sent"], true);
        let smtp = fake_smtp(seen.clone(), false);
        let sent = send_mail_via(&cfg(imap), &smtp_settings(smtp), &mail(), now()).unwrap();
        assert!(sent.appended_to.is_none());
        assert!(sent.append_error.is_some());
        assert!(!seen.lock().unwrap().smtp_data.is_empty());
    }

    #[test]
    fn rejected_login_is_an_auth_error_and_nothing_is_appended() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let imap = fake_imap(seen.clone(), &["INBOX", "Sent"], false);
        let smtp = fake_smtp(seen.clone(), true);
        let r = send_mail_via(&cfg(imap), &smtp_settings(smtp), &mail(), now());
        assert!(matches!(r, Err(MailError::Auth)), "{r:?}");
        assert!(seen.lock().unwrap().appended.is_empty());
    }
}
