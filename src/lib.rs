//! Gregorian chant (GABC) and hymn (ABC) notation: parsing, validation and, in time,
//! rendering through one HTML string into PDF and PNG.
//!
//! So far the two parsers: [`gabc::parse`] for chant and [`abc::parse`] for hymns. Both are
//! strict, report every problem they find with its position, and still return what they
//! could read.

pub mod abc;
pub mod diagnostics;
pub mod gabc;
pub mod glyphs;
pub mod markdown;
pub mod neume;

pub use diagnostics::{Diagnostic, Severity, Span};
