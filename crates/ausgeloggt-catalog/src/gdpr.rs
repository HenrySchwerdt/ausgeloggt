//! GDPR Art. 17 erasure requests (and Art. 15 access requests as an option).

use ausgeloggt_core::{Account, Settings};
use chrono::{DateTime, Duration, Months, Utc};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Letter {
    pub to: Option<String>,
    pub subject: String,
    pub body: String,
}

fn is_english(settings: &Settings) -> bool {
    settings.letter_language.trim().eq_ignore_ascii_case("en")
}

fn signer(settings: &Settings, en: bool) -> String {
    let n = settings.full_name.trim();
    if n.is_empty() {
        if en { "[Your name]" } else { "[Dein Name]" }.to_string()
    } else {
        n.to_string()
    }
}

fn identifiers(account: &Account) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for i in &account.identifiers {
        let i = i.trim();
        if !i.is_empty() && !v.iter().any(|x| x.eq_ignore_ascii_case(i)) {
            v.push(i.to_string());
        }
    }
    v
}

fn service(account: &Account) -> String {
    let name = account.name.trim();
    let domain = account.domain.trim();
    match (
        name.is_empty(),
        domain.is_empty() || domain.starts_with("unresolved:"),
    ) {
        (false, false) if !name.eq_ignore_ascii_case(domain) => format!("{name} ({domain})"),
        (false, _) => name.to_string(),
        (true, false) => domain.to_string(),
        (true, true) => "-".to_string(),
    }
}

fn fmt_date(d: DateTime<Utc>, en: bool) -> String {
    if en {
        d.format("%d %B %Y").to_string()
    } else {
        d.format("%d.%m.%Y").to_string()
    }
}

fn identifier_block(account: &Account, en: bool) -> String {
    let ids = identifiers(account);
    if ids.is_empty() {
        return if en {
            "(No account identifier is known to me; please search by my name and the e-mail address from which this message is sent.)\n".into()
        } else {
            "(Mir ist keine Kennung bekannt; bitte suchen Sie anhand meines Namens und der E-Mail-Adresse, von der diese Nachricht gesendet wird.)\n".into()
        };
    }
    let mut s = String::new();
    for i in ids {
        let _ = writeln!(s, "- {i}");
    }
    s
}

/// Build an erasure request for `account` in `settings.letter_language` ("de"/"en"),
/// signed with `settings.full_name`, listing the account's identifiers. Cites Art. 17
/// and Art. 12(3) GDPR (one month), asks for confirmation, and for erasure at processors.
pub fn erasure_request(account: &Account, settings: &Settings, to: Option<&str>) -> Letter {
    let en = is_english(settings);
    let name = signer(settings, en);
    let svc = service(account);
    let ids = identifier_block(account, en);
    let (subject, body) = if en {
        (
            format!("Request for erasure of personal data pursuant to Art. 17 GDPR ({name})"),
            format!(
                "Dear Sir or Madam,\n\n\
                 pursuant to Art. 17(1) of the General Data Protection Regulation (GDPR), I request that you erase without undue delay all personal data concerning me, and delete my user account.\n\n\
                 Service: {svc}\n\
                 Account identifiers (e-mail addresses / user names):\n{ids}\n\
                 I ask you to:\n\
                 1. erase all personal data relating to me, including my account, profile, content, usage and log data, and backups as soon as technically possible;\n\
                 2. instruct all processors and any third parties to which you have disclosed or made public my data to erase it as well (Art. 17(2) and Art. 19 GDPR), and inform me of the recipients on request;\n\
                 3. not use my data for any further processing, in particular not for advertising, profiling or analytics;\n\
                 4. confirm the erasure to me in writing.\n\n\
                 To identify me, please use only the data you already hold about me (e.g. confirmation via the e-mail address of the account). Requesting further proof of identity, such as a copy of an ID document, would be disproportionate (Art. 12(6) GDPR, Art. 5(1)(c) GDPR).\n\n\
                 If you are legally required to retain certain data (e.g. tax or commercial retention obligations), please erase everything else, restrict the processing of the retained data (Art. 18 GDPR), and tell me which data you retain and on what legal basis.\n\n\
                 Under Art. 12(3) GDPR you must respond within one month of receipt of this request. Please send your reply to this e-mail address.\n\n\
                 Yours faithfully,\n\n{name}\n"
            ),
        )
    } else {
        (
            format!("Antrag auf Löschung personenbezogener Daten gemäß Art. 17 DSGVO ({name})"),
            format!(
                "Sehr geehrte Damen und Herren,\n\n\
                 gemäß Art. 17 Abs. 1 der Datenschutz-Grundverordnung (DSGVO) beantrage ich die unverzügliche Löschung aller mich betreffenden personenbezogenen Daten sowie die Löschung meines Nutzerkontos.\n\n\
                 Dienst: {svc}\n\
                 Kennungen des Kontos (E-Mail-Adressen / Benutzernamen):\n{ids}\n\
                 Ich bitte Sie,\n\
                 1. alle personenbezogenen Daten zu meiner Person zu löschen, einschließlich Konto, Profil, Inhalte sowie Nutzungs- und Protokolldaten, und Backups sobald technisch möglich;\n\
                 2. auch alle Auftragsverarbeiter und sonstigen Empfänger, denen Sie meine Daten offengelegt oder öffentlich gemacht haben, zur Löschung aufzufordern (Art. 17 Abs. 2 und Art. 19 DSGVO) und mir auf Verlangen die Empfänger mitzuteilen;\n\
                 3. meine Daten nicht weiterzuverarbeiten, insbesondere nicht für Werbung, Profiling oder Analysen;\n\
                 4. mir die Löschung schriftlich zu bestätigen.\n\n\
                 Zur Identifizierung nutzen Sie bitte ausschließlich die bei Ihnen bereits vorhandenen Daten (z. B. Bestätigung über die E-Mail-Adresse des Kontos). Die Anforderung weiterer Nachweise, etwa einer Ausweiskopie, wäre unverhältnismäßig (Art. 12 Abs. 6, Art. 5 Abs. 1 lit. c DSGVO).\n\n\
                 Sofern Sie bestimmte Daten aufgrund gesetzlicher Aufbewahrungspflichten (z. B. steuer- oder handelsrechtlich) behalten müssen, löschen Sie bitte alle übrigen Daten, schränken Sie die Verarbeitung der verbleibenden Daten ein (Art. 18 DSGVO) und teilen Sie mir mit, welche Daten Sie aus welchem Grund aufbewahren.\n\n\
                 Nach Art. 12 Abs. 3 DSGVO haben Sie mir innerhalb eines Monats nach Eingang dieses Antrags zu antworten. Bitte senden Sie Ihre Antwort an diese E-Mail-Adresse.\n\n\
                 Mit freundlichen Grüßen\n\n{name}\n"
            ),
        )
    };
    Letter {
        to: to.map(str::to_string),
        subject,
        body,
    }
}

/// Follow-up letter when the deadline has passed without an answer
/// (mentions the right to complain to a supervisory authority, Art. 77).
pub fn reminder(
    account: &Account,
    settings: &Settings,
    sent_at: DateTime<Utc>,
    to: Option<&str>,
) -> Letter {
    let en = is_english(settings);
    let name = signer(settings, en);
    let svc = service(account);
    let ids = identifier_block(account, en);
    let sent = fmt_date(sent_at, en);
    let expired = fmt_date(deadline_from(sent_at), en);
    let final_deadline = fmt_date(Utc::now() + Duration::days(14), en);
    let (subject, body) = if en {
        (
            format!("Reminder: request for erasure pursuant to Art. 17 GDPR of {sent} ({name})"),
            format!(
                "Dear Sir or Madam,\n\n\
                 on {sent} I asked you, pursuant to Art. 17 GDPR, to erase all personal data concerning me (service: {svc}). The one-month period under Art. 12(3) GDPR expired on {expired}. I have not received a response or confirmation of erasure.\n\n\
                 Account identifiers:\n{ids}\n\
                 I therefore ask you to comply with my request and confirm the erasure in writing within 14 days of the date of this letter, i.e. by {final_deadline} at the latest. This also covers erasure at your processors (Art. 17(2), Art. 19 GDPR).\n\n\
                 If I do not receive a response by then, I will lodge a complaint with the competent supervisory authority pursuant to Art. 77 GDPR and reserve the right to take further legal action, including claims under Art. 82 GDPR.\n\n\
                 Yours faithfully,\n\n{name}\n"
            ),
        )
    } else {
        (
            format!("Erinnerung: Antrag auf Löschung gemäß Art. 17 DSGVO vom {sent} ({name})"),
            format!(
                "Sehr geehrte Damen und Herren,\n\n\
                 am {sent} habe ich Sie gemäß Art. 17 DSGVO aufgefordert, alle mich betreffenden personenbezogenen Daten zu löschen (Dienst: {svc}). Die Monatsfrist nach Art. 12 Abs. 3 DSGVO ist am {expired} abgelaufen. Eine Antwort oder Löschbestätigung habe ich bislang nicht erhalten.\n\n\
                 Kennungen des Kontos:\n{ids}\n\
                 Ich bitte Sie daher, meinem Antrag nachzukommen und die Löschung innerhalb von 14 Tagen ab dem Datum dieses Schreibens, also spätestens bis zum {final_deadline}, schriftlich zu bestätigen. Dies umfasst auch die Löschung bei Ihren Auftragsverarbeitern (Art. 17 Abs. 2, Art. 19 DSGVO).\n\n\
                 Sollte ich bis dahin keine Antwort erhalten, werde ich mich gemäß Art. 77 DSGVO bei der zuständigen Datenschutz-Aufsichtsbehörde beschweren und behalte mir weitere rechtliche Schritte vor, einschließlich Ansprüchen nach Art. 82 DSGVO.\n\n\
                 Mit freundlichen Grüßen\n\n{name}\n"
            ),
        )
    };
    Letter {
        to: to.map(str::to_string),
        subject,
        body,
    }
}

/// RFC 3986 percent-encoding; `keep` lists extra bytes left as-is. Newlines become CRLF (RFC 6068).
fn pct_encode(s: &str, keep: &[u8]) -> String {
    let normalized = s
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n");
    let mut out = String::with_capacity(normalized.len() * 2);
    for &b in normalized.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') || keep.contains(&b)
        {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

/// `mailto:` URL with percent-encoded subject and body, for the default mail client.
pub fn mailto_url(letter: &Letter) -> String {
    let to = letter
        .to
        .as_deref()
        .map(|t| pct_encode(t.trim(), b"@"))
        .unwrap_or_default();
    format!(
        "mailto:{to}?subject={}&body={}",
        pct_encode(&letter.subject, b""),
        pct_encode(&letter.body, b"")
    )
}

/// GDPR deadline: one calendar month after `sent_at` (Art. 12(3)).
pub fn deadline_from(sent_at: DateTime<Utc>) -> DateTime<Utc> {
    // chrono clamps to the last day of the month (Jan 31 -> Feb 28/29).
    sent_at
        .checked_add_months(Months::new(1))
        .or_else(|| sent_at.checked_add_signed(Duration::days(31)))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn account() -> Account {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        Account {
            id: Uuid::nil(),
            domain: "spotify.com".into(),
            name: "Spotify".into(),
            identifiers: vec![
                "max@example.org".into(),
                "maxm".into(),
                "MAX@example.org".into(),
            ],
            sso: vec![],
            evidence: vec![],
            status: Default::default(),
            confidence: 1.0,
            first_seen: None,
            last_seen: None,
            tags: vec![],
            notes: String::new(),
            deletion: vec![],
            created_at: now,
            updated_at: now,
            payments: Vec::new(),
            security: Default::default(),
        }
    }

    fn settings(lang: &str, name: &str) -> Settings {
        Settings {
            letter_language: lang.into(),
            full_name: name.into(),
            ..Settings::default()
        }
    }

    #[test]
    fn german_letter() {
        let l = erasure_request(
            &account(),
            &settings("de", "Max Mustermann"),
            Some("privacy@spotify.com"),
        );
        assert_eq!(l.to.as_deref(), Some("privacy@spotify.com"));
        assert_eq!(
            l.subject,
            "Antrag auf Löschung personenbezogener Daten gemäß Art. 17 DSGVO (Max Mustermann)"
        );
        for needle in [
            "max@example.org",
            "- maxm",
            "Spotify (spotify.com)",
            "Art. 17",
            "Art. 12 Abs. 3",
            "Art. 19",
            "Art. 18",
            "Max Mustermann",
        ] {
            assert!(l.body.contains(needle), "missing {needle}");
        }
        assert_eq!(
            l.body.matches("max@example.org").count(),
            1,
            "deduplicated identifiers"
        );
    }

    #[test]
    fn english_letter_and_placeholder() {
        let l = erasure_request(&account(), &settings("en", "  "), None);
        assert!(l.to.is_none());
        assert!(l
            .subject
            .starts_with("Request for erasure of personal data pursuant to Art. 17 GDPR"));
        assert!(l.body.contains("[Your name]"));
        assert!(
            l.body.contains("Art. 12(3)")
                && l.body.contains("Art. 19")
                && l.body.contains("Art. 18")
        );
        assert!(l.body.contains("max@example.org"));
        let de = erasure_request(&account(), &settings("de", ""), None);
        assert!(de.body.contains("[Dein Name]"));
    }

    #[test]
    fn reminder_mentions_dates_and_art77() {
        let sent = Utc.with_ymd_and_hms(2026, 3, 5, 10, 0, 0).unwrap();
        let de = reminder(&account(), &settings("de", "Max"), sent, None);
        assert!(de.body.contains("05.03.2026") && de.body.contains("05.04.2026"));
        assert!(de.body.contains("Art. 77") && de.body.contains("14 Tagen"));
        let en = reminder(&account(), &settings("en", "Max"), sent, None);
        assert!(
            en.body.contains("05 March 2026")
                && en.body.contains("Art. 77")
                && en.body.contains("14 days")
        );
    }

    #[test]
    fn mailto_encoding() {
        let l = Letter {
            to: Some("a@b.de".into()),
            subject: "Löschung & Co".into(),
            body: "Zeile 1\nZeile 2 100%".into(),
        };
        assert_eq!(
            mailto_url(&l),
            "mailto:a@b.de?subject=L%C3%B6schung%20%26%20Co&body=Zeile%201%0D%0AZeile%202%20100%25"
        );
        let l = Letter {
            to: None,
            subject: "x".into(),
            body: "a\r\nb".into(),
        };
        assert_eq!(mailto_url(&l), "mailto:?subject=x&body=a%0D%0Ab");
    }

    #[test]
    fn deadline_edges() {
        let d = |y, m, day| Utc.with_ymd_and_hms(y, m, day, 12, 0, 0).unwrap();
        assert_eq!(deadline_from(d(2026, 1, 31)), d(2026, 2, 28));
        assert_eq!(deadline_from(d(2028, 1, 31)), d(2028, 2, 29));
        assert_eq!(deadline_from(d(2026, 12, 15)), d(2027, 1, 15));
        assert_eq!(deadline_from(d(2026, 3, 31)), d(2026, 4, 30));
    }
}
