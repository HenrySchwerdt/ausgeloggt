//! Emergency kit ("Digitaler Nachlass") and privacy report PDFs.

use crate::layout::{Canvas, Kind, Style, ACCENT, BLACK, GRAY, GREEN, LIGHT, RED, RULE};
use crate::util::{fmt_date, fmt_datetime, fmt_money};
use crate::LetterError;
use chrono::{DateTime, Utc};
use spurlos_core::insights::PrivacyScore;
use spurlos_core::{
    Account, AccountStatus, PaymentKind, PaymentRecord, RequestStatus, Settings, VaultData,
};

fn is_open(a: &Account) -> bool {
    !matches!(
        a.status,
        AccountStatus::Deleted | AccountStatus::NotAnAccount | AccountStatus::Ignored
    )
}

/// Latest recurring charge of an account unless a later cancellation exists.
fn active_recurring(a: &Account) -> Option<&PaymentRecord> {
    let last_renewal = a
        .payments
        .iter()
        .filter(|p| matches!(p.kind, PaymentKind::Renewal))
        .max_by_key(|p| p.date)?;
    let cancelled_after = a
        .payments
        .iter()
        .any(|p| matches!(p.kind, PaymentKind::Cancelled) && p.date > last_renewal.date);
    (!cancelled_after).then_some(last_renewal)
}

fn title_block(c: &mut Canvas, title: &str, subtitle: &str) {
    c.line_at(c.left, title, Style::Bold, 24.0, ACCENT, 1.25);
    c.line_at(c.left, subtitle, Style::Regular, 10.5, GRAY, 1.4);
    c.gap(3.0);
    c.hline(c.left, c.right(), c.y, 0.6, ACCENT);
    c.gap(6.0);
}

fn section(c: &mut Canvas, title: &str) {
    c.gap(5.0);
    c.ensure(24.0);
    c.line_at(c.left, title, Style::Bold, 13.0, ACCENT, 1.3);
    c.hline(c.left, c.right(), c.y + 0.5, 0.3, RULE);
    c.gap(3.5);
}

fn body(c: &mut Canvas, text: &str) {
    c.para(text, Style::Regular, 10.0, BLACK, c.left, c.width, 1.4);
    c.gap(2.5);
}

/// One account block: bold title + domain, then label/value rows.
fn card(c: &mut Canvas, title: &str, subtitle: &str, rows: &[(String, String)]) {
    let label_w = 34.0;
    let value_w = c.width - label_w - 2.0;
    let size = 9.5;
    let lh = size / (72.0 / 25.4) * 1.35;
    let wrapped: Vec<(String, Vec<String>)> = rows
        .iter()
        .map(|(l, v)| (l.clone(), c.wrap(Style::Regular, size, v, value_w)))
        .collect();
    let h = 8.0
        + wrapped
            .iter()
            .map(|(_, v)| v.len().max(1) as f32 * lh)
            .sum::<f32>()
        + 3.0;
    c.ensure(h.min(60.0));
    c.hline(c.left, c.right(), c.y, 0.3, RULE);
    c.gap(1.2);
    c.line_at(c.left, title, Style::Bold, 11.5, BLACK, 1.4);
    // domain right-aligned on the title line
    let ty = c.y - 1.4;
    c.text_right(c.right(), ty, subtitle, Style::Regular, 9.0, GRAY);
    for (label, lines) in &wrapped {
        let mut first = true;
        for l in lines {
            c.ensure(lh);
            let by = c.y + lh * 0.75;
            if first {
                c.text(c.left, by, label, Style::Regular, 8.5, GRAY);
                first = false;
            }
            c.text(c.left + label_w, by, l, Style::Regular, size, BLACK);
            c.y += lh;
        }
    }
    c.gap(3.0);
}

fn money_of(p: &PaymentRecord) -> Option<String> {
    p.amount_minor.map(|m| fmt_money(m, p.currency.as_deref()))
}

/// "Digitaler Nachlass" PDF for next of kin: all open accounts (name, domain, login
/// identifiers, SSO, active subscriptions with amounts, payment methods brand+last4,
/// deletion hints/links), grouped and sorted; never passwords. Cover page explains
/// how to use it and to store it safely.
pub fn emergency_kit_pdf(
    settings: &Settings,
    data: &VaultData,
    now: DateTime<Utc>,
) -> Result<Vec<u8>, LetterError> {
    let mut c = Canvas::new("Digitaler Nachlass", Kind::Report)?;
    c.footer_left = "Spurlos · Digitaler Nachlass · vertraulich".into();
    let catalog = spurlos_catalog::Catalog::bundled();

    let mut open: Vec<&Account> = data.accounts.iter().filter(|a| is_open(a)).collect();
    open.sort_by_key(|a| a.name.to_lowercase());
    let (paid, other): (Vec<&Account>, Vec<&Account>) =
        open.iter().partition(|a| !a.payments.is_empty());

    let owner = settings.full_name.trim();
    let subtitle = if owner.is_empty() {
        format!("Stand: {}", fmt_date(now))
    } else {
        format!("von {owner} · Stand: {}", fmt_date(now))
    };
    title_block(&mut c, "Digitaler Nachlass", &subtitle);

    section(&mut c, "Wozu dieses Dokument dient");
    body(&mut c, "Dieses Dokument listet die Online-Konten, Abonnements und Zahlungsarten auf, die Spurlos für diese Person gefunden hat. Es soll Angehörigen oder Vertrauenspersonen im Notfall helfen, den Überblick zu behalten, Verträge zu kündigen und Konten zu schließen.");
    section(&mut c, "So wird es verwendet");
    body(&mut c, "1. Zuerst Zugriff auf das Haupt-E-Mail-Postfach und das Smartphone sichern: Fast alle Dienste setzen Passwörter über die E-Mail-Adresse zurück.\n\
2. Laufende Abonnements und Zahlungen zuerst beenden (Abschnitt „Bezahlte Dienste und Abonnements“), damit keine weiteren Kosten entstehen.\n\
3. Danach die übrigen Konten schließen: über den angegebenen Löschlink oder mit einem formlosen Löschantrag nach Art. 17 DSGVO an den Anbieter. Viele Anbieter verlangen eine Sterbeurkunde oder Vollmacht.\n\
4. Bei Diensten mit Anmeldung über Google, Apple oder Microsoft zuerst das jeweilige Hauptkonto regeln.");
    section(&mut c, "Sicher aufbewahren");
    body(&mut c, "Dieses Dokument enthält keine Passwörter, aber persönliche Angaben (E-Mail-Adressen, Benutzernamen, Zahlungsarten mit den letzten vier Ziffern). Bewahren Sie den Ausdruck an einem sicheren Ort auf (z. B. Schließfach, Tresor oder bei einer Vertrauensperson) und speichern Sie die Datei nur verschlüsselt. Passwörter gehören in einen Passwort-Manager, dessen Notfallzugang Sie separat regeln.");

    let recurring: Vec<(&Account, &PaymentRecord)> = open
        .iter()
        .filter_map(|a| active_recurring(a).map(|p| (*a, p)))
        .collect();
    let total: i64 = recurring
        .iter()
        .filter(|(_, p)| matches!(p.currency.as_deref(), None | Some("EUR")))
        .filter_map(|(_, p)| p.amount_minor)
        .sum();
    section(&mut c, "Übersicht");
    let mut overview = format!(
        "Offene Konten: {}\nDavon mit Zahlungen: {}\nLaufende Abonnements: {}",
        open.len(),
        paid.len(),
        recurring.len()
    );
    if total > 0 {
        overview.push_str(&format!(
            "\nSumme der zuletzt abgebuchten Abo-Beträge: {} (je Abo letzte Abbuchung, ohne Gewähr)",
            fmt_money(total, Some("EUR"))
        ));
    }
    body(&mut c, &overview);

    c.new_page();
    let groups = [
        ("Bezahlte Dienste und Abonnements", paid),
        ("Weitere Konten", other),
    ];
    let mut any = false;
    for (name, accounts) in groups {
        if accounts.is_empty() {
            continue;
        }
        any = true;
        section(&mut c, &format!("{name} ({})", accounts.len()));
        for a in accounts {
            let mut rows: Vec<(String, String)> = Vec::new();
            let ids: Vec<&str> = a.identifiers.iter().map(String::as_str).collect();
            rows.push((
                "Kennungen".into(),
                if ids.is_empty() {
                    "unbekannt".into()
                } else {
                    ids.join("\n")
                },
            ));
            if !a.sso.is_empty() {
                let sso: Vec<&str> = a.sso.iter().map(|s| s.label()).collect();
                rows.push(("Anmeldung über".into(), sso.join(", ")));
            }
            if let Some(p) = active_recurring(a) {
                let mut s = p.plan.clone().unwrap_or_else(|| "Abonnement".into());
                if let Some(m) = money_of(p) {
                    s.push_str(&format!(" – {m}"));
                }
                if let Some(d) = p.date {
                    s.push_str(&format!(" (zuletzt {})", fmt_date(d)));
                }
                rows.push(("Abonnement".into(), s));
            }
            let mut methods: Vec<String> = Vec::new();
            for m in a.payments.iter().filter_map(|p| p.method.as_ref()) {
                let l = m.label_de();
                if !methods.contains(&l) {
                    methods.push(l);
                }
            }
            if !methods.is_empty() {
                rows.push(("Zahlungsarten".into(), methods.join(", ")));
            }
            rows.push(("Status".into(), a.status.label_de().to_string()));
            let hint = match catalog.lookup(&a.domain) {
                Some(info) => {
                    let mut parts = Vec::new();
                    if let Some(u) = &info.url {
                        parts.push(format!("Löschseite: {u}"));
                    }
                    if let Some(e) = info.privacy_email.as_ref().or(info.email.as_ref()) {
                        parts.push(format!("Datenschutz-Kontakt: {e}"));
                    }
                    if parts.is_empty() {
                        "Löschantrag nach Art. 17 DSGVO an den Anbieter".to_string()
                    } else {
                        parts.join("\n")
                    }
                }
                None => format!(
                    "Kein Katalogeintrag: Löschantrag nach Art. 17 DSGVO an {}",
                    a.domain
                ),
            };
            rows.push(("Löschen".into(), hint));
            card(&mut c, &a.name, &a.domain, &rows);
        }
    }
    if !any {
        body(&mut c, "Es sind keine offenen Konten vorhanden.");
    }
    c.finish()
}

fn score_color(score: u8) -> (f32, f32, f32) {
    if score >= 75 {
        GREEN
    } else if score >= 50 {
        (0.78, 0.50, 0.05)
    } else {
        RED
    }
}

/// Privacy report PDF: score with breakdown, counts, top risks, subscriptions total,
/// open requests and deadlines.
pub fn privacy_report_pdf(
    data: &VaultData,
    score: &PrivacyScore,
    now: DateTime<Utc>,
) -> Result<Vec<u8>, LetterError> {
    let mut c = Canvas::new("Datenschutz-Bericht", Kind::Report)?;
    c.footer_left = "Spurlos · Datenschutz-Bericht".into();
    title_block(
        &mut c,
        "Datenschutz-Bericht",
        &format!("Stand: {}", fmt_datetime(now)),
    );

    // Score box.
    let col = score_color(score.score);
    c.ensure(34.0);
    let y0 = c.y;
    c.fill_rect(c.left, y0, c.width, 31.0, LIGHT);
    c.text(
        c.left + 6.0,
        y0 + 19.0,
        &score.score.to_string(),
        Style::Bold,
        40.0,
        col,
    );
    let num_w = c.text_w(Style::Bold, 40.0, &score.score.to_string());
    c.text(
        c.left + 8.0 + num_w,
        y0 + 19.0,
        "von 100 Punkten",
        Style::Regular,
        12.0,
        GRAY,
    );
    let verdict = if score.score >= 75 {
        "Gut aufgestellt"
    } else if score.score >= 50 {
        "Verbesserungswürdig"
    } else {
        "Kritisch: Handlungsbedarf"
    };
    c.text_right(c.right() - 6.0, y0 + 12.0, verdict, Style::Bold, 12.0, col);
    let bar_x = c.left + 6.0;
    let bar_w = c.width - 12.0;
    c.fill_rect(bar_x, y0 + 25.0, bar_w, 2.4, (0.82, 0.82, 0.85));
    c.fill_rect(
        bar_x,
        y0 + 25.0,
        bar_w * f32::from(score.score.min(100)) / 100.0,
        2.4,
        col,
    );
    c.y = y0 + 33.0;

    section(&mut c, "So setzt sich der Wert zusammen");
    for (label, got, max) in &score.parts {
        c.ensure(7.0);
        let y = c.y;
        c.text(c.left, y + 4.0, label, Style::Regular, 9.5, BLACK);
        let bx = c.left + 88.0;
        let bw = c.width - 88.0 - 22.0;
        c.fill_rect(bx, y + 1.6, bw, 2.6, (0.86, 0.86, 0.88));
        let frac = if *max == 0 {
            0.0
        } else {
            (*got as f32 / *max as f32).clamp(0.0, 1.0)
        };
        let col = if frac >= 0.75 {
            GREEN
        } else if frac >= 0.4 {
            (0.78, 0.50, 0.05)
        } else {
            RED
        };
        c.fill_rect(bx, y + 1.6, bw * frac, 2.6, col);
        c.text_right(
            c.right(),
            y + 4.0,
            &format!("{got} / {max}"),
            Style::Regular,
            9.0,
            GRAY,
        );
        c.y += 6.5;
    }

    section(&mut c, "Überblick");
    let rows = vec![
        vec!["Offene Konten".into(), score.open_accounts.to_string()],
        vec![
            "Gelöschte Konten".into(),
            score.deleted_accounts.to_string(),
        ],
        vec![
            "Konten mit hohem Risiko".into(),
            score.high_risk_accounts.to_string(),
        ],
        vec!["Offene Anfragen".into(), score.pending_requests.to_string()],
        vec![
            "Überfällige Anfragen".into(),
            score.overdue_requests.to_string(),
        ],
    ];
    c.table(&["Kennzahl".into(), "Anzahl".into()], &rows, 9.5);

    // Top risks.
    section(&mut c, "Wichtigste Risiken");
    let mut risks: Vec<(u32, &Account, Vec<String>)> = Vec::new();
    for a in data.accounts.iter().filter(|a| is_open(a)) {
        let mut pts = 0;
        let mut why = Vec::new();
        if a.security.password_pwned == Some(true) {
            pts += 5;
            why.push("Passwort in bekannten Datenlecks gefunden".to_string());
        }
        let confirmed: Vec<&str> = a
            .security
            .breaches
            .iter()
            .filter(|b| b.confirmed_for_user)
            .map(|b| b.title.as_str())
            .collect();
        if !confirmed.is_empty() {
            pts += 4;
            why.push(format!("Von Datenleck betroffen: {}", confirmed.join(", ")));
        }
        if let Some(n) = a.security.password_reuse_count.filter(|n| *n > 0) {
            pts += 3;
            why.push(format!("Passwort auch bei {n} weiteren Konten verwendet"));
        }
        if a.security.has_2fa == Some(false) {
            pts += 1;
            why.push("Keine Zwei-Faktor-Anmeldung".to_string());
        }
        if a.security.phone_login {
            pts += 1;
            why.push("Anmeldung per Telefonnummer (SIM-Swap-Risiko)".to_string());
        }
        if pts > 0 {
            risks.push((pts, a, why));
        }
    }
    risks.sort_by(|x, y| y.0.cmp(&x.0).then_with(|| x.1.name.cmp(&y.1.name)));
    if risks.is_empty() {
        body(&mut c, "Es sind keine bekannten Risiken erfasst.");
    } else {
        let rows: Vec<Vec<String>> = risks
            .iter()
            .take(10)
            .map(|(_, a, why)| vec![a.name.clone(), why.join("; ")])
            .collect();
        c.table(&["Konto".into(), "Risiko".into()], &rows, 9.0);
    }

    // Subscriptions.
    section(&mut c, "Abonnements und Zahlungen");
    let mut subs: Vec<(&Account, &PaymentRecord)> = data
        .accounts
        .iter()
        .filter(|a| is_open(a))
        .filter_map(|a| active_recurring(a).map(|p| (a, p)))
        .collect();
    subs.sort_by_key(|(a, _)| a.name.to_lowercase());
    if subs.is_empty() {
        body(&mut c, "Keine laufenden Abonnements erkannt.");
    } else {
        let mut total = 0i64;
        let mut rows: Vec<Vec<String>> = Vec::new();
        for (a, p) in &subs {
            if matches!(p.currency.as_deref(), None | Some("EUR")) {
                total += p.amount_minor.unwrap_or(0);
            }
            rows.push(vec![
                a.name.clone(),
                p.plan.clone().unwrap_or_else(|| "–".into()),
                money_of(p).unwrap_or_else(|| "–".into()),
                p.method
                    .as_ref()
                    .map_or_else(|| "–".into(), |m| m.label_de()),
                p.date.map_or_else(|| "–".into(), fmt_date),
            ]);
        }
        c.table(
            &[
                "Dienst".into(),
                "Tarif".into(),
                "Betrag".into(),
                "Zahlungsart".into(),
                "Zuletzt".into(),
            ],
            &rows,
            9.0,
        );
        c.gap(2.0);
        body(
            &mut c,
            &format!(
                "Summe der zuletzt abgebuchten Beträge (EUR): {}. Die Abrechnungszeiträume sind nicht bekannt; der Wert ist eine Orientierung.",
                fmt_money(total, Some("EUR"))
            ),
        );
    }

    // Requests.
    section(&mut c, "Offene Anfragen und Fristen");
    let mut reqs: Vec<&spurlos_core::DataRequest> = data
        .requests
        .iter()
        .filter(|r| matches!(r.status, RequestStatus::Draft | RequestStatus::Sent))
        .collect();
    reqs.sort_by_key(|r| r.deadline.unwrap_or(r.created_at));
    if reqs.is_empty() {
        body(&mut c, "Keine offenen Anfragen.");
    } else {
        let rows: Vec<Vec<String>> = reqs
            .iter()
            .map(|r| {
                let overdue =
                    r.status == RequestStatus::Sent && r.deadline.is_some_and(|d| d < now);
                vec![
                    r.target.clone(),
                    r.kind.label_de().to_string(),
                    r.sent_at.map_or_else(|| "–".into(), fmt_date),
                    r.deadline.map_or_else(|| "–".into(), fmt_date),
                    if overdue {
                        "überfällig".to_string()
                    } else {
                        r.status.label_de().to_string()
                    },
                ]
            })
            .collect();
        c.table(
            &[
                "Empfänger".into(),
                "Art".into(),
                "Gesendet".into(),
                "Frist".into(),
                "Status".into(),
            ],
            &rows,
            9.0,
        );
    }
    c.finish()
}
