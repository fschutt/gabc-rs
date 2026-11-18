//! Development tasks for gabc-rs.
//!
//! `cargo run -p xtask -- font` builds `assets/fonts/gabc-neumes.ttf` from the glyph outlines
//! of exsurge (`third_party/exsurge/Exsurge.Glyphs.js`, MIT, © Fr. Matthew Spencer, OSJ).

mod svg_path;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use allsorts::font_builder::{Outline, TrueTypeBuilder};
use svg_path::{Contour, Segment};

/// Font units per em. A power of two, as TrueType rasterizers prefer.
const UNITS_PER_EM: u16 = 2048;

/// How many units of the exsurge glyph table make one em.
///
/// In exsurge one staff step (from a line to the space next to it) is as high as a punctum is
/// wide: 100 units. Two neighbouring staff lines are therefore 200 units apart, and with this
/// value **one em is four staff spaces**: the lines are 0.25em apart, a staff step is 0.125em,
/// and a punctum is 0.125em wide.
const SOURCE_UNITS_PER_EM: f64 = 800.0;

/// The glyph's origin, which is the middle of the note's staff position, sits on the baseline.
/// With ascender = -descender = half an em and `line-height: 1`, the baseline is exactly in the
/// middle of the glyph's box, so placing the box places the note.
const ASCENDER: i16 = (UNITS_PER_EM / 2) as i16;

/// The private-use code point of every glyph. This table is the contract between the font and
/// the layout code: entries are only ever appended, never reordered.
const GLYPHS: &[(char, &str)] = &[
    ('\u{E000}', "AcuteAccent"),
    ('\u{E001}', "Stropha"),
    ('\u{E002}', "BeginningAscLiquescent"),
    ('\u{E003}', "BeginningDesLiquescent"),
    ('\u{E004}', "CustosDescLong"),
    ('\u{E005}', "CustosDescShort"),
    ('\u{E006}', "CustosLong"),
    ('\u{E007}', "CustosShort"),
    ('\u{E008}', "DoClef"),
    ('\u{E009}', "FaClef"),
    ('\u{E00A}', "Flat"),
    ('\u{E00B}', "Mora"),
    ('\u{E00C}', "Natural"),
    ('\u{E00D}', "OriscusAsc"),
    ('\u{E00E}', "OriscusDes"),
    ('\u{E00F}', "OriscusLiquescent"),
    ('\u{E010}', "PodatusLower"),
    ('\u{E011}', "PodatusUpper"),
    ('\u{E012}', "Porrectus1"),
    ('\u{E013}', "Porrectus2"),
    ('\u{E014}', "Porrectus3"),
    ('\u{E015}', "Porrectus4"),
    ('\u{E016}', "PunctumCavum"),
    ('\u{E017}', "PunctumQuadratum"),
    ('\u{E018}', "PunctumQuadratumAscLiquescent"),
    ('\u{E019}', "PunctumQuadratumDesLiquescent"),
    ('\u{E01A}', "PunctumInclinatum"),
    ('\u{E01B}', "PunctumInclinatumLiquescent"),
    ('\u{E01C}', "Quilisma"),
    ('\u{E01D}', "TerminatingAscLiquescent"),
    ('\u{E01E}', "TerminatingDesLiquescent"),
    ('\u{E01F}', "VerticalEpisemaAbove"),
    ('\u{E020}', "VerticalEpisemaBelow"),
    ('\u{E021}', "VirgaLong"),
    ('\u{E022}', "VirgaShort"),
    ('\u{E023}', "Virgula"),
];

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "font" => build_font(),
        _ => Err(String::from("usage: cargo run -p xtask -- font")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repository root")
        .to_path_buf()
}

fn build_font() -> Result<(), String> {
    let root = repo_root();
    let source_path = root.join("third_party/exsurge/Exsurge.Glyphs.js");
    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("{}: {e}", source_path.display()))?;

    // The file is `export let Glyphs = { ...JSON... };`
    let marker = "export let Glyphs = ";
    let start = source
        .find(marker)
        .ok_or_else(|| format!("{}: no `{marker}`", source_path.display()))?
        + marker.len();
    let json = source[start..].trim_end().trim_end_matches(';');
    let table: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("glyph table is not JSON: {e}"))?;
    let table = table.as_object().ok_or("glyph table is not an object")?;

    // A glyph that appears upstream without a code point here must not go unnoticed.
    for name in table.keys() {
        if name != "None" && !GLYPHS.iter().any(|(_, n)| n == name) {
            return Err(format!("glyph {name:?} of the table has no code point in GLYPHS"));
        }
    }

    let scale = f64::from(UNITS_PER_EM) / SOURCE_UNITS_PER_EM;
    let mut builder = TrueTypeBuilder::new("Gabc Neumes", UNITS_PER_EM);
    builder
        .vertical_metrics(ASCENDER, -ASCENDER, 0)
        .version("Version 0.001")
        .name(
            0,
            "Glyph outlines Copyright (c) 2008-2016 Fr. Matthew Spencer, OSJ (exsurge). \
             Font assembled by the gabc-rs contributors.",
        )
        .name(13, "MIT License")
        .name(14, "https://opensource.org/licenses/MIT");
    builder
        .add_glyph(&[' '], UNITS_PER_EM / 8, Outline::new())
        .map_err(|e| e.to_string())?;

    println!("{:<8} {:<30} {:>7} {:>22} {:>8}", "code", "glyph", "advance", "ink x / y (units)", "points");
    for (code, name) in GLYPHS {
        let glyph = table
            .get(*name)
            .ok_or_else(|| format!("the glyph table has no {name:?}"))?;
        let number = |path: &[&str]| -> Result<f64, String> {
            let mut value = glyph;
            for key in path {
                value = &value[*key];
            }
            value
                .as_f64()
                .ok_or_else(|| format!("{name}: {} is not a number", path.join(".")))
        };
        let origin_x = number(&["origin", "x"])?;
        let width = number(&["bounds", "width"])?;

        // Path coordinates are relative to the glyph's origin, y pointing down. In the font
        // the ink starts at x = 0 (as exsurge's bounds box does) and y points up.
        let to_font = |(x, y): svg_path::Point| ((x + origin_x) * scale, -y * scale);

        let mut outline = Outline::new();
        let mut ink: Option<(f64, f64, f64, f64)> = None;
        let mut points = 0;
        let paths = glyph["paths"].as_array().ok_or_else(|| format!("{name}: no paths"))?;
        for path in paths {
            let data = path["data"].as_str().ok_or_else(|| format!("{name}: path without data"))?;
            let hole = match path["type"].as_str() {
                Some("positive") => false,
                Some("negative") => true,
                other => return Err(format!("{name}: unknown path type {other:?}")),
            };
            let mut contours: Vec<Contour> = svg_path::parse(data)
                .map_err(|e| format!("{name}: {e}"))?
                .iter()
                .map(|contour| contour.map(to_font))
                .collect();

            // TrueType fills by the non-zero winding rule: filled contours run clockwise
            // (negative area with y up), holes counter-clockwise. The largest contour of a path
            // decides which way the path runs; the others keep their direction relative to it.
            let largest = contours
                .iter()
                .map(Contour::signed_area)
                .fold(0.0_f64, |best, area| if area.abs() > best.abs() { area } else { best });
            let runs_clockwise = largest < 0.0;
            if runs_clockwise == hole {
                contours = contours.iter().map(Contour::reversed).collect();
            }

            for contour in &contours {
                let mut see = |(x, y): svg_path::Point| {
                    ink = Some(match ink {
                        None => (x, x, y, y),
                        Some((x0, x1, y0, y1)) => (x0.min(x), x1.max(x), y0.min(y), y1.max(y)),
                    });
                };
                see(contour.start);
                outline.move_to(contour.start.0 as f32, contour.start.1 as f32);
                for segment in &contour.segments {
                    points += 1;
                    match *segment {
                        Segment::Line { to } => {
                            see(to);
                            outline.line_to(to.0 as f32, to.1 as f32);
                        }
                        Segment::Cubic { ctrl1, ctrl2, to } => {
                            see(to);
                            outline.cubic_to(
                                ctrl1.0 as f32,
                                ctrl1.1 as f32,
                                ctrl2.0 as f32,
                                ctrl2.1 as f32,
                                to.0 as f32,
                                to.1 as f32,
                            );
                        }
                    }
                }
                outline.close();
            }
        }

        let advance = (width * scale).round();
        if !(0.0..=f64::from(u16::MAX)).contains(&advance) {
            return Err(format!("{name}: advance {advance} does not fit"));
        }
        builder
            .add_glyph(&[*code], advance as u16, outline)
            .map_err(|e| format!("{name}: {e}"))?;
        let (x0, x1, y0, y1) = ink.unwrap_or_default();
        println!(
            "U+{:04X}   {:<30} {:>7} {:>10.0}..{:<5.0} {:>5.0}..{:<5.0} {:>5}",
            *code as u32, name, advance, x0, x1, y0, y1, points
        );
    }

    let glyph_count = builder.glyph_count();
    let font = builder.build().map_err(|e| e.to_string())?;
    let out_dir = root.join("assets/fonts");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let out_path = out_dir.join("gabc-neumes.ttf");
    std::fs::write(&out_path, &font).map_err(|e| format!("{}: {e}", out_path.display()))?;
    println!(
        "\nwrote {} ({} bytes, {} glyphs, {} units per em)",
        out_path.display(),
        font.len(),
        glyph_count,
        UNITS_PER_EM
    );
    Ok(())
}
