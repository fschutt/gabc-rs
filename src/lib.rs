//! Gregorian chant (GABC) and hymn (ABC) notation: parsing, validation and, in time,
//! rendering through one HTML string into PDF and PNG.
//!
//! So far: [`gabc::parse`], a strict GABC parser that reports every problem it finds with its
//! position and still returns what it could read.

pub mod diagnostics;
pub mod gabc;

pub use diagnostics::{Diagnostic, Severity, Span};
