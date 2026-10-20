//! Laying out a block of text: digits, bidi, shaping, line breaking, alignment and kashida.
//!
//! Each paragraph (split at '\n') goes through:
//! 1. digit substitution (Western, Arabic-Indic or Persian),
//! 2. bidi (UAX #9, unicode-bidi) to find the runs of each direction,
//! 3. shaping of each run with harfrust (HarfBuzz's Rust port) to measure it,
//! 4. greedy line breaking at UAX #14 opportunities (icu_segmenter) when there is a width,
//! 5. each line reordered for display and shaped run by run, in visual order,
//! 6. alignment, or justification by kashida (tatweel, W3C ALREQ §7.2.5) and then by spaces.
//!
//! Coordinates are px with y down. Point text (no width) is anchored at x = 0 on the first
//! baseline (y = 0); box text fills 0..width with the first baseline at the font's ascent.

use std::ops::Range;

use harfrust::{shape, Buffer, Direction, ShapeOptions, ShaperFont};
use icu_segmenter::LineSegmenter;
use serde::{Deserialize, Serialize};
use unicode_bidi::{BidiInfo, Level};

use super::fonts::{face, shaping_font, Face, Family};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Start,
    Center,
    End,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextDirection {
    Rtl,
    Ltr,
    /// From the first strong letter of each paragraph.
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Digits {
    Western,
    ArabicIndic,
    Persian,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kashida {
    None,
    Short,
    Medium,
    Long,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    pub family: Family,
    pub weight: u16,
    #[serde(default)]
    pub italic: bool,
    /// px
    pub size: f32,
    /// straight RGBA
    pub color: [u8; 4],
    /// Distance between baselines in px; `None` = auto (120% of the size).
    pub line_height: Option<f32>,
    /// Letter spacing in 1/1000 em.
    pub tracking: f32,
    pub align: Align,
    pub direction: TextDirection,
    pub digits: Digits,
    pub kashida: Kashida,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            family: Family::IbmPlexSansArabic,
            weight: 400,
            italic: false,
            size: 48.0,
            color: [0, 0, 0, 255],
            line_height: None,
            tracking: 0.0,
            align: Align::Start,
            direction: TextDirection::Rtl,
            digits: Digits::Western,
            kashida: Kashida::None,
        }
    }
}

impl TextStyle {
    pub fn line_height(&self) -> f32 {
        self.line_height.unwrap_or(self.size * 1.2)
    }

    pub fn face(&self) -> Face {
        face(self.family, self.weight, self.italic)
    }
}

/// A glyph placed on its baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    /// Index into `Layout::faces` (0 = the chosen font; others fill in missing characters).
    pub face: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub baseline: f32,
    /// Horizontal extent of the line's ink advance.
    pub x0: f32,
    pub x1: f32,
    /// The paragraph-direction of the line (true = right to left).
    pub rtl: bool,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub glyphs: Vec<Glyph>,
    pub lines: Vec<Line>,
    pub face: Face,
    /// The chosen font, then the fonts used for characters it doesn't have.
    pub faces: Vec<Face>,
    pub size: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
}

impl Layout {
    /// Bounds of the lines (from ascent to descent), as (x0, y0, x1, y1).
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let mut b = (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for l in &self.lines {
            b.0 = b.0.min(l.x0);
            b.2 = b.2.max(l.x1);
            b.1 = b.1.min(l.baseline - self.ascent);
            b.3 = b.3.max(l.baseline + self.descent);
        }
        if b.0 > b.2 {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            b
        }
    }
}

/// Replaces ASCII digits with the chosen digit shapes.
pub fn substitute_digits(text: &str, d: Digits) -> String {
    let base = match d {
        Digits::Western => return text.to_string(),
        Digits::ArabicIndic => 0x0660,
        Digits::Persian => 0x06F0,
    };
    text.chars().map(|c| if c.is_ascii_digit() { char::from_u32(base + (c as u32 - '0' as u32)).unwrap() } else { c }).collect()
}

/// A shaped glyph before placement: its id, the byte where its cluster starts, and its advance
/// and offsets in px.
struct Shaped {
    id: u32,
    face: u8,
    cluster: usize,
    advance: f32,
    dx: f32,
    dy: f32,
}

/// Fonts to try, in order, for characters the chosen font doesn't have.
fn fallbacks(primary: &Face) -> Vec<Face> {
    let mut out = vec![*primary];
    for (f, w) in [(Family::IbmPlexSansArabic, 400), (Family::Amiri, 400), (Family::Cairo, 400), (Family::Tajawal, 400)] {
        let face = face(f, w, false);
        if !out.iter().any(|o| o.bytes.as_ptr() == face.bytes.as_ptr()) {
            out.push(face);
        }
    }
    out
}

fn covers(face: &Face, c: char) -> bool {
    c.is_whitespace() || c.is_control() || matches!(c, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}') || shaping_font(face).charmap().map_unicode(c).is_some()
}

/// Splits text into runs per font: each word in the first font that has all its characters
/// (so the letters of a word still join), the chosen font wherever it can.
fn font_runs(text: &str, faces: &[Face]) -> Vec<(Range<usize>, usize)> {
    let mut out: Vec<(Range<usize>, usize)> = Vec::new();
    let mut push = |r: Range<usize>, f: usize| match out.last_mut() {
        Some(last) if last.1 == f && last.0.end == r.start => last.0.end = r.end,
        _ => out.push((r, f)),
    };
    let mut start = 0;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < bytes.len() {
        // a word and the spaces after it
        let mut j = i;
        while j < bytes.len() && !bytes[j].1.is_whitespace() {
            j += 1;
        }
        while j < bytes.len() && bytes[j].1.is_whitespace() {
            j += 1;
        }
        let end = bytes.get(j).map_or(text.len(), |b| b.0);
        let word = &text[start..end];
        let f = (0..faces.len()).find(|&k| word.chars().all(|c| covers(&faces[k], c))).unwrap_or(0);
        push(start..end, f);
        start = end;
        i = j;
    }
    out
}

/// Shapes a run of one direction, filling in missing characters from fallback fonts. Glyphs
/// come out in visual order.
fn shape_run(faces: &[Face], text: &str, rtl: bool, size: f32) -> Vec<Shaped> {
    let mut parts: Vec<Vec<Shaped>> = font_runs(text, faces)
        .into_iter()
        .map(|(r, f)| {
            let mut g = shape_one(&faces[f], &text[r.clone()], rtl, size);
            for s in &mut g {
                s.face = f as u8;
                s.cluster += r.start;
            }
            g
        })
        .collect();
    // pieces of a right-to-left run are displayed last piece first
    if rtl {
        parts.reverse();
    }
    parts.into_iter().flatten().collect()
}

fn shape_one(face: &Face, text: &str, rtl: bool, size: f32) -> Vec<Shaped> {
    let font = shaping_font(face);
    let shaper = ShaperFont::new(&font);
    let mut buf = Buffer::new();
    buf.push_str(text);
    buf.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
    buf.guess_segment_properties();
    if shape(&shaper, &mut buf, ShapeOptions::new()).is_err() {
        return Vec::new();
    }
    let k = size / font.units_per_em() as f32;
    buf.glyph_infos()
        .iter()
        .zip(buf.glyph_positions())
        .map(|(i, p)| Shaped { id: i.glyph_id, face: 0, cluster: i.cluster as usize, advance: p.x_advance as f32 * k, dx: p.x_offset as f32 * k, dy: p.y_offset as f32 * k })
        .collect()
}

/// Raw shaping output in font units: (glyph id, cluster byte, x advance, x offset, y offset), for
/// comparing with HarfBuzz itself (tools/shaping-oracle.mjs).
pub fn shape_units(face: &Face, text: &str, rtl: bool) -> Vec<(u32, u32, i32, i32, i32)> {
    let upem = shaping_font(face).units_per_em() as f32;
    shape_one(face, text, rtl, upem).into_iter().map(|g| (g.id, g.cluster as u32, g.advance.round() as i32, g.dx.round() as i32, g.dy.round() as i32)).collect()
}

/// Shapes and places one line of text (its own bidi pass with the paragraph's level), adding
/// `space_extra` after each space (justification). Returns glyphs at x from 0 and the width.
fn place_line(face: &[Face], text: &str, level: Level, size: f32, tracking: f32, space_extra: f32) -> (Vec<Glyph>, f32) {
    if text.is_empty() {
        return (Vec::new(), 0.0);
    }
    let bidi = BidiInfo::new(text, Some(level));
    let para = &bidi.paragraphs[0];
    let (levels, runs) = bidi.visual_runs(para, para.range.clone());
    let mut out = Vec::new();
    let mut pen = 0.0;
    for run in runs {
        let rtl = levels[run.start].is_rtl();
        let s = &text[run.clone()];
        let glyphs = shape_run(face, s, rtl, size);
        for (i, g) in glyphs.iter().enumerate() {
            out.push(Glyph { id: g.id, x: pen + g.dx, y: -g.dy, face: g.face });
            pen += g.advance;
            // letter spacing between clusters, extra width after spaces
            let next_cluster = glyphs.get(i + 1).map(|n| n.cluster);
            if next_cluster != Some(g.cluster) {
                pen += tracking;
                if s[g.cluster..].chars().next().is_some_and(char::is_whitespace) {
                    pen += space_extra;
                }
            }
        }
    }
    // no letter spacing after the last cluster
    if tracking != 0.0 && !out.is_empty() {
        pen -= tracking;
    }
    (out, pen)
}

/// Advance of each byte's cluster in a paragraph (logical order), for measuring line widths.
fn measure(face: &[Face], para: &str, bidi: &BidiInfo, size: f32, tracking: f32) -> Vec<f32> {
    let mut adv = vec![0.0f32; para.len() + 1];
    let levels = &bidi.levels;
    let mut a = 0;
    while a < para.len() {
        let mut b = a + 1;
        while b < para.len() && levels[b] == levels[a] {
            b += 1;
        }
        // runs end on character boundaries since levels are per byte of whole characters
        for g in shape_run(face, &para[a..b], levels[a].is_rtl(), size) {
            adv[a + g.cluster] += g.advance;
        }
        // letter spacing per character start
        if tracking != 0.0 {
            for (i, _) in para[a..b].char_indices() {
                adv[a + i] += tracking;
            }
        }
        a = b;
    }
    adv
}

fn trim_end(s: &str, a: usize, mut b: usize) -> usize {
    while b > a {
        let c = s[..b].chars().next_back().unwrap();
        if !c.is_whitespace() {
            break;
        }
        b -= c.len_utf8();
    }
    b
}

/// Greedy line breaking: as many words per line as fit in `width`.
fn break_lines(para: &str, adv: &[f32], width: f32) -> Vec<Range<usize>> {
    let breaks: Vec<usize> = LineSegmenter::new_auto(Default::default()).segment_str(para).filter(|&b| b > 0).collect();
    let w = |a: usize, b: usize| adv[a..trim_end(para, a, b)].iter().sum::<f32>();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut fit: Option<usize> = None;
    for bp in breaks {
        if w(start, bp) <= width + 1e-3 {
            fit = Some(bp);
            continue;
        }
        match fit {
            Some(f) if f > start => {
                lines.push(start..f);
                start = f;
                if w(start, bp) <= width + 1e-3 {
                    fit = Some(bp);
                } else {
                    // a single segment wider than the box stays whole
                    lines.push(start..bp);
                    start = bp;
                    fit = None;
                }
            }
            _ => {
                lines.push(start..bp);
                start = bp;
                fit = None;
            }
        }
    }
    if start < para.len() {
        lines.push(start..para.len());
    }
    if lines.is_empty() {
        lines.push(0..para.len());
    }
    lines
}

// ---- kashida ----

/// Letters that connect to the letter after them (dual-joining), per ArabicShaping.txt.
fn joins_next(c: char) -> bool {
    matches!(c,
        'ئ' | 'ب' | 'ت' | 'ث' | 'ج' | 'ح' | 'خ' | 'س' | 'ش' | 'ص' | 'ض' | 'ط' | 'ظ' | 'ع' | 'غ' | 'ف' | 'ق' | 'ك' | 'ل' | 'م' | 'ن' | 'ه' | 'ي' | 'ى' | 'ـ'
        | 'پ' | 'چ' | 'ک' | 'گ' | 'ی' | 'ٹ' | 'ڤ' | 'ڭ')
}

/// Letters that connect to the letter before them (dual- or right-joining).
fn joins_prev(c: char) -> bool {
    joins_next(c) || matches!(c, 'آ' | 'أ' | 'ؤ' | 'إ' | 'ا' | 'ة' | 'د' | 'ذ' | 'ر' | 'ز' | 'و' | 'ژ' | 'ۀ')
}

fn is_arabic_mark(c: char) -> bool {
    matches!(c as u32, 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED)
}

/// Kashida opportunities in a line, at most one per word: byte offsets where a tatweel can go,
/// with ALREQ's preferences simplified to three levels (after Seen-family letters; before the
/// word's last letter; any other joint), the last joint of a word winning ties.
fn kashida_points(text: &str) -> Vec<(usize, u8)> {
    let mut out = Vec::new();
    let mut word_start = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i <= chars.len() {
        let end_of_word = i == chars.len() || chars[i].1.is_whitespace();
        if end_of_word {
            // letters of the word, skipping marks
            let letters: Vec<(usize, char)> = chars[word_start..i].iter().copied().filter(|(_, c)| !is_arabic_mark(*c)).collect();
            let mut best: Option<(usize, u8)> = None;
            for k in 0..letters.len().saturating_sub(1) {
                let (_, c) = letters[k];
                let (pos_next, n) = letters[k + 1];
                if !(joins_next(c) && joins_prev(n)) {
                    continue;
                }
                let prio = if matches!(c, 'س' | 'ش' | 'ص' | 'ض') {
                    3
                } else if k + 2 == letters.len() {
                    2
                } else {
                    1
                };
                // the tatweel goes after the letter and its marks: right before the next letter
                if best.is_none_or(|b| prio >= b.1) {
                    best = Some((pos_next, prio));
                }
            }
            out.extend(best);
            word_start = i + 1;
        }
        i += 1;
    }
    out
}

/// Inserts tatweels to fill `extra` px. Returns the new text (or None when nothing fits).
fn insert_kashidas(text: &str, extra: f32, tatweel: f32, k: Kashida) -> Option<String> {
    let cap = match k {
        Kashida::None => return None,
        Kashida::Short => 1,
        Kashida::Medium => 2,
        Kashida::Long => 4,
    };
    if tatweel <= 0.0 {
        return None;
    }
    let mut points = kashida_points(text);
    if points.is_empty() {
        return None;
    }
    points.sort_by(|a, b| b.1.cmp(&a.1));
    let mut n = ((extra / tatweel).floor() as usize).min(points.len() * cap);
    if n == 0 {
        return None;
    }
    let mut counts = vec![0usize; points.len()];
    'fill: for _ in 0..cap {
        for c in counts.iter_mut() {
            if n == 0 {
                break 'fill;
            }
            *c += 1;
            n -= 1;
        }
    }
    let mut at: Vec<(usize, usize)> = points.iter().map(|p| p.0).zip(counts).filter(|(_, c)| *c > 0).collect();
    at.sort_by_key(|p| p.0);
    let mut out = String::with_capacity(text.len() + at.len() * 4);
    let mut last = 0;
    for (pos, c) in at {
        out.push_str(&text[last..pos]);
        for _ in 0..c {
            out.push('\u{0640}');
        }
        last = pos;
    }
    out.push_str(&text[last..]);
    Some(out)
}

/// Stretches a line to `w`: kashidas first (when asked), then spaces for what is left.
/// Tatweels can change the letters around them (contextual forms), so the stretched line is
/// measured, and fewer kashidas are tried while it comes out too wide.
#[allow(clippy::too_many_arguments)]
fn justify_line(face: &[Face], content: &str, level: Level, size: f32, tracking: f32, w: f32, lw: f32, placed: Vec<Glyph>, tatweel: f32, k: Kashida) -> (Vec<Glyph>, f32) {
    let spaces = |t: &str| t.chars().filter(|c| *c == ' ').count();
    let mut text = content.to_string();
    let mut width = lw;
    let mut budget = w - lw;
    for _ in 0..6 {
        let Some(stretched) = insert_kashidas(content, budget, tatweel, k) else { break };
        let (_, l) = place_line(face, &stretched, level, size, tracking, 0.0);
        if l <= w + 0.5 {
            text = stretched;
            width = l;
            break;
        }
        // too wide: ask for less, by what the overshoot shows a tatweel really costs
        budget -= (l - w).max(tatweel);
        if budget < tatweel {
            break;
        }
    }
    let n = spaces(&text);
    if n > 0 && w - width > 0.0 {
        return place_line(face, &text, level, size, tracking, (w - width) / n as f32);
    }
    if text != content {
        return place_line(face, &text, level, size, tracking, 0.0);
    }
    (placed, lw)
}

/// Lays out text. `width`: the box width for paragraph text, `None` for point text.
pub fn layout(text: &str, style: &TextStyle, width: Option<f32>) -> Layout {
    let face = style.face();
    let faces = fallbacks(&face);
    let size = style.size.max(0.1);
    let (ascent, descent) = super::render::vertical_metrics(&face, size);
    let line_height = style.line_height();
    let tracking = style.tracking / 1000.0 * size;
    let text = substitute_digits(text, style.digits);
    let first_baseline = if width.is_some() { ascent } else { 0.0 };
    let tatweel = if style.kashida != Kashida::None { place_line(&faces, "\u{0640}", Level::rtl(), size, 0.0, 0.0).1 } else { 0.0 };
    let mut glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut row = 0usize;
    for para in text.split('\n') {
        let level = match style.direction {
            TextDirection::Rtl => Some(Level::rtl()),
            TextDirection::Ltr => Some(Level::ltr()),
            TextDirection::Auto => None,
        };
        let bidi = BidiInfo::new(para, level);
        let para_level = bidi.paragraphs.first().map_or(level.unwrap_or(Level::rtl()), |p| p.level);
        let rtl = para_level.is_rtl();
        let ranges = match width {
            Some(w) if !para.is_empty() => break_lines(para, &measure(&faces, para, &bidi, size, tracking), w),
            _ => vec![0..para.len()],
        };
        let count = ranges.len();
        for (li, r) in ranges.into_iter().enumerate() {
            let content = &para[r.start..trim_end(para, r.start, r.end)];
            let baseline = first_baseline + row as f32 * line_height;
            row += 1;
            let (mut placed, mut lw) = place_line(&faces, content, para_level, size, tracking, 0.0);
            let justify = style.align == Align::Justify && li + 1 < count;
            if let (true, Some(w)) = (justify, width) {
                if w - lw > 0.0 {
                    (placed, lw) = justify_line(&faces, content, para_level, size, tracking, w, lw, placed, tatweel, style.kashida);
                }
            }
            let avail = width.unwrap_or(0.0);
            let align = if style.align == Align::Justify { Align::Start } else { style.align };
            let x0 = if justify && width.is_some() {
                0.0
            } else {
                match (align, rtl) {
                    (Align::Start, false) | (Align::End, true) => 0.0,
                    (Align::Start, true) | (Align::End, false) => avail - lw,
                    _ => (avail - lw) / 2.0,
                }
            };
            for g in placed {
                glyphs.push(Glyph { id: g.id, x: x0 + g.x, y: baseline + g.y, face: g.face });
            }
            lines.push(Line { baseline, x0, x1: x0 + lw, rtl });
        }
    }
    Layout { glyphs, lines, face, faces, size, ascent, descent, line_height }
}
