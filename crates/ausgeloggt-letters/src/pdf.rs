//! DIN 5008 (Form B) letter PDF.

use crate::layout::{Canvas, Kind, Style, BLACK, GRAY, RULE};
use crate::util::{attachment_blocks, Block};
use crate::{LetterDoc, LetterError};

const BODY: f32 = 11.0;

/// DIN 5008 PDF.
pub fn render_pdf(doc: &LetterDoc) -> Result<Vec<u8>, LetterError> {
    let mut c = Canvas::new(&doc.subject, Kind::Letter)?;
    c.cont_header = format!(
        "{} an {} · {}",
        doc.sender.name, doc.recipient.name, doc.place_date
    );

    // Sender block (letterhead area, top right).
    let sx = 125.0;
    let mut y = 21.0;
    c.text(sx, y, &doc.sender.name, Style::Bold, 10.0, BLACK);
    y += 4.6;
    for l in &doc.sender.address_lines {
        c.text(sx, y, l, Style::Regular, 9.0, BLACK);
        y += 4.1;
    }
    if let Some(e) = &doc.sender.email {
        c.text(sx, y, e, Style::Regular, 9.0, GRAY);
    }

    // Return address line (Rücksendeangabe) and address window: 20 mm / 45 mm, 85 x 45 mm.
    let mut back = doc.sender.name.clone();
    for l in &doc.sender.address_lines {
        back.push_str(" · ");
        back.push_str(l);
    }
    let back_size = 7.0;
    c.text(20.0, 50.0, &back, Style::Regular, back_size, GRAY);
    let bw = c
        .text_w(Style::Regular, back_size, &c.clean(Style::Regular, &back))
        .min(85.0);
    c.hline(20.0, 20.0 + bw, 51.0, 0.3, GRAY);
    let mut lines: Vec<String> = vec![doc.recipient.name.clone()];
    lines.extend(doc.recipient.address_lines.iter().cloned());
    let step = 4.5;
    let n = lines.len() as f32;
    let mut ay = 62.7 + 3.6;
    if n > 5.0 {
        ay -= (n - 5.0) * step;
    }
    for l in &lines {
        // Shrink very long lines to fit the 85 mm window.
        let mut size = 11.0;
        while size > 8.0 && c.text_w(Style::Regular, size, &c.clean(Style::Regular, l)) > 85.0 {
            size -= 0.5;
        }
        c.text(20.0, ay, l, Style::Regular, size, BLACK);
        ay += step;
    }

    // Info block (right): place and date.
    c.text(sx, 56.0, &doc.place_date, Style::Regular, 10.0, BLACK);

    // Subject.
    c.y = 98.5;
    c.para(&doc.subject, Style::Bold, BODY, BLACK, c.left, c.width, 1.3);
    if let Some(r) = &doc.reference {
        c.gap(1.0);
        c.para(r, Style::Regular, 9.5, GRAY, c.left, c.width, 1.3);
    }
    c.gap(7.0);

    c.line_at(c.left, &doc.salutation, Style::Regular, BODY, BLACK, 1.35);
    c.gap(3.0);
    for p in &doc.paragraphs {
        c.para(p, Style::Regular, BODY, BLACK, c.left, c.width, 1.35);
        c.gap(3.0);
    }

    // Closing + signature (kept together).
    c.ensure(38.0);
    c.line_at(c.left, &doc.closing, Style::Regular, BODY, BLACK, 1.35);
    c.gap(15.0);
    c.line_at(c.left, &doc.sender.name, Style::Regular, BODY, BLACK, 1.35);

    let rendered: Vec<(String, Vec<Block>)> =
        doc.attachments.iter().map(attachment_blocks).collect();
    if !rendered.is_empty() {
        c.gap(7.0);
        c.ensure(6.0 + 4.6 * rendered.len() as f32);
        c.line_at(c.left, "Anlagen", Style::Bold, 9.5, BLACK, 1.35);
        for (i, (title, _)) in rendered.iter().enumerate() {
            let line = format!("{}. {title}", i + 1);
            c.para(&line, Style::Regular, 9.5, BLACK, c.left, c.width, 1.35);
        }
    }

    for (i, (title, blocks)) in rendered.iter().enumerate() {
        c.new_page();
        c.para(
            &format!("Anlage {}: {title}", i + 1),
            Style::Bold,
            14.0,
            BLACK,
            c.left,
            c.width,
            1.3,
        );
        c.hline(c.left, c.right(), c.y + 1.0, 0.5, RULE);
        c.gap(5.0);
        draw_blocks(&mut c, blocks);
    }
    c.finish()
}

pub(crate) fn draw_blocks(c: &mut Canvas, blocks: &[Block]) {
    for b in blocks {
        match b {
            Block::Heading(h) => {
                c.gap(4.0);
                c.ensure(14.0);
                c.para(h, Style::Bold, 11.0, BLACK, c.left, c.width, 1.3);
                c.gap(2.0);
            }
            Block::Para(p) => {
                c.para(p, Style::Regular, 10.0, BLACK, c.left, c.width, 1.35);
                c.gap(3.0);
            }
            Block::Table { columns, rows } => {
                c.table(columns, rows, 8.5);
                c.gap(4.0);
            }
            Block::Mono { caption, text } => {
                c.ensure(18.0);
                c.line_at(c.left, caption, Style::Bold, 8.5, GRAY, 1.3);
                let size = 6.8;
                let lh = size / (72.0 / 25.4) * 1.2;
                let lines: Vec<String> = text
                    .replace("\r\n", "\n")
                    .split('\n')
                    .flat_map(|l| c.wrap(Style::Mono, size, &mono_prepare(l), c.width - 3.0))
                    .collect();
                for l in lines {
                    c.ensure(lh);
                    let y = c.y;
                    c.text(c.left + 1.5, y + lh * 0.78, &l, Style::Mono, size, BLACK);
                    c.y += lh;
                }
                c.gap(4.0);
            }
            Block::Signature { left, right } => {
                c.ensure(30.0);
                c.gap(18.0);
                let y = c.y;
                let half = (c.width - 10.0) / 2.0;
                c.hline(c.left, c.left + half, y, 0.4, BLACK);
                c.hline(c.left + half + 10.0, c.right(), y, 0.4, BLACK);
                c.gap(1.5);
                let ly = c.y + 3.0;
                c.text(c.left, ly, left, Style::Regular, 8.5, GRAY);
                c.text(c.left + half + 10.0, ly, right, Style::Regular, 8.5, GRAY);
                c.gap(6.0);
            }
        }
    }
}

/// Keep leading indentation of header continuation lines visible in monospace.
fn mono_prepare(l: &str) -> String {
    l.replace('\t', "    ")
}
