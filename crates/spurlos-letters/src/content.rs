//! German letter templates. Each builder returns a structured [`LetterDoc`].

use crate::util::{fmt_date, fmt_datetime, fmt_opt_datetime};
use crate::{Attachment, LetterDoc, LetterError, Party};
use chrono::{DateTime, Duration, Months, Utc};
use spurlos_core::{
    Account, DataRequest, Identity, MailRef, RequestChannel, RequestKind, RequestStatus, Settings,
    Subscription, UnsubscribeMethod,
};

const SALUTATION: &str = "Sehr geehrte Damen und Herren,";
const CLOSING: &str = "Mit freundlichen Grüßen";

/// Convenience: identity lines for a party ("Straße", "PLZ Ort", "Land" if not Deutschland).
pub fn identity_lines(identity: &Identity) -> Vec<String> {
    let mut lines = Vec::new();
    let street = identity.street.trim();
    if !street.is_empty() {
        lines.push(street.to_string());
    }
    let city = format!("{} {}", identity.postal_code.trim(), identity.city.trim());
    let city = city.trim();
    if !city.is_empty() {
        lines.push(city.to_string());
    }
    let country = identity.country.trim();
    if !country.is_empty()
        && !country.eq_ignore_ascii_case("deutschland")
        && !country.eq_ignore_ascii_case("germany")
    {
        lines.push(country.to_string());
    }
    lines
}

/// Sender party from settings (name + identity); error lists the missing field.
pub fn sender_from(settings: &Settings) -> Result<Party, LetterError> {
    let id = &settings.identity;
    if settings.full_name.trim().is_empty() {
        return Err(LetterError::MissingSender("Name"));
    }
    if id.street.trim().is_empty() {
        return Err(LetterError::MissingSender("Straße"));
    }
    if id.postal_code.trim().is_empty() {
        return Err(LetterError::MissingSender("PLZ"));
    }
    if id.city.trim().is_empty() {
        return Err(LetterError::MissingSender("Ort"));
    }
    let email = id.email.trim();
    Ok(Party {
        name: settings.full_name.trim().to_string(),
        address_lines: identity_lines(id),
        email: (!email.is_empty()).then(|| email.to_string()),
    })
}

/// Recipient party from a catalog entry (legal name + postal address + email).
pub fn recipient_from_catalog(info: &spurlos_catalog::DeletionInfo) -> Party {
    let name = info
        .legal_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(info.name.as_str())
        .to_string();
    let address_lines = info
        .address
        .as_deref()
        .map(|a| {
            a.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let email = info
        .privacy_email
        .as_deref()
        .or(info.email.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    Party {
        name,
        address_lines,
        email,
    }
}

fn check_recipient(r: &Party) -> Result<(), LetterError> {
    if r.name.trim().is_empty() {
        Err(LetterError::MissingRecipient)
    } else {
        Ok(())
    }
}

fn one_month(now: DateTime<Utc>) -> DateTime<Utc> {
    now.checked_add_months(Months::new(1))
        .unwrap_or(now + Duration::days(30))
}

fn two_weeks(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::days(14)
}

fn identifiers(account: Option<&Account>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(a) = account {
        for i in &a.identifiers {
            let i = i.trim();
            if !i.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(i)) {
                out.push(i.to_string());
            }
        }
    }
    out
}

fn identifier_paragraph(account: Option<&Account>) -> String {
    let ids = identifiers(account);
    if ids.is_empty() {
        "Mir ist keine Kennung bekannt; bitte suchen Sie anhand meines Namens und meiner Anschrift sowie der E-Mail-Adresse, von der aus ich Ihnen schreibe."
            .to_string()
    } else {
        let mut s = String::from(
            "Zur Zuordnung nenne ich Ihnen die folgenden Kennungen (E-Mail-Adressen, Benutzernamen, Relay-/Alias-Adressen), unter denen Daten zu mir bei Ihnen vorliegen können:",
        );
        for i in ids {
            s.push_str("\n- ");
            s.push_str(&i);
        }
        s
    }
}

fn reference_for(account: Option<&Account>) -> Option<String> {
    let a = account?;
    let ids = identifiers(account);
    let mut s = format!("Konto: {} ({})", a.name, a.domain);
    if !ids.is_empty() {
        s.push_str("; Kennung(en): ");
        s.push_str(&ids.join(", "));
    }
    Some(s)
}

struct Draft {
    subject: String,
    reference: Option<String>,
    paragraphs: Vec<String>,
    attachments: Vec<Attachment>,
    deadline: Option<DateTime<Utc>>,
}

fn doc(
    settings: &Settings,
    sender: Party,
    recipient: &Party,
    now: DateTime<Utc>,
    d: Draft,
) -> LetterDoc {
    LetterDoc {
        place_date: format!("{}, {}", settings.identity.city.trim(), fmt_date(now)),
        sender,
        recipient: recipient.clone(),
        subject: d.subject,
        reference: d.reference,
        salutation: SALUTATION.to_string(),
        paragraphs: d.paragraphs,
        closing: CLOSING.to_string(),
        attachments: d.attachments,
        deadline: d.deadline,
    }
}

/// Standard block of the Art. 15 GDPR information list.
fn art15_list() -> String {
    "1. eine Bestätigung, ob Sie mich betreffende personenbezogene Daten verarbeiten;\n\
     2. eine Kopie aller mich betreffenden personenbezogenen Daten, die Gegenstand der Verarbeitung sind (Art. 15 Abs. 3 DSGVO);\n\
     3. die Verarbeitungszwecke (Art. 15 Abs. 1 lit. a DSGVO);\n\
     4. die Kategorien der verarbeiteten Daten (lit. b);\n\
     5. die Empfänger oder Kategorien von Empfängern, gegenüber denen die Daten offengelegt wurden oder werden, insbesondere Auftragsverarbeiter und Empfänger in Drittländern (lit. c, Art. 15 Abs. 2 DSGVO);\n\
     6. die geplante Speicherdauer oder, falls dies nicht möglich ist, die Kriterien für die Festlegung dieser Dauer (lit. d);\n\
     7. Informationen über das Bestehen der Rechte auf Berichtigung, Löschung, Einschränkung und Widerspruch sowie des Beschwerderechts bei einer Aufsichtsbehörde (lit. e und f);\n\
     8. alle verfügbaren Informationen über die Herkunft der Daten, sofern sie nicht bei mir erhoben wurden (lit. g);\n\
     9. das Bestehen einer automatisierten Entscheidungsfindung einschließlich Profiling mit aussagekräftigen Informationen über die involvierte Logik sowie Tragweite und angestrebte Auswirkungen (lit. h)."
        .to_string()
}

const FREE_ONE_MONTH: &str = "Die Auskunft und die erste Kopie sind für mich unentgeltlich (Art. 12 Abs. 5, Art. 15 Abs. 3 DSGVO). Nach Art. 12 Abs. 3 DSGVO haben Sie mir die Informationen unverzüglich, spätestens innerhalb eines Monats nach Eingang dieses Schreibens, zur Verfügung zu stellen. Bitte übermitteln Sie die Antwort in einem gängigen elektronischen Format (Art. 15 Abs. 3 Satz 3 DSGVO).";

const NO_ID_COPY: &str = "Zur Identifizierung nutzen Sie bitte ausschließlich die bei Ihnen bereits vorhandenen Daten (z. B. Bestätigung über die E-Mail-Adresse). Die Anforderung weiterer Nachweise, etwa einer Ausweiskopie, wäre unverhältnismäßig (Art. 12 Abs. 6, Art. 5 Abs. 1 lit. c DSGVO); sollten Sie begründete Zweifel an meiner Identität haben, teilen Sie mir bitte mit, welche Angaben konkret erforderlich sind.";

/// Unterlassungsaufforderung (self-written cease-and-desist) after advertising mails
/// despite unsubscribe/objection. Cites Art. 21 Abs. 3 DSGVO, §§ 823 Abs. 1, 1004 BGB
/// analog (allgemeines Persönlichkeitsrecht), § 7 UWG as orientation; demands a
/// strafbewehrte Unterlassungserklärung (attached, Hamburger Brauch) within 14 days,
/// Art. 15 information about stored data and source, and erasure of the advertising
/// use; states that no costs are claimed (self-written). Attaches the evidence table
/// built from `sub.history` + `sub.post_unsubscribe_mails`.
pub fn cease_and_desist(
    settings: &Settings,
    recipient: &Party,
    sub: &Subscription,
    account: Option<&Account>,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = two_weeks(now);
    let dl = fmt_date(deadline);

    // All addresses concerned: the receiving mailbox plus all e-mail identifiers.
    let mut addresses: Vec<String> = Vec::new();
    for a in std::iter::once(sub.mailbox.trim().to_string())
        .chain(identifiers(account).into_iter().filter(|i| i.contains('@')))
    {
        if !a.is_empty() && !addresses.iter().any(|o| o.eq_ignore_ascii_case(&a)) {
            addresses.push(a);
        }
    }
    let addr_text = addresses.join(", ");

    let mut mails: Vec<&MailRef> = sub.post_unsubscribe_mails.iter().collect();
    mails.sort_by_key(|m| m.date);
    let n_mails = mails.len();
    let has_attempts = !sub.history.is_empty();

    let mut paras = Vec::new();
    let mut intro = format!(
        "ich habe der Zusendung von Werbung durch Sie widersprochen und mich von Ihrem Verteiler („{}“, Absender: {}) abgemeldet. Dennoch erhalte ich weiterhin unverlangt Werbe-E-Mails von Ihnen an {}.",
        sub.name, sub.sender, addr_text
    );
    if n_mails > 0 {
        let first = mails.iter().filter_map(|m| m.date).min();
        let last = mails.iter().filter_map(|m| m.date).max();
        intro.push_str(&format!(
            " Nach meiner Abmeldung sind bereits {n_mails} weitere E-Mail(s) eingegangen"
        ));
        if let (Some(f), Some(l)) = (first, last) {
            intro.push_str(&format!(
                ", die erste am {}, die letzte am {}",
                fmt_date(f),
                fmt_date(l)
            ));
        }
        intro.push('.');
    }
    paras.push(intro);

    let docs_ref = match (has_attempts, n_mails > 0) {
        (true, true) => Some("Meine Abmelde- und Widerspruchsversuche (Anlage 2) sowie die danach eingegangenen E-Mails samt Kopfzeilen (Anlage 3) sind dokumentiert.".to_string()),
        (true, false) => Some("Meine Abmelde- und Widerspruchsversuche sind als Anlage 2 dokumentiert.".to_string()),
        (false, true) => Some("Die nach meiner Abmeldung eingegangenen E-Mails sind samt Kopfzeilen als Anlage 2 dokumentiert.".to_string()),
        (false, false) => None,
    };
    paras.extend(docs_ref);

    paras.push(
        "Der Versand von Werbe-E-Mails ohne meine Einwilligung ist eine unzumutbare Belästigung (§ 7 Abs. 2 Nr. 3 UWG) und greift in mein allgemeines Persönlichkeitsrecht ein; mir stehen Unterlassungsansprüche entsprechend §§ 823 Abs. 1, 1004 Abs. 1 Satz 2 BGB zu. Zudem habe ich der Verarbeitung meiner Daten zu Werbezwecken nach Art. 21 Abs. 2 und 3 DSGVO widersprochen; seit dem Widerspruch dürfen meine Daten nicht mehr für diese Zwecke verarbeitet werden. Eine Einwilligung habe ich nicht erteilt; vorsorglich widerrufe ich eine etwaige Einwilligung mit Wirkung für die Zukunft (Art. 7 Abs. 3 DSGVO). Durch die bereits erfolgten Verstöße wird die Wiederholungsgefahr vermutet; sie kann nur durch eine strafbewehrte Unterlassungserklärung ausgeräumt werden."
            .to_string(),
    );
    paras.push(format!(
        "Ich fordere Sie daher auf,\n\
         1. bis zum {dl} (Eingang bei mir) die als Anlage 1 beigefügte strafbewehrte Unterlassungserklärung rechtsverbindlich unterschrieben abzugeben;\n\
         2. die Verarbeitung meiner Daten zu Werbezwecken unverzüglich einzustellen, meine Adressen in Ihren Verteilern und bei beauftragten Dienstleistern zu sperren und die Daten zu löschen, soweit keine gesetzlichen Aufbewahrungspflichten entgegenstehen (Art. 17 Abs. 1 lit. c DSGVO);\n\
         3. mir nach Art. 15 DSGVO Auskunft über die zu meiner Person gespeicherten Daten, die Zwecke, die Empfänger und insbesondere die Herkunft der Daten zu erteilen (Art. 15 Abs. 1 lit. g DSGVO) und mir eine Kopie zur Verfügung zu stellen (Art. 15 Abs. 3 DSGVO); hierfür gilt die Monatsfrist des Art. 12 Abs. 3 DSGVO."
    ));
    paras.push(
        "Da ich dieses Schreiben selbst verfasst habe, mache ich derzeit keine Kosten geltend. Sollten Sie die Frist verstreichen lassen, behalte ich mir vor, gerichtliche Hilfe in Anspruch zu nehmen (einstweilige Verfügung bzw. Unterlassungsklage), Schadensersatz nach Art. 82 DSGVO zu verlangen und mich bei der zuständigen Datenschutz-Aufsichtsbehörde zu beschweren (Art. 77 DSGVO)."
            .to_string(),
    );

    let mut attachments = vec![Attachment::CeaseAndDesistDeclaration {
        sender: sender.clone(),
        recipient: recipient.clone(),
        scope: addr_text.clone(),
    }];
    if has_attempts {
        let rows = sub
            .history
            .iter()
            .map(|a| {
                let method = match &a.method {
                    UnsubscribeMethod::OneClick { .. } => {
                        "One-Click-Abmeldung (RFC 8058)".to_string()
                    }
                    UnsubscribeMethod::Mailto { address, .. } => {
                        format!("Abmelde-E-Mail an {address}")
                    }
                    UnsubscribeMethod::Link { .. } => "Abmeldelink (Browser)".to_string(),
                };
                let msg = a.message.trim();
                let result = match (a.ok, msg.is_empty()) {
                    (true, true) => "Erfolgreich übermittelt".to_string(),
                    (true, false) => format!("Erfolgreich übermittelt: {msg}"),
                    (false, true) => "Fehlgeschlagen".to_string(),
                    (false, false) => format!("Fehlgeschlagen: {msg}"),
                };
                vec![fmt_datetime(a.at), method, result]
            })
            .collect();
        attachments.push(Attachment::EvidenceTable {
            title: format!("Abmelde- und Widerspruchsversuche ({})", sub.name),
            columns: vec!["Datum (UTC)".into(), "Methode".into(), "Ergebnis".into()],
            rows,
            raw_headers: Vec::new(),
        });
    }
    if n_mails > 0 {
        let rows = mails
            .iter()
            .map(|m| {
                vec![
                    fmt_opt_datetime(m.date),
                    m.from.clone(),
                    m.subject.clone(),
                    m.message_id.clone().unwrap_or_else(|| "–".into()),
                ]
            })
            .collect();
        let raw_headers = mails
            .iter()
            .filter(|m| !m.raw_headers.trim().is_empty())
            .map(|m| m.raw_headers.trim_end().to_string())
            .collect();
        attachments.push(Attachment::EvidenceTable {
            title: "Nach der Abmeldung eingegangene Werbe-E-Mails".into(),
            columns: vec![
                "Datum (UTC)".into(),
                "Absender".into(),
                "Betreff".into(),
                "Message-ID".into(),
            ],
            rows,
            raw_headers,
        });
    }

    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject: format!(
                "Unterlassungsaufforderung wegen unerlaubter Werbe-E-Mails an {addr_text}"
            ),
            reference: Some(format!("Frist: {dl}; Absender der Werbung: {}", sub.sender)),
            paragraphs: paras,
            attachments,
            deadline: Some(deadline),
        },
    ))
}

/// Art. 21 objection to direct marketing + Art. 15 access in one letter (escalation step 1).
pub fn objection_and_access(
    settings: &Settings,
    recipient: &Party,
    account: &Account,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = one_month(now);
    let paras = vec![
        format!(
            "ich bin bei Ihnen unter „{}“ registriert bzw. Sie verarbeiten personenbezogene Daten von mir.",
            account.name
        ),
        identifier_paragraph(Some(account)),
        "Widerspruch gegen Werbung: Gemäß Art. 21 Abs. 2 und 3 DSGVO widerspreche ich der Verarbeitung meiner personenbezogenen Daten für Zwecke der Direktwerbung einschließlich des damit verbundenen Profilings, auf allen Kanälen (insbesondere E-Mail, Post, Telefon, Messenger). Ab Zugang dieses Widerspruchs dürfen meine Daten nicht mehr zu diesen Zwecken verarbeitet werden. Eine erteilte Einwilligung widerrufe ich mit Wirkung für die Zukunft (Art. 7 Abs. 3 DSGVO). Bitte sperren Sie meine Adressen für Werbung und bestätigen Sie mir dies schriftlich.".to_string(),
        format!("Auskunft: Zugleich verlange ich nach Art. 15 DSGVO Auskunft über die zu meiner Person gespeicherten Daten. Ich bitte um:\n{}", art15_list()),
        format!("{FREE_ONE_MONTH} Ich bitte um Antwort bis zum {}.", fmt_date(deadline)),
        NO_ID_COPY.to_string(),
        "Sollte ich innerhalb der Frist keine Antwort erhalten oder weiterhin Werbung bekommen, behalte ich mir eine Beschwerde bei der zuständigen Aufsichtsbehörde (Art. 77 DSGVO) und weitere rechtliche Schritte vor.".to_string(),
    ];
    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject:
                "Widerspruch gegen Werbung (Art. 21 DSGVO) und Auskunftsersuchen (Art. 15 DSGVO)"
                    .into(),
            reference: reference_for(Some(account)),
            paragraphs: paras,
            attachments: Vec::new(),
            deadline: Some(deadline),
        },
    ))
}

/// Art. 15 access request ("Was wisst ihr über mich?").
pub fn access_request(
    settings: &Settings,
    recipient: &Party,
    account: Option<&Account>,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = one_month(now);
    let paras = vec![
        "gemäß Art. 15 der Datenschutz-Grundverordnung (DSGVO) verlange ich Auskunft über die bei Ihnen zu meiner Person gespeicherten Daten.".to_string(),
        identifier_paragraph(account),
        format!("Ich bitte um folgende Informationen:\n{}", art15_list()),
        format!("{FREE_ONE_MONTH} Ich bitte um Antwort bis zum {}.", fmt_date(deadline)),
        NO_ID_COPY.to_string(),
        "Sollte ich keine oder keine vollständige Auskunft erhalten, behalte ich mir eine Beschwerde bei der zuständigen Aufsichtsbehörde (Art. 77 DSGVO) sowie weitere rechtliche Schritte, einschließlich Ansprüchen nach Art. 82 DSGVO, vor.".to_string(),
    ];
    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject: "Auskunftsersuchen nach Art. 15 DSGVO".into(),
            reference: reference_for(account),
            paragraphs: paras,
            attachments: Vec::new(),
            deadline: Some(deadline),
        },
    ))
}

/// Kündigung in Textform (§ 312k BGB context), "zum nächstmöglichen Zeitpunkt",
/// asks for written confirmation with end date, revokes direct-debit mandate /
/// payment authorisation for future charges, and asks to delete data afterwards (Art. 17).
pub fn cancellation(
    settings: &Settings,
    recipient: &Party,
    account: &Account,
    plan: Option<&str>,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = two_weeks(now);

    let latest_payment = account
        .payments
        .iter()
        .filter(|p| p.method.is_some())
        .max_by_key(|p| p.date);
    let plan_text = plan
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| {
            account
                .payments
                .iter()
                .filter(|p| p.plan.as_deref().is_some_and(|s| !s.trim().is_empty()))
                .max_by_key(|p| p.date)
                .and_then(|p| p.plan.clone())
        });
    let contract = match &plan_text {
        Some(p) => format!(
            "den zwischen uns bestehenden Vertrag „{p}“ bei {}",
            account.name
        ),
        None => format!("den zwischen uns bestehenden Vertrag bei {}", account.name),
    };
    let method = latest_payment
        .and_then(|p| p.method.as_ref())
        .map(|m| {
            format!(
                " (zuletzt genutzte Zahlungsart laut meinen Unterlagen: {})",
                m.label_de()
            )
        })
        .unwrap_or_default();
    let paras = vec![
        format!(
            "hiermit kündige ich {contract} ordentlich und fristgerecht zum nächstmöglichen Zeitpunkt, hilfsweise zum nächstmöglichen anderen Zeitpunkt. Die Kündigung erfolgt auch für alle damit verbundenen Zusatzleistungen und Testphasen."
        ),
        identifier_paragraph(Some(account)),
        format!(
            "Bitte bestätigen Sie mir die Kündigung und das Datum der Vertragsbeendigung bis zum {} in Textform (z. B. per E-Mail).",
            fmt_date(deadline)
        ),
        format!(
            "Mit Wirkung zum Zeitpunkt der Vertragsbeendigung widerrufe ich ein erteiltes SEPA-Lastschriftmandat bzw. jede sonstige Zahlungsautorisierung (z. B. Kreditkarte, PayPal oder Zahlungsdienste Dritter){method} für alle künftigen Abbuchungen. Entgelte für Zeiträume nach dem Vertragsende sind nicht geschuldet; ich bitte Sie, entsprechende Abbuchungen zu unterlassen. Zu Unrecht eingezogene Beträge werde ich zurückbuchen lassen bzw. zurückfordern."
        ),
        "Nach Beendigung des Vertrags bitte ich Sie nach Art. 17 DSGVO, alle personenbezogenen Daten zu meiner Person und mein Kundenkonto zu löschen, soweit nicht gesetzliche Aufbewahrungspflichten (z. B. § 147 AO, § 257 HGB) entgegenstehen. Für solche Daten bitte ich um Einschränkung der Verarbeitung (Art. 18 DSGVO) und um Mitteilung, welche Daten Sie aus welchem Grund aufbewahren. Bitte bestätigen Sie die Löschung schriftlich."
            .to_string(),
    ];
    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject: format!("Kündigung zum nächstmöglichen Zeitpunkt – {}", account.name),
            reference: reference_for(Some(account)),
            paragraphs: paras,
            attachments: Vec::new(),
            deadline: Some(deadline),
        },
    ))
}

fn fmt_birth_date(s: &str) -> String {
    let s = s.trim();
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        d.format("%d.%m.%Y").to_string()
    } else {
        s.to_string()
    }
}

/// Credit bureau self-disclosure (Art. 15, free copy), with birth date + previous addresses.
pub fn credit_bureau_access(
    settings: &Settings,
    recipient: &Party,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = one_month(now);
    let id = &settings.identity;
    let mut ident = format!("Zu meiner Person:\n- Name: {}", sender.name);
    let birth = fmt_birth_date(&id.birth_date);
    if !birth.is_empty() {
        ident.push_str(&format!("\n- Geburtsdatum: {birth}"));
    }
    ident.push_str(&format!(
        "\n- Aktuelle Anschrift: {}",
        sender.address_lines.join(", ")
    ));
    for p in id
        .previous_addresses
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        ident.push_str(&format!("\n- Frühere Anschrift: {p}"));
    }
    let paras = vec![
        "gemäß Art. 15 der Datenschutz-Grundverordnung (DSGVO) beantrage ich die Selbstauskunft über alle bei Ihnen zu meiner Person gespeicherten Daten in Form einer unentgeltlichen Datenkopie.".to_string(),
        ident,
        "Die Auskunft soll insbesondere umfassen:\n\
         1. alle zu meiner Person gespeicherten Daten, einschließlich der Angaben zu früheren Anschriften, Anfragen und Merkmalen, sowie deren Herkunft (Art. 15 Abs. 1 lit. g DSGVO);\n\
         2. die an Dritte übermittelten Scorewerte der letzten 12 Monate samt Zeitpunkt der Berechnung, Scorewert und Empfänger, sowie die zur Berechnung verwendeten Datenarten und die Erläuterung der involvierten Logik (Art. 15 Abs. 1 lit. h DSGVO);\n\
         3. die Empfänger oder Kategorien von Empfängern, denen meine Daten in den letzten 12 Monaten übermittelt wurden (lit. c);\n\
         4. die Verarbeitungszwecke, Datenkategorien und die vorgesehene Speicherdauer (lit. a, b, d).".to_string(),
        format!("{FREE_ONE_MONTH} Ich bitte um Antwort bis zum {} per Post an die oben genannte Anschrift.", fmt_date(deadline)),
        "Ich füge keine Ausweiskopie bei. Sollten Sie begründete Zweifel an meiner Identität haben, teilen Sie mir bitte mit, welche Angaben konkret erforderlich sind (Art. 12 Abs. 6 DSGVO).".to_string(),
    ];
    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject: "Selbstauskunft nach Art. 15 DSGVO (kostenlose Datenkopie)".into(),
            reference: None,
            paragraphs: paras,
            attachments: Vec::new(),
            deadline: Some(deadline),
        },
    ))
}

/// Objection against address trading / advertising use (Art. 21) at address brokers,
/// incl. request to name recipients of the data (Art. 15 Abs. 1 lit. c).
pub fn address_broker_objection(
    settings: &Settings,
    recipient: &Party,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = one_month(now);
    let id = &settings.identity;
    let mut ident = format!(
        "Zur Zuordnung meine Daten:\n- Name: {}\n- Anschrift: {}",
        sender.name,
        sender.address_lines.join(", ")
    );
    let birth = fmt_birth_date(&id.birth_date);
    if !birth.is_empty() {
        ident.push_str(&format!("\n- Geburtsdatum: {birth}"));
    }
    for p in id
        .previous_addresses
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        ident.push_str(&format!("\n- Frühere Anschrift: {p}"));
    }
    if let Some(e) = &sender.email {
        ident.push_str(&format!("\n- E-Mail-Adresse: {e}"));
    }
    let paras = vec![
        "Sie verarbeiten möglicherweise meine personenbezogenen Daten zu Werbe- und Adresshandelszwecken.".to_string(),
        ident,
        "Widerspruch: Gemäß Art. 21 Abs. 2 und 3 DSGVO widerspreche ich der Verarbeitung meiner personenbezogenen Daten für Zwecke der Direktwerbung einschließlich Adresshandel, Adressvermietung, Anreicherung, Selektion und Profiling. Ab Zugang dieses Widerspruchs dürfen meine Daten nicht mehr zu diesen Zwecken verarbeitet oder übermittelt werden.".to_string(),
        "Löschung: Ich verlange nach Art. 17 Abs. 1 lit. c DSGVO die Löschung aller zu Werbezwecken gespeicherten Daten. Soweit Sie zur künftigen Unterdrückung von Werbung Daten vorhalten müssen (Werbesperre), beschränken Sie diese auf das erforderliche Minimum und verwenden Sie sie ausschließlich dafür. Bitte informieren Sie alle Empfänger meiner Daten über Widerspruch und Löschung (Art. 19 DSGVO).".to_string(),
        "Auskunft: Zugleich verlange ich nach Art. 15 DSGVO Auskunft, insbesondere über\n\
         1. die zu meiner Person gespeicherten Daten samt Kopie (Abs. 3);\n\
         2. die Empfänger oder Kategorien von Empfängern, an die Sie meine Daten weitergegeben haben, mit Namen und Anschrift (Art. 15 Abs. 1 lit. c DSGVO);\n\
         3. die Herkunft der Daten (lit. g) sowie die Zwecke, Datenkategorien und die Speicherdauer (lit. a, b, d).".to_string(),
        format!("{FREE_ONE_MONTH} Ich bitte um Ihre schriftliche Bestätigung bis zum {}.", fmt_date(deadline)),
        "Sollte ich keine fristgerechte Antwort erhalten, werde ich die zuständige Datenschutz-Aufsichtsbehörde einschalten (Art. 77 DSGVO).".to_string(),
    ];
    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject:
                "Widerspruch gegen Adresshandel und Werbung (Art. 21 DSGVO), Auskunft und Löschung"
                    .into(),
            reference: None,
            paragraphs: paras,
            attachments: Vec::new(),
            deadline: Some(deadline),
        },
    ))
}

/// What a request kind is about, for reminders and complaints: (short noun phrase,
/// legal basis).
fn kind_text(kind: RequestKind) -> (&'static str, &'static str) {
    match kind {
        RequestKind::Erasure => (
            "Antrag auf Löschung meiner personenbezogenen Daten",
            "Art. 17 DSGVO",
        ),
        RequestKind::Access => ("Auskunftsersuchen", "Art. 15 DSGVO"),
        RequestKind::Objection => (
            "Widerspruch gegen die Verarbeitung meiner Daten für Werbung",
            "Art. 21 DSGVO",
        ),
        RequestKind::Cancellation => ("Kündigung", "§ 312k BGB"),
        RequestKind::CeaseAndDesist => (
            "Unterlassungsaufforderung",
            "§§ 823, 1004 BGB, § 7 UWG, Art. 21 DSGVO",
        ),
        RequestKind::Complaint => ("Beschwerde", "Art. 77 DSGVO"),
        RequestKind::Reminder => ("Erinnerung", "Art. 12 Abs. 3 DSGVO"),
        RequestKind::CreditBureauAccess => ("Antrag auf Selbstauskunft", "Art. 15 DSGVO"),
        RequestKind::AdvertisingObjection => (
            "Widerspruch gegen Adresshandel und Werbung",
            "Art. 21 DSGVO",
        ),
    }
}

fn channel_label(c: RequestChannel) -> &'static str {
    match c {
        RequestChannel::Email => "E-Mail",
        RequestChannel::Post => "Post",
        RequestChannel::WebForm => "Webformular",
    }
}

fn request_date(r: &DataRequest) -> DateTime<Utc> {
    r.sent_at.unwrap_or(r.created_at)
}

/// Art. 77 complaint to a supervisory authority about an unanswered / refused request,
/// summarising the timeline of `request` (+ its follow-ups) with an evidence attachment.
pub fn complaint(
    settings: &Settings,
    authority: &Party,
    request: &DataRequest,
    followups: &[DataRequest],
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(authority)?;
    let (what, basis) = kind_text(request.kind);
    let sent = fmt_date(request_date(request));

    let mut all: Vec<&DataRequest> = std::iter::once(request).chain(followups.iter()).collect();
    all.sort_by_key(|r| request_date(r));

    let mut timeline = String::from("Zeitlicher Ablauf:");
    for r in &all {
        let (w, _) = kind_text(r.kind);
        timeline.push_str(&format!(
            "\n- {}: {} per {} an {}",
            fmt_date(request_date(r)),
            w,
            channel_label(r.channel),
            r.target
        ));
        if let Some(d) = r.deadline {
            timeline.push_str(&format!(" (Frist bis {})", fmt_date(d)));
        }
    }

    let status = match request.status {
        RequestStatus::Rejected => "Mein Anliegen wurde abgelehnt, ohne dass ich eine nachvollziehbare Rechtsgrundlage dafür erkennen kann.".to_string(),
        RequestStatus::Answered => "Die Antwort ist nach meiner Einschätzung unvollständig bzw. wird meinem Anliegen nicht gerecht.".to_string(),
        _ => "Eine Antwort oder Erledigung ist innerhalb der gesetzlichen Frist nicht erfolgt (Art. 12 Abs. 3 DSGVO: ein Monat). Auch gesetzte Nachfristen sind ohne Reaktion verstrichen.".to_string(),
    };

    let mut who = format!("Beschwerdegegner: {}", request.target);
    if let Some(d) = &request.domain {
        who.push_str(&format!(" ({d})"));
    }
    if !request.recipient.trim().is_empty() {
        who.push_str(&format!(
            "\nAnschrift bzw. Kontakt: {}",
            request.recipient.trim().replace('\n', ", ")
        ));
    }

    let paras = vec![
        format!("hiermit erhebe ich Beschwerde nach Art. 77 DSGVO gegen {}, weil mein Anliegen ({what}, {basis}) vom {sent} nicht ordnungsgemäß bearbeitet wurde.", request.target),
        who,
        format!("Gegenstand: {what} ({basis}), versendet am {sent} per {}. Betreff: „{}“.", channel_label(request.channel), request.subject),
        timeline,
        status,
        "Ich bitte Sie, den Sachverhalt zu prüfen und den Verantwortlichen im Rahmen Ihrer Befugnisse nach Art. 58 DSGVO anzuweisen, meinem Anliegen zu entsprechen. Über das Ergebnis bitte ich um Mitteilung (Art. 77 Abs. 2 DSGVO).".to_string(),
        "Die zugehörigen Schreiben und eine Chronologie füge ich als Anlagen bei.".to_string(),
    ];

    let rows = all
        .iter()
        .map(|r| {
            vec![
                fmt_date(request_date(r)),
                r.kind.label_de().to_string(),
                channel_label(r.channel).to_string(),
                r.target.clone(),
                r.deadline.map(fmt_date).unwrap_or_else(|| "–".into()),
                r.status.label_de().to_string(),
            ]
        })
        .collect();
    let mut attachments = vec![Attachment::EvidenceTable {
        title: "Chronologie der Anfragen".into(),
        columns: vec![
            "Datum".into(),
            "Art".into(),
            "Kanal".into(),
            "Empfänger".into(),
            "Frist".into(),
            "Status".into(),
        ],
        rows,
        raw_headers: Vec::new(),
    }];
    for r in &all {
        if r.body.trim().is_empty() {
            continue;
        }
        let paragraphs: Vec<String> = r
            .body
            .replace("\r\n", "\n")
            .split("\n\n")
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
        attachments.push(Attachment::Section {
            title: format!(
                "Wortlaut: {} vom {}",
                r.kind.label_de(),
                fmt_date(request_date(r))
            ),
            paragraphs,
        });
    }

    Ok(doc(
        settings,
        sender,
        authority,
        now,
        Draft {
            subject: format!("Beschwerde nach Art. 77 DSGVO gegen {}", request.target),
            reference: None,
            paragraphs: paras,
            attachments,
            deadline: None,
        },
    ))
}

/// Generic reminder for an overdue request (14-day final deadline, mentions Art. 77).
pub fn reminder(
    settings: &Settings,
    recipient: &Party,
    request: &DataRequest,
    now: DateTime<Utc>,
) -> Result<LetterDoc, LetterError> {
    let sender = sender_from(settings)?;
    check_recipient(recipient)?;
    let deadline = two_weeks(now);
    let (what, basis) = kind_text(request.kind);
    let sent = fmt_date(request_date(request));
    let expired = request
        .deadline
        .unwrap_or_else(|| one_month(request_date(request)));
    let ask = match request.kind {
        RequestKind::Erasure => {
            "meinem Löschantrag nachzukommen und die Löschung schriftlich zu bestätigen"
        }
        RequestKind::Access | RequestKind::CreditBureauAccess => {
            "mir die vollständige Auskunft nebst Datenkopie zu erteilen"
        }
        RequestKind::Objection | RequestKind::AdvertisingObjection => {
            "meinen Widerspruch umzusetzen und mir dies schriftlich zu bestätigen"
        }
        RequestKind::Cancellation => {
            "die Kündigung samt Beendigungsdatum schriftlich zu bestätigen"
        }
        RequestKind::CeaseAndDesist => {
            "die geforderte strafbewehrte Unterlassungserklärung abzugeben"
        }
        _ => "mein Anliegen zu bearbeiten und mir schriftlich zu antworten",
    };
    let paras = vec![
        format!(
            "am {sent} habe ich Ihnen mein Schreiben „{}“ ({what}, {basis}) übermittelt. Die Frist ist am {} abgelaufen. Eine Antwort oder Erledigung habe ich bislang nicht erhalten.",
            request.subject,
            fmt_date(expired)
        ),
        format!(
            "Ich fordere Sie daher letztmalig auf, bis zum {} (Eingang bei mir) {ask}.",
            fmt_date(deadline)
        ),
        "Sollte ich bis dahin keine Antwort erhalten, werde ich mich bei der zuständigen Datenschutz-Aufsichtsbehörde nach Art. 77 DSGVO beschweren und behalte mir weitere rechtliche Schritte vor, einschließlich Schadensersatzansprüchen nach Art. 82 DSGVO.".to_string(),
    ];
    Ok(doc(
        settings,
        sender,
        recipient,
        now,
        Draft {
            subject: format!("Erinnerung und letzte Fristsetzung: {what} vom {sent}"),
            reference: Some(format!("Mein Schreiben vom {sent} an {}", request.target)),
            paragraphs: paras,
            attachments: Vec::new(),
            deadline: Some(deadline),
        },
    ))
}
