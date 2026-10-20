//! The bundled fonts (SIL OFL 1.1, unmodified; each family's OFL.txt sits next to its files in
//! assets/fonts). They are compiled into the program, so text renders the same everywhere and
//! documents never miss a font.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use harfrust::Font;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    IbmPlexSansArabic,
    Cairo,
    Tajawal,
    Amiri,
}

impl Family {
    pub const ALL: [Family; 4] = [Family::IbmPlexSansArabic, Family::Cairo, Family::Tajawal, Family::Amiri];

    /// The family name as the fonts themselves (and the UI's CSS) call it.
    pub fn name(self) -> &'static str {
        match self {
            Family::IbmPlexSansArabic => "IBM Plex Sans Arabic",
            Family::Cairo => "Cairo",
            Family::Tajawal => "Tajawal",
            Family::Amiri => "Amiri",
        }
    }

    /// Weights the family offers.
    pub fn weights(self) -> &'static [u16] {
        match self {
            Family::IbmPlexSansArabic => &[100, 200, 300, 400, 500, 600, 700],
            Family::Cairo => &[200, 300, 400, 500, 600, 700, 800, 900, 1000],
            Family::Tajawal => &[200, 300, 400, 500, 700, 800, 900],
            Family::Amiri => &[400, 700],
        }
    }

    pub fn has_italic(self) -> bool {
        self == Family::Amiri
    }
}

macro_rules! font {
    ($path:literal) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/", $path)) as &'static [u8]
    };
}

/// One font file: (family, weight, italic, bytes). Cairo is a single variable file (wght 200–1000).
static FILES: &[(Family, u16, bool, &[u8])] = &[
    (Family::IbmPlexSansArabic, 100, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-Thin.ttf")),
    (Family::IbmPlexSansArabic, 200, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-ExtraLight.ttf")),
    (Family::IbmPlexSansArabic, 300, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-Light.ttf")),
    (Family::IbmPlexSansArabic, 400, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-Regular.ttf")),
    (Family::IbmPlexSansArabic, 500, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-Medium.ttf")),
    (Family::IbmPlexSansArabic, 600, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-SemiBold.ttf")),
    (Family::IbmPlexSansArabic, 700, false, font!("ibm-plex-sans-arabic/IBMPlexSansArabic-Bold.ttf")),
    (Family::Cairo, 0, false, font!("cairo/Cairo[slnt,wght].ttf")),
    (Family::Tajawal, 200, false, font!("tajawal/Tajawal-ExtraLight.ttf")),
    (Family::Tajawal, 300, false, font!("tajawal/Tajawal-Light.ttf")),
    (Family::Tajawal, 400, false, font!("tajawal/Tajawal-Regular.ttf")),
    (Family::Tajawal, 500, false, font!("tajawal/Tajawal-Medium.ttf")),
    (Family::Tajawal, 700, false, font!("tajawal/Tajawal-Bold.ttf")),
    (Family::Tajawal, 800, false, font!("tajawal/Tajawal-ExtraBold.ttf")),
    (Family::Tajawal, 900, false, font!("tajawal/Tajawal-Black.ttf")),
    (Family::Amiri, 400, false, font!("amiri/Amiri-Regular.ttf")),
    (Family::Amiri, 700, false, font!("amiri/Amiri-Bold.ttf")),
    (Family::Amiri, 400, true, font!("amiri/Amiri-Italic.ttf")),
    (Family::Amiri, 700, true, font!("amiri/Amiri-BoldItalic.ttf")),
];

/// A face ready to use: the file and, for the variable font, the weight to set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub family: Family,
    pub bytes: &'static [u8],
    /// The `wght` axis value for variable fonts.
    pub variable_weight: Option<f32>,
}

/// The face closest to the asked weight (and italic where the family has one).
pub fn face(family: Family, weight: u16, italic: bool) -> Face {
    if family == Family::Cairo {
        let bytes = FILES.iter().find(|f| f.0 == Family::Cairo).unwrap().3;
        return Face { family, bytes, variable_weight: Some(weight.clamp(200, 1000) as f32) };
    }
    let italic = italic && family.has_italic();
    let best = FILES
        .iter()
        .filter(|f| f.0 == family && f.2 == italic)
        .min_by_key(|f| (f.1 as i32 - weight as i32).abs())
        .unwrap();
    Face { family, bytes: best.3, variable_weight: None }
}

/// The shaping font for a face (cached: building one reads the font's tables).
pub fn shaping_font(face: &Face) -> Font {
    static CACHE: OnceLock<Mutex<HashMap<(usize, u32), Font>>> = OnceLock::new();
    let key = (face.bytes.as_ptr() as usize, face.variable_weight.map_or(0, f32::to_bits));
    let mut c = CACHE.get_or_init(Default::default).lock().unwrap();
    c.entry(key)
        .or_insert_with(|| {
            let base = Font::new(face.bytes, 0).expect("a bundled font");
            match face.variable_weight {
                Some(w) => base.instance_builder().variations([("wght", w)]).build(),
                None => base,
            }
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_font_loads_and_weights_resolve() {
        for f in FILES {
            assert!(Font::new(f.3, 0).is_some(), "{:?} {}", f.0, f.1);
        }
        // 600 is as near to 500 as to 700: the lighter one wins
        assert_eq!(face(Family::Tajawal, 600, false).bytes.as_ptr(), face(Family::Tajawal, 500, false).bytes.as_ptr());
        assert_eq!(face(Family::Cairo, 1200, false).variable_weight, Some(1000.0));
        assert!(face(Family::Amiri, 700, true).bytes.len() > 1000);
        // an italic asked of a family without one falls back to upright
        assert_eq!(face(Family::Tajawal, 400, true).bytes.as_ptr(), face(Family::Tajawal, 400, false).bytes.as_ptr());
    }
}
