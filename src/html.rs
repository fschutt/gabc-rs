//! The engraving as HTML and CSS.
//!
//! The same string goes to a browser, to printpdf (PDF) and to azul's CPU renderer (PNG):
//! one block per line that is kept together across pages, staff lines as thin boxes, every
//! neume glyph an absolutely placed character of the neume font, the lyrics as real text.
//! All offsets are in em, so the font size of the score scales everything.

use std::fmt::Write as _;

use crate::layout::{Engraving, Line};
use crate::neume::STEP;

/// The family name of the neume font (`assets/fonts/gabc-neumes.ttf`).
pub const NEUME_FAMILY: &str = "Gabc Neumes";
/// The family name of the music font for hymns (`assets/fonts/gabc-music.ttf`).
pub const MUSIC_FAMILY: &str = "Gabc Music";

/// The style rules that [`score`] relies on, in black. `size` is the font size of the score
/// with its unit (one em is four staff spaces): `"34px"`. The size of the lyrics is written
/// with each of them, so one set of rules serves chant and hymns on the same page.
pub fn style(size: &str) -> String {
    style_in(size, "#000000")
}

/// [`style`] with the colour of the ink given, for a page that has its own: `"currentColor"`
/// follows the text around the score, in a dark theme too. `size` may be any CSS length, such
/// as `"min(40px, calc(100cqw / 18.6))"` for a score that is as wide as its container.
pub fn style_in(size: &str, ink: &str) -> String {
    format!(
        ".gabc-score {{ font-size: {size}; color: {ink}; }}\n\
         .gabc-line {{ position: relative; width: 100%; break-inside: avoid; page-break-inside: avoid; }}\n\
         .gabc-rule {{ position: absolute; background: {ink}; }}\n\
         .gabc-g {{ position: absolute; font-family: \"{NEUME_FAMILY}\"; font-size: 1em; line-height: 1; }}\n\
         .gabc-m {{ position: absolute; font-family: \"{MUSIC_FAMILY}\"; font-size: 1em; line-height: 1; }}\n\
         .gabc-bow {{ position: absolute; overflow: visible; }}\n\
         .gabc-ly {{ position: absolute; line-height: 1; white-space: nowrap; }}\n\
         .gabc-sc {{ font-variant: small-caps; }}\n\
         .gabc-c {{ color: #b00000; }}\n"
    )
}

/// The score as a `<div class="gabc-score">` with one block per line, in black.
pub fn score(engraving: &Engraving) -> String {
    score_in(engraving, "#000000")
}

/// [`score`] with the colour of the ink given, as for [`style_in`]. The bows of ties and
/// slurs are SVG paths and carry their colour themselves.
pub fn score_in(engraving: &Engraving, ink: &str) -> String {
    let mut out = String::from("<div class=\"gabc-score\">\n");
    for line in &engraving.lines {
        self::line(&mut out, line, engraving, ink);
    }
    out.push_str("</div>\n");
    out
}

fn line(out: &mut String, line: &Line, engraving: &Engraving, ink: &str) {
    let size = engraving.lyric_size;
    let _ = writeln!(
        out,
        "<div class=\"gabc-line\" style=\"height:{:.4}em;margin-bottom:{:.4}em\">",
        line.height(size),
        2.0 * STEP
    );
    for &pitch in &line.staff {
        let _ = writeln!(
            out,
            "<div class=\"gabc-rule\" style=\"left:0;top:{:.4}em;width:{:.4}em;height:{:.4}em\"></div>",
            line.y(pitch) - line.staff_weight / 2.0,
            engraving.width,
            line.staff_weight
        );
    }
    for rule in &line.rules {
        let _ = writeln!(
            out,
            "<div class=\"gabc-rule\" style=\"left:{:.4}em;top:{:.4}em;width:{:.4}em;height:{:.4}em\"></div>",
            rule.x,
            line.y(rule.top),
            rule.width,
            rule.height * STEP
        );
    }
    for glyph in &line.glyphs {
        // The glyph's origin is in the middle of its box of one em.
        let _ = writeln!(
            out,
            "<span class=\"gabc-g\" style=\"left:{:.4}em;top:{:.4}em\">{}</span>",
            glyph.x,
            line.y(glyph.pitch) - 0.5,
            glyph.glyph.char()
        );
    }
    for curve in &line.curves {
        // A bow as a thin lens: an SVG as wide as the bow, its path in hundredths of an em.
        let width = (curve.x1 - curve.x0).max(0.05);
        let rise = (0.1 + 0.05 * width).min(0.3);
        let (y0, y1) = (line.y(curve.pitch0), line.y(curve.pitch1));
        let top = y0.min(y1) - rise - 0.02;
        let height = (y0 - y1).abs() + rise + 0.04;
        let unit = |em: f32| (em * 100.0).round();
        let (a, b) = (unit(y0 - top), unit(y1 - top));
        // The middle of the bow, away from the notes, and its inner edge a little nearer.
        let outer = if curve.above { unit(y0.min(y1) - top - rise) * 2.0 - (a + b) / 2.0 } else { unit(y0.max(y1) - top + rise) * 2.0 - (a + b) / 2.0 };
        // The control point of the inner edge: twice the thickness nearer, so that the middle
        // of the bow is as thick as asked for.
        let inner = outer + unit(curve.thickness) * if curve.above { 2.0 } else { -2.0 };
        let w = unit(width);
        let _ = writeln!(
            out,
            "<svg class=\"gabc-bow\" style=\"left:{:.4}em;top:{:.4}em;width:{:.4}em;height:{:.4}em\" viewBox=\"0 0 {w} {}\">\
             <path d=\"M0 {a} Q{} {outer} {w} {b} Q{} {inner} 0 {a} Z\" fill=\"{ink}\"/></svg>",
            curve.x0,
            top,
            width,
            height,
            unit(height),
            w / 2.0,
            w / 2.0
        );
    }
    for glyph in &line.music {
        let _ = writeln!(
            out,
            "<span class=\"gabc-m\" style=\"left:{:.4}em;top:{:.4}em\">{}</span>",
            glyph.x,
            line.y(glyph.pitch) - 0.5,
            glyph.glyph.char()
        );
    }
    for lyric in &line.lyrics {
        // An em in `left` and `top` is the element's own font size, so offsets in the
        // score's em are divided by the size of the lyrics.
        let _ = write!(
            out,
            "<span class=\"gabc-ly\" style=\"font-size:{size}em;left:{:.4}em;top:{:.4}em\">",
            lyric.x / size,
            line.lyric_row_y(lyric.row, size) / size
        );
        for span in &lyric.spans {
            let mut open = String::new();
            let mut close = String::new();
            let mut wrap = |tag_open: &str, tag_close: &str| {
                open.push_str(tag_open);
                close.insert_str(0, tag_close);
            };
            if span.italic {
                wrap("<i>", "</i>");
            }
            if span.bold {
                wrap("<b>", "</b>");
            }
            if span.small_caps {
                wrap("<span class=\"gabc-sc\">", "</span>");
            }
            if span.coloured {
                wrap("<span class=\"gabc-c\">", "</span>");
            }
            let _ = write!(out, "{open}{}{close}", escape(&span.text));
        }
        out.push_str("</span>\n");
    }
    out.push_str("</div>\n");
}

/// A whole HTML document around the score, for looking at one score on its own.
pub fn document(engraving: &Engraving, title: &str, text_family: &str, size: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><title>{}</title><style>\n\
         body {{ font-family: \"{}\"; font-size: 12pt; margin: 0; color: #000000; }}\n\
         h1 {{ font-size: 15pt; font-weight: normal; margin: 0 0 10pt 0; }}\n\
         {}</style></head><body>\n<h1>{}</h1>\n{}</body></html>\n",
        escape(title),
        escape(text_family),
        style(size),
        escape(title),
        score(engraving)
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{engrave, Options};

    #[test]
    fn a_score_becomes_lines_of_placed_glyphs_and_text() {
        let parsed = crate::gabc::parse("%%\n(c4) <i>KY</i>(f)ri(gh)e(h.) <&(::)");
        let html = score(&engrave(&parsed.score, &Options::default()));
        assert_eq!(html.matches("class=\"gabc-line\"").count(), 1);
        // Clef, three notes of which two are a podatus, and a dot.
        assert_eq!(html.matches("class=\"gabc-g\"").count(), 6);
        assert!(html.contains("<i>KY-</i>"), "{html}");
        assert!(html.contains("&lt;&amp;"), "{html}");
        assert!(html.contains('\u{E008}'), "the do clef");
        let doc = document(&engrave(&parsed.score, &Options::default()), "Kyrie", "Serif", "34px");
        assert!(doc.contains(".gabc-score { font-size: 34px;"));
    }
}
