//! Notation blocks in a Markdown document.
//!
//! A piece's notation sits in a collapsible block, so that the text reads without it:
//!
//! ````markdown
//! <details class="notation">
//! <summary>Notation</summary>
//!
//! ```abc
//! X:1
//! ...
//! ```
//!
//! </details>
//! ````
//!
//! The fence language is `abc` or `gabc`. The two empty lines are required: without them
//! CommonMark reads the fence as raw HTML. The block may not be the first thing in the body.

use crate::diagnostics::{Diagnostic, Severity, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Abc,
    Gabc,
}

/// One fenced `abc` or `gabc` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub language: Language,
    /// What stands between the fences.
    pub source: String,
    /// Byte offset of `source` in the document.
    pub offset: usize,
}

/// Finds the notation blocks of a document and checks how each is wrapped. The diagnostics'
/// spans are positions in `text`.
pub fn notation_blocks(text: &str) -> (Vec<Block>, Vec<Diagnostic>) {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        lines.push((offset, raw.trim_end_matches(['\n', '\r'])));
        offset += raw.len();
    }
    let error = |code, (at, line): (usize, &str), message: &str| Diagnostic {
        severity: Severity::Error,
        code,
        message: message.to_string(),
        span: Span::new(at, at + line.len()),
    };
    let line_at = |n: Option<usize>| n.and_then(|n| lines.get(n)).map_or("", |l| l.1.trim());

    // The body starts after the front matter, if there is one.
    let body_start = match lines.first() {
        Some((_, "---")) => lines.iter().skip(1).position(|l| l.1.trim() == "---").map_or(0, |n| n + 2),
        _ => 0,
    };
    let first_in_body = (body_start..lines.len()).find(|n| !lines[*n].1.trim().is_empty());

    let mut blocks = Vec::new();
    let mut diagnostics = Vec::new();
    let mut n = 0;
    while n < lines.len() {
        let open = n;
        let language = match lines[n].1.trim() {
            "```abc" => Language::Abc,
            "```gabc" => Language::Gabc,
            _ => {
                n += 1;
                continue;
            }
        };
        let Some(close) = (open + 1..lines.len()).find(|k| lines[*k].1.trim() == "```") else {
            diagnostics.push(error("E302", lines[open], "this notation block is never closed"));
            break;
        };
        let start = lines.get(open + 1).map_or(text.len(), |l| l.0);
        blocks.push(Block { language, source: text[start..lines[close].0].to_string(), offset: start });

        let opens_right = line_at(open.checked_sub(3)) == "<details class=\"notation\">"
            && line_at(open.checked_sub(2)).starts_with("<summary>")
            && line_at(open.checked_sub(2)).ends_with("</summary>")
            && open >= 3
            && line_at(open.checked_sub(1)).is_empty();
        if !opens_right {
            diagnostics.push(error(
                "E301",
                lines[open],
                "above the block: `<details class=\"notation\">`, a `<summary>...</summary>` line, an empty line",
            ));
        } else if first_in_body == open.checked_sub(3) {
            diagnostics.push(error(
                "E303",
                lines[open - 3],
                "the notation block may not be the first thing in the body; put it after the opening text",
            ));
        }
        let closes_right = close + 2 < lines.len() + usize::from(close + 2 == lines.len())
            && line_at(Some(close + 1)).is_empty()
            && line_at(Some(close + 2)) == "</details>";
        if !closes_right {
            diagnostics.push(error("E301", lines[close], "below the block: an empty line, then `</details>`"));
        }
        n = close + 1;
    }
    (blocks, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<&'static str> {
        notation_blocks(text).1.iter().map(|d| d.code).collect()
    }

    const GOOD: &str = "---\ntitle: x\n---\n\nText.\n\n<details class=\"notation\">\n<summary>Noten</summary>\n\n```abc\nX:1\nK:C\n```\n\n</details>\n";

    #[test]
    fn a_block_is_found_with_its_position() {
        let (blocks, diagnostics) = notation_blocks(GOOD);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, Language::Abc);
        assert_eq!(blocks[0].source, "X:1\nK:C\n");
        assert_eq!(&GOOD[blocks[0].offset..blocks[0].offset + 3], "X:1");
    }

    #[test]
    fn the_wrapping_is_checked() {
        assert_eq!(codes(&GOOD.replace("</summary>\n\n", "</summary>\n")), ["E301"]);
        assert_eq!(codes(&GOOD.replace("```\n\n</details>", "```\n</details>")), ["E301"]);
        assert_eq!(codes(&GOOD.replace("<details class=\"notation\">", "<details>")), ["E301"]);
        assert_eq!(codes(&GOOD.replace("Text.\n\n", "")), ["E303"]);
        assert_eq!(codes(&GOOD.replace("```\n\n</details>\n", "")), ["E302"]);
        // Other fenced blocks are not notation.
        assert!(notation_blocks("```rust\nfn main() {}\n```\n").0.is_empty());
    }
}
