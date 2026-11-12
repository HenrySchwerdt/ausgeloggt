//! Shared helpers: German date formatting and the neutral block model that both the
//! text and the PDF renderer consume.

use crate::{Attachment, Party};
use chrono::{DateTime, Utc};

/// `dd.mm.yyyy`
pub(crate) fn fmt_date(d: DateTime<Utc>) -> String {
    d.format("%d.%m.%Y").to_string()
}

/// `dd.mm.yyyy HH:MM`
pub(crate) fn fmt_datetime(d: DateTime<Utc>) -> String {
    d.format("%d.%m.%Y %H:%M").to_string()
}

pub(crate) fn fmt_opt_datetime(d: Option<DateTime<Utc>>) -> String {
    d.map(fmt_datetime).unwrap_or_else(|| "keine Angabe".into())
}

/// Amount in minor units as German money string, e.g. `9,99 €`.
pub(crate) fn fmt_money(minor: i64, currency: Option<&str>) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    let cur = match currency {
        Some("EUR") | None => "€".to_string(),
        Some("USD") => "US-$".to_string(),
        Some(c) => c.to_string(),
    };
    format!("{sign}{},{:02} {cur}", abs / 100, abs % 100)
}

/// Neutral content block of an attachment.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Block {
    Heading(String),
    Para(String),
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// Small monospace block (raw headers) with a caption.
    Mono {
        caption: String,
        text: String,
    },
    /// Two signature lines side by side.
    Signature {
        left: String,
        right: String,
    },
}

pub(crate) fn party_one_line(p: &Party) -> String {
    let mut s = p.name.clone();
    for l in &p.address_lines {
        if !l.trim().is_empty() {
            s.push_str(", ");
            s.push_str(l.trim());
        }
    }
    s
}

/// Title and blocks of attachment `att` (without the "Anlage n:" prefix).
pub(crate) fn attachment_blocks(att: &Attachment) -> (String, Vec<Block>) {
    match att {
        Attachment::CeaseAndDesistDeclaration {
            sender,
            recipient,
            scope,
        } => (
            "Strafbewehrte Unterlassungserklärung".to_string(),
            declaration_blocks(sender, recipient, scope),
        ),
        Attachment::EvidenceTable {
            title,
            columns,
            rows,
            raw_headers,
        } => {
            let mut blocks = vec![Block::Table {
                columns: columns.clone(),
                rows: rows.clone(),
            }];
            if !raw_headers.is_empty() {
                blocks.push(Block::Heading("Rohe Kopfzeilen der Nachrichten".into()));
                let n = raw_headers.len();
                for (i, h) in raw_headers.iter().enumerate() {
                    blocks.push(Block::Mono {
                        caption: format!("Nachricht {} von {n}", i + 1),
                        text: h.clone(),
                    });
                }
            }
            (title.clone(), blocks)
        }
        Attachment::Section { title, paragraphs } => (
            title.clone(),
            paragraphs.iter().cloned().map(Block::Para).collect(),
        ),
    }
}

fn declaration_blocks(sender: &Party, recipient: &Party, scope: &str) -> Vec<Block> {
    let scope = scope.trim();
    let target = if scope.is_empty() {
        "an die E-Mail-Adresse(n) des Gläubigers".to_string()
    } else {
        format!("an die E-Mail-Adresse(n) {scope}")
    };
    vec![
        Block::Para(format!(
            "Der/die Unterzeichnende, {} (nachfolgend „Schuldner“), verpflichtet sich gegenüber {} (nachfolgend „Gläubiger“), ohne Anerkennung einer Rechtspflicht, jedoch rechtsverbindlich:",
            party_one_line(recipient),
            party_one_line(sender)
        )),
        Block::Para(format!(
            "1. Es ab sofort zu unterlassen, ohne vorherige ausdrückliche Einwilligung des Gläubigers Werbe-E-Mails, Newsletter oder sonstige Werbemitteilungen {target} zu versenden oder durch Dritte versenden zu lassen."
        )),
        Block::Para(
            "2. Für jeden Fall der schuldhaften Zuwiderhandlung gegen die Verpflichtung aus Ziffer 1 an den Gläubiger eine Vertragsstrafe zu zahlen, deren Höhe der Gläubiger nach billigem Ermessen festsetzt (§ 315 BGB) und die im Streitfall vom zuständigen Gericht auf ihre Angemessenheit überprüft werden kann (Hamburger Brauch)."
                .into(),
        ),
        Block::Para(
            "3. Die zu Werbezwecken gespeicherten personenbezogenen Daten des Gläubigers nicht weiter zu Werbezwecken zu verarbeiten, nicht an Dritte zum Zweck der Werbung weiterzugeben und sie zu löschen, soweit keine gesetzlichen Aufbewahrungspflichten entgegenstehen; in diesem Fall die Verarbeitung einzuschränken."
                .into(),
        ),
        Block::Para(
            "Diese Erklärung ist unterschrieben an den Gläubiger zurückzusenden (vorab per E-Mail genügt zur Fristwahrung, das Original ist nachzureichen)."
                .into(),
        ),
        Block::Signature {
            left: "Ort, Datum".into(),
            right: "Unterschrift (vertretungsberechtigte Person)".into(),
        },
    ]
}
