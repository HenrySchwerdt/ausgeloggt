//! Mailbox scanning over IMAP (TLS via rustls, no OpenSSL) and mail classification.
//!
//! Privacy: only headers are fetched (`BODY.PEEK[HEADER.FIELDS (...)]`, never marks
//! mails as read). Bodies are fetched only for the few SSO security mails where the
//! app name is in the body. Nothing is sent anywhere except the user's IMAP server.

pub mod classify;
mod imap;
pub mod receipts;
pub mod unsubscribe;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use ausgeloggt_core::{
    Evidence, EvidenceKind, Finding, FolderProgress, MailCategory, MailRef, MailboxConfig,
    PaymentKind, PaymentObservation, PaymentRecord, SourceKind, SubscriptionObservation,
    UnsubscribeMethod,
};
use chrono::{DateTime, Utc};
use mailparse::MailHeaderMap;

use classify::MailHeader;
use imap::{Client, Folder};

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("Verbindung zu {0} fehlgeschlagen: {1}")]
    Connect(String, String),
    #[error("Anmeldung fehlgeschlagen. Ist das App-Passwort korrekt?")]
    Auth,
    #[error("IMAP-Fehler: {0}")]
    Imap(String),
    #[error("Abgebrochen")]
    Cancelled,
}

/// Known mail providers with IMAP settings and how to get an app password.
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub imap_host: &'static str,
    pub imap_port: u16,
    /// Domains whose addresses belong to this provider (`gmail.com`, `googlemail.com`).
    pub domains: &'static [&'static str],
    /// Folders to scan by default (e.g. Gmail `[Gmail]/All Mail` resolved via SPECIAL-USE \All).
    /// Empty = resolve automatically via SPECIAL-USE (the folder names are localised).
    pub default_folders: &'static [&'static str],
    /// Where the user creates an app password.
    pub app_password_url: Option<&'static str>,
    /// German hint shown in the setup dialog.
    pub hint_de: &'static str,
}

static PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "gmail",
        name: "Gmail",
        imap_host: "imap.gmail.com",
        imap_port: 993,
        domains: &["gmail.com", "googlemail.com"],
        default_folders: &[],
        app_password_url: Some("https://myaccount.google.com/apppasswords"),
        hint_de: "Gmail braucht die Bestätigung in zwei Schritten und ein App-Passwort (nicht dein normales Passwort). Erstelle es unter myaccount.google.com/apppasswords. Gescannt wird \"Alle Nachrichten\".",
    },
    ProviderPreset {
        id: "outlook",
        name: "Outlook.com / Hotmail / Live",
        imap_host: "outlook.office365.com",
        imap_port: 993,
        domains: &[
            "outlook.com",
            "outlook.de",
            "hotmail.com",
            "hotmail.de",
            "hotmail.fr",
            "hotmail.it",
            "hotmail.es",
            "hotmail.co.uk",
            "live.com",
            "live.de",
            "live.fr",
            "live.it",
            "live.nl",
            "msn.com",
        ],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Microsoft hat die Anmeldung per Passwort/App-Passwort für IMAP abgeschaltet. Outlook.com und Hotmail funktionieren erst, wenn die Anmeldung über Microsoft (OAuth) in einer späteren Version unterstützt wird.",
    },
    ProviderPreset {
        id: "icloud",
        name: "iCloud Mail",
        imap_host: "imap.mail.me.com",
        imap_port: 993,
        domains: &["icloud.com", "me.com", "mac.com"],
        default_folders: &[],
        app_password_url: Some("https://account.apple.com"),
        hint_de: "Für iCloud brauchst du ein app-spezifisches Passwort: auf account.apple.com unter \"Anmelden und Sicherheit\" erstellen (Zwei-Faktor-Authentifizierung muss aktiv sein). Benutzername ist deine iCloud-Adresse.",
    },
    ProviderPreset {
        id: "yahoo",
        name: "Yahoo Mail",
        imap_host: "imap.mail.yahoo.com",
        imap_port: 993,
        domains: &["yahoo.com", "yahoo.de", "yahoo.fr", "yahoo.it", "yahoo.es", "yahoo.co.uk", "ymail.com", "rocketmail.com"],
        default_folders: &[],
        app_password_url: Some("https://login.yahoo.com/account/security"),
        hint_de: "Yahoo verlangt ein App-Passwort: in den Kontosicherheits-Einstellungen \"App-Passwort generieren\" wählen.",
    },
    ProviderPreset {
        id: "gmx",
        name: "GMX",
        imap_host: "imap.gmx.net",
        imap_port: 993,
        domains: &["gmx.de", "gmx.net", "gmx.at", "gmx.ch", "gmx.com", "gmx.eu"],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Aktiviere in den GMX-Einstellungen unter \"POP3 & IMAP Abruf\" die Option \"Zugriff auf GMX Postfach per POP3 und IMAP erlauben\". Bei aktiver Zwei-Faktor-Anmeldung brauchst du ein eigenes Anwendungspasswort.",
    },
    ProviderPreset {
        id: "webde",
        name: "WEB.DE",
        imap_host: "imap.web.de",
        imap_port: 993,
        domains: &["web.de"],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Aktiviere in den WEB.DE-Einstellungen unter \"POP3 & IMAP Abruf\" den IMAP-Zugriff. Bei aktiver Zwei-Faktor-Anmeldung brauchst du ein eigenes Anwendungspasswort.",
    },
    ProviderPreset {
        id: "t-online",
        name: "T-Online",
        imap_host: "secureimap.t-online.de",
        imap_port: 993,
        domains: &["t-online.de", "magenta.de"],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Vergib im Kundencenter ein eigenes E-Mail-Passwort für externe Programme und aktiviere dort den Zugriff mit E-Mail-Programmen. Benutzername ist die vollständige T-Online-Adresse.",
    },
    ProviderPreset {
        id: "posteo",
        name: "Posteo",
        imap_host: "posteo.de",
        imap_port: 993,
        domains: &["posteo.de", "posteo.net", "posteo.eu", "posteo.at", "posteo.ch"],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Posteo funktioniert mit deinem Postfach-Passwort. Ist Zwei-Faktor-Authentifizierung aktiv, erstelle in den Einstellungen unter \"Externer Zugriff\" ein App-Passwort.",
    },
    ProviderPreset {
        id: "mailbox-org",
        name: "mailbox.org",
        imap_host: "imap.mailbox.org",
        imap_port: 993,
        domains: &["mailbox.org"],
        default_folders: &[],
        app_password_url: Some("https://login.mailbox.org"),
        hint_de: "Bei aktiver Zwei-Faktor-Anmeldung legst du unter \"Sicherheit\" ein eigenes App-Passwort für IMAP an.",
    },
    ProviderPreset {
        id: "proton",
        name: "Proton Mail (Bridge)",
        imap_host: "127.0.0.1",
        imap_port: 1143,
        domains: &["proton.me", "protonmail.com", "protonmail.ch", "pm.me"],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Proton Mail braucht die Proton Mail Bridge (bezahltes Konto), die auf diesem Rechner laufen muss. Verwende Benutzername und Passwort, die die Bridge anzeigt, nicht dein Proton-Passwort.",
    },
    ProviderPreset {
        id: "fastmail",
        name: "Fastmail",
        imap_host: "imap.fastmail.com",
        imap_port: 993,
        domains: &["fastmail.com", "fastmail.fm"],
        default_folders: &[],
        app_password_url: Some("https://app.fastmail.com/settings/security/devicekeys/new"),
        hint_de: "Fastmail verlangt ein App-Passwort mit IMAP-Zugriff (Einstellungen > Datenschutz & Sicherheit > App-Passwörter).",
    },
    ProviderPreset {
        id: "aol",
        name: "AOL Mail",
        imap_host: "imap.aol.com",
        imap_port: 993,
        domains: &["aol.com", "aol.de"],
        default_folders: &[],
        app_password_url: Some("https://login.aol.com/account/security"),
        hint_de: "AOL verlangt ein App-Passwort: in der Kontosicherheit \"App-Passwort generieren\" wählen.",
    },
    ProviderPreset {
        id: "freenet",
        name: "freenet.de",
        imap_host: "mx.freenet.de",
        imap_port: 993,
        domains: &["freenet.de"],
        default_folders: &[],
        app_password_url: None,
        hint_de: "Aktiviere in den freenet-Mail-Einstellungen den Zugriff per IMAP/POP3. Benutzername ist die vollständige Adresse.",
    },
];

/// Gmail, Outlook/Hotmail (hint: needs OAuth, not yet supported), iCloud, Yahoo, GMX,
/// WEB.DE, T-Online, Posteo, mailbox.org, Proton (Bridge, 127.0.0.1:1143), Fastmail, AOL, freenet.
pub fn presets() -> &'static [ProviderPreset] {
    PRESETS
}

pub fn preset_for_address(address: &str) -> Option<&'static ProviderPreset> {
    let domain = address
        .trim()
        .rsplit_once('@')?
        .1
        .trim()
        .to_ascii_lowercase();
    PRESETS
        .iter()
        .find(|p| p.domains.contains(&domain.as_str()))
}

fn connect_and_login(cfg: &MailboxConfig) -> Result<Client, MailError> {
    let mut client = Client::connect(&cfg.imap_host, cfg.imap_port)?;
    client.login(&cfg.username, &cfg.password)?;
    Ok(client)
}

/// Log in and list folders. Used by the "Verbindung testen" button.
pub fn test_connection(cfg: &MailboxConfig) -> Result<Vec<String>, MailError> {
    let mut client = connect_and_login(cfg)?;
    let special_use = client
        .capability()
        .unwrap_or_default()
        .iter()
        .any(|c| c == "SPECIAL-USE");
    let folders = client.list_folders(special_use);
    client.logout();
    Ok(folders?
        .into_iter()
        .filter(|f| !has_attr(f, "\\noselect") && !has_attr(f, "\\nonexistent"))
        .map(|f| f.name)
        .collect())
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScanEvent {
    Connected,
    FolderStarted {
        folder: String,
        total: u64,
    },
    Progress {
        folder: String,
        done: u64,
        total: u64,
        findings: u64,
    },
    FolderFinished {
        folder: String,
    },
}

#[derive(Debug, Clone)]
pub struct ScanOutcome {
    pub findings: Vec<Finding>,
    /// One entry per list seen in this scan, aggregated over the scanned mails.
    pub subscriptions: Vec<SubscriptionObservation>,
    pub items_scanned: u64,
    /// New progress markers to store back into `MailboxConfig.progress`.
    pub progress: Vec<FolderProgress>,
    /// Payments / renewals / price changes seen (merged by `merge::merge_payments`).
    pub payments: Vec<PaymentObservation>,
}

/// Options for [`scan_mailbox_with`].
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Ignore stored progress and rescan everything.
    pub full_rescan: bool,
    /// Lowercase list keys of unsubscribed lists: every mail of these lists becomes a
    /// `MailRef` (with raw header block) in `SubscriptionObservation.watched_mails`.
    pub watch_lists: Vec<String>,
    /// Fetch bodies of receipt / renewal mails (cap 200 KB) to extract amount, plan and
    /// payment method (card brand + last 4 only). Without it only subjects are used.
    pub read_receipt_bodies: bool,
}

const BATCH: usize = 500;
const HEADER_ITEMS: &str = "(UID INTERNALDATE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID TO DELIVERED-TO LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST LIST-ID PRECEDENCE SENDER RETURN-PATH)])";
const BODY_ITEMS: &str = "(UID BODY.PEEK[HEADER.FIELDS (CONTENT-TYPE CONTENT-TRANSFER-ENCODING)] BODY.PEEK[TEXT]<0.204800>)";
/// Max evidence entries kept per (domain, kind): the oldest plus the newest ones.
const MAX_EVIDENCE: usize = 6;

fn has_attr(f: &Folder, attr: &str) -> bool {
    f.attrs.iter().any(|a| a == attr)
}

/// Folders to scan: the configured ones, else `\All` (Gmail), else INBOX + `\Archive`.
/// Trash, Drafts, Sent and Junk are never picked automatically.
fn select_folders(cfg: &MailboxConfig, all: &[Folder]) -> Vec<String> {
    if !cfg.folders.is_empty() {
        return cfg.folders.clone();
    }
    let usable: Vec<&Folder> = all
        .iter()
        .filter(|f| !has_attr(f, "\\noselect") && !has_attr(f, "\\nonexistent"))
        .filter(|f| {
            !["\\trash", "\\drafts", "\\sent", "\\junk"]
                .iter()
                .any(|a| has_attr(f, a))
        })
        .collect();
    if let Some(a) = usable.iter().find(|f| has_attr(f, "\\all")) {
        return vec![a.name.clone()];
    }
    let mut out = vec![usable
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case("INBOX"))
        .map(|f| f.name.clone())
        .unwrap_or_else(|| "INBOX".to_string())];
    for f in usable.iter().filter(|f| has_attr(f, "\\archive")) {
        if !out.contains(&f.name) {
            out.push(f.name.clone());
        }
    }
    out
}

fn parse_date(raw: &str) -> Option<DateTime<Utc>> {
    let ts = mailparse::dateparse(raw.trim()).ok()?;
    let d = DateTime::from_timestamp(ts, 0)?;
    // Spam with dates far in the future would distort first/last seen.
    (d.timestamp() > 315_532_800 && d <= Utc::now() + chrono::Duration::days(2)).then_some(d)
}

fn parse_internal_date(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    DateTime::parse_from_str(raw, "%d-%b-%Y %H:%M:%S %z")
        .or_else(|_| DateTime::parse_from_str(raw, "%e-%b-%Y %H:%M:%S %z"))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Build a [`MailHeader`] from the fetched header block.
fn parse_header_block(bytes: &[u8]) -> Option<MailHeader> {
    let (headers, _) = mailparse::parse_headers(bytes).ok()?;
    let get = |name: &str| -> Option<String> {
        headers
            .get_first_header(name)
            .map(|h| {
                String::from_utf8_lossy(h.get_value_raw())
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
    };
    let to_header = get("To");
    let delivered_to = get("Delivered-To");
    // Apple relay addresses show up in To; otherwise Delivered-To is the real recipient.
    let to = match (&to_header, &delivered_to) {
        (Some(t), _) if t.to_ascii_lowercase().contains("@privaterelay.appleid.com") => t.clone(),
        (_, Some(d)) => d.clone(),
        (Some(t), None) => t.clone(),
        (None, None) => String::new(),
    };
    let list_unsubscribe = get("List-Unsubscribe");
    Some(MailHeader {
        from: get("From").unwrap_or_default(),
        subject: get("Subject").unwrap_or_default(),
        date: get("Date").as_deref().and_then(parse_date),
        message_id: get("Message-ID"),
        to,
        has_list_unsubscribe: list_unsubscribe.is_some(),
        list_unsubscribe,
        list_unsubscribe_post: get("List-Unsubscribe-Post"),
        list_id: get("List-Id"),
        precedence: get("Precedence"),
        sender: get("Sender").or_else(|| get("Return-Path")),
    })
}

/// Decode the (possibly multipart / encoded) body into text, preferring text/plain.
fn decode_body_text(raw: &[u8]) -> String {
    fn collect(part: &mailparse::ParsedMail, plain: &mut String, html: &mut String) {
        if part.subparts.is_empty() {
            let mime = part.ctype.mimetype.to_ascii_lowercase();
            let is_plain = mime == "text/plain";
            if is_plain || mime == "text/html" {
                if let Ok(body) = part.get_body() {
                    let t = if is_plain { &mut *plain } else { &mut *html };
                    t.push_str(&body);
                    t.push('\n');
                }
            }
        }
        for sub in &part.subparts {
            collect(sub, plain, html);
        }
    }
    match mailparse::parse_mail(raw) {
        Ok(mail) => {
            let (mut plain, mut html) = (String::new(), String::new());
            collect(&mail, &mut plain, &mut html);
            if !plain.trim().is_empty() {
                plain
            } else if !html.trim().is_empty() {
                html
            } else {
                String::from_utf8_lossy(raw).into_owned()
            }
        }
        Err(_) => String::from_utf8_lossy(raw).into_owned(),
    }
}

fn fetch_sso_text(client: &mut Client, uid: u32) -> Option<String> {
    let items = client.uid_fetch(&uid.to_string(), BODY_ITEMS).ok()?;
    let item = items.into_iter().find(|i| i.uid == Some(uid))?;
    let mut raw = item.header.unwrap_or_default();
    if !raw.ends_with(b"\r\n\r\n") {
        raw.extend_from_slice(b"\r\n\r\n");
    }
    raw.extend_from_slice(&item.body?);
    Some(decode_body_text(&raw))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

// ------------------------------------------------------------------------------------------
// Aggregation
// ------------------------------------------------------------------------------------------

struct SubAgg {
    sender: String,
    sender_name: Option<String>,
    domain: String,
    unsubscribe: Vec<UnsubscribeMethod>,
    unsubscribe_date: Option<DateTime<Utc>>,
    count: u64,
    first: Option<DateTime<Utc>>,
    last: Option<DateTime<Utc>>,
    subjects: Vec<(Option<DateTime<Utc>>, String)>,
    categories: HashMap<MailCategory, u32>,
    watched: Vec<MailRef>,
}

/// Max watched mails (evidence) kept per list and scan.
const MAX_WATCHED: usize = 200;

impl SubAgg {
    fn into_observation(self, mailbox: &str, list_key: String) -> SubscriptionObservation {
        let mut watched = self.watched;
        watched.sort_by_key(|m| m.date);
        let mut subjects = self.subjects;
        subjects.sort_by_key(|a| std::cmp::Reverse(a.0));
        let sample_subjects = subjects.into_iter().map(|(_, s)| s).take(5).collect();
        let category = self
            .categories
            .iter()
            .filter(|(c, _)| **c != MailCategory::Unknown)
            .max_by_key(|(_, n)| **n)
            .map(|(c, _)| *c)
            .unwrap_or(MailCategory::Unknown);
        SubscriptionObservation {
            mailbox: mailbox.to_string(),
            list_key,
            sender: self.sender,
            sender_name: self.sender_name,
            domain: self.domain,
            unsubscribe: self.unsubscribe,
            message_count: self.count,
            first_seen: self.first,
            last_seen: self.last,
            sample_subjects,
            category,
            watched_mails: watched,
        }
    }
}

struct ScanState {
    mailbox: String,
    findings: HashMap<(String, EvidenceKind), Vec<Finding>>,
    subs: HashMap<String, SubAgg>,
    seen: HashSet<String>,
    classified: u64,
    payments: Vec<PaymentObservation>,
    /// Lowercase list keys whose mails are kept as `MailRef` evidence.
    watch: HashSet<String>,
}

struct Ingested {
    needs_body: bool,
    source_ref: String,
    /// Set if the subject / sender looks like a receipt, renewal, price change ...
    payment_kind: Option<PaymentKind>,
}

/// Subject phrases of one-time-code / second-factor mails (folded, see [`classify::fold`]).
const TWO_FACTOR: &[&str] = &[
    "bestaetigungscode",
    "verification code",
    "anmeldecode",
    "login code",
    "login-code",
    "log-in code",
    "sign-in code",
    "signin code",
    "security code",
    "sicherheitscode",
    "one-time code",
    "one-time password",
    "one time code",
    "einmalcode",
    "einmal-code",
    "einmalpasswort",
    "2-step verification",
    "2-schritt",
    "zwei-faktor",
    "2-faktor",
    "two-factor",
    "two factor",
    "2fa",
    "your code is",
    "dein code",
    "ihr code",
    "code de verification",
    "code de securite",
    "code de connexion",
    "codigo de verificacion",
    "codigo de seguridad",
    "codigo de acceso",
    "codice di verifica",
    "verificatiecode",
    "inlogcode",
];

fn is_two_factor(subject: &str) -> bool {
    let f = classify::fold(&classify::decode_header(subject));
    TWO_FACTOR.iter().any(|k| f.contains(k))
}

impl ScanState {
    #[cfg(test)]
    fn new(mailbox: &str) -> Self {
        Self::with_options(mailbox, &ScanOptions::default())
    }

    fn with_options(mailbox: &str, options: &ScanOptions) -> Self {
        ScanState {
            mailbox: mailbox.to_string(),
            findings: HashMap::new(),
            subs: HashMap::new(),
            seen: HashSet::new(),
            classified: 0,
            payments: Vec::new(),
            watch: options
                .watch_lists
                .iter()
                .map(|k| k.trim().to_lowercase())
                .collect(),
        }
    }

    fn evidence(&self, kind: EvidenceKind, h: &MailHeader, source_ref: &str) -> Evidence {
        let subject = classify::decode_header(&h.subject);
        Evidence {
            kind,
            source: SourceKind::Mail {
                mailbox: self.mailbox.clone(),
            },
            summary: if subject.is_empty() {
                "(ohne Betreff)".into()
            } else {
                truncate(&subject, 120)
            },
            date: h.date,
            source_ref: Some(source_ref.to_string()),
        }
    }

    fn push_finding(&mut self, c: classify::Classification, h: &MailHeader, source_ref: &str) {
        let finding = Finding {
            domain: c.domain.clone(),
            name: c.name,
            identifier: c.identifier,
            sso: c.sso,
            evidence: self.evidence(c.kind, h, source_ref),
        };
        self.classified += 1;
        let list = self.findings.entry((c.domain, c.kind)).or_default();
        list.push(finding);
        if list.len() > MAX_EVIDENCE {
            // Keep the oldest and the newest evidence; drop the second oldest.
            list.sort_by_key(|a| a.evidence.date);
            list.remove(1);
        }
    }

    /// Process one mail's headers. Returns `None` for a duplicate (same Message-ID).
    #[cfg(test)]
    fn ingest(&mut self, h: &MailHeader, fallback_ref: &str, own: &str) -> Option<Ingested> {
        self.ingest_raw(h, fallback_ref, own, "")
    }

    /// Like [`Self::ingest`]; `raw_headers` is the fetched header block (kept as evidence
    /// for watched lists).
    fn ingest_raw(
        &mut self,
        h: &MailHeader,
        fallback_ref: &str,
        own: &str,
        raw_headers: &str,
    ) -> Option<Ingested> {
        let source_ref = match h
            .message_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(id) => {
                // Scoped per sender: colliding ids of unrelated senders are different mails.
                let sender = classify::parse_address(&h.from).1.unwrap_or_default();
                if !self.seen.insert(format!("{id}|{sender}")) {
                    return None;
                }
                id.to_string()
            }
            None => fallback_ref.to_string(),
        };
        if let Some(mut c) = classify::classify_for(h, Some(own)) {
            // One-time codes / 2FA mails are second-factor evidence, plain alerts stay alerts.
            if matches!(
                c.kind,
                EvidenceKind::LoginAlert | EvidenceKind::Verification
            ) && is_two_factor(&h.subject)
            {
                c.kind = EvidenceKind::TwoFactor;
            }
            self.push_finding(c, h, &source_ref);
        }
        let payment_kind = receipts::payment_kind(h);
        if payment_kind.is_some() {
            let lines = receipts::extract_from_subject(h);
            self.add_payments(h, &lines, &source_ref, own);
        }
        if classify::is_bulk(h) {
            self.aggregate_subscription(h, own, raw_headers);
        }
        Some(Ingested {
            needs_body: classify::needs_body(h),
            source_ref,
            payment_kind,
        })
    }

    fn ingest_sso(&mut self, h: &MailHeader, body: &str, source_ref: &str) {
        if let Some(c) = classify::classify_sso(h, body) {
            self.push_finding(c, h, source_ref);
        }
    }

    /// Replace the subject-derived payments of a mail by the ones found in its body.
    /// Gaps in the body result (e.g. no amount) are filled from the subject.
    fn apply_receipt_body(&mut self, h: &MailHeader, body: &str, source_ref: &str, own: &str) {
        let mut lines = receipts::extract_from_body(h, body);
        if lines.is_empty() {
            return;
        }
        let subject = receipts::extract_from_subject(h);
        if let ([line], [subj]) = (lines.as_mut_slice(), subject.as_slice()) {
            if line.amount_minor.is_none() {
                line.amount_minor = subj.amount_minor;
                line.currency = subj.currency.clone();
            }
        }
        self.remove_payments(source_ref);
        self.add_payments(h, &lines, source_ref, own);
    }

    fn remove_payments(&mut self, source_ref: &str) {
        let prefix = format!("{source_ref}#");
        let matches =
            |r: Option<&str>| r.is_some_and(|r| r == source_ref || r.starts_with(&prefix));
        self.payments
            .retain(|p| !matches(p.record.source_ref.as_deref()));
        for ((_, kind), list) in self.findings.iter_mut() {
            if *kind == EvidenceKind::Payment {
                list.retain(|f| !matches(f.evidence.source_ref.as_deref()));
            }
        }
        self.findings.retain(|_, l| !l.is_empty());
    }

    /// Turn receipt lines into `PaymentObservation`s plus `Payment` evidence for the merchant.
    fn add_payments(
        &mut self,
        h: &MailHeader,
        lines: &[receipts::ReceiptLine],
        source_ref: &str,
        own: &str,
    ) {
        let (_, from) = classify::parse_address(&h.from);
        let Some(from) = from else { return };
        let host = classify::host_of(&from);
        if from.eq_ignore_ascii_case(own.trim())
            || !host.contains('.')
            || classify::is_freemail(host)
        {
            return;
        }
        let subject = classify::decode_header(&h.subject);
        let identifier = classify::parse_address(&h.to).1;
        for (i, line) in lines.iter().enumerate() {
            let Some(domain) = line
                .merchant_domain
                .clone()
                .filter(|d| !d.trim().is_empty())
            else {
                continue;
            };
            let line_ref = if line.via.is_some() {
                format!("{source_ref}#{i}")
            } else {
                source_ref.to_string()
            };
            let summary = match (&line.merchant_name, &line.plan, line.via.is_some()) {
                (Some(n), Some(p), true) => format!("{n} – {p}"),
                (Some(n), None, true) if lines.len() > 1 => n.clone(),
                _ if subject.is_empty() => "(ohne Betreff)".to_string(),
                _ => subject.clone(),
            };
            self.payments.push(PaymentObservation {
                domain: domain.clone(),
                name: line.merchant_name.clone(),
                identifier: identifier.clone(),
                source: SourceKind::Mail {
                    mailbox: self.mailbox.clone(),
                },
                record: PaymentRecord {
                    date: h.date,
                    kind: line.kind,
                    amount_minor: line.amount_minor,
                    currency: line.currency.clone(),
                    plan: line.plan.clone(),
                    method: line.method.clone(),
                    via: line.via.clone(),
                    summary: truncate(&summary, 140),
                    source_ref: Some(line_ref.clone()),
                },
            });
            self.push_finding(
                classify::Classification {
                    kind: EvidenceKind::Payment,
                    domain,
                    name: line.merchant_name.clone(),
                    sso: None,
                    identifier: identifier.clone(),
                },
                h,
                &line_ref,
            );
        }
    }

    fn aggregate_subscription(&mut self, h: &MailHeader, own: &str, raw_headers: &str) {
        let (name, addr) = classify::parse_address(&h.from);
        let Some(addr) = addr else { return };
        if addr.eq_ignore_ascii_case(own.trim()) {
            return;
        }
        let list_key = h
            .list_id
            .as_deref()
            .map(|l| {
                let l = match (l.find('<'), l.rfind('>')) {
                    (Some(a), Some(b)) if b > a => &l[a + 1..b],
                    _ => l,
                };
                l.trim().to_lowercase()
            })
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| addr.clone());
        let domain = addr
            .rsplit_once('@')
            .map(|(_, d)| d.to_string())
            .unwrap_or_default();
        let methods = h
            .list_unsubscribe
            .as_deref()
            .map(|v| unsubscribe::parse_list_unsubscribe(v, h.list_unsubscribe_post.as_deref()))
            .unwrap_or_default();
        let category = classify::categorize(h);
        let subject = classify::decode_header(&h.subject);

        let watched = self.watch.contains(&list_key);
        let agg = self.subs.entry(list_key).or_insert_with(|| SubAgg {
            sender: addr.clone(),
            sender_name: name.clone(),
            domain: domain.clone(),
            unsubscribe: Vec::new(),
            unsubscribe_date: None,
            count: 0,
            first: None,
            last: None,
            subjects: Vec::new(),
            categories: HashMap::new(),
            watched: Vec::new(),
        });
        let newest = match (h.date, agg.last) {
            (Some(d), Some(l)) => d >= l,
            (Some(_), None) => true,
            (None, _) => agg.count == 0,
        };
        agg.count += 1;
        if let Some(d) = h.date {
            agg.first = Some(agg.first.map_or(d, |f| f.min(d)));
            agg.last = Some(agg.last.map_or(d, |l| l.max(d)));
        }
        if newest {
            agg.sender = addr;
            agg.sender_name = name.or_else(|| agg.sender_name.take());
            agg.domain = domain;
        }
        if !methods.is_empty() {
            let newer_methods = match (h.date, agg.unsubscribe_date) {
                (Some(d), Some(u)) => d >= u,
                (Some(_), None) => true,
                (None, _) => agg.unsubscribe.is_empty(),
            };
            if newer_methods {
                agg.unsubscribe = methods;
                agg.unsubscribe_date = h.date;
            }
        }
        if !subject.is_empty() {
            let s = truncate(&subject, 140);
            if !agg.subjects.iter().any(|(_, x)| *x == s) {
                agg.subjects.push((h.date, s));
                if agg.subjects.len() > 20 {
                    agg.subjects.sort_by_key(|a| std::cmp::Reverse(a.0));
                    agg.subjects.truncate(5);
                }
            }
        }
        *agg.categories.entry(category).or_insert(0) += 1;
        if watched && agg.watched.len() < MAX_WATCHED {
            agg.watched.push(MailRef {
                date: h.date,
                from: classify::decode_header(&h.from),
                subject: subject.clone(),
                message_id: h
                    .message_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                raw_headers: raw_headers.trim_end().to_string(),
            });
        }
    }

    fn finish(self) -> (Vec<Finding>, Vec<SubscriptionObservation>) {
        let mut findings: Vec<Finding> = self.findings.into_values().flatten().collect();
        findings.sort_by(|a, b| {
            a.domain
                .cmp(&b.domain)
                .then(a.evidence.date.cmp(&b.evidence.date))
        });
        let mailbox = self.mailbox;
        let mut subs: Vec<SubscriptionObservation> = self
            .subs
            .into_iter()
            .map(|(key, agg)| agg.into_observation(&mailbox, key))
            .collect();
        subs.sort_by(|a, b| a.list_key.cmp(&b.list_key));
        (findings, subs)
    }
}

// ------------------------------------------------------------------------------------------
// Scan
// ------------------------------------------------------------------------------------------

fn check_cancel(cancel: &AtomicBool) -> Result<(), MailError> {
    if cancel.load(Ordering::Relaxed) {
        Err(MailError::Cancelled)
    } else {
        Ok(())
    }
}

/// Scan the configured folders. Incremental by default: only UIDs above the stored
/// progress (if UIDVALIDITY matches). `full_rescan` ignores progress.
/// Runs blocking; call from a worker thread. Checks `cancel` between batches.
pub fn scan_mailbox(
    cfg: &MailboxConfig,
    full_rescan: bool,
    on_event: &mut dyn FnMut(ScanEvent),
    cancel: &AtomicBool,
) -> Result<ScanOutcome, MailError> {
    let options = ScanOptions {
        full_rescan,
        ..Default::default()
    };
    scan_mailbox_with(cfg, &options, on_event, cancel)
}

/// Like [`scan_mailbox`] with all wave-2 options (payment extraction, watched lists).
pub fn scan_mailbox_with(
    cfg: &MailboxConfig,
    options: &ScanOptions,
    on_event: &mut dyn FnMut(ScanEvent),
    cancel: &AtomicBool,
) -> Result<ScanOutcome, MailError> {
    check_cancel(cancel)?;
    let mut client = connect_and_login(cfg)?;
    let result = scan_with_client(&mut client, cfg, options, on_event, cancel);
    client.logout();
    result
}

fn scan_with_client(
    client: &mut Client,
    cfg: &MailboxConfig,
    options: &ScanOptions,
    on_event: &mut dyn FnMut(ScanEvent),
    cancel: &AtomicBool,
) -> Result<ScanOutcome, MailError> {
    on_event(ScanEvent::Connected);
    let special_use = client
        .capability()
        .unwrap_or_default()
        .iter()
        .any(|c| c == "SPECIAL-USE");
    let folders = if cfg.folders.is_empty() {
        select_folders(cfg, &client.list_folders(special_use)?)
    } else {
        cfg.folders.clone()
    };

    let mut state = ScanState::with_options(&cfg.address, options);
    let mut progress = Vec::new();
    let mut items_scanned = 0u64;
    for folder in &folders {
        check_cancel(cancel)?;
        progress.push(scan_folder(
            client,
            cfg,
            folder,
            options,
            &mut state,
            &mut items_scanned,
            on_event,
            cancel,
        )?);
    }
    let mut payments = std::mem::take(&mut state.payments);
    payments.sort_by(|a, b| {
        a.record
            .date
            .cmp(&b.record.date)
            .then_with(|| a.record.source_ref.cmp(&b.record.source_ref))
    });
    let (findings, subscriptions) = state.finish();
    Ok(ScanOutcome {
        findings,
        subscriptions,
        items_scanned,
        progress,
        payments,
    })
}

#[allow(clippy::too_many_arguments)]
fn scan_folder(
    client: &mut Client,
    cfg: &MailboxConfig,
    folder: &str,
    options: &ScanOptions,
    state: &mut ScanState,
    items_scanned: &mut u64,
    on_event: &mut dyn FnMut(ScanEvent),
    cancel: &AtomicBool,
) -> Result<FolderProgress, MailError> {
    let ex = client.examine(folder)?;
    let prev = cfg.progress.iter().find(|p| p.folder == folder);
    let start = match prev {
        Some(p) if !options.full_rescan && p.uid_validity == ex.uid_validity => {
            p.last_uid.saturating_add(1)
        }
        _ => 1,
    };
    let uids = if ex.exists == 0 {
        Vec::new()
    } else {
        client.uid_search_from(start)?
    };
    let total = uids.len() as u64;
    on_event(ScanEvent::FolderStarted {
        folder: folder.to_string(),
        total,
    });

    let mut last_uid = start.saturating_sub(1);
    let mut done = 0u64;
    for batch in uids.chunks(BATCH) {
        check_cancel(cancel)?;
        let set = format!("{}:{}", batch[0], batch[batch.len() - 1]);
        let mut items = client.uid_fetch(&set, HEADER_ITEMS)?;
        items.sort_by_key(|i| i.uid);
        for item in items {
            let (Some(uid), Some(raw)) = (item.uid, item.header.as_deref()) else {
                continue;
            };
            let Some(mut header) = parse_header_block(raw) else {
                continue;
            };
            if header.date.is_none() {
                header.date = item.internal_date.as_deref().and_then(parse_internal_date);
            }
            *items_scanned += 1;
            let fallback = format!("{folder}/{}/{uid}", ex.uid_validity);
            let raw_text = String::from_utf8_lossy(raw);
            let Some(ing) = state.ingest_raw(&header, &fallback, &cfg.address, &raw_text) else {
                continue;
            };
            // Best effort: a failing body fetch never aborts the scan.
            let mut body: Option<Option<String>> = None;
            if ing.needs_body {
                let text = fetch_sso_text(client, uid);
                if let Some(text) = &text {
                    state.ingest_sso(&header, text, &ing.source_ref);
                }
                body = Some(text);
            }
            if options.read_receipt_bodies && ing.payment_kind.is_some() {
                let text = body.take().unwrap_or_else(|| fetch_sso_text(client, uid));
                if let Some(text) = text {
                    state.apply_receipt_body(&header, &text, &ing.source_ref, &cfg.address);
                }
            }
        }
        done += batch.len() as u64;
        last_uid = last_uid.max(batch[batch.len() - 1]);
        on_event(ScanEvent::Progress {
            folder: folder.to_string(),
            done,
            total,
            findings: state.classified,
        });
    }
    on_event(ScanEvent::FolderFinished {
        folder: folder.to_string(),
    });
    Ok(FolderProgress {
        folder: folder.to_string(),
        uid_validity: ex.uid_validity,
        last_uid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::thread;
    use uuid::Uuid;

    fn cfg_for(port: u16) -> MailboxConfig {
        MailboxConfig {
            id: Uuid::nil(),
            address: "me@example.org".into(),
            imap_host: "127.0.0.1".into(),
            imap_port: port,
            username: "me@example.org".into(),
            password: "secret".into(),
            folders: vec![],
            progress: vec![],
            last_scan: None,
        }
    }

    // ---- presets ---------------------------------------------------------------------

    #[test]
    fn presets_are_complete_and_unique() {
        let ids: HashSet<_> = presets().iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), presets().len());
        assert_eq!(presets().len(), 13);
        for p in presets() {
            assert!(!p.hint_de.is_empty() && !p.name.is_empty() && !p.domains.is_empty());
            assert!(p.imap_port == 993 || p.imap_port == 1143);
        }
    }

    #[test]
    fn preset_lookup_by_address() {
        assert_eq!(preset_for_address("Max@GMail.com").unwrap().id, "gmail");
        assert_eq!(preset_for_address("a@googlemail.com").unwrap().id, "gmail");
        assert_eq!(preset_for_address("a@gmx.de").unwrap().id, "gmx");
        assert_eq!(preset_for_address("a@web.de").unwrap().id, "webde");
        assert_eq!(preset_for_address("a@hotmail.com").unwrap().id, "outlook");
        assert_eq!(preset_for_address("a@proton.me").unwrap().imap_port, 1143);
        assert!(preset_for_address("a@firma.example").is_none());
        assert!(preset_for_address("no-at-sign").is_none());
        assert_eq!(
            preset_for_address("a@gmail.com").unwrap().app_password_url,
            Some("https://myaccount.google.com/apppasswords")
        );
    }

    // ---- folder selection --------------------------------------------------------------

    fn folder(name: &str, attrs: &[&str]) -> Folder {
        Folder {
            name: name.into(),
            attrs: attrs.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    #[test]
    fn folder_selection_prefers_all_mail() {
        let all = vec![
            folder("INBOX", &[]),
            folder("[Gmail]/All Mail", &["\\all"]),
            folder("[Gmail]/Spam", &["\\junk"]),
            folder("[Gmail]/Trash", &["\\trash"]),
        ];
        assert_eq!(select_folders(&cfg_for(1), &all), vec!["[Gmail]/All Mail"]);
    }

    #[test]
    fn folder_selection_inbox_and_archive() {
        let all = vec![
            folder("INBOX", &[]),
            folder("Archive", &["\\archive"]),
            folder("Sent", &["\\sent"]),
            folder("Trash", &["\\trash"]),
            folder("Drafts", &["\\drafts"]),
            folder("Junk", &["\\junk"]),
            folder("Shared", &["\\noselect"]),
        ];
        assert_eq!(select_folders(&cfg_for(1), &all), vec!["INBOX", "Archive"]);
    }

    #[test]
    fn folder_selection_configured_wins() {
        let mut cfg = cfg_for(1);
        cfg.folders = vec!["Newsletter".into()];
        assert_eq!(
            select_folders(&cfg, &[folder("INBOX", &[])]),
            vec!["Newsletter"]
        );
    }

    // ---- header parsing ------------------------------------------------------------------

    #[test]
    fn header_block_parsing() {
        let raw = b"From: =?UTF-8?Q?Shop_K=C3=B6ln?= <news@shop.example>\r\nSubject: =?UTF-8?Q?Gutschein?=\r\n =?UTF-8?Q?_f=C3=BCr_dich?=\r\nDate: Tue, 05 Mar 2024 10:00:00 +0100\r\nMessage-ID: <abc@shop.example>\r\nTo: Max <max@example.org>\r\nList-Unsubscribe: <https://shop.example/u/1>,\r\n <mailto:u@shop.example>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\nList-Id: <news.shop.example>\r\nPrecedence: bulk\r\nReturn-Path: <bounce@esp.example>\r\n\r\n";
        let h = parse_header_block(raw).unwrap();
        assert!(h.has_list_unsubscribe);
        assert_eq!(h.message_id.as_deref(), Some("<abc@shop.example>"));
        assert_eq!(h.date.unwrap().to_rfc3339(), "2024-03-05T09:00:00+00:00");
        assert_eq!(classify::decode_header(&h.subject), "Gutschein für dich");
        assert_eq!(h.to, "Max <max@example.org>");
        assert_eq!(h.sender.as_deref(), Some("<bounce@esp.example>"));
        assert_eq!(h.precedence.as_deref(), Some("bulk"));
    }

    #[test]
    fn delivered_to_preferred_but_apple_relay_kept() {
        let raw = b"From: a@b.example\r\nTo: xyz@privaterelay.appleid.com\r\nDelivered-To: me@example.org\r\n\r\n";
        assert_eq!(
            parse_header_block(raw).unwrap().to,
            "xyz@privaterelay.appleid.com"
        );
        let raw =
            b"From: a@b.example\r\nTo: list@x.example\r\nDelivered-To: me@example.org\r\n\r\n";
        assert_eq!(parse_header_block(raw).unwrap().to, "me@example.org");
    }

    #[test]
    fn future_dates_dropped_and_internal_date_parsed() {
        assert!(parse_date("Tue, 05 Mar 2999 10:00:00 +0000").is_none());
        assert!(parse_date("garbage").is_none());
        assert_eq!(
            parse_internal_date(" 7-Mar-2024 10:00:00 +0100")
                .unwrap()
                .to_rfc3339(),
            "2024-03-07T09:00:00+00:00"
        );
        assert!(parse_internal_date("nonsense").is_none());
    }

    #[test]
    fn body_decoding_multipart_qp() {
        let raw = b"Content-Type: multipart/alternative; boundary=\"b\"\r\n\r\n--b\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nNotion hat Zugriff auf Ihr Google-Konto erhalten=\r\n .\r\n--b\r\nContent-Type: text/html\r\n\r\n<p>x</p>\r\n--b--\r\n";
        let t = decode_body_text(raw);
        assert!(
            t.contains("Notion hat Zugriff auf Ihr Google-Konto erhalten"),
            "{t}"
        );
        assert!(!t.contains("<p>"));
    }

    // ---- aggregation -----------------------------------------------------------------------

    fn mail(from: &str, subject: &str, date: &str, id: &str) -> MailHeader {
        MailHeader {
            from: from.into(),
            subject: subject.into(),
            date: parse_date(date),
            message_id: Some(id.into()),
            to: "me@example.org".into(),
            ..Default::default()
        }
    }

    fn newsletter(subject: &str, date: &str, id: &str, unsub: &str) -> MailHeader {
        let mut h = mail("Shop News <news@shop.example>", subject, date, id);
        h.has_list_unsubscribe = true;
        h.list_unsubscribe = Some(unsub.into());
        h.list_id = Some("Shop <news.shop.example>".into());
        h
    }

    #[test]
    fn aggregates_subscriptions() {
        let mut st = ScanState::new("me@example.org");
        for (i, day) in (1..=7).enumerate() {
            let h = newsletter(
                &format!("Angebot {day}"),
                &format!("Mon, {day:02} Jan 2024 10:00:00 +0000"),
                &format!("<m{i}@shop.example>"),
                &format!("<https://shop.example/u/{day}>"),
            );
            st.ingest(&h, "f", "me@example.org").unwrap();
        }
        let (findings, subs) = st.finish();
        assert_eq!(subs.len(), 1);
        let s = &subs[0];
        assert_eq!(s.list_key, "news.shop.example");
        assert_eq!(s.message_count, 7);
        assert_eq!(s.sample_subjects.len(), 5);
        assert_eq!(s.sample_subjects[0], "Angebot 7");
        assert_eq!(
            s.first_seen.unwrap().to_rfc3339(),
            "2024-01-01T10:00:00+00:00"
        );
        assert_eq!(
            s.last_seen.unwrap().to_rfc3339(),
            "2024-01-07T10:00:00+00:00"
        );
        assert_eq!(s.category, MailCategory::Marketing);
        assert_eq!(s.sender, "news@shop.example");
        assert_eq!(s.sender_name.as_deref(), Some("Shop News"));
        assert_eq!(s.domain, "shop.example");
        assert_eq!(
            s.unsubscribe,
            vec![UnsubscribeMethod::Link {
                url: "https://shop.example/u/7".into()
            }]
        );
        // Newsletter evidence is capped, not one per mail.
        assert_eq!(findings.len(), MAX_EVIDENCE);
        assert!(findings
            .iter()
            .all(|f| f.evidence.kind == EvidenceKind::Newsletter));
        assert_eq!(
            findings[0].evidence.date.unwrap().to_rfc3339(),
            "2024-01-01T10:00:00+00:00"
        );
        assert_eq!(
            findings.last().unwrap().evidence.date.unwrap().to_rfc3339(),
            "2024-01-07T10:00:00+00:00"
        );
    }

    #[test]
    fn list_key_falls_back_to_sender_address() {
        let mut st = ScanState::new("me@example.org");
        let mut h = mail(
            "Foo <Hello@Foo.example>",
            "Hi",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<1@foo>",
        );
        h.precedence = Some("bulk".into());
        st.ingest(&h, "f", "me@example.org").unwrap();
        let (_, subs) = st.finish();
        assert_eq!(subs[0].list_key, "hello@foo.example");
        assert!(subs[0].unsubscribe.is_empty());
    }

    #[test]
    fn duplicate_message_ids_counted_once() {
        let mut st = ScanState::new("me@example.org");
        let h = newsletter(
            "Hi",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<same@x>",
            "<mailto:u@shop.example>",
        );
        assert!(st.ingest(&h, "f", "me@example.org").is_some());
        assert!(st.ingest(&h, "f", "me@example.org").is_none());
        let (_, subs) = st.finish();
        assert_eq!(subs[0].message_count, 1);
    }

    #[test]
    fn missing_message_id_uses_folder_uid_ref() {
        let mut st = ScanState::new("me@example.org");
        let mut h = mail(
            "Foo <noreply@foo.example>",
            "Willkommen bei Foo",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "",
        );
        h.message_id = None;
        let ing = st.ingest(&h, "INBOX/42/7", "me@example.org").unwrap();
        assert_eq!(ing.source_ref, "INBOX/42/7");
        let (findings, _) = st.finish();
        assert_eq!(
            findings[0].evidence.source_ref.as_deref(),
            Some("INBOX/42/7")
        );
        assert_eq!(findings[0].evidence.kind, EvidenceKind::Welcome);
        assert_eq!(
            findings[0].evidence.source,
            SourceKind::Mail {
                mailbox: "me@example.org".into()
            }
        );
    }

    #[test]
    fn own_mails_not_aggregated() {
        let mut st = ScanState::new("me@example.org");
        let mut h = mail(
            "Me <me@example.org>",
            "Notiz",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<o@x>",
        );
        h.precedence = Some("list".into());
        st.ingest(&h, "f", "me@example.org").unwrap();
        let (f, s) = st.finish();
        assert!(f.is_empty() && s.is_empty());
    }

    #[test]
    fn sso_body_becomes_grant_finding() {
        let mut st = ScanState::new("me@example.org");
        let h = mail(
            "Google <no-reply@accounts.google.com>",
            "Security alert",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<g@google>",
        );
        let ing = st.ingest(&h, "f", "me@example.org").unwrap();
        assert!(ing.needs_body);
        st.ingest_sso(
            &h,
            "Spotify was granted access to your Google Account",
            &ing.source_ref,
        );
        let (findings, _) = st.finish();
        let kinds: Vec<_> = findings.iter().map(|f| f.evidence.kind).collect();
        assert!(kinds.contains(&EvidenceKind::LoginAlert));
        let grant = findings
            .iter()
            .find(|f| f.evidence.kind == EvidenceKind::SsoGrant)
            .unwrap();
        assert_eq!(grant.domain, "unresolved:spotify");
        assert_eq!(grant.sso, Some(ausgeloggt_core::SsoProvider::Google));
    }

    // ---- end-to-end against a fake IMAP server -------------------------------------------------

    struct FakeMail {
        uid: u32,
        header: String,
        body: Option<String>,
    }

    fn fake_mails() -> Vec<FakeMail> {
        vec![
            FakeMail {
                uid: 10,
                header: "From: Spotify <no-reply@spotify.com>\r\nSubject: Willkommen bei Spotify\r\nDate: Mon, 01 Jan 2024 10:00:00 +0000\r\nMessage-ID: <w1@spotify.com>\r\nTo: me@example.org\r\n\r\n".into(),
                body: None,
            },
            FakeMail {
                uid: 11,
                header: "From: Shop <news@shop.example>\r\nSubject: 20% Rabatt\r\nDate: Tue, 02 Jan 2024 10:00:00 +0000\r\nMessage-ID: <n1@shop.example>\r\nTo: me@example.org\r\nList-Unsubscribe: <https://shop.example/u?id=1>, <mailto:unsub@shop.example>\r\nList-Unsubscribe-Post: List-Unsubscribe=One-Click\r\n\r\n".into(),
                body: None,
            },
            FakeMail {
                uid: 12,
                header: "From: Google <no-reply@accounts.google.com>\r\nSubject: Sicherheitswarnung\r\nDate: Wed, 03 Jan 2024 10:00:00 +0000\r\nMessage-ID: <s1@google.com>\r\nTo: me@example.org\r\n\r\n".into(),
                body: Some("Sicherheitswarnung\r\n\r\nNotion hat Zugriff auf Ihr Google-Konto erhalten\r\nhttps://www.notion.so/\r\n".into()),
            },
        ]
    }

    fn write_fetch(out: &mut impl Write, seq: usize, m: &FakeMail, body_request: bool) {
        if body_request {
            let hdr = "Content-Type: text/plain; charset=utf-8\r\n\r\n";
            let body = m.body.clone().unwrap_or_default();
            write!(
                out,
                "* {seq} FETCH (UID {} BODY[HEADER.FIELDS (CONTENT-TYPE CONTENT-TRANSFER-ENCODING)] {{{}}}\r\n{hdr} BODY[TEXT]<0> {{{}}}\r\n{body})\r\n",
                m.uid,
                hdr.len(),
                body.len()
            )
            .unwrap();
        } else {
            write!(
                out,
                "* {seq} FETCH (UID {} INTERNALDATE \"01-Jan-2024 10:00:00 +0000\" BODY[HEADER.FIELDS (FROM SUBJECT DATE MESSAGE-ID TO DELIVERED-TO LIST-UNSUBSCRIBE LIST-UNSUBSCRIBE-POST LIST-ID PRECEDENCE SENDER RETURN-PATH)] {{{}}}\r\n{})\r\n",
                m.uid,
                m.header.len(),
                m.header
            )
            .unwrap();
        }
    }

    /// One-shot fake IMAP server on loopback (plain, no STARTTLS). Returns the port.
    fn spawn_server(mails: Vec<FakeMail>, uidvalidity: u32) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut out = stream.try_clone().unwrap();
            let mut rd = BufReader::new(stream);
            out.write_all(b"* OK fake imap ready\r\n").unwrap();
            loop {
                let mut line = String::new();
                if rd.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let line = line.trim_end().to_string();
                let (tag, cmd) = line.split_once(' ').unwrap_or((&line, ""));
                let up = cmd.to_ascii_uppercase();
                if up.starts_with("CAPABILITY") {
                    write!(
                        out,
                        "* CAPABILITY IMAP4rev1 SPECIAL-USE\r\n{tag} OK done\r\n"
                    )
                    .unwrap();
                } else if up.starts_with("LOGIN") {
                    if cmd.contains("wrongpw") {
                        write!(
                            out,
                            "{tag} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n"
                        )
                        .unwrap();
                    } else {
                        write!(out, "{tag} OK logged in\r\n").unwrap();
                    }
                } else if up.starts_with("LIST") {
                    write!(
                        out,
                        "* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n* LIST (\\HasNoChildren \\Sent) \"/\" \"Sent\"\r\n* LIST (\\Noselect \\HasChildren) \"/\" \"[Gmail]\"\r\n{tag} OK done\r\n"
                    )
                    .unwrap();
                } else if up.starts_with("EXAMINE") {
                    write!(
                        out,
                        "* {} EXISTS\r\n* OK [UIDVALIDITY {uidvalidity}] ok\r\n* OK [UIDNEXT 13] ok\r\n{tag} OK [READ-ONLY] done\r\n",
                        mails.len()
                    )
                    .unwrap();
                } else if up.starts_with("UID SEARCH") {
                    let start: u32 = up
                        .split("UID ")
                        .nth(2)
                        .and_then(|s| s.split(':').next())
                        .and_then(|s| s.trim().parse().ok())
                        .unwrap_or(1);
                    let mut uids: Vec<u32> = mails
                        .iter()
                        .map(|m| m.uid)
                        .filter(|u| *u >= start)
                        .collect();
                    if uids.is_empty() {
                        // Real servers return the highest UID for `n:*` even if below n.
                        uids.push(mails.last().unwrap().uid);
                    }
                    let list: Vec<String> = uids.iter().map(u32::to_string).collect();
                    write!(out, "* SEARCH {}\r\n{tag} OK done\r\n", list.join(" ")).unwrap();
                } else if up.starts_with("UID FETCH") {
                    let set = up.split_whitespace().nth(2).unwrap_or("");
                    let (a, b) = set.split_once(':').unwrap_or((set, set));
                    let (a, b): (u32, u32) =
                        (a.parse().unwrap_or(0), b.parse().unwrap_or(u32::MAX));
                    let body_request = up.contains("BODY.PEEK[TEXT]");
                    assert!(
                        up.contains("BODY.PEEK"),
                        "must never fetch without PEEK: {up}"
                    );
                    for (i, m) in mails.iter().enumerate() {
                        if m.uid >= a && m.uid <= b {
                            write_fetch(&mut out, i + 1, m, body_request);
                        }
                    }
                    write!(out, "{tag} OK done\r\n").unwrap();
                } else if up.starts_with("LOGOUT") {
                    write!(out, "* BYE bye\r\n{tag} OK logout\r\n").unwrap();
                    return;
                } else {
                    write!(out, "{tag} BAD unknown\r\n").unwrap();
                }
            }
        });
        port
    }

    #[test]
    fn end_to_end_scan() {
        let port = spawn_server(fake_mails(), 777);
        let cfg = cfg_for(port);
        let mut events = Vec::new();
        let cancel = AtomicBool::new(false);
        let out = scan_mailbox(&cfg, false, &mut |e| events.push(e), &cancel).unwrap();

        assert_eq!(out.items_scanned, 3);
        assert_eq!(
            out.progress,
            vec![FolderProgress {
                folder: "INBOX".into(),
                uid_validity: 777,
                last_uid: 12
            }]
        );
        assert_eq!(events.first(), Some(&ScanEvent::Connected));
        assert!(matches!(
            events[1],
            ScanEvent::FolderStarted { total: 3, .. }
        ));
        assert!(matches!(
            events[2],
            ScanEvent::Progress {
                done: 3,
                total: 3,
                ..
            }
        ));
        assert!(matches!(
            events.last(),
            Some(ScanEvent::FolderFinished { .. })
        ));

        let welcome = out
            .findings
            .iter()
            .find(|f| f.evidence.kind == EvidenceKind::Welcome)
            .unwrap();
        assert_eq!(welcome.domain, "spotify.com");
        assert_eq!(welcome.identifier.as_deref(), Some("me@example.org"));
        assert_eq!(
            welcome.evidence.source_ref.as_deref(),
            Some("<w1@spotify.com>")
        );
        let grant = out
            .findings
            .iter()
            .find(|f| f.evidence.kind == EvidenceKind::SsoGrant)
            .unwrap();
        assert_eq!(grant.domain, "www.notion.so");
        assert_eq!(grant.name.as_deref(), Some("Notion"));
        assert_eq!(out.subscriptions.len(), 1);
        let sub = &out.subscriptions[0];
        assert_eq!(sub.list_key, "news@shop.example");
        assert_eq!(sub.category, MailCategory::Marketing);
        assert_eq!(sub.unsubscribe.len(), 2);
        assert!(matches!(
            sub.unsubscribe[0],
            UnsubscribeMethod::OneClick { .. }
        ));
    }

    #[test]
    fn incremental_scan_skips_known_uids() {
        let port = spawn_server(fake_mails(), 777);
        let mut cfg = cfg_for(port);
        cfg.progress = vec![FolderProgress {
            folder: "INBOX".into(),
            uid_validity: 777,
            last_uid: 12,
        }];
        let cancel = AtomicBool::new(false);
        let out = scan_mailbox(&cfg, false, &mut |_| {}, &cancel).unwrap();
        assert_eq!(out.items_scanned, 0);
        assert_eq!(out.progress[0].last_uid, 12);
    }

    #[test]
    fn incremental_scan_picks_up_new_uids_only() {
        let port = spawn_server(fake_mails(), 777);
        let mut cfg = cfg_for(port);
        cfg.progress = vec![FolderProgress {
            folder: "INBOX".into(),
            uid_validity: 777,
            last_uid: 10,
        }];
        let cancel = AtomicBool::new(false);
        let out = scan_mailbox(&cfg, false, &mut |_| {}, &cancel).unwrap();
        assert_eq!(out.items_scanned, 2);
        assert!(out.findings.iter().all(|f| f.domain != "spotify.com"));
    }

    #[test]
    fn changed_uidvalidity_or_full_rescan_starts_over() {
        let cancel = AtomicBool::new(false);
        let port = spawn_server(fake_mails(), 999);
        let mut cfg = cfg_for(port);
        cfg.progress = vec![FolderProgress {
            folder: "INBOX".into(),
            uid_validity: 777,
            last_uid: 12,
        }];
        let out = scan_mailbox(&cfg, false, &mut |_| {}, &cancel).unwrap();
        assert_eq!(out.items_scanned, 3);
        assert_eq!(out.progress[0].uid_validity, 999);

        let port = spawn_server(fake_mails(), 777);
        let mut cfg = cfg_for(port);
        cfg.progress = vec![FolderProgress {
            folder: "INBOX".into(),
            uid_validity: 777,
            last_uid: 12,
        }];
        let out = scan_mailbox(&cfg, true, &mut |_| {}, &cancel).unwrap();
        assert_eq!(out.items_scanned, 3);
    }

    #[test]
    fn wrong_password_is_auth_error() {
        let port = spawn_server(fake_mails(), 1);
        let mut cfg = cfg_for(port);
        cfg.password = "wrongpw".into();
        let cancel = AtomicBool::new(false);
        let err = scan_mailbox(&cfg, false, &mut |_| {}, &cancel).unwrap_err();
        assert!(matches!(err, MailError::Auth), "{err:?}");
    }

    #[test]
    fn cancel_before_start() {
        let cancel = AtomicBool::new(true);
        let err = scan_mailbox(&cfg_for(1), false, &mut |_| {}, &cancel).unwrap_err();
        assert!(matches!(err, MailError::Cancelled));
    }

    // ---- wave 2: payments, two-factor, watched lists ---------------------------------------------

    #[test]
    fn two_factor_vs_login_alert() {
        let mut st = ScanState::new("me@example.org");
        let code = mail(
            "Service <login@service.example>",
            "Dein Bestätigungscode",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<c1@service.example>",
        );
        let alert = mail(
            "Service <login@service.example>",
            "Neue Anmeldung auf einem Gerät",
            "Tue, 02 Jan 2024 10:00:00 +0000",
            "<c2@service.example>",
        );
        let two_step = mail(
            "Google <no-reply@accounts.google.com>",
            "2-Step Verification turned on",
            "Wed, 03 Jan 2024 10:00:00 +0000",
            "<c3@google.com>",
        );
        for h in [&code, &alert, &two_step] {
            st.ingest(h, "f", "me@example.org").unwrap();
        }
        let (findings, _) = st.finish();
        let kind_of = |id: &str| {
            findings
                .iter()
                .find(|f| f.evidence.source_ref.as_deref() == Some(id))
                .map(|f| f.evidence.kind)
        };
        assert_eq!(
            kind_of("<c1@service.example>"),
            Some(EvidenceKind::TwoFactor)
        );
        assert_eq!(
            kind_of("<c2@service.example>"),
            Some(EvidenceKind::LoginAlert)
        );
        assert_eq!(kind_of("<c3@google.com>"), Some(EvidenceKind::TwoFactor));
    }

    #[test]
    fn subject_payment_creates_observation_and_evidence() {
        let mut st = ScanState::new("me@example.org");
        let h = mail(
            "Spotify <no-reply@spotify.com>",
            "Your receipt from Spotify – €10.99",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<p1@spotify.com>",
        );
        let ing = st.ingest(&h, "f", "me@example.org").unwrap();
        assert_eq!(ing.payment_kind, Some(PaymentKind::Charge));
        assert_eq!(st.payments.len(), 1);
        let p = &st.payments[0];
        assert_eq!(p.domain, "spotify.com");
        assert_eq!(p.record.amount_minor, Some(1099));
        assert_eq!(p.record.source_ref.as_deref(), Some("<p1@spotify.com>"));
        assert_eq!(p.identifier.as_deref(), Some("me@example.org"));
        let (findings, _) = st.finish();
        assert!(findings
            .iter()
            .any(|f| f.domain == "spotify.com" && f.evidence.kind == EvidenceKind::Payment));
    }

    #[test]
    fn freemail_and_own_senders_yield_no_payments() {
        let mut st = ScanState::new("me@example.org");
        for (from, id) in [
            ("Max <max@gmail.com>", "<a@x>"),
            ("Me <me@example.org>", "<b@x>"),
        ] {
            let h = mail(from, "Rechnung", "Mon, 01 Jan 2024 10:00:00 +0000", id);
            st.ingest(&h, "f", "me@example.org").unwrap();
        }
        assert!(st.payments.is_empty());
    }

    const APPLE_BODY: &str = "Deine Rechnung von Apple.\nABGERECHNET AN\tVisa •••• 1234\nSpotify\nSpotify Premium (1 Monat)\nVerlängert sich am 12.04.2024\n9,99 €\nDisney+\nDisney+ Standard (Monatlich)\n8,99 €\nSUMME\t18,98 €\n";

    #[test]
    fn body_replaces_subject_payments() {
        let mut st = ScanState::new("me@example.org");
        let h = mail(
            "Apple <no_reply@email.apple.com>",
            "Deine Rechnung von Apple.",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<r@apple>",
        );
        let ing = st.ingest(&h, "f", "me@example.org").unwrap();
        assert_eq!(st.payments.len(), 1);
        assert_eq!(st.payments[0].domain, "apple.com");
        st.apply_receipt_body(&h, APPLE_BODY, &ing.source_ref, "me@example.org");
        let domains: Vec<_> = st.payments.iter().map(|p| p.domain.as_str()).collect();
        assert_eq!(domains, vec!["unresolved:spotify", "unresolved:disney"]);
        let refs: Vec<_> = st
            .payments
            .iter()
            .map(|p| p.record.source_ref.clone().unwrap())
            .collect();
        assert_eq!(refs, vec!["<r@apple>#0", "<r@apple>#1"]);
        assert_eq!(
            st.payments[0].record.method,
            Some(ausgeloggt_core::PaymentMethod::Card {
                brand: "Visa".into(),
                last4: "1234".into()
            })
        );
        assert_eq!(st.payments[0].record.via.as_deref(), Some("apple.com"));
        let (findings, _) = st.finish();
        let pay: Vec<_> = findings
            .iter()
            .filter(|f| f.evidence.kind == EvidenceKind::Payment)
            .map(|f| f.domain.as_str())
            .collect();
        assert_eq!(pay, vec!["unresolved:disney", "unresolved:spotify"]);
    }

    #[test]
    fn empty_body_result_keeps_subject_payment() {
        let mut st = ScanState::new("me@example.org");
        let h = mail(
            "Spotify <no-reply@spotify.com>",
            "Your receipt from Spotify – €10.99",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<p2@spotify.com>",
        );
        let ing = st.ingest(&h, "f", "me@example.org").unwrap();
        st.apply_receipt_body(&h, "Thanks!", &ing.source_ref, "me@example.org");
        assert_eq!(st.payments.len(), 1);
        // The body had no amount: the subject's amount fills the gap.
        assert_eq!(st.payments[0].record.amount_minor, Some(1099));
    }

    #[test]
    fn watched_lists_collect_mail_refs_capped() {
        let opts = ScanOptions {
            watch_lists: vec!["News.Shop.Example".into()],
            ..Default::default()
        };
        let mut st = ScanState::with_options("me@example.org", &opts);
        for i in 0..(MAX_WATCHED + 20) {
            let h = newsletter(
                &format!("Angebot {i}"),
                "Mon, 01 Jan 2024 10:00:00 +0000",
                &format!("<w{i}@shop.example>"),
                "<mailto:u@shop.example>",
            );
            st.ingest_raw(
                &h,
                "f",
                "me@example.org",
                "From: Shop\r\nSubject: x\r\n\r\n",
            )
            .unwrap();
        }
        // A second, unwatched list.
        let mut other = mail(
            "Other <o@other.example>",
            "Hi",
            "Mon, 01 Jan 2024 10:00:00 +0000",
            "<o1@other.example>",
        );
        other.precedence = Some("bulk".into());
        st.ingest_raw(&other, "f", "me@example.org", "From: Other\r\n\r\n")
            .unwrap();
        let (_, subs) = st.finish();
        let shop = subs
            .iter()
            .find(|s| s.list_key == "news.shop.example")
            .unwrap();
        assert_eq!(shop.message_count as usize, MAX_WATCHED + 20);
        assert_eq!(shop.watched_mails.len(), MAX_WATCHED);
        let m = &shop.watched_mails[0];
        assert_eq!(m.from, "Shop News <news@shop.example>");
        assert!(m.subject.starts_with("Angebot"));
        assert!(m.message_id.as_deref().unwrap().ends_with("@shop.example>"));
        assert!(m.raw_headers.starts_with("From: Shop"));
        let other = subs
            .iter()
            .find(|s| s.list_key == "o@other.example")
            .unwrap();
        assert!(other.watched_mails.is_empty());
    }

    fn wave2_mails() -> Vec<FakeMail> {
        let apple_body = APPLE_BODY.replace('\n', "\r\n");
        let spotify_body = "Danke für deine Zahlung!\r\nPlan: Premium Family\r\nGesamt 16,80 €\r\nBezahlt mit Visa ending in 4242\r\n";
        vec![
            FakeMail {
                uid: 20,
                header: "From: Apple <no_reply@email.apple.com>\r\nSubject: Deine Rechnung von Apple.\r\nDate: Mon, 04 Mar 2024 10:00:00 +0000\r\nMessage-ID: <r1@apple.com>\r\nTo: me@example.org\r\n\r\n".into(),
                body: Some(apple_body),
            },
            FakeMail {
                uid: 21,
                header: "From: Spotify <no-reply@spotify.com>\r\nSubject: Deine Rechnung von Spotify\r\nDate: Tue, 05 Mar 2024 10:00:00 +0000\r\nMessage-ID: <r2@spotify.com>\r\nTo: me@example.org\r\n\r\n".into(),
                body: Some(spotify_body.into()),
            },
            FakeMail {
                uid: 22,
                header: "From: Shop <news@shop.example>\r\nSubject: 20% Rabatt\r\nDate: Wed, 06 Mar 2024 10:00:00 +0000\r\nMessage-ID: <n1@shop.example>\r\nTo: me@example.org\r\nList-Id: <news.shop.example>\r\nList-Unsubscribe: <mailto:unsub@shop.example>\r\n\r\n".into(),
                body: None,
            },
            FakeMail {
                uid: 23,
                header: "From: Service <login@service.example>\r\nSubject: Dein Bestätigungscode\r\nDate: Thu, 07 Mar 2024 10:00:00 +0000\r\nMessage-ID: <c1@service.example>\r\nTo: me@example.org\r\n\r\n".into(),
                body: None,
            },
        ]
    }

    #[test]
    fn end_to_end_receipts_subject_only() {
        let port = spawn_server(wave2_mails(), 5);
        let opts = ScanOptions {
            watch_lists: vec!["news.shop.example".into()],
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let out = scan_mailbox_with(&cfg_for(port), &opts, &mut |_| {}, &cancel).unwrap();
        assert_eq!(out.items_scanned, 4);
        // Subjects only: no amounts, no card.
        assert_eq!(out.payments.len(), 2);
        assert!(out.payments.iter().all(|p| p.record.amount_minor.is_none()));
        assert!(out.payments.iter().all(|p| p.record.method.is_none()));
        let apple = &out.payments[0];
        assert_eq!(apple.domain, "apple.com");
        assert_eq!(apple.record.via.as_deref(), Some("apple.com"));
        assert_eq!(apple.record.kind, PaymentKind::Charge);
        assert_eq!(out.payments[1].domain, "spotify.com");
        assert!(out
            .findings
            .iter()
            .any(|f| f.domain == "spotify.com" && f.evidence.kind == EvidenceKind::Payment));
        let tf = out
            .findings
            .iter()
            .find(|f| f.evidence.kind == EvidenceKind::TwoFactor)
            .unwrap();
        assert_eq!(tf.domain, "service.example");
        // Watched list.
        let sub = out
            .subscriptions
            .iter()
            .find(|s| s.list_key == "news.shop.example")
            .unwrap();
        assert_eq!(sub.watched_mails.len(), 1);
        assert_eq!(sub.watched_mails[0].subject, "20% Rabatt");
        assert!(sub.watched_mails[0]
            .raw_headers
            .contains("Message-ID: <n1@shop.example>"));
        assert!(sub.watched_mails[0].date.is_some());
    }

    #[test]
    fn end_to_end_receipt_bodies_on() {
        let port = spawn_server(wave2_mails(), 5);
        let opts = ScanOptions {
            read_receipt_bodies: true,
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let out = scan_mailbox_with(&cfg_for(port), &opts, &mut |_| {}, &cancel).unwrap();
        let by_domain = |d: &str| out.payments.iter().find(|p| p.domain == d).unwrap();
        let spotify = by_domain("spotify.com");
        assert_eq!(spotify.record.amount_minor, Some(1680));
        assert_eq!(spotify.record.plan.as_deref(), Some("Premium Family"));
        assert_eq!(
            spotify.record.method,
            Some(ausgeloggt_core::PaymentMethod::Card {
                brand: "Visa".into(),
                last4: "4242".into()
            })
        );
        assert_eq!(
            spotify.record.source_ref.as_deref(),
            Some("<r2@spotify.com>")
        );
        let sp = by_domain("unresolved:spotify");
        assert_eq!(sp.record.amount_minor, Some(999));
        assert_eq!(sp.record.source_ref.as_deref(), Some("<r1@apple.com>#0"));
        assert_eq!(sp.record.via.as_deref(), Some("apple.com"));
        let di = by_domain("unresolved:disney");
        assert_eq!(di.record.amount_minor, Some(899));
        assert_eq!(di.record.source_ref.as_deref(), Some("<r1@apple.com>#1"));
        // The subject-level Apple payment was replaced by the item lines.
        assert!(out.payments.iter().all(|p| p.domain != "apple.com"));
        assert!(out
            .findings
            .iter()
            .all(|f| !(f.domain == "apple.com" && f.evidence.kind == EvidenceKind::Payment)));
        // No watch list requested: nothing collected.
        assert!(out.subscriptions.iter().all(|s| s.watched_mails.is_empty()));
    }

    #[test]
    fn test_connection_lists_selectable_folders() {
        let port = spawn_server(fake_mails(), 1);
        let folders = test_connection(&cfg_for(port)).unwrap();
        assert_eq!(folders, vec!["INBOX", "Sent"]);
    }

    #[test]
    fn unreachable_host_is_connect_error_quickly() {
        let start = std::time::Instant::now();
        let err = test_connection(&cfg_for(1)).unwrap_err();
        assert!(matches!(err, MailError::Connect(..)), "{err:?}");
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn same_message_id_from_different_senders_are_different_mails() {
        let mut st = ScanState::new("me@example.org");
        let mk = |from: &str| MailHeader {
            from: from.into(),
            subject: "Willkommen".into(),
            message_id: Some("<1@localhost>".into()),
            to: "me@example.org".into(),
            ..Default::default()
        };
        assert!(st
            .ingest(&mk("A <hello@a-service.de>"), "f", "me@example.org")
            .is_some());
        assert!(st
            .ingest(&mk("B <hello@b-service.de>"), "f", "me@example.org")
            .is_some());
        assert!(st
            .ingest(&mk("B <hello@b-service.de>"), "f", "me@example.org")
            .is_none());
    }
}
