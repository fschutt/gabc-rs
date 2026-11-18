//! gabc-rs for the browser.
//!
//! Three plain functions, so that the page needs no glue library: the page asks for memory,
//! writes the source into it as UTF-8, and calls [`gabc_render`], which answers with the
//! place and length of a JSON text:
//!
//! ```json
//! { "html": "<div class=\"gabc-score\">...", "style": ".gabc-score { ... }",
//!   "diagnostics": [ { "line": 3, "column": 14, "severity": "error", "code": "E011", "message": "..." } ] }
//! ```
//!
//! `html` is empty if the source has errors. The page frees both pieces of memory again.
//!
//! The module asks the page for one thing: `env.gabc_measure(pointer, length)`, the width of
//! a piece of text in em in the font the page shows the lyrics in. So the layout works with
//! the widths the browser draws, not with a guess.

use gabc_rs::diagnostics::line_col;
use gabc_rs::markdown::Language;
use gabc_rs::{abc, gabc, Diagnostic, Severity};

/// Memory for `len` bytes, to be given back with [`gabc_free`].
#[no_mangle]
pub extern "C" fn gabc_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len.max(1));
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Gives back memory from [`gabc_alloc`] or from a result of [`gabc_render`].
///
/// # Safety
/// `pointer` and `len` must be what one of those two handed out, and not freed before.
#[no_mangle]
pub unsafe extern "C" fn gabc_free(pointer: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(pointer, 0, len.max(1)));
}

/// Checks and draws a score. `language` is 0 for GABC and 1 for ABC, `width` the width of a
/// line in em of the score, `size` the lyric size as a part of that em. The answer is a JSON
/// text; its address is in the upper half of the returned number and its length in the lower.
///
/// # Safety
/// `source` must point to `len` bytes that the caller wrote.
#[no_mangle]
pub unsafe extern "C" fn gabc_render(language: u32, source: *const u8, len: usize, width: f32, size: f32) -> u64 {
    let bytes = std::slice::from_raw_parts(source, len);
    let text = String::from_utf8_lossy(bytes);
    let json = render(language, &text, width, size).into_bytes();
    let out_len = json.len();
    let pointer = gabc_alloc(out_len);
    std::ptr::copy_nonoverlapping(json.as_ptr(), pointer, out_len);
    ((pointer as u64) << 32) | out_len as u64
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    /// The width of the UTF-8 text at `pointer`, in em, measured by the page.
    fn gabc_measure(pointer: *const u8, len: usize) -> f32;
}

/// The page's measure of a text, where there is a page.
#[cfg(target_arch = "wasm32")]
fn page_measure() -> Option<&'static dyn Fn(&str) -> f32> {
    fn measure(text: &str) -> f32 {
        unsafe { gabc_measure(text.as_ptr(), text.len()) }
    }
    Some(&measure)
}

#[cfg(not(target_arch = "wasm32"))]
fn page_measure() -> Option<&'static dyn Fn(&str) -> f32> {
    None
}

fn render(language: u32, source: &str, width: f32, size: f32) -> String {
    let language = if language == 1 { Language::Abc } else { Language::Gabc };
    let options = gabc_rs::layout::Options {
        width: width.clamp(4.0, 60.0),
        lyric_size: size.clamp(0.2, 0.8),
        measure: page_measure(),
    };
    let diagnostics = match language {
        Language::Abc => abc::parse(source).diagnostics,
        Language::Gabc => gabc::parse(source).diagnostics,
    };
    let html = gabc_rs::render_in(language, source, &options, "currentColor").unwrap_or_default();
    let style = gabc_rs::html::style_in("var(--gabc-size, 34px)", options.lyric_size, "currentColor");
    let list: Vec<String> = diagnostics.iter().map(|d| diagnostic(d, source)).collect();
    format!("{{\"html\":{},\"style\":{},\"diagnostics\":[{}]}}", quote(&html), quote(&style), list.join(","))
}

fn diagnostic(d: &Diagnostic, source: &str) -> String {
    let (line, column) = line_col(source, d.span.start);
    let severity = if d.severity == Severity::Error { "error" } else { "warning" };
    format!(
        "{{\"line\":{line},\"column\":{column},\"severity\":\"{severity}\",\"code\":\"{}\",\"message\":{}}}",
        d.code,
        quote(&d.message)
    )
}

/// A JSON string.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_score_comes_back_as_json_with_its_html() {
        let json = render(0, "name: Kyrie;\n%%\n(c4) KY(f)ri(gh)e(h.) (::)\n", 18.0, 0.32);
        assert!(json.starts_with("{\"html\":\"<div class=\\\"gabc-score\\\">"), "{json}");
        assert!(json.ends_with("\"diagnostics\":[]}"), "{json}");
        let json = render(1, "X:1\nT:t\nM:3/4\nL:1/4\nK:F\nF G A |]\n", 18.0, 0.42);
        assert!(json.contains("gabc-m"), "{json}");
    }

    #[test]
    fn a_wrong_score_comes_back_with_where_it_is_wrong() {
        let json = render(0, "%%\n(c4) A(f?)", 18.0, 0.42);
        assert!(json.starts_with("{\"html\":\"\","), "{json}");
        assert!(
            json.contains("{\"line\":2,\"column\":9,\"severity\":\"error\",\"code\":\"E011\",\"message\":\"`?` is not part of the notation\"}"),
            "{json}"
        );
        assert_eq!(quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
