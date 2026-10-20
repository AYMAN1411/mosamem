//! Prints the engine's shaping of the test corpus as JSON, for tools/shaping-oracle.mjs to check
//! against HarfBuzz itself: `cargo run -p lawha-core --example shaping_golden > docs/shaping-golden.json`

use lawha_core::text::fonts::{face, Family};
use lawha_core::text::layout::shape_units;

const CORPUS: &[(&str, bool)] = &[
    ("لا", true),
    ("ببب", true),
    ("مرحبا", true),
    ("بَّ", true),
    ("بِسْمِ اللَّهِ الرَّحْمَٰنِ الرَّحِيمِ", true),
    ("كِتَابًا", true),
    ("شُكْرًا جَزِيلًا", true),
    ("لوحة: محرر صور عربي أولاً", true),
    ("قال: (نعم)", true),
    ("ـــ تطويل ـــ", true),
    ("١٢٣٤٥٦٧٨٩٠", false),
    ("Hello, Lawha 2026!", false),
];

fn main() {
    let files = [
        (Family::IbmPlexSansArabic, 400, "ibm-plex-sans-arabic/IBMPlexSansArabic-Regular.ttf"),
        (Family::IbmPlexSansArabic, 700, "ibm-plex-sans-arabic/IBMPlexSansArabic-Bold.ttf"),
        (Family::Tajawal, 400, "tajawal/Tajawal-Regular.ttf"),
        (Family::Amiri, 400, "amiri/Amiri-Regular.ttf"),
        (Family::Cairo, 400, "cairo/Cairo[slnt,wght].ttf"),
    ];
    let mut out = String::from("[\n");
    let mut first = true;
    for (fam, w, file) in files {
        let f = face(fam, w, false);
        for (text, rtl) in CORPUS {
            let g = shape_units(&f, text, *rtl);
            if !first {
                out.push_str(",\n");
            }
            first = false;
            let glyphs: Vec<String> = g.iter().map(|(id, c, a, dx, dy)| format!("[{id},{c},{a},{dx},{dy}]")).collect();
            out.push_str(&format!(
                "  {{\"font\":{},\"weight\":{},\"variableWeight\":{},\"text\":{},\"rtl\":{},\"glyphs\":[{}]}}",
                serde_json::to_string(file).unwrap(),
                w,
                f.variable_weight.map_or("null".to_string(), |v| v.to_string()),
                serde_json::to_string(text).unwrap(),
                rtl,
                glyphs.join(",")
            ));
        }
    }
    out.push_str("\n]\n");
    print!("{out}");
}
