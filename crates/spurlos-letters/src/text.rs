//! Plain-text rendering of a [`LetterDoc`] for e-mail.

use crate::util::{attachment_blocks, Block};
use crate::LetterDoc;
use std::fmt::Write;

/// Plain-text rendering for e-mail (no attachments inline except a short list of them;
/// evidence tables rendered as text).
pub fn render_text(doc: &LetterDoc) -> String {
    let mut s = String::new();

    let _ = writeln!(s, "Betreff: {}", doc.subject);
    if let Some(r) = &doc.reference {
        let _ = writeln!(s, "{r}");
    }
    s.push('\n');
    let _ = writeln!(s, "{}\n", doc.salutation);
    for p in &doc.paragraphs {
        let _ = writeln!(s, "{}\n", p.trim_end());
    }
    let _ = writeln!(s, "{}\n", doc.closing);
    let _ = writeln!(s, "{}", doc.sender.name);
    for l in &doc.sender.address_lines {
        let _ = writeln!(s, "{l}");
    }
    if let Some(e) = &doc.sender.email {
        let _ = writeln!(s, "{e}");
    }
    let _ = writeln!(s, "{}", doc.place_date);

    if !doc.attachments.is_empty() {
        let _ = writeln!(s, "\nAnlagen:");
        let rendered: Vec<(String, Vec<Block>)> =
            doc.attachments.iter().map(attachment_blocks).collect();
        for (i, (title, _)) in rendered.iter().enumerate() {
            let _ = writeln!(s, "{}. {title}", i + 1);
        }
        for (i, (title, blocks)) in rendered.iter().enumerate() {
            let _ = writeln!(s, "\n----------------------------------------");
            let _ = writeln!(s, "Anlage {}: {title}", i + 1);
            let _ = writeln!(s, "----------------------------------------");
            for b in blocks {
                match b {
                    Block::Heading(h) => {
                        let _ = writeln!(s, "\n{h}");
                    }
                    Block::Para(p) => {
                        let _ = writeln!(s, "\n{}", p.trim_end());
                    }
                    Block::Table { columns, rows } => {
                        let _ = writeln!(s, "\n{}", columns.join(" | "));
                        for r in rows {
                            let _ = writeln!(s, "{}", r.join(" | "));
                        }
                    }
                    Block::Mono { caption, text } => {
                        let _ = writeln!(s, "\n[{caption}]\n{}", text.trim_end());
                    }
                    Block::Signature { left, right } => {
                        let _ = writeln!(s, "\n______________________    ______________________");
                        let _ = writeln!(s, "{left}    {right}");
                    }
                }
            }
        }
    }
    let mut out = s.trim_end().to_string();
    out.push('\n');
    out
}
