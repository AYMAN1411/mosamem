//! Shaping and layout goldens (docs/research.md §7.4). Glyph names come from the fonts' own
//! tables, so these fail if shaping (joining, ligatures, bidi, mirroring) goes wrong.

use super::fonts::{face, shaping_font, Family};
use super::layout::{layout, Align, Digits, Kashida, TextDirection, TextStyle};
use super::render::render;

fn style(family: Family) -> TextStyle {
    TextStyle { family, size: 100.0, ..TextStyle::default() }
}

/// Glyph names of a single line, left to right as displayed.
fn names(family: Family, text: &str) -> Vec<String> {
    let s = style(family);
    let l = layout(text, &s, None);
    let font = shaping_font(&face(family, 400, false));
    let mut g = l.glyphs.clone();
    g.sort_by(|a, b| a.x.total_cmp(&b.x));
    g.iter().map(|g| font.glyph_name(harfrust::GlyphId::new(g.id)).map(|n| n.as_str().to_string()).unwrap_or_default()).collect()
}

fn glyph_for(family: Family, c: char) -> u32 {
    let font = shaping_font(&face(family, 400, false));
    font.charmap().map_unicode(c).map(|g| g.to_u32()).unwrap_or(0)
}

/// x of the glyph for a character (first match).
fn x_of(family: Family, l: &super::Layout, c: char) -> f32 {
    let id = glyph_for(family, c);
    l.glyphs.iter().find(|g| g.id == id).map(|g| g.x).unwrap_or_else(|| panic!("no glyph for {c}"))
}

#[test]
fn lam_alef_becomes_a_ligature() {
    assert_eq!(names(Family::IbmPlexSansArabic, "لا"), vec!["uniFEFB"]);
    // Amiri has no glyph names; its lam-alef is a contextual pair, neither of them the plain letter
    let l = layout("لا", &style(Family::Amiri), None);
    assert_eq!(l.glyphs.len(), 2);
    let plain = [glyph_for(Family::Amiri, 'ل'), glyph_for(Family::Amiri, 'ا')];
    assert!(l.glyphs.iter().all(|g| !plain.contains(&g.id)), "{:?} vs {plain:?}", l.glyphs);
}

#[test]
fn letters_take_their_joining_forms() {
    // displayed left to right: final, medial, initial
    assert_eq!(names(Family::IbmPlexSansArabic, "ببب"), vec!["uniFE90", "uniFE92", "uniFE91"]);
    // alone, the letter is drawn with the font's base glyph
    assert_eq!(names(Family::IbmPlexSansArabic, "ب"), vec!["uni0628"]);
    assert_eq!(names(Family::IbmPlexSansArabic, "مرحبا"), vec!["uniFE8E", "uniFE92", "uniFEA3", "uniFEAE", "uniFEE3"]);
}

#[test]
fn diacritics_stack_and_nothing_is_missing() {
    // shadda + fatha on beh: the base and its marks, never a missing glyph
    let n = names(Family::IbmPlexSansArabic, "بَّ");
    assert!(n.len() >= 2 && n.len() <= 3, "{n:?}");
    let corpus = ["بِسْمِ اللَّهِ", "كِتَابًا", "شُكْرًا", "مرحبا Hello 123", "(سلام)", "عام 2026", "لوحة: محرر صور"];
    // alef wasla (ٱ) isn't in Tajawal, but the fallback fonts fill it in
    for f in Family::ALL {
        assert!(layout("بِسْمِ ٱللَّهِ", &style(f), None).glyphs.iter().all(|g| g.id != 0), "{f:?}");
    }
    for f in Family::ALL {
        for t in corpus {
            let l = layout(t, &style(f), None);
            assert!(!l.glyphs.is_empty());
            assert!(l.glyphs.iter().all(|g| g.id != 0), "{f:?}: missing glyph in «{t}»");
        }
    }
}

#[test]
fn missing_characters_come_from_another_bundled_font() {
    // Tajawal has neither alef wasla nor Persian digits: those words use a font that has them
    let mut s = style(Family::Tajawal);
    s.digits = Digits::Persian;
    let l = layout("بِسْمِ ٱللَّهِ 123", &s, None);
    assert!(l.glyphs.iter().all(|g| g.id != 0), "no missing glyphs");
    assert!(l.glyphs.iter().any(|g| g.face == 0) && l.glyphs.iter().any(|g| g.face > 0), "both fonts used");
    let r = render(&l, [0, 0, 0, 255], [1.0, 0.0, 0.0, 1.0, 1200.0, 200.0], (0, 0, 2000, 400)).unwrap();
    assert!(r.rgba.chunks_exact(4).filter(|p| p[3] == 255).count() > 1000);
    // the chosen font is used wherever it can be
    assert!(layout("مرحبا", &style(Family::Tajawal), None).glyphs.iter().all(|g| g.face == 0));
}

#[test]
fn mixed_arabic_and_latin_read_correctly() {
    let f = Family::IbmPlexSansArabic;
    let l = layout("مرحبا Hello 123", &style(f), None);
    // the Latin run reads left to right, to the left of the Arabic
    let (h, e, one, three) = (x_of(f, &l, 'H'), x_of(f, &l, 'e'), x_of(f, &l, '1'), x_of(f, &l, '3'));
    assert!(h < e && e < one && one < three, "{h} {e} {one} {three}");
    let rightmost = l.glyphs.iter().map(|g| g.x).fold(f32::MIN, f32::max);
    assert!(three < rightmost);
    // a number inside Arabic stays left to right
    let l = layout("عام 2026", &style(f), None);
    let (two, six) = (x_of(f, &l, '2'), x_of(f, &l, '6'));
    assert!(two < six);
}

#[test]
fn parentheses_mirror_in_right_to_left_text() {
    let f = Family::IbmPlexSansArabic;
    let l = layout("(سلام)", &style(f), None);
    let mut g = l.glyphs.clone();
    g.sort_by(|a, b| a.x.total_cmp(&b.x));
    // "(" comes first in reading order, so it sits at the right; mirrored, it is drawn with the
    // font's ")" glyph, which opens toward the text
    assert_eq!(g.last().unwrap().id, glyph_for(f, ')'));
    assert_eq!(g.first().unwrap().id, glyph_for(f, '('));
}

#[test]
fn digit_styles() {
    let f = Family::IbmPlexSansArabic;
    let mut s = style(f);
    s.digits = Digits::ArabicIndic;
    let l = layout("123", &s, None);
    let ids: Vec<u32> = l.glyphs.iter().map(|g| g.id).collect();
    for c in ['١', '٢', '٣'] {
        assert!(ids.contains(&glyph_for(f, c)), "{c}");
    }
    assert!(x_of(f, &l, '١') < x_of(f, &l, '٣'), "Arabic-Indic digits still read left to right");
    s.digits = Digits::Persian;
    let l = layout("45", &s, None);
    assert!(l.glyphs.iter().any(|g| g.id == glyph_for(f, '۴')));
}

#[test]
fn box_text_wraps_and_aligns() {
    let f = Family::IbmPlexSansArabic;
    let mut s = style(f);
    s.size = 40.0;
    let text = "هذه جملة عربية طويلة تكفي لتلتف على أكثر من سطر داخل المربع";
    let l = layout(text, &s, Some(300.0));
    assert!(l.lines.len() >= 3, "{}", l.lines.len());
    for line in &l.lines {
        assert!(line.x1 - line.x0 <= 300.5, "{line:?}");
        // start alignment in right-to-left text: flush right
        assert!((line.x1 - 300.0).abs() < 0.5, "{line:?}");
        assert!(line.rtl);
    }
    assert!((l.lines[1].baseline - l.lines[0].baseline - 48.0).abs() < 1e-3, "auto leading is 120%");
    // centered and end
    s.align = Align::Center;
    let c = layout(text, &s, Some(300.0));
    assert!((c.lines[0].x0 + c.lines[0].x1 - 300.0).abs() < 0.5);
    s.align = Align::End;
    let e = layout(text, &s, Some(300.0));
    assert!(e.lines[0].x0.abs() < 0.5);
}

#[test]
fn justification_with_spaces_and_with_kashida() {
    let f = Family::IbmPlexSansArabic;
    let mut s = style(f);
    s.size = 40.0;
    s.align = Align::Justify;
    let text = "هذه جملة عربية طويلة تكفي لتلتف على أكثر من سطر داخل المربع";
    let l = layout(text, &s, Some(320.0));
    let n = l.lines.len();
    for line in &l.lines[..n - 1] {
        assert!((line.x1 - line.x0 - 320.0).abs() < 0.6, "full width: {line:?}");
    }
    // the last line is not stretched
    assert!(l.lines[n - 1].x1 - l.lines[n - 1].x0 < 319.0);
    s.kashida = Kashida::Long;
    let k = layout(text, &s, Some(320.0));
    let tatweel = glyph_for(f, '\u{0640}');
    assert!(k.glyphs.iter().any(|g| g.id == tatweel), "kashidas were inserted");
    for line in &k.lines[..k.lines.len() - 1] {
        assert!((line.x1 - line.x0 - 320.0).abs() < 0.6, "{line:?}");
    }
    assert!(!l.glyphs.iter().any(|g| g.id == tatweel), "no kashida unless asked");
}

#[test]
fn kashida_works_with_contextual_fonts() {
    // Amiri changes letter forms around tatweels; lines must still fill the box exactly
    let mut s = style(Family::Amiri);
    s.size = 52.0;
    s.align = Align::Justify;
    s.kashida = Kashida::Long;
    let text = "النص المضبوط بالكشيدة: هذه فقرة عربية طويلة تمتد على عدة أسطر داخل مربع، ليظهر الضبط بالتطويل بدل المسافات الواسعة بين الكلمات.";
    let l = layout(text, &s, Some(1000.0));
    for line in &l.lines[..l.lines.len() - 1] {
        assert!((line.x1 - line.x0 - 1000.0).abs() < 0.6, "{line:?}");
    }
    let tatweel = glyph_for(Family::Amiri, '\u{0640}');
    let n = l.glyphs.iter().filter(|g| g.id == tatweel).count();
    let mut plain = s.clone();
    plain.kashida = Kashida::None;
    let p = layout(text, &plain, Some(1000.0));
    assert!(n > 0 || l.glyphs.len() > p.glyphs.len(), "kashidas were inserted: {} vs {} glyphs, {n} tatweels", l.glyphs.len(), p.glyphs.len());
}

#[test]
fn direction_follows_the_setting_or_the_first_letter() {
    let f = Family::IbmPlexSansArabic;
    let mut s = style(f);
    s.direction = TextDirection::Auto;
    assert!(!layout("Hello مرحبا", &s, None).lines[0].rtl);
    assert!(layout("مرحبا Hello", &s, None).lines[0].rtl);
    s.direction = TextDirection::Ltr;
    assert!(!layout("مرحبا", &s, None).lines[0].rtl);
    // point text: right-to-left start alignment ends at the anchor
    let r = layout("مرحبا", &style(f), None);
    assert!(r.lines[0].x1.abs() < 1e-3 && r.lines[0].x0 < 0.0);
}

#[test]
fn rendering_fills_the_glyphs_in_color() {
    let mut s = style(Family::Amiri);
    s.color = [200, 30, 40, 255];
    let l = layout("بسم الله", &s, None);
    let r = render(&l, s.color, [1.0, 0.0, 0.0, 1.0, 500.0, 300.0], (0, 0, 2000, 2000)).unwrap();
    assert!(r.x < 500 && r.x + r.w as i64 > 300 && r.y < 300);
    let solid = r.rgba.chunks_exact(4).filter(|p| p[3] == 255).count();
    assert!(solid > 1000, "{solid}");
    // edges keep the color and only alpha fades (a few levels of rounding on faint pixels)
    let near = |p: &[u8]| (p[0] as i32 - 200).abs() <= 3 && (p[1] as i32 - 30).abs() <= 3 && (p[2] as i32 - 40).abs() <= 3;
    assert!(r.rgba.chunks_exact(4).filter(|p| p[3] >= 32).all(near), "edges keep the color");
    // a turned layer: still drawn
    let a = 0.5f32;
    let t = render(&l, s.color, [a.cos(), a.sin(), -a.sin(), a.cos(), 500.0, 300.0], (0, 0, 2000, 2000)).unwrap();
    assert!(t.rgba.chunks_exact(4).any(|p| p[3] == 255));
}
