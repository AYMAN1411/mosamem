//! Text: Arabic first. Bundled fonts, bidi, shaping (harfrust), line breaking, kashida
//! justification, and drawing. docs/research.md §7.

pub mod fonts;
pub mod layout;
pub mod render;

#[cfg(test)]
mod tests;

pub use fonts::Family;
pub use layout::{layout, Align, Digits, Kashida, Layout, TextDirection, TextStyle};
pub use render::{render, Rendered};
