//! Tiny layout engine on top of `printpdf`: measured text wrapping, paragraphs, tables,
//! page breaks, page numbers. All coordinates are millimetres from the top-left corner
//! of the A4 page (converted to PDF points at the very end).

use crate::LetterError;
use printpdf::{
    Color, Line, LinePoint, Mm, Op, PaintMode, ParsedFont, PdfDocument, PdfFontHandle, PdfPage,
    PdfSaveOptions, Point, Pt, Rect, Rgb, TextItem,
};
use ttf_parser::Face;

static SANS: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
static BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");
static MONO: &[u8] = include_bytes!("../fonts/LiberationMono-Regular.ttf");

pub(crate) const PAGE_W: f32 = 210.0;
pub(crate) const PAGE_H: f32 = 297.0;
const MM_TO_PT: f32 = 72.0 / 25.4;

pub(crate) type Rgbf = (f32, f32, f32);
pub(crate) const BLACK: Rgbf = (0.0, 0.0, 0.0);
pub(crate) const GRAY: Rgbf = (0.38, 0.38, 0.38);
pub(crate) const LIGHT: Rgbf = (0.93, 0.93, 0.94);
pub(crate) const RULE: Rgbf = (0.72, 0.72, 0.74);
pub(crate) const RED: Rgbf = (0.72, 0.12, 0.12);
pub(crate) const GREEN: Rgbf = (0.12, 0.5, 0.25);
pub(crate) const ACCENT: Rgbf = (0.12, 0.30, 0.55);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Style {
    Regular,
    Bold,
    Mono,
}

impl Style {
    fn idx(self) -> usize {
        match self {
            Style::Regular => 0,
            Style::Bold => 1,
            Style::Mono => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// DIN 5008 letter: marks, continuation header.
    Letter,
    /// Report / emergency kit: plain pages with footer title.
    Report,
}

fn pt(mm: f32) -> Pt {
    Pt(mm * MM_TO_PT)
}

fn rgb(c: Rgbf) -> Color {
    Color::Rgb(Rgb::new(c.0, c.1, c.2, None))
}

pub(crate) struct Canvas {
    doc: PdfDocument,
    ids: Vec<printpdf::FontId>,
    faces: Vec<Face<'static>>,
    pages: Vec<Vec<Op>>,
    cur: Vec<Op>,
    kind: Kind,
    /// Current vertical position (top of the next line), mm from the top.
    pub y: f32,
    pub left: f32,
    pub width: f32,
    /// Top of the writing area on continuation pages.
    pub top: f32,
    /// Bottom of the writing area.
    pub bottom: f32,
    pub footer_left: String,
    pub cont_header: String,
    first_page: bool,
}

impl Canvas {
    pub(crate) fn new(title: &str, kind: Kind) -> Result<Self, LetterError> {
        let mut doc = PdfDocument::new(title);
        doc.metadata.info.creator = "Spurlos".to_string();
        doc.metadata.info.producer = "Spurlos".to_string();
        let mut ids = Vec::new();
        let mut faces = Vec::new();
        for bytes in [SANS, BOLD, MONO] {
            let mut warnings = Vec::new();
            let parsed = ParsedFont::from_bytes(bytes, 0, &mut warnings)
                .ok_or_else(|| LetterError::Pdf("Schrift konnte nicht geladen werden".into()))?;
            ids.push(doc.add_font(&parsed));
            faces.push(
                Face::parse(bytes, 0)
                    .map_err(|e| LetterError::Pdf(format!("Schriftmetriken: {e}")))?,
            );
        }
        let (left, width, top) = match kind {
            Kind::Letter => (25.0, 165.0, 27.0),
            Kind::Report => (20.0, 170.0, 22.0),
        };
        let mut c = Canvas {
            doc,
            ids,
            faces,
            pages: Vec::new(),
            cur: Vec::new(),
            kind,
            y: top,
            left,
            width,
            top,
            bottom: 274.0,
            footer_left: String::new(),
            cont_header: String::new(),
            first_page: true,
        };
        c.decorate();
        Ok(c)
    }

    pub(crate) fn right(&self) -> f32 {
        self.left + self.width
    }

    // ---- fonts / measuring -------------------------------------------------

    /// Replace characters the font cannot show; drop control characters.
    pub(crate) fn clean(&self, style: Style, s: &str) -> String {
        let face = &self.faces[style.idx()];
        s.chars()
            .filter_map(|c| match c {
                '\t' => Some(' '),
                '\n' => Some('\n'),
                '\u{a0}' | '\u{202f}' | '\u{2009}' => Some(' '),
                '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}' | '\u{ad}' => None,
                c if c.is_control() => None,
                c if face.glyph_index(c).is_some() => Some(c),
                _ => Some('?'),
            })
            .collect()
    }

    fn char_w(&self, style: Style, size: f32, c: char) -> f32 {
        let face = &self.faces[style.idx()];
        let upm = f32::from(face.units_per_em());
        face.glyph_index(c)
            .and_then(|g| face.glyph_hor_advance(g))
            .map_or(0.5, |a| f32::from(a) / upm)
            * size
            / MM_TO_PT
    }

    /// Width of `s` in mm at `size` pt.
    pub(crate) fn text_w(&self, style: Style, size: f32, s: &str) -> f32 {
        s.chars().map(|c| self.char_w(style, size, c)).sum()
    }

    /// Greedy wrap to `width` mm; over-long tokens are broken by characters.
    pub(crate) fn wrap(&self, style: Style, size: f32, text: &str, width: f32) -> Vec<String> {
        let text = self.clean(style, text);
        let mut out = Vec::new();
        for raw in text.split('\n') {
            let lead: String = raw.chars().take_while(|c| *c == ' ').collect();
            let lead_w = self.text_w(style, size, &lead);
            let mut line = lead.clone();
            let mut line_w = lead_w;
            let mut have_word = false;
            for word in raw.trim_start_matches(' ').split(' ') {
                if word.is_empty() {
                    continue;
                }
                let ww = self.text_w(style, size, word);
                let space = self.char_w(style, size, ' ');
                let need = if have_word { space + ww } else { ww };
                if line_w + need <= width + 0.01 {
                    if have_word {
                        line.push(' ');
                    }
                    line.push_str(word);
                    line_w += need;
                    have_word = true;
                    continue;
                }
                if have_word {
                    out.push(std::mem::take(&mut line));
                    line = lead.clone();
                    line_w = lead_w;
                    have_word = false;
                }
                if ww <= width - lead_w + 0.01 {
                    line.push_str(word);
                    line_w += ww;
                    have_word = true;
                } else {
                    // hard break
                    for ch in word.chars() {
                        let cw = self.char_w(style, size, ch);
                        if line_w + cw > width && have_word {
                            out.push(std::mem::take(&mut line));
                            line = lead.clone();
                            line_w = lead_w;
                        }
                        line.push(ch);
                        line_w += cw;
                        have_word = true;
                    }
                }
            }
            out.push(line.trim_end().to_string());
        }
        out
    }

    // ---- primitives ---------------------------------------------------------

    /// Text with its baseline at `y` mm from the top.
    pub(crate) fn text(&mut self, x: f32, y: f32, s: &str, style: Style, size: f32, col: Rgbf) {
        let s = self.clean(style, s);
        if s.trim().is_empty() {
            return;
        }
        self.cur.extend([
            Op::StartTextSection,
            Op::SetFont {
                font: PdfFontHandle::External(self.ids[style.idx()].clone()),
                size: Pt(size),
            },
            Op::SetFillColor { col: rgb(col) },
            Op::SetTextCursor {
                pos: Point {
                    x: pt(x),
                    y: pt(PAGE_H - y),
                },
            },
            Op::ShowText {
                items: vec![TextItem::Text(s)],
            },
            Op::EndTextSection,
        ]);
    }

    pub(crate) fn text_right(
        &mut self,
        x_right: f32,
        y: f32,
        s: &str,
        style: Style,
        size: f32,
        col: Rgbf,
    ) {
        let w = self.text_w(style, size, &self.clean(style, s));
        self.text(x_right - w, y, s, style, size, col);
    }

    pub(crate) fn hline(&mut self, x1: f32, x2: f32, y: f32, thickness: f32, col: Rgbf) {
        self.line(x1, y, x2, y, thickness, col);
    }

    pub(crate) fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, col: Rgbf) {
        self.cur.extend([
            Op::SetOutlineColor { col: rgb(col) },
            Op::SetOutlineThickness { pt: Pt(thickness) },
            Op::DrawLine {
                line: Line {
                    points: vec![
                        LinePoint {
                            p: Point {
                                x: pt(x1),
                                y: pt(PAGE_H - y1),
                            },
                            bezier: false,
                        },
                        LinePoint {
                            p: Point {
                                x: pt(x2),
                                y: pt(PAGE_H - y2),
                            },
                            bezier: false,
                        },
                    ],
                    is_closed: false,
                },
            },
        ]);
    }

    /// Filled rectangle, `y` = top edge.
    pub(crate) fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, col: Rgbf) {
        self.cur.extend([
            Op::SetFillColor { col: rgb(col) },
            Op::DrawRectangle {
                rectangle: Rect {
                    x: pt(x),
                    y: pt(PAGE_H - y - h),
                    width: pt(w),
                    height: pt(h),
                    mode: Some(PaintMode::Fill),
                    winding_order: None,
                },
            },
        ]);
    }

    // ---- pages --------------------------------------------------------------

    /// Page furniture: DIN fold / hole marks.
    fn decorate(&mut self) {
        if self.kind == Kind::Letter {
            if self.first_page {
                self.hline(2.0, 7.0, 105.0, 0.4, GRAY);
                self.hline(2.0, 7.0, 210.0, 0.4, GRAY);
            }
            self.hline(2.0, 6.0, 148.5, 0.4, GRAY);
        }
    }

    pub(crate) fn new_page(&mut self) {
        let done = std::mem::take(&mut self.cur);
        self.pages.push(done);
        self.first_page = false;
        self.y = self.top;
        self.decorate();
        if self.kind == Kind::Letter && !self.cont_header.is_empty() {
            let h = self.cont_header.clone();
            self.text(self.left, 17.0, &h, Style::Regular, 8.0, GRAY);
            self.hline(self.left, self.right(), 19.0, 0.3, RULE);
        }
    }

    /// Start a new page unless `h` mm still fit.
    pub(crate) fn ensure(&mut self, h: f32) {
        if self.y + h > self.bottom {
            self.new_page();
        }
    }

    pub(crate) fn gap(&mut self, h: f32) {
        self.y += h;
    }

    // ---- text blocks --------------------------------------------------------

    fn base_offset(size: f32, lh: f32) -> f32 {
        let sz = size / MM_TO_PT;
        (lh - sz) / 2.0 + 0.8 * sz
    }

    /// Single line at the current position, advancing by one line.
    pub(crate) fn line_at(
        &mut self,
        x: f32,
        s: &str,
        style: Style,
        size: f32,
        col: Rgbf,
        lh_factor: f32,
    ) {
        let lh = size / MM_TO_PT * lh_factor;
        self.ensure(lh);
        let by = self.y + Self::base_offset(size, lh);
        self.text(x, by, s, style, size, col);
        self.y += lh;
    }

    /// Wrapped paragraph (lines separated by `\n`; "- " and "1. " items get hanging
    /// indents) flowing over page breaks.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn para(
        &mut self,
        text: &str,
        style: Style,
        size: f32,
        col: Rgbf,
        x: f32,
        width: f32,
        lh_factor: f32,
    ) {
        for raw in text.split('\n') {
            let raw = raw.trim_end();
            if raw.is_empty() {
                self.y += size / MM_TO_PT * 0.6;
                continue;
            }
            let (marker, rest) = split_marker(raw);
            let (hang, rest) = match marker {
                Some(m) => {
                    let m = if m == "-" { "•".to_string() } else { m };
                    let w = self.text_w(style, size, &format!("{m} ")).max(4.0);
                    (Some((m, w)), rest)
                }
                None => (None, raw),
            };
            let indent = hang.as_ref().map_or(0.0, |(_, w)| *w);
            let lines = self.wrap(style, size, rest, width - indent);
            for (i, l) in lines.iter().enumerate() {
                let lh = size / MM_TO_PT * lh_factor;
                self.ensure(lh);
                let by = self.y + Self::base_offset(size, lh);
                if i == 0 {
                    if let Some((m, _)) = &hang {
                        self.text(x, by, m, style, size, col);
                    }
                }
                self.text(x + indent, by, l, style, size, col);
                self.y += lh;
            }
        }
    }

    // ---- tables -------------------------------------------------------------

    fn col_widths(
        &self,
        columns: &[String],
        rows: &[Vec<String>],
        size: f32,
        total: f32,
        pad: f32,
    ) -> Vec<f32> {
        let n = columns.len().max(1);
        let mut min = vec![0.0f32; n];
        let mut pref = vec![0.0f32; n];
        let measure = |c: usize, s: &str, bold: bool, min: &mut [f32], pref: &mut [f32]| {
            let st = if bold { Style::Bold } else { Style::Regular };
            let s = self.clean(st, s);
            let full = self.text_w(st, size, &s);
            let longest_word = s
                .split_whitespace()
                .map(|w| self.text_w(st, size, w))
                .fold(0.0, f32::max);
            min[c] = min[c].max(longest_word.min(60.0));
            pref[c] = pref[c].max(full.min(80.0));
        };
        for (c, h) in columns.iter().enumerate() {
            measure(c, h, true, &mut min, &mut pref);
        }
        for r in rows {
            for (c, cell) in r.iter().enumerate().take(n) {
                measure(c, cell, false, &mut min, &mut pref);
            }
        }
        let avail = total - 2.0 * pad * n as f32;
        // Short columns (dates, counts) keep their natural width; the rest shares the
        // remaining space between its minimum and preferred width.
        let fixed: Vec<bool> = pref.iter().map(|p| *p <= 34.0).collect();
        let fixed_sum: f32 = pref
            .iter()
            .zip(&fixed)
            .filter(|(_, f)| **f)
            .map(|(p, _)| p)
            .sum();
        let flex: Vec<usize> = (0..n).filter(|i| !fixed[*i]).collect();
        let rest = avail - fixed_sum;
        let sum_min: f32 = flex.iter().map(|i| min[*i]).sum();
        let sum_pref: f32 = flex.iter().map(|i| pref[*i]).sum();
        let mut w = pref.clone();
        if flex.is_empty() {
            let extra = avail - fixed_sum;
            for p in &mut w {
                *p += extra * *p / fixed_sum.max(0.001);
            }
        } else if sum_pref <= rest {
            let extra = rest - sum_pref;
            for i in &flex {
                w[*i] = pref[*i] + extra * pref[*i] / sum_pref.max(0.001);
            }
        } else if sum_min >= rest {
            for i in &flex {
                w[*i] = min[*i] * rest / sum_min.max(0.001);
            }
        } else {
            let spare = rest - sum_min;
            let want: f32 = flex.iter().map(|i| pref[*i] - min[*i]).sum();
            for i in &flex {
                w[*i] = min[*i] + spare * (pref[*i] - min[*i]) / want.max(0.001);
            }
        }
        for x in &mut w {
            *x += 2.0 * pad;
        }
        w
    }

    /// Table with a shaded header row (repeated after page breaks) and wrapping cells.
    pub(crate) fn table(&mut self, columns: &[String], rows: &[Vec<String>], size: f32) {
        let pad = 1.6;
        let lh = size / MM_TO_PT * 1.25;
        let widths = self.col_widths(columns, rows, size, self.width, pad);
        let header_h = {
            let lines = columns
                .iter()
                .zip(&widths)
                .map(|(c, w)| self.wrap(Style::Bold, size, c, w - 2.0 * pad).len())
                .max()
                .unwrap_or(1);
            lines as f32 * lh + 2.0 * pad
        };
        let max_lines = (((self.bottom - self.top - header_h) / lh) as usize)
            .saturating_sub(2)
            .max(3);
        let draw_header = |c: &mut Canvas| {
            c.ensure(header_h + 12.0);
            let y0 = c.y;
            c.fill_rect(c.left, y0, c.width, header_h, LIGHT);
            let mut x = c.left;
            for (col, w) in columns.iter().zip(&widths) {
                let lines = c.wrap(Style::Bold, size, col, w - 2.0 * pad);
                for (i, l) in lines.iter().enumerate() {
                    let by = y0 + pad + i as f32 * lh + Self::base_offset(size, lh);
                    c.text(x + pad, by, l, Style::Bold, size, BLACK);
                }
                x += w;
            }
            c.hline(c.left, c.right(), y0 + header_h, 0.5, GRAY);
            c.y = y0 + header_h;
        };
        draw_header(self);
        for (ri, row) in rows.iter().enumerate() {
            let mut cells: Vec<Vec<String>> = widths
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    let text = row.get(i).map_or("", String::as_str);
                    let mut l = self.wrap(Style::Regular, size, text, w - 2.0 * pad);
                    if l.len() > max_lines {
                        l.truncate(max_lines);
                        if let Some(last) = l.last_mut() {
                            last.push('…');
                        }
                    }
                    l
                })
                .collect();
            let n = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
            let h = n as f32 * lh + 2.0 * pad;
            if self.y + h > self.bottom {
                self.new_page();
                draw_header(self);
            }
            let y0 = self.y;
            if ri % 2 == 1 {
                self.fill_rect(self.left, y0, self.width, h, (0.975, 0.975, 0.98));
            }
            let mut x = self.left;
            for (cell, w) in cells.iter_mut().zip(&widths) {
                for (i, l) in cell.iter().enumerate() {
                    let by = y0 + pad + i as f32 * lh + Self::base_offset(size, lh);
                    self.text(x + pad, by, l, Style::Regular, size, BLACK);
                }
                x += w;
            }
            self.hline(self.left, self.right(), y0 + h, 0.25, RULE);
            self.y = y0 + h;
        }
    }

    // ---- finish -------------------------------------------------------------

    pub(crate) fn finish(mut self) -> Result<Vec<u8>, LetterError> {
        let last = std::mem::take(&mut self.cur);
        self.pages.push(last);
        let total = self.pages.len();
        let mut pages = std::mem::take(&mut self.pages);
        // Footers need `self` for measuring; build them one by one.
        let mut footer_ops: Vec<Vec<Op>> = Vec::new();
        for i in 0..total {
            self.cur = Vec::new();
            let label = format!("Seite {} von {total}", i + 1);
            self.text_right(self.right(), 285.0, &label, Style::Regular, 8.0, GRAY);
            if !self.footer_left.is_empty() {
                let f = self.footer_left.clone();
                self.text(self.left, 285.0, &f, Style::Regular, 8.0, GRAY);
            }
            footer_ops.push(std::mem::take(&mut self.cur));
        }
        for (p, f) in pages.iter_mut().zip(footer_ops) {
            p.extend(f);
        }
        let pdf_pages: Vec<PdfPage> = pages
            .into_iter()
            .map(|ops| PdfPage::new(Mm(PAGE_W), Mm(PAGE_H), ops))
            .collect();
        let mut warnings = Vec::new();
        let bytes = self
            .doc
            .with_pages(pdf_pages)
            .save(&PdfSaveOptions::default(), &mut warnings);
        if bytes.is_empty() {
            return Err(LetterError::Pdf("leeres PDF".into()));
        }
        Ok(bytes)
    }
}

/// Detect "- " / "• " / "12. " list markers: returns (marker, rest).
fn split_marker(line: &str) -> (Option<String>, &str) {
    if let Some(r) = line.strip_prefix("- ").or_else(|| line.strip_prefix("• ")) {
        return (Some("-".to_string()), r);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if (1..=2).contains(&digits) && line[digits..].starts_with(". ") {
        return (Some(line[..=digits].to_string()), &line[digits + 2..]);
    }
    (None, line)
}
