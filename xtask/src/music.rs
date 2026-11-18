//! `cargo run -p xtask -- music-font`: the font for hymns in modern notation.
//!
//! Takes the few glyphs a hymn needs (clef, note heads, flags, rests, accidentals, time
//! signatures) from Bravura (Steinberg, SIL Open Font License 1.1) and writes them as a
//! small TrueType font of its own name, `assets/fonts/gabc-music.ttf`, with the table
//! `src/music_glyphs.rs` for the layout code. The licence reserves the name "Bravura", so
//! the font is called "Gabc Music"; it stays under the Open Font License, whose text is
//! written next to it.
//!
//! Bravura follows SMuFL: one em is four staff spaces, a glyph's origin is at the height of
//! the staff position it belongs to. That is the convention of the neume font too. The code
//! points are SMuFL's.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use allsorts::binary::read::ReadScope;
use allsorts::cff::outline::CFFOutlines;
use allsorts::cff::CFF;
use allsorts::font::{Font, MatchingPresentation};
use allsorts::font_builder::{Outline, TrueTypeBuilder};
use allsorts::font_data::FontData;
use allsorts::outline::{OutlineBuilder, OutlineSink};
use allsorts::pathfinder_geometry::line_segment::LineSegment2F;
use allsorts::pathfinder_geometry::vector::Vector2F;
use allsorts::tables::FontTableProvider;
use allsorts::tag;

const SOURCE: &str = "https://raw.githubusercontent.com/steinbergmedia/bravura/master/redist";
const UNITS_PER_EM: u16 = 2048;

/// The measures of Bravura's own metadata that the layout uses, in staff spaces there and in
/// em in the table that is written (a staff space is a quarter of an em).
const DEFAULTS: &[(&str, &str, &str)] = &[
    ("STAFF_LINE", "staffLineThickness", "The weight of a staff line."),
    ("STEM", "stemThickness", "The weight of a stem."),
    ("LEDGER_LINE", "legerLineThickness", "The weight of a ledger line."),
    ("LEDGER_EXTENSION", "legerLineExtension", "How far a ledger line reaches beyond the note head, on each side."),
    ("THIN_BARLINE", "thinBarlineThickness", "The weight of a bar line."),
    ("THICK_BARLINE", "thickBarlineThickness", "The weight of the thick line of a final or repeat bar."),
    ("BARLINE_SEPARATION", "barlineSeparation", "The room between the two lines of a double bar."),
    ("THIN_THICK_SEPARATION", "thinThickBarlineSeparation", "The room between the thin and the thick line of a final bar."),
    ("REPEAT_DOT_SEPARATION", "repeatBarlineDotSeparation", "The room between a repeat bar and its dots."),
    ("BEAM", "beamThickness", "The thickness of a beam."),
    ("BEAM_SPACING", "beamSpacing", "The room between two beams."),
    ("TIE_END", "tieEndpointThickness", "The thickness of a tie or slur at its ends."),
    ("TIE_MIDDLE", "tieMidpointThickness", "The thickness of a tie or slur in its middle."),
];

/// The glyphs taken over, with their SMuFL code points. Only ever appended to.
const GLYPHS: &[(&str, u32)] = &[
    ("GClef", 0xE050),
    ("TimeSig0", 0xE080),
    ("TimeSig1", 0xE081),
    ("TimeSig2", 0xE082),
    ("TimeSig3", 0xE083),
    ("TimeSig4", 0xE084),
    ("TimeSig5", 0xE085),
    ("TimeSig6", 0xE086),
    ("TimeSig7", 0xE087),
    ("TimeSig8", 0xE088),
    ("TimeSig9", 0xE089),
    ("TimeSigCommon", 0xE08A),
    ("TimeSigCutCommon", 0xE08B),
    ("NoteheadDoubleWhole", 0xE0A0),
    ("NoteheadWhole", 0xE0A2),
    ("NoteheadHalf", 0xE0A3),
    ("NoteheadBlack", 0xE0A4),
    ("AugmentationDot", 0xE1E7),
    ("Flag8thUp", 0xE240),
    ("Flag8thDown", 0xE241),
    ("Flag16thUp", 0xE242),
    ("Flag16thDown", 0xE243),
    ("AccidentalFlat", 0xE260),
    ("AccidentalNatural", 0xE261),
    ("AccidentalSharp", 0xE262),
    ("FermataAbove", 0xE4C0),
    ("BreathMarkComma", 0xE4CE),
    ("RestDoubleWhole", 0xE4E2),
    ("RestWhole", 0xE4E3),
    ("RestHalf", 0xE4E4),
    ("RestQuarter", 0xE4E5),
    ("Rest8th", 0xE4E6),
    ("Rest16th", 0xE4E7),
];

/// Copies a glyph's outline into ours, scaled, and keeps its bounds.
struct Copy {
    outline: Outline,
    scale: f32,
    bounds: Option<(f32, f32, f32, f32)>,
}

impl Copy {
    fn see(&mut self, p: Vector2F) -> (f32, f32) {
        let (x, y) = (p.x() * self.scale, p.y() * self.scale);
        self.bounds = Some(match self.bounds {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
        (x, y)
    }
}

impl OutlineSink for Copy {
    fn move_to(&mut self, to: Vector2F) {
        let (x, y) = self.see(to);
        self.outline.move_to(x, y);
    }

    fn line_to(&mut self, to: Vector2F) {
        let (x, y) = self.see(to);
        self.outline.line_to(x, y);
    }

    fn quadratic_curve_to(&mut self, ctrl: Vector2F, to: Vector2F) {
        let (cx, cy) = (ctrl.x() * self.scale, ctrl.y() * self.scale);
        let (x, y) = self.see(to);
        self.outline.quad_to(cx, cy, x, y);
    }

    fn cubic_curve_to(&mut self, ctrl: LineSegment2F, to: Vector2F) {
        let s = self.scale;
        let (x, y) = self.see(to);
        self.outline.cubic_to(ctrl.from_x() * s, ctrl.from_y() * s, ctrl.to_x() * s, ctrl.to_y() * s, x, y);
    }

    fn close(&mut self) {
        self.outline.close();
    }
}

pub fn build(root: &Path) -> Result<(), String> {
    let cache = root.join("target/ref/bravura");
    std::fs::create_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    for file in ["otf/Bravura.otf", "OFL.txt", "Bravura.json"] {
        let name = file.rsplit('/').next().unwrap_or(file);
        let path = cache.join(name);
        if !path.exists() {
            println!("downloading {SOURCE}/{file}");
            let status = Command::new("curl")
                .args(["--location", "--fail", "--silent", "--show-error", "--output"])
                .arg(&path)
                .arg(format!("{SOURCE}/{file}"))
                .status()
                .map_err(|e| format!("curl: {e}"))?;
            if !status.success() {
                return Err(format!("curl failed for {SOURCE}/{file}"));
            }
        }
    }
    let data = std::fs::read(cache.join("Bravura.otf")).map_err(|e| format!("Bravura.otf: {e}"))?;
    let licence = std::fs::read_to_string(cache.join("OFL.txt")).map_err(|e| format!("OFL.txt: {e}"))?;
    let metadata = std::fs::read_to_string(cache.join("Bravura.json")).map_err(|e| format!("Bravura.json: {e}"))?;
    let metadata: serde_json::Value = serde_json::from_str(&metadata).map_err(|e| format!("Bravura.json: {e}"))?;
    let error = |e: &dyn std::fmt::Display| format!("Bravura.otf: {e}");

    let scope = ReadScope::new(&data);
    let file = scope.read::<FontData<'_>>().map_err(|e| error(&e))?;
    let provider = file.table_provider(0).map_err(|e| error(&e))?;
    let mut font = Font::new(provider).map_err(|e| error(&e))?;
    let source_em = f32::from(font.head_table.units_per_em);
    let scale = f32::from(UNITS_PER_EM) / source_em;
    // An owned copy of the outlines table, so that the font stays free for the lookups.
    let cff_data = font.font_table_provider.read_table_data(tag::CFF).map_err(|e| error(&e))?.into_owned();
    let cff = ReadScope::new(&cff_data).read::<CFF<'_>>().map_err(|e| error(&e))?;

    let half = (UNITS_PER_EM / 2) as i16;
    let mut builder = TrueTypeBuilder::new("Gabc Music", UNITS_PER_EM);
    builder
        .vertical_metrics(half, -half, 0)
        .version("Version 0.001")
        .name(
            0,
            "Copyright (c) 2015, Steinberg Media Technologies GmbH (http://www.steinberg.net/). \
             A subset of the glyphs of Bravura under another name, assembled by the gabc-rs contributors.",
        )
        .name(13, "This Font Software is licensed under the SIL Open Font License, Version 1.1.")
        .name(14, "http://scripts.sil.org/OFL");
    builder.add_glyph(&[' '], UNITS_PER_EM / 8, Outline::new()).map_err(|e| e.to_string())?;

    // Where a stem meets a note head and a flag its stem, from the metadata, in staff spaces.
    let anchor = |name: &str, which: &str| -> Option<(f64, f64)> {
        // `NoteheadBlack` here is `noteheadBlack` there.
        let smufl = format!("{}{}", name[..1].to_lowercase(), &name[1..]);
        let point = metadata["glyphsWithAnchors"][smufl.as_str()][which].as_array()?;
        Some((point.first()?.as_f64()?, point.get(1)?.as_f64()?))
    };
    let mut anchors = String::new();
    let mut table = String::new();
    for (name, code) in GLYPHS {
        for (which, function) in [("stemUpSE", "stem_up"), ("stemDownNW", "stem_down"), ("stemUpNW", "on_stem_up"), ("stemDownSW", "on_stem_down")] {
            if let Some((x, y)) = anchor(name, which) {
                let _ = writeln!(anchors, "            (MusicGlyph::{name}, \"{function}\") => Some(({:.5}, {:.5})),", x / 4.0, y / 4.0);
            }
        }
        let ch = char::from_u32(*code).ok_or("a code point that is no character")?;
        let (glyph, _) = font.lookup_glyph_index(ch, MatchingPresentation::NotRequired, None);
        if glyph == 0 {
            return Err(format!("Bravura has no glyph for {name} (U+{code:04X})"));
        }
        let advance = font.horizontal_advance(glyph).ok_or_else(|| format!("{name}: no advance"))?;
        let mut copy = Copy { outline: Outline::new(), scale, bounds: None };
        let mut outlines = CFFOutlines { table: &cff };
        outlines.visit(glyph, None, &mut copy).map_err(|e| format!("{name}: {e}"))?;
        // PostScript outlines run the other way round than TrueType ones.
        for contour in 0..copy.outline.contour_count() {
            copy.outline.reverse_contour(contour);
        }
        let advance_units = (f32::from(advance) * scale).round();
        builder.add_glyph(&[ch], advance_units as u16, copy.outline).map_err(|e| format!("{name}: {e}"))?;
        let (x0, y0, x1, y1) = copy.bounds.unwrap_or_default();
        let em = |units: f32| units / f32::from(UNITS_PER_EM);
        let _ = writeln!(
            table,
            "            MusicGlyph::{name} => m({:.5}, {:.5}, {:.5}, {:.5}, {:.5}),",
            em(advance_units),
            em(x0),
            em(x1),
            em(y1),
            em(-y0)
        );
        println!("U+{code:04X}  {name:<22} advance {advance_units:>5}  ink x {x0:>6.0}..{x1:<6.0} y {y0:>6.0}..{y1:<6.0}");
    }

    let glyphs = builder.glyph_count();
    let bytes = builder.build().map_err(|e| e.to_string())?;
    let fonts = root.join("assets/fonts");
    std::fs::create_dir_all(&fonts).map_err(|e| format!("{}: {e}", fonts.display()))?;
    std::fs::write(fonts.join("gabc-music.ttf"), &bytes).map_err(|e| format!("gabc-music.ttf: {e}"))?;
    let note = "gabc-music.ttf holds a subset of the glyphs of Bravura, under the name \"Gabc Music\" as the\n\
                licence asks for a changed version. It is licensed like Bravura:\n\n";
    std::fs::write(fonts.join("gabc-music-OFL.txt"), format!("{note}{licence}")).map_err(|e| format!("OFL: {e}"))?;

    let mut s = String::from(
        "//! The glyphs of the music font (`assets/fonts/gabc-music.ttf`) with their code points and\n\
         //! sizes. Generated by `cargo run -p xtask -- music-font` from Bravura; do not edit.\n\
         //!\n\
         //! Lengths are in em. One em is four staff spaces, so a staff step is 0.125 em. A glyph's\n\
         //! origin is at the height of the staff position it belongs to, at the left of its advance.\n\n\
         /// A glyph of the music font.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n\
         pub enum MusicGlyph {\n",
    );
    for (name, _) in GLYPHS {
        let _ = writeln!(s, "    {name},");
    }
    s.push_str(
        "}\n\n\
         /// The size of a glyph, in em.\n\
         #[derive(Debug, Clone, Copy, PartialEq)]\n\
         pub struct MusicMetrics {\n\
         \x20   pub advance: f32,\n\
         \x20   /// Where the ink starts and ends, from the origin.\n\
         \x20   pub left: f32,\n\
         \x20   pub right: f32,\n\
         \x20   /// How far the ink reaches above and below the origin.\n\
         \x20   pub above: f32,\n\
         \x20   pub below: f32,\n\
         }\n\n\
         impl MusicGlyph {\n\
         \x20   /// The character of the glyph in the font: its SMuFL code point.\n\
         \x20   pub fn char(self) -> char {\n\
         \x20       match self {\n",
    );
    for (name, code) in GLYPHS {
        let _ = writeln!(s, "            MusicGlyph::{name} => '\\u{{{code:04X}}}',");
    }
    s.push_str(
        "        }\n    }\n\n\
         \x20   pub fn metrics(self) -> MusicMetrics {\n\
         \x20       let m = |advance, left, right, above, below| MusicMetrics { advance, left, right, above, below };\n\
         \x20       match self {\n",
    );
    s.push_str(&table);
    s.push_str("        }\n    }\n\n");
    s.push_str(
        "    fn anchor(self, which: &str) -> Option<(f32, f32)> {\n\
         \x20       match (self, which) {\n",
    );
    s.push_str(&anchors);
    s.push_str(
        "            _ => None,\n        }\n    }\n\n\
         \x20   /// Where the lower right corner of a stem that points up meets this note head: to the\n\
         \x20   /// right of the origin and above it, in em.\n\
         \x20   pub fn stem_up(self) -> Option<(f32, f32)> {\n        self.anchor(\"stem_up\")\n    }\n\n\
         \x20   /// Where the upper left corner of a stem that points down meets this note head.\n\
         \x20   pub fn stem_down(self) -> Option<(f32, f32)> {\n        self.anchor(\"stem_down\")\n    }\n\n\
         \x20   /// For a flag: where the end of its stem lies, from the flag's origin.\n\
         \x20   pub fn on_stem(self) -> Option<(f32, f32)> {\n        self.anchor(\"on_stem_up\").or(self.anchor(\"on_stem_down\"))\n    }\n}\n\n",
    );
    let version = metadata["fontVersion"].as_f64().unwrap_or(0.0);
    let _ = writeln!(s, "/// The measures of the font's engraving defaults (Bravura {version}), in em.");
    s.push_str("pub mod engraving {\n");
    for (constant, key, doc) in DEFAULTS {
        let spaces = metadata["engravingDefaults"][*key].as_f64().ok_or_else(|| format!("Bravura.json has no {key}"))?;
        let _ = writeln!(s, "    /// {doc}\n    pub const {constant}: f32 = {:.5};", spaces / 4.0);
    }
    s.push_str("}\n");
    std::fs::write(root.join("src/music_glyphs.rs"), s).map_err(|e| format!("src/music_glyphs.rs: {e}"))?;
    println!("\nwrote assets/fonts/gabc-music.ttf ({} bytes, {glyphs} glyphs)", bytes.len());
    Ok(())
}
