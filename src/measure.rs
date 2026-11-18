//! The width of text in a font, from azul's text engine (feature `measure`).
//!
//! The layout has to know how wide each syllable is to place it under its notes and to keep
//! neighbours apart. This asks the engine that will set the text when the score is turned
//! into a PDF or a PNG: the same shaping, kerning and ligatures, so the measured width is the
//! width that is drawn.
//!
//! ```ignore
//! let font = gabc_rs::measure::FontMeasure::new(&std::fs::read("Lyrics.ttf")?).unwrap();
//! let measure = |text: &str| font.width(text);
//! let options = gabc_rs::layout::Options { measure: Some(&measure), ..Default::default() };
//! ```

use azul_css::props::basic::FontRef;
use azul_layout::text3::api::{layout_text, TextLayoutOptions};

/// A font to measure text in.
pub struct FontMeasure {
    font: FontRef,
}

/// The font size the text is laid out at for measuring. Widths are divided by it.
const SIZE: f32 = 1000.0;

impl FontMeasure {
    /// Reads a TrueType or OpenType font. `None` if it cannot be parsed.
    pub fn new(font: &[u8]) -> Option<Self> {
        azul_layout::text3::default::font_ref_from_bytes(font, 0, false).map(|font| FontMeasure { font })
    }

    /// The width of `text` on one line, in em of the font size: where the pen stands after
    /// its last glyph. Text that cannot be shaped measures 0.
    pub fn width(&self, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        let options = TextLayoutOptions { font_size: SIZE, ..TextLayoutOptions::default() };
        let Some(layout) = layout_text(text, std::slice::from_ref(&self.font), &options) else {
            return 0.0;
        };
        layout
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .map(|glyph| glyph.x + glyph.advance)
            .fold(0.0, f32::max)
            / SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyphs::Glyph;

    /// The neume font is a font like any other, with advances that are known exactly.
    #[test]
    fn widths_are_the_advances_of_the_font() {
        let bytes = include_bytes!("../assets/fonts/gabc-neumes.ttf");
        let font = FontMeasure::new(bytes).expect("the neume font parses");
        let punctum = Glyph::PunctumQuadratum;
        let one = font.width(&punctum.char().to_string());
        assert!((one - punctum.metrics().width).abs() < 0.001, "{one}");
        let clef = Glyph::DoClef;
        let both = font.width(&format!("{}{}", punctum.char(), clef.char()));
        assert!((both - punctum.metrics().width - clef.metrics().width).abs() < 0.002, "{both}");
        assert_eq!(font.width(""), 0.0);
        assert!(FontMeasure::new(b"not a font").is_none());
    }
}
