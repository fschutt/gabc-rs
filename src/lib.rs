//! Gregorian chant (GABC) and hymn (ABC) notation: parsing, validation and, in time,
//! rendering through one HTML string into PDF and PNG.
//!
//! The two parsers, [`gabc::parse`] for chant and [`abc::parse`] for hymns, are strict, report
//! every problem they find with its position, and still return what they could read.
//!
//! A chant score goes on through [`neume`] (notes into neumes, neumes into glyphs),
//! [`layout::engrave`] (syllables into lines of a given width) and [`html::score`] (the lines
//! as HTML and CSS).

pub mod abc;
pub mod abc_layout;
pub mod diagnostics;
pub mod gabc;
pub mod glyphs;
pub mod html;
pub mod layout;
pub mod markdown;
pub mod music_glyphs;
pub mod neume;

pub use diagnostics::{Diagnostic, Severity, Span};

/// The score of one notation block as HTML: the `<div class="gabc-score">` of [`html::score`],
/// to go with the rules of [`html::style`]. A block with errors gives its diagnostics instead,
/// with spans that are positions in `source`.
pub fn render(
    language: markdown::Language,
    source: &str,
    options: &layout::Options,
) -> Result<String, Vec<Diagnostic>> {
    let (diagnostics, engraving) = match language {
        markdown::Language::Abc => {
            let parsed = abc::parse(source);
            let engraving = parsed.is_valid().then(|| abc_layout::engrave(&parsed.tune, options));
            (parsed.diagnostics, engraving)
        }
        markdown::Language::Gabc => {
            let parsed = gabc::parse(source);
            let engraving = parsed.is_valid().then(|| layout::engrave(&parsed.score, options));
            (parsed.diagnostics, engraving)
        }
    };
    match engraving {
        Some(engraving) => Ok(html::score(&engraving)),
        None => Err(diagnostics),
    }
}
