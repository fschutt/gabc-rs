//! M0 spike: one hand-written chant line, repeated over three pages, pushed
//! through both renderers. It answers, by looking at the output:
//!
//! 1. Are absolutely positioned glyph spans placed where their `em` offsets say,
//!    identically in the PDF and in the PNG?
//! 2. Does the neume font (private-use code points, built by `xtask font`) work
//!    in both engines?
//! 3. Are thin boxes usable as staff lines?
//! 4. Does a `break-inside: avoid` line block stay whole across a page break?
//! 5. What happens to an SVG path (the known vector-path bug)?
//!
//! Usage: spike <text-font.ttf> <text-family> <neume-font.ttf> <out-prefix> [lines]

use std::collections::BTreeMap;

use azul_css::props::basic::{ColorU, FontRef};
use azul_layout::cpurender::{render_pages, PageRenderOptions, TextAaSettings};
use azul_layout::font_traits::FontManager;
use azul_layout::solver3::page_breaks::BreakPolicy;
use printpdf::{Base64OrRaw, GeneratePdfOptions, PdfDocument, PdfSaveOptions};

/// A4 with 20 mm margins, in mm and in CSS px (96 px per inch).
const PAGE_W_MM: f32 = 210.0;
const PAGE_H_MM: f32 = 297.0;
const MARGIN_MM: f32 = 20.0;
const MM_TO_PX: f32 = 96.0 / 25.4;

const NEUME_FAMILY: &str = "Gabc Neumes";

// Code points of the neume font (see xtask/src/main.rs, GLYPHS).
const CUSTOS_LONG: char = '\u{E006}';
const DO_CLEF: char = '\u{E008}';
const MORA: char = '\u{E00B}';
const PODATUS_LOWER: char = '\u{E010}';
const PODATUS_UPPER: char = '\u{E011}';
const PORRECTUS_2: char = '\u{E013}';
const PUNCTUM: char = '\u{E017}';
const PUNCTUM_INCLINATUM: char = '\u{E01A}';
const QUILISMA: char = '\u{E01C}';
const VIRGA_LONG: char = '\u{E021}';

/// One em of the score is four staff spaces: lines 0.25em apart, a staff step 0.125em.
const STEP: f32 = 0.125;
/// y of the top staff line inside a line block, in em.
const STAFF_TOP: f32 = 0.5;
/// Thickness of a staff line in em (exsurge: an eighth of a punctum width).
const LINE_WEIGHT: f32 = 0.0156;

struct Fonts {
    text_family: String,
    text: Vec<u8>,
    neumes: Vec<u8>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!("usage: spike <text-font.ttf> <text-family> <neume-font.ttf> <out-prefix> [lines]");
        std::process::exit(2);
    }
    let fonts = Fonts {
        text: std::fs::read(&args[1]).expect("read text font"),
        text_family: args[2].clone(),
        neumes: std::fs::read(&args[3]).expect("read neume font"),
    };
    let out = args[4].clone();
    let lines: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(24);

    // SPIKE_HTML=<file> renders that document instead of the chant line, to isolate one
    // engine behaviour at a time.
    let html = match std::env::var("SPIKE_HTML") {
        Ok(path) => std::fs::read_to_string(&path).expect("read SPIKE_HTML"),
        Err(_) => build_html(&fonts.text_family, lines),
    };
    std::fs::write(format!("{out}.html"), &html).expect("write html");

    match render_pdf(&html, &fonts) {
        Ok((bytes, pages, warnings)) => {
            std::fs::write(format!("{out}.pdf"), &bytes).expect("write pdf");
            println!("pdf: {} bytes, {} pages, {} warnings", bytes.len(), pages, warnings.len());
            for w in warnings.iter().take(12) {
                println!("  pdf warning: {w}");
            }
        }
        Err(e) => println!("pdf: FAILED: {e}"),
    }

    match render_png(&html, &fonts) {
        Ok((bytes, w, h)) => {
            std::fs::write(format!("{out}.png"), &bytes).expect("write png");
            println!("png: {} bytes, {w}x{h} px", bytes.len());
        }
        Err(e) => println!("png: FAILED: {e}"),
    }
}

/// y of a staff position in em, measured from the top of the line block.
/// Position 0 is the top line, each step down is +1 (so 2 is the second line).
fn staff_y(position: i32) -> f32 {
    STAFF_TOP + position as f32 * STEP
}

/// A neume glyph whose origin (the middle of its 1em box) lands on (x, staff position).
/// The red square is the box the glyph must sit in; it is drawn only as a check.
fn glyph(out: &mut String, ch: char, x: f32, position: i32, marker: bool) {
    let top = staff_y(position) - 0.5;
    if marker {
        out.push_str(&format!(
            "<div class=\"ref\" style=\"left:{x:.4}em;top:{:.4}em\"></div>\n",
            staff_y(position) - STEP
        ));
    }
    out.push_str(&format!(
        "<span class=\"g\" style=\"left:{x:.4}em;top:{top:.4}em\">{ch}</span>\n"
    ));
}

/// A vertical stroke between two staff positions, as the neume connecting lines are.
fn stem(out: &mut String, x: f32, from: i32, to: i32) {
    let (a, b) = (staff_y(from.min(to)), staff_y(from.max(to)));
    out.push_str(&format!(
        "<div class=\"st\" style=\"left:{x:.4}em;top:{a:.4}em;height:{:.4}em\"></div>\n",
        b - a
    ));
}

/// Text inside the score. An `em` in `left`/`top` is the element's own font size, so the
/// offsets, which are in the score's em, are divided by the text's size factor. (A wrapper
/// with the score's font size does not work: its line box is as tall as the score's font,
/// and small text sits on that line's baseline, most of an em too low.)
fn text(out: &mut String, class: &str, size: f32, content: &str, x: f32, y: f32) {
    out.push_str(&format!(
        "<span class=\"{class}\" style=\"left:{:.4}em;top:{:.4}em\">{content}</span>\n",
        x / size,
        y / size
    ));
}

/// Size of lyric text and of the line numbers, as a fraction of the score's em.
const LYRIC_SIZE: f32 = 0.3;
const NUMBER_SIZE: f32 = 0.18;

fn lyric(out: &mut String, content: &str, x: f32) {
    text(out, "ly", LYRIC_SIZE, content, x, staff_y(6) + 0.2);
}

fn build_html(text_family: &str, lines: usize) -> String {
    let mut body = String::new();
    body.push_str("<h1>Spike: chant line</h1>\n");
    body.push_str("<p class=\"intro\">Neumes are glyphs of the neume font at em offsets. The red squares on the first line mark one staff space around each note position.</p>\n");
    // The known bug: a filled SVG path. Expected: a black triangle on grey.
    body.push_str(
        "<div class=\"svgbox\"><svg width=\"120\" height=\"60\" viewBox=\"0 0 120 60\">\
         <path d=\"M10 50 L60 10 L110 50 Z\" fill=\"#000000\"/></svg></div>\n",
    );
    body.push_str("<div class=\"score\">\n");
    for line in 0..lines {
        let m = line == 0; // markers on the first line only
        body.push_str("<div class=\"line\">\n");
        for i in 0..4 {
            body.push_str(&format!(
                "<div class=\"sl\" style=\"top:{:.4}em\"></div>\n",
                staff_y(i * 2) - LINE_WEIGHT / 2.0
            ));
        }
        text(&mut body, "num", NUMBER_SIZE, &(line + 1).to_string(), 0.0, staff_y(-3));

        // do clef on the top line (c4)
        glyph(&mut body, DO_CLEF, 0.1, 0, false);

        // Sal: punctum on the second space from the bottom ... positions count down from the top line
        glyph(&mut body, PUNCTUM, 0.75, 5, m);
        lyric(&mut body, "Sal", 0.70);
        // ve: podatus (lower note, upper note above it, joined by a stroke on the right)
        glyph(&mut body, PODATUS_LOWER, 1.45, 5, m);
        glyph(&mut body, PODATUS_UPPER, 1.45 + 0.011, 3, m);
        stem(&mut body, 1.45 + 0.125 - LINE_WEIGHT, 3, 5);
        lyric(&mut body, "ve,", 1.43);
        // Re: virga
        glyph(&mut body, VIRGA_LONG, 2.25, 2, m);
        lyric(&mut body, "Re", 2.18);
        // gí: quilisma group: punctum, quilisma, punctum rising
        glyph(&mut body, PUNCTUM, 2.95, 4, m);
        glyph(&mut body, QUILISMA, 3.075, 3, m);
        glyph(&mut body, PUNCTUM, 3.2, 2, m);
        lyric(&mut body, "gí", 3.02);
        // na: porrectus, then a dotted punctum
        glyph(&mut body, PORRECTUS_2, 3.9, 1, m);
        glyph(&mut body, PUNCTUM, 3.9 + 0.472 - 0.125, 2, false);
        lyric(&mut body, "na,", 3.95);
        // ma: climacus: virga and two diamonds
        glyph(&mut body, VIRGA_LONG, 4.95, 1, m);
        glyph(&mut body, PUNCTUM_INCLINATUM, 5.1, 2, m);
        glyph(&mut body, PUNCTUM_INCLINATUM, 5.24, 3, m);
        lyric(&mut body, "ma", 4.95);
        // ter: punctum with mora
        glyph(&mut body, PUNCTUM, 5.9, 4, m);
        glyph(&mut body, MORA, 6.06, 3, false);
        lyric(&mut body, "ter", 5.86);
        // full bar
        stem(&mut body, 6.6, 0, 6);
        // custos at the end of the line
        glyph(&mut body, CUSTOS_LONG, 7.2, 4, false);

        body.push_str("</div>\n");
    }
    body.push_str("</div>\n");

    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><style>\n\
         body {{ font-family: \"{text_family}\"; font-size: 12pt; margin: 0; color: #000000; }}\n\
         h1 {{ font-size: 16pt; margin: 0 0 6pt 0; }}\n\
         .intro {{ margin: 0 0 8pt 0; }}\n\
         .svgbox {{ width: 120px; height: 60px; margin-bottom: 8pt; background: #dddddd; }}\n\
         .score {{ font-size: 56px; }}\n\
         .line {{ position: relative; height: 2em; width: 100%; margin-bottom: 0.15em; \
                  break-inside: avoid; page-break-inside: avoid; background: #f6f6f6; }}\n\
         .sl {{ position: absolute; left: 0; width: 100%; height: {LINE_WEIGHT}em; background: #000000; }}\n\
         .st {{ position: absolute; width: {LINE_WEIGHT}em; background: #000000; }}\n\
         .ref {{ position: absolute; width: 0.125em; height: 0.25em; background: #ffb0b0; }}\n\
         .g {{ position: absolute; font-family: \"{NEUME_FAMILY}\"; font-size: 1em; line-height: 1; }}\n\
         .ly {{ position: absolute; font-size: {LYRIC_SIZE}em; line-height: 1; }}\n\
         .num {{ position: absolute; font-size: {NUMBER_SIZE}em; line-height: 1; color: #0000cc; }}\n\
         </style></head><body>\n{body}</body></html>\n"
    )
}

fn render_pdf(html: &str, fonts: &Fonts) -> Result<(Vec<u8>, usize, Vec<String>), String> {
    let mut map = BTreeMap::new();
    map.insert(fonts.text_family.clone(), Base64OrRaw::Raw(fonts.text.clone()));
    map.insert(NEUME_FAMILY.to_string(), Base64OrRaw::Raw(fonts.neumes.clone()));
    let options = GeneratePdfOptions {
        font_embedding: Some(true),
        page_width: Some(PAGE_W_MM),
        page_height: Some(PAGE_H_MM),
        margin_top: Some(MARGIN_MM),
        margin_right: Some(MARGIN_MM),
        margin_bottom: Some(MARGIN_MM),
        margin_left: Some(MARGIN_MM),
        ..Default::default()
    };
    let mut warnings = Vec::new();
    let doc = PdfDocument::from_html(html, &BTreeMap::new(), &map, &options, &mut warnings)?;
    let pages = doc.pages.len();
    let mut save_warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut save_warnings);
    let msgs = warnings
        .iter()
        .chain(save_warnings.iter())
        .map(|w| format!("{w:?}"))
        .collect();
    Ok((bytes, pages, msgs))
}

fn render_png(html: &str, fonts: &Fonts) -> Result<(Vec<u8>, u32, u32), String> {
    let styled_dom =
        azul_layout::xml::parse_xml_to_styled_dom(html).map_err(|e| format!("xml: {e:?}"))?;

    // No system fonts: only what we register, as it will be in WASM.
    let mut fm: FontManager<FontRef> = FontManager::new(rust_fontconfig::FcFontCache::default())
        .map_err(|e| format!("font manager: {e:?}"))?;
    for (family, bytes) in [
        (fonts.text_family.as_str(), &fonts.text),
        (NEUME_FAMILY, &fonts.neumes),
    ] {
        let coverage = rust_fontconfig::FcParseFontBytes(bytes, family)
            .and_then(|p| p.first().map(|(pattern, _)| pattern.unicode_ranges.clone()))
            .unwrap_or_default();
        println!("png: font {family:?} covers {} unicode ranges", coverage.len());
        for name in [family.to_string(), format!("\"{family}\""), format!("'{family}'")] {
            let _ = fm.register_named_font(&name, bytes, coverage.clone());
        }
    }

    let policy = BreakPolicy {
        honor_break_inside: true,
        widows_orphans: true,
        atomic_lines: true,
        ..BreakPolicy::default()
    };
    let opts = PageRenderOptions {
        page_width: (PAGE_W_MM - 2.0 * MARGIN_MM) * MM_TO_PX,
        page_height: (PAGE_H_MM - 2.0 * MARGIN_MM) * MM_TO_PX,
        dpi_factor: 2.0,
        background_color: ColorU { r: 255, g: 255, b: 255, a: 255 },
        first_page: 0,
        page_count: 3,
        // A page image is saved and scaled: no LCD stripes.
        text_aa: TextAaSettings::grayscale(),
    };
    let pixmap = render_pages(
        &styled_dom,
        &mut fm,
        &azul_core::resources::ImageCache::default(),
        policy,
        opts,
    )?;
    let png = pixmap.encode_png()?;
    Ok((png, pixmap.width, pixmap.height))
}
