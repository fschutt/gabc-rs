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
pub mod diagnostics;
pub mod gabc;
pub mod glyphs;
pub mod html;
pub mod layout;
pub mod markdown;
pub mod neume;

pub use diagnostics::{Diagnostic, Severity, Span};
