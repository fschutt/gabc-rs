//! From a parsed score to lines of placed things: the engraving.
//!
//! Each syllable's notes are grouped into neumes and set ([`crate::neume`]), its text is put
//! under them with the first vowel under the first note, and the syllables are filled into
//! lines of a given width. A line ends with a custos that shows the first note of the next
//! line, which starts with the clef again. Nothing here knows about HTML or fonts beyond a
//! function that says how wide a piece of text is.
//!
//! All lengths are in em of the score, one em being four staff spaces. Heights are pitches
//! (`a` = 0, one per staff step); [`Line::y`] turns one into a distance from the top of the
//! line.

use crate::gabc::{AccidentalKind, Bar, Clef, ElementKind, NoteGroup, Score, Space};
use crate::glyphs::Glyph;
use crate::neume::{self, Head, LINE_WEIGHT, NEUME_SPACE, STEP};

/// How the score is to be laid out.
pub struct Options<'a> {
    /// The width of a line, in em of the score.
    pub width: f32,
    /// The size of the lyrics as a part of the score's em.
    pub lyric_size: f32,
    /// The width of a piece of lyric text, in em of the lyric size. `None` estimates it from
    /// the letters, which is close enough to look at a score and not to print one.
    pub measure: Option<&'a dyn Fn(&str) -> f32>,
}

/// The size of the lyrics under chant, as a part of the score's em: exsurge's text of 16 px
/// on a staff whose em is 50 px.
pub const CHANT_LYRIC_SIZE: f32 = 0.32;
/// The size of the lyrics under a hymn. A choice of this crate: Bravura's metadata says
/// nothing about text, and hymnals differ.
pub const HYMN_LYRIC_SIZE: f32 = 0.42;

impl Default for Options<'_> {
    /// The options for chant at a width of 18 em.
    fn default() -> Self {
        Options::chant(18.0)
    }
}

impl Options<'_> {
    /// For chant at a line width in em, with estimated text widths.
    pub fn chant(width: f32) -> Self {
        Options { width, lyric_size: CHANT_LYRIC_SIZE, measure: None }
    }

    /// For a hymn at a line width in em, with estimated text widths.
    pub fn hymn(width: f32) -> Self {
        Options { width, lyric_size: HYMN_LYRIC_SIZE, measure: None }
    }
}

/// A glyph of the neume font at its place in a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub glyph: Glyph,
    pub x: f32,
    pub pitch: f32,
}

/// A thin line, vertical (stems, joining lines, bars) or horizontal (episema).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    pub x: f32,
    /// The upper edge, as a pitch.
    pub top: f32,
    pub width: f32,
    /// How far down it reaches, in staff steps.
    pub height: f32,
}

/// A glyph of the music font at its place in a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedMusic {
    pub glyph: crate::music_glyphs::MusicGlyph,
    pub x: f32,
    pub pitch: f32,
}

/// A tie or slur: a thin bow from one point to another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub x0: f32,
    pub pitch0: f32,
    pub x1: f32,
    pub pitch1: f32,
    /// True if the bow arches upwards.
    pub above: bool,
    /// How thick it is in its middle, in em.
    pub thickness: f32,
}

/// A syllable's text at its place.
#[derive(Debug, Clone, PartialEq)]
pub struct Lyric {
    pub x: f32,
    /// Which row of text under the staff: 0 for chant and for a hymn's first verse.
    pub row: usize,
    pub spans: Vec<Span>,
}

/// A piece of text in one style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    pub text: String,
    pub italic: bool,
    pub bold: bool,
    pub small_caps: bool,
    /// `<c>`: in the colour of rubrics.
    pub coloured: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// The pitches of the staff lines: four for chant, five for a hymn.
    pub staff: Vec<f32>,
    /// The weight of a staff line, in em.
    pub staff_weight: f32,
    pub glyphs: Vec<PlacedGlyph>,
    pub music: Vec<PlacedMusic>,
    pub rules: Vec<Rule>,
    pub curves: Vec<Curve>,
    pub lyrics: Vec<Lyric>,
    /// The highest and lowest pitch drawn on the line, staff included.
    pub highest: f32,
    pub lowest: f32,
}

/// The pitch of the top staff line on a staff of four lines.
const TOP_LINE: f32 = 9.0;
/// The pitch of the bottom staff line.
const BOTTOM_LINE: f32 = 3.0;

impl Default for Line {
    fn default() -> Self {
        Line {
            staff: vec![3.0, 5.0, 7.0, 9.0],
            staff_weight: LINE_WEIGHT,
            glyphs: Vec::new(),
            music: Vec::new(),
            rules: Vec::new(),
            curves: Vec::new(),
            lyrics: Vec::new(),
            highest: TOP_LINE,
            lowest: BOTTOM_LINE,
        }
    }
}

impl Line {
    fn top_line(&self) -> f32 {
        self.staff.iter().copied().fold(f32::MIN, f32::max)
    }

    fn bottom_line(&self) -> f32 {
        self.staff.iter().copied().fold(f32::MAX, f32::min)
    }

    /// The room above the top staff line, in em.
    pub fn head_room(&self) -> f32 {
        ((self.highest - self.top_line()).max(0.0) + 2.5) * STEP
    }

    /// The distance of a pitch from the top of the line, in em.
    pub fn y(&self, pitch: f32) -> f32 {
        self.head_room() + (self.top_line() - pitch) * STEP
    }

    /// The top of the first row of lyrics, in em from the top of the line.
    pub fn lyric_y(&self) -> f32 {
        self.y(self.lowest.min(self.bottom_line())) + 2.2 * STEP
    }

    /// The top of a row of lyrics.
    pub fn lyric_row_y(&self, row: usize, lyric_size: f32) -> f32 {
        self.lyric_y() + row as f32 * lyric_size * 1.3
    }

    /// The height of the line with its lyrics, in em.
    pub fn height(&self, lyric_size: f32) -> f32 {
        let rows = self.lyrics.iter().map(|l| l.row + 1).max().unwrap_or(1);
        self.lyric_row_y(rows - 1, lyric_size) + lyric_size * 1.35
    }
}

/// The lines of a score.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Engraving {
    pub lines: Vec<Line>,
    pub width: f32,
    pub lyric_size: f32,
}

/// A syllable set on its own, before it has a place in a line.
struct Block {
    glyphs: Vec<PlacedGlyph>,
    rules: Vec<Rule>,
    spans: Vec<Span>,
    /// The width of the notes.
    notes: f32,
    /// The point of the notes that the text is centred under: the middle of the first note,
    /// or the middle of a bar.
    origin: f32,
    /// The room after the notes before the next ones.
    trailing: f32,
    /// The width of the text, in em of the score, and with a hyphen after it.
    text: f32,
    /// The point of the text that stands under `origin`, from the text's left edge.
    centre: f32,
    /// The syllable goes on in the next one: a hyphen may be needed after it.
    allows_hyphen: bool,
    /// The hyphen is needed: the next syllable could not be set right against this one.
    hyphen: bool,
    /// Holds nothing but a bar.
    is_bar: bool,
    /// The first sounding pitch, for the custos of the line before.
    first_pitch: Option<u8>,
    /// A clef that takes effect here, to be repeated on later lines.
    clef: Option<Clef>,
    /// Holds only a clef: it is drawn at the start of the line, not here.
    only_clef: bool,
    /// A forced line break after this block.
    breaks: bool,
    /// Where the notes start in their line.
    x: f32,
}

impl Block {
    fn has_text(&self) -> bool {
        !self.spans.is_empty()
    }

    fn text_left(&self) -> f32 {
        self.x + self.origin - self.centre
    }

    fn text_right(&self, hyphen_width: f32) -> f32 {
        self.text_left() + self.text + if self.hyphen { hyphen_width } else { 0.0 }
    }

    fn notes_right(&self) -> f32 {
        self.x + self.notes
    }
}

/// Lays a score out in lines.
///
/// How a syllable is put against the one before it, where a line ends, what is spread when a
/// line is justified and where the custos stands follow exsurge's `ChantLine`
/// (`positionNotationElement`, `buildFromChantNotationIndex`, `justifyElements`).
pub fn engrave(score: &Score, options: &Options) -> Engraving {
    let estimate = |text: &str| estimate_width(text);
    let measure: &dyn Fn(&str) -> f32 = options.measure.unwrap_or(&estimate);
    let size = options.lyric_size;
    let hyphen_width = measure("-") * size;
    // Between two words there is at least the width of a hyphen.
    let word_space = hyphen_width;

    // Gregorio reads a score without a clef with a do clef on the third line.
    let mut clef = Clef { do_clef: true, line: 3, flat: false };
    let mut styles = Styles::default();
    let mut blocks = Vec::new();
    for (k, syllable) in score.syllables.iter().enumerate() {
        let read = styles.read(&syllable.text);
        let plain = read.plain();
        let empty = NoteGroup::default();
        let group = syllable.notes.as_ref().unwrap_or(&empty);
        let mut block = set_group(group);
        block.text = measure(&plain) * size;
        // The text stands with the middle of its vowel under the middle of the first note:
        // the part in `{...}` if there is one, else what the language says, else its middle.
        let (start, len) = read.centre(&plain);
        let spans = read.spans;
        let (before, upto) = (measure(&plain[..start]) * size, measure(&plain[..start + len]) * size);
        block.centre = before + (upto - before) / 2.0;
        block.allows_hyphen = !plain.is_empty() && score.syllables.get(k + 1).is_some_and(|next| !next.starts_word && !next.text.is_empty());
        block.spans = spans;
        if let Some(new) = block.clef {
            clef = new;
        }
        block.clef = Some(clef);
        blocks.push(block);
    }

    let clef_right = Glyph::DoClef.metrics().width + 4.0 * NEUME_SPACE;
    let custos_width = Glyph::CustosLong.metrics().width;
    // What is left of a line when room is kept for a custos.
    let notes_limit = options.width - custos_width - 4.0 * NEUME_SPACE;

    let mut engraving = Engraving { lines: Vec::new(), width: options.width, lyric_size: size };
    let mut on_line: Vec<usize> = Vec::new();
    let mut line_clef = blocks.first().and_then(|b| b.clef).unwrap_or(clef);
    let mut k = 0usize;
    let last = blocks.len().saturating_sub(1);
    while k < blocks.len() {
        if blocks[k].only_clef && on_line.is_empty() {
            // At the start of a line the clef is drawn by the line itself.
            line_clef = blocks[k].clef.unwrap_or(line_clef);
            k += 1;
            continue;
        }
        // Right after what stands before it, whatever the text says.
        let (prev_right, prev_trailing) = match on_line.last() {
            Some(&p) => (blocks[p].notes_right(), blocks[p].trailing),
            None => (clef_right, 0.0),
        };
        let mut x = prev_right + prev_trailing;
        let with_text = on_line.iter().rev().copied().find(|&p| blocks[p].has_text());
        blocks[k].x = x;
        blocks[k].hyphen = false;
        match with_text {
            None => {
                // The first text on the line may not hang out on the left.
                let left = blocks[k].text_left();
                if blocks[k].has_text() && left < 0.0 {
                    x -= left;
                }
            }
            Some(p) if !blocks[k].has_text() => {
                // Notes without text: the syllable before them keeps its hyphen.
                if blocks[p].allows_hyphen {
                    blocks[p].hyphen = true;
                }
            }
            Some(p) => {
                let left = blocks[k].text_left();
                if !blocks[p].allows_hyphen {
                    // A new word: a space between the two.
                    let right = blocks[p].text_right(hyphen_width) + word_space;
                    if right > left {
                        x += right - left;
                    }
                } else {
                    blocks[p].hyphen = false;
                    let right = blocks[p].text_right(hyphen_width);
                    if right > left {
                        // The two syllables meet: set them right against each other.
                        x += right - left;
                    } else {
                        // There is room between them, which a hyphen fills.
                        blocks[p].hyphen = true;
                        let right = blocks[p].text_right(hyphen_width);
                        if right > left {
                            x += right - left;
                        }
                    }
                }
            }
        }
        blocks[k].x = x;
        // The last syllable of the score needs no room for a custos.
        let limit = if k == last { options.width } else { notes_limit };
        let fits = blocks[k].notes_right() + if k == last { 0.0 } else { blocks[k].trailing } < limit
            && (!blocks[k].has_text() || blocks[k].text_right(hyphen_width) <= options.width);
        if !fits && !on_line.is_empty() {
            // A bar is not left alone at the start of a line: what stands before it goes along.
            if blocks[k].is_bar && on_line.len() > 1 {
                k = on_line.pop().unwrap_or(k);
            }
            // The word goes on in the next line.
            if let Some(p) = on_line.iter().rev().copied().find(|&p| blocks[p].has_text()) {
                if blocks[p].allows_hyphen {
                    blocks[p].hyphen = true;
                }
            }
            let custos = blocks[k..].iter().find_map(|b| b.first_pitch);
            engraving.lines.push(finish_line(&mut blocks, &on_line, line_clef, custos, true, options, hyphen_width));
            on_line.clear();
            line_clef = blocks[k].clef.unwrap_or(line_clef);
            continue;
        }
        on_line.push(k);
        if blocks[k].breaks {
            let custos = blocks[k + 1..].iter().find_map(|b| b.first_pitch);
            engraving.lines.push(finish_line(&mut blocks, &on_line, line_clef, custos, false, options, hyphen_width));
            on_line.clear();
            line_clef = blocks.get(k + 1).and_then(|b| b.clef).unwrap_or(line_clef);
        }
        k += 1;
    }
    if !on_line.is_empty() {
        // The last line is only spread if it ends with a bar and is nearly full.
        let end = on_line.last().map_or(0.0, |&p| blocks[p].notes_right());
        let justify = on_line.last().is_some_and(|&p| blocks[p].is_bar) && (options.width - end) / options.width < 0.1;
        engraving.lines.push(finish_line(&mut blocks, &on_line, line_clef, None, justify, options, hyphen_width));
    }
    engraving
}

/// Spreads a line to its width if asked to, and draws it.
fn finish_line(
    blocks: &mut [Block],
    on_line: &[usize],
    clef: Clef,
    custos: Option<u8>,
    justify: bool,
    options: &Options,
    hyphen_width: f32,
) -> Line {
    let custos_glyph = custos.map(custos_glyph);
    if justify {
        // The room that is left, after the text and the notes and before the custos.
        let mut room = 0.0;
        if let Some(&p) = on_line.last() {
            let text = on_line.iter().rev().find(|&&q| blocks[q].has_text()).map_or(f32::MIN, |&q| blocks[q].text_right(hyphen_width));
            room = options.width - text.max(blocks[p].notes_right() + blocks[p].trailing);
        }
        if let Some(glyph) = custos_glyph {
            room -= glyph.metrics().width;
        }
        if room > 0.0 {
            // Room goes before every syllable, except between two that were set right
            // against each other.
            let mut movable = Vec::new();
            let mut with_text: Option<usize> = None;
            for (n, &k) in on_line.iter().enumerate() {
                let joined = with_text.is_some_and(|p| blocks[p].allows_hyphen && !blocks[p].hyphen);
                if !joined {
                    movable.push(n);
                }
                if blocks[k].has_text() {
                    with_text = Some(k);
                }
            }
            if !movable.is_empty() {
                let step = room / movable.len() as f32;
                let mut offset = 0.0;
                for (n, &k) in on_line.iter().enumerate() {
                    if movable.contains(&n) {
                        offset += step;
                    }
                    blocks[k].x += offset;
                }
            }
        }
    }

    let mut line = Line::default();
    place_clef(&mut line, clef);
    for &k in on_line {
        let block = &blocks[k];
        for glyph in &block.glyphs {
            line.glyphs.push(PlacedGlyph { x: glyph.x + block.x, ..*glyph });
            let metrics = glyph.glyph.metrics();
            line.highest = line.highest.max(glyph.pitch + metrics.above / STEP);
            line.lowest = line.lowest.min(glyph.pitch - metrics.below / STEP);
        }
        for rule in &block.rules {
            line.rules.push(Rule { x: rule.x + block.x, ..*rule });
            line.highest = line.highest.max(rule.top);
            line.lowest = line.lowest.min(rule.top - rule.height);
        }
        if block.has_text() {
            let mut spans = block.spans.clone();
            if block.hyphen {
                if let Some(last) = spans.last_mut() {
                    last.text.push('-');
                }
            }
            line.lyrics.push(Lyric { x: block.text_left(), row: 0, spans });
        }
    }
    if let (Some(pitch), Some(glyph)) = (custos, custos_glyph) {
        // At the very end of the line.
        let pitch = f32::from(pitch);
        line.glyphs.push(PlacedGlyph { glyph, x: options.width - glyph.metrics().width, pitch });
        line.highest = line.highest.max(pitch + glyph.metrics().above / STEP);
        line.lowest = line.lowest.min(pitch - glyph.metrics().below / STEP);
    }
    line
}

/// The custos for a pitch, as exsurge chooses it: its stem up from the upper part of the
/// staff on, and the long form on a line.
fn custos_glyph(pitch: u8) -> Glyph {
    match (pitch <= 8, pitch % 2 == 1) {
        (true, true) => Glyph::CustosLong,
        (true, false) => Glyph::CustosShort,
        (false, true) => Glyph::CustosDescLong,
        (false, false) => Glyph::CustosDescShort,
    }
}

fn place_clef(line: &mut Line, clef: Clef) {
    // Line 1 is the bottom line.
    let pitch = BOTTOM_LINE + 2.0 * (f32::from(clef.line) - 1.0);
    let glyph = if clef.do_clef { Glyph::DoClef } else { Glyph::FaClef };
    let mut x = 0.0;
    if clef.flat {
        // The flat of a flat clef stands in the space below the clef's line, as in exsurge.
        line.glyphs.push(PlacedGlyph { glyph: Glyph::Flat, x, pitch: pitch - 1.0 });
        x += Glyph::Flat.metrics().width + NEUME_SPACE / 2.0;
    }
    line.glyphs.push(PlacedGlyph { glyph, x, pitch });
    line.highest = line.highest.max(pitch + glyph.metrics().above / STEP);
}

/// Sets what stands between one pair of parentheses. The places of the dot, the vertical and
/// the horizontal episema follow exsurge's `Markings`; the bars its `Signs`.
fn set_group(group: &NoteGroup) -> Block {
    let mut block = Block {
        glyphs: Vec::new(),
        rules: Vec::new(),
        spans: Vec::new(),
        notes: 0.0,
        origin: 0.0,
        // The room after a neume or a bar, where nothing else is written.
        trailing: 4.0 * NEUME_SPACE,
        text: 0.0,
        centre: 0.0,
        allows_hyphen: false,
        hyphen: false,
        is_bar: !group.elements.is_empty(),
        first_pitch: None,
        clef: None,
        only_clef: !group.elements.is_empty(),
        breaks: false,
        x: 0.0,
    };
    let mut x = 0.0f32;
    // The notes that stand together, waiting to become neumes.
    let mut run: Vec<Head> = Vec::new();
    let mut pending_space = 0.0f32;
    // The point the text is centred under: set by the first thing that is drawn.
    let mut origin: Option<f32> = None;

    fn flush(run: &mut Vec<Head>, group: &NoteGroup, block: &mut Block, x: &mut f32, origin: &mut Option<f32>) {
        if run.is_empty() {
            return;
        }
        let found = neume::neumes(run);
        for (n, neume) in found.iter().enumerate() {
            if n > 0 {
                *x += NEUME_SPACE;
            }
            let set = neume::set(neume, run);
            if let Some(first) = set.glyphs.first() {
                origin.get_or_insert(*x + first.x + first.glyph.metrics().origin);
            }
            for glyph in &set.glyphs {
                block.glyphs.push(PlacedGlyph { glyph: glyph.glyph, x: glyph.x + *x, pitch: glyph.pitch });
            }
            for stroke in &set.strokes {
                block.rules.push(Rule {
                    x: stroke.x + *x,
                    top: stroke.top,
                    width: LINE_WEIGHT,
                    height: stroke.top - stroke.bottom,
                });
            }
            // The glyph that stands for a head, with its left edge in the block.
            let glyph_of = |index: usize| set.glyphs.iter().rev().find(|g| g.head == index).or(set.glyphs.last());
            let mut dots_right = 0.0f32;
            for (k, head) in run[neume.heads.clone()].iter().enumerate() {
                let index = neume.heads.start + k;
                // A doubled shape carries its signs on its last head.
                if run.get(index + 1).is_some_and(|next| next.note == head.note) {
                    continue;
                }
                let ElementKind::Note(note) = &group.elements[head.note].kind else { continue };
                let Some(placed) = glyph_of(index) else { continue };
                let metrics = placed.glyph.metrics();
                let (left, right) = (placed.x + *x, placed.x + *x + metrics.width);
                let pitch = f32::from(head.pitch);
                let on_line = head.pitch % 2 == 1;

                // Dots: a quarter of a step after the note; beside a note on a line the dot
                // moves up into the space. A second one goes to the other side.
                for dot in 0..note.mora {
                    let at = match (dot, on_line) {
                        (0, true) => pitch + 0.75,
                        (0, false) => pitch,
                        (_, true) => pitch - 0.75,
                        (_, false) => pitch + 1.75,
                    };
                    let dot_x = right + STEP / 4.0;
                    block.glyphs.push(PlacedGlyph { glyph: Glyph::Mora, x: dot_x, pitch: at });
                    dots_right = dots_right.max(dot_x + Glyph::Mora.metrics().width - *x - set.width);
                }
                // The vertical episema: under the middle of the note, or over it for `'1`.
                if note.ictus {
                    let above = note.raw_modifiers.contains("'1");
                    let glyph = if above { Glyph::VerticalEpisemaAbove } else { Glyph::VerticalEpisemaBelow };
                    let away = match (above, on_line) {
                        (true, false) => 1.5,
                        (true, true) => 0.9,
                        (false, false) => -1.5,
                        (false, true) => -0.8,
                    };
                    block.glyphs.push(PlacedGlyph {
                        glyph,
                        x: left + metrics.width / 2.0 - glyph.metrics().origin,
                        pitch: pitch + away,
                    });
                }
                // The horizontal episema: `_` written n times lies over this note and the
                // n - 1 before it, as a stroke per note in the nearest space that is clear.
                for back in 0..usize::from(note.episema) {
                    let Some(over) = index.checked_sub(back).filter(|i| *i >= neume.heads.start).and_then(|i| glyph_of(i)) else { break };
                    let over_metrics = over.glyph.metrics();
                    let below = note.raw_modifiers.contains("_0");
                    let top = if below {
                        let lowest = (over.pitch - over_metrics.below / STEP - 0.4).ceil();
                        if lowest.rem_euclid(2.0) == 1.0 { lowest - 1.0 } else { lowest }
                    } else {
                        let highest = (over.pitch + over_metrics.above / STEP + 0.4).floor();
                        if highest.rem_euclid(2.0) == 1.0 { highest + 1.0 } else { highest }
                    };
                    // Over a porrectus stroke it is as wide as one note.
                    let porrectus = matches!(over.glyph, Glyph::Porrectus1 | Glyph::Porrectus2 | Glyph::Porrectus3 | Glyph::Porrectus4);
                    let width = if porrectus { STEP } else { over_metrics.width };
                    block.rules.push(Rule { x: over.x + *x, top, width, height: LINE_WEIGHT / STEP });
                }
            }
            *x += set.width + dots_right.max(0.0);
        }
        run.clear();
    }

    for (index, element) in group.elements.iter().enumerate() {
        if !matches!(element.kind, ElementKind::Clef(_) | ElementKind::SecondaryClef(_)) {
            block.only_clef = false;
        }
        if !matches!(element.kind, ElementKind::Bar(_)) {
            block.is_bar = false;
        }
        match &element.kind {
            ElementKind::Note(note) => {
                if run.is_empty() {
                    x += pending_space;
                    pending_space = 0.0;
                }
                block.first_pitch.get_or_insert(note.pitch);
                run.extend(neume::heads(note, index));
            }
            ElementKind::Join | ElementKind::At | ElementKind::Attribute { .. } => {
                // `!` ends the neume and sets the next one right against it.
                if element.kind == ElementKind::Join {
                    flush(&mut run, group, &mut block, &mut x, &mut origin);
                }
            }
            ElementKind::Space(space) => {
                flush(&mut run, group, &mut block, &mut x, &mut origin);
                // `/` is the room between two neumes, `//` and a blank twice that.
                pending_space = match space {
                    Space::Small | Space::SmallJoined | Space::SmallNoBreak | Space::Half => NEUME_SPACE,
                    Space::Medium | Space::Large | Space::Sized => 2.0 * NEUME_SPACE,
                };
            }
            ElementKind::Accidental { pitch, kind, .. } => {
                flush(&mut run, group, &mut block, &mut x, &mut origin);
                x += pending_space;
                // The font has no sharp: a natural stands in for it for now.
                let glyph = if *kind == AccidentalKind::Flat { Glyph::Flat } else { Glyph::Natural };
                origin.get_or_insert(x + glyph.metrics().origin);
                block.glyphs.push(PlacedGlyph { glyph, x, pitch: f32::from(*pitch) });
                x += glyph.metrics().width;
                pending_space = 2.0 * NEUME_SPACE;
            }
            ElementKind::Bar(bar) => {
                flush(&mut run, group, &mut block, &mut x, &mut origin);
                x += pending_space;
                if !block.glyphs.is_empty() || !block.rules.is_empty() {
                    x += 2.0 * NEUME_SPACE;
                }
                let start = x;
                let mut rule = |x: f32, top: f32, bottom: f32| {
                    block.rules.push(Rule { x, top, width: LINE_WEIGHT, height: top - bottom });
                };
                match bar {
                    Bar::Virgula { high } => {
                        let pitch = if *high { TOP_LINE + 2.0 } else { TOP_LINE };
                        block.glyphs.push(PlacedGlyph { glyph: Glyph::Virgula, x, pitch });
                        x += Glyph::Virgula.metrics().width - LINE_WEIGHT;
                    }
                    // exsurge has the quarter, half, full and double bar. The stroke shorter
                    // than a quarter bar, the forms above the staff and the dotted bar are
                    // drawn here by their likeness to those; the Dominican bars as half bars.
                    Bar::Minimis { high } => {
                        let top = TOP_LINE + if *high { 2.5 } else { 0.5 };
                        rule(x, top, top - 1.0);
                    }
                    Bar::Minima { high } => {
                        let top = TOP_LINE + if *high { 3.0 } else { 1.0 };
                        rule(x, top, top - 2.0);
                    }
                    Bar::Minor | Bar::Dominican(_) => rule(x, TOP_LINE - 1.0, BOTTOM_LINE + 1.0),
                    Bar::Maior | Bar::MaiorDotted => rule(x, TOP_LINE, BOTTOM_LINE),
                    Bar::Finalis => {
                        rule(x, TOP_LINE, BOTTOM_LINE);
                        x += 2.0 * NEUME_SPACE;
                        rule(x, TOP_LINE, BOTTOM_LINE);
                    }
                }
                x += LINE_WEIGHT;
                // Text under a bar is centred under it.
                origin.get_or_insert((start + x) / 2.0);
                pending_space = 2.0 * NEUME_SPACE;
            }
            ElementKind::Custos(pitch) => {
                flush(&mut run, group, &mut block, &mut x, &mut origin);
                if let Some(pitch) = pitch {
                    x += pending_space + NEUME_SPACE;
                    pending_space = 0.0;
                    let glyph = custos_glyph(*pitch);
                    block.glyphs.push(PlacedGlyph { glyph, x, pitch: f32::from(*pitch) });
                    x += glyph.metrics().width;
                }
            }
            ElementKind::Clef(clef) => {
                flush(&mut run, group, &mut block, &mut x, &mut origin);
                block.clef = Some(*clef);
            }
            ElementKind::LineBreak { .. } => {
                flush(&mut run, group, &mut block, &mut x, &mut origin);
                block.breaks = true;
            }
            ElementKind::SecondaryClef(_)
            | ElementKind::VoiceStart
            | ElementKind::VoiceEnd
            | ElementKind::NoBreak { .. } => {}
        }
    }
    flush(&mut run, group, &mut block, &mut x, &mut origin);
    block.notes = x;
    block.origin = origin.unwrap_or(0.0);
    block
}

/// The styles that are open in the text, carried from syllable to syllable.
#[derive(Default)]
struct Styles {
    italic: bool,
    bold: bool,
    small_caps: bool,
    coloured: bool,
}

impl Styles {
    /// Reads a syllable's text as written into spans of plain text. With them comes the part
    /// that was written in `{...}`, as where it starts in the plain text and how long it is:
    /// the text is to be centred on it.
    fn read(&mut self, raw: &str) -> Read {
        let mut spans: Vec<Span> = Vec::new();
        let mut text = String::new();
        // How much plain text there is so far, and the part in braces.
        let mut plain_len = 0usize;
        let mut brace: Option<usize> = None;
        let mut forced: Option<(usize, usize)> = None;
        let mut special: Vec<(usize, usize)> = Vec::new();
        let mut i = 0;
        let flush = |text: &mut String, spans: &mut Vec<Span>, s: &Styles, plain_len: &mut usize| {
            *plain_len += text.len();
            if !text.is_empty() {
                spans.push(Span {
                    text: std::mem::take(text),
                    italic: s.italic,
                    bold: s.bold,
                    small_caps: s.small_caps,
                    coloured: s.coloured,
                });
            }
        };
        while i < raw.len() {
            let rest = &raw[i..];
            let c = rest.chars().next().unwrap_or(' ');
            if c == '$' {
                // The next character as it stands.
                if let Some(next) = rest[1..].chars().next() {
                    text.push(next);
                    i += 1 + next.len_utf8();
                } else {
                    i += 1;
                }
                continue;
            }
            if c == '<' {
                if let Some(end) = rest.find('>') {
                    let tag = &rest[1..end];
                    let (closing, name) = match tag.strip_prefix('/') {
                        Some(name) => (true, name),
                        None => (false, tag.split(':').next().unwrap_or(tag)),
                    };
                    // TeX and text above the staff are not drawn; a special character is.
                    if !closing && matches!(name, "v" | "alt" | "sp") {
                        let close = format!("</{name}>");
                        let inner_end = rest.find(&close).unwrap_or(rest.len());
                        if name == "sp" {
                            let start = plain_len + text.len();
                            text.push_str(special_character(&rest[end + 1..inner_end]));
                            special.push((start, plain_len + text.len()));
                        }
                        i += (inner_end + close.len()).min(rest.len());
                        continue;
                    }
                    let known = matches!(name, "i" | "b" | "sc" | "c" | "ul" | "tt" | "e" | "eu" | "nlba" | "pr" | "clear");
                    if known {
                        flush(&mut text, &mut spans, self, &mut plain_len);
                        match name {
                            "i" | "e" => self.italic = !closing,
                            "b" => self.bold = !closing,
                            "sc" => self.small_caps = !closing,
                            "c" => self.coloured = !closing,
                            _ => {}
                        }
                        i += end + 1;
                        continue;
                    }
                }
            }
            // The braces that mark what to centre on are not printed. Only the first pair counts.
            match c {
                '{' if forced.is_none() && brace.is_none() => brace = Some(plain_len + text.len()),
                '}' => {
                    if let Some(start) = brace.take() {
                        forced = Some((start, plain_len + text.len() - start));
                    }
                }
                '{' => {}
                _ => text.push(c),
            }
            i += c.len_utf8();
        }
        flush(&mut text, &mut spans, self, &mut plain_len);
        Read { spans, forced, special }
    }
}

/// A syllable's text as read.
struct Read {
    /// The plain text in its styles.
    spans: Vec<Span>,
    /// The part written in `{...}`: where it starts in the plain text and how long it is.
    forced: Option<(usize, usize)>,
    /// The parts that came from `<sp>` codes, as ranges of the plain text.
    special: Vec<(usize, usize)>,
}

impl Read {
    fn plain(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// The part of the plain text that stands under the middle of the first note.
    fn centre(&self, plain: &str) -> (usize, usize) {
        self.forced.or_else(|| vowel_segment(plain, &self.special)).unwrap_or((0, plain.len()))
    }
}

/// The character that a `<sp>` code stands for. These are the codes the format's description
/// names; any other code is printed as it is written, since nothing says what it stands for.
/// (The description also has `A/`, a barred A that is not in Unicode; it is left as written.)
fn special_character(code: &str) -> &str {
    match code {
        "V/" => "\u{2123}",
        "R/" => "\u{211F}",
        "ae" => "æ",
        "'ae" | "'æ" => "ǽ",
        "oe" => "œ",
        "'oe" | "'œ" => "œ\u{301}",
        other => other,
    }
}

/// A syllable's text as it is printed, and the part of it that stands under the middle of
/// the first note: the part written in `{...}`, or what [`vowel_segment`] finds. For checking
/// the centring against a reference.
pub fn centred_part(raw: &str) -> (String, String) {
    let read = Styles::default().read(raw);
    let plain = read.plain();
    let (start, len) = read.centre(&plain);
    let centre = plain.get(start..start + len).unwrap_or("").to_string();
    (plain, centre)
}

/// The letters of a syllable that stand under the middle of its first note, as where they
/// start in the text and how many bytes they are. `None` for an empty text.
///
/// This is what Gregorio centres. The rule was read off the 1.16 million syllables of the
/// GregoBase library, of which it gives the same letters for 99.7% (`docs/gabc-observed.md`):
/// the first run of vowels, where an `i` or `u` before another vowel counts as a consonant
/// (qui, cui, iam, sanguis). A text with no vowel is centred as a whole. A character written
/// as a `<sp>` code is no vowel, whatever it shows (`special` are those parts of the text).
/// The vowels are the letters that Gregorio centres in that data; exsurge's rule differs
/// (it takes `ui` as a diphthong and centres one vowel otherwise).
fn vowel_segment(text: &str, special: &[(usize, usize)]) -> Option<(usize, usize)> {
    const VOWELS: &str = "aeiouyáéíóúýàèìòùâêôëæœǽăąęơư";
    // Lower case, letter for letter, with where each letter starts in the text.
    let letters: Vec<(usize, char)> =
        text.char_indices().map(|(at, c)| (at, c.to_lowercase().next().unwrap_or(c))).collect();
    let end = |n: usize| letters.get(n).map_or(text.len(), |l| l.0);
    let is_vowel = |n: usize| {
        letters.get(n).is_some_and(|l| VOWELS.contains(l.1) && !special.iter().any(|r| r.0 <= l.0 && l.0 < r.1))
    };
    let Some(mut first) = (0..letters.len()).find(|&n| is_vowel(n)) else {
        return (!text.is_empty()).then_some((0, text.len()));
    };
    while matches!(letters[first].1, 'i' | 'u') && is_vowel(first + 1) {
        first += 1;
    }
    let mut after = first + 1;
    while is_vowel(after) {
        after += 1;
    }
    Some((letters[first].0, end(after) - letters[first].0))
}

/// A rough width of text in em, for when no font is at hand.
pub(crate) fn estimate_width(text: &str) -> f32 {
    text.chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | 'í' | '.' | ',' | ';' | ':' | '!' | '\'' | '|' | ' ' | '\u{a0}' => 0.28,
            'f' | 't' | 'r' | '-' | '(' | ')' | '*' => 0.34,
            'm' | 'w' | 'M' | 'W' | 'æ' | 'œ' | 'Æ' | 'Œ' | 'ǽ' => 0.82,
            '\u{300}'..='\u{36f}' => 0.0,
            c if c.is_uppercase() => 0.68,
            _ => 0.5,
        })
        .sum::<f32>()
        // Rather a little too wide than text that runs into its neighbour.
        * 1.06
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gabc::parse;

    fn engraved(source: &str, width: f32) -> Engraving {
        let parsed = parse(source);
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        engrave(&parsed.score, &Options { width, ..Options::default() })
    }

    #[test]
    fn a_short_score_is_one_line_with_clef_notes_bar_and_lyrics() {
        let e = engraved("%%\n(c4) KY(f)ri(gh)e(h.) (::)", 18.0);
        assert_eq!(e.lines.len(), 1);
        let line = &e.lines[0];
        let glyphs: Vec<Glyph> = line.glyphs.iter().map(|g| g.glyph).collect();
        assert_eq!(
            glyphs,
            [Glyph::DoClef, Glyph::PunctumQuadratum, Glyph::PodatusLower, Glyph::PodatusUpper, Glyph::PunctumQuadratum, Glyph::Mora]
        );
        // The clef sits on the top line, the notes follow from left to right.
        assert_eq!(line.glyphs[0].pitch, 9.0);
        assert!(line.glyphs[1].x > Glyph::DoClef.metrics().width);
        assert!(line.glyphs[2].x > line.glyphs[1].x);
        // With a quarter of an em between the neumes the syllables stand apart, joined by hyphens.
        let texts: Vec<String> = line.lyrics.iter().map(|l| l.spans[0].text.clone()).collect();
        assert_eq!(texts, ["KY-", "ri-", "e"]);
        // The double bar is two full-height rules.
        let bars: Vec<&Rule> = line.rules.iter().filter(|r| r.height == 6.0).collect();
        assert_eq!(bars.len(), 2);
    }

    #[test]
    fn a_long_score_breaks_into_lines_with_custos_and_clef() {
        let body: String = (0..40).map(|n| format!(" la{n}(fgh) ")).collect();
        let e = engraved(&format!("%%\n(c3){body}(::)"), 8.0);
        assert!(e.lines.len() > 3, "{} lines", e.lines.len());
        for (n, line) in e.lines.iter().enumerate() {
            assert_eq!(line.glyphs[0].glyph, Glyph::DoClef, "line {n}");
            // c3: the clef is on the third line.
            assert_eq!(line.glyphs[0].pitch, 7.0);
            let last = line.glyphs.last().unwrap();
            if n + 1 < e.lines.len() {
                // Every line but the last ends with a custos at the next line's first pitch.
                assert_eq!(last.glyph, Glyph::CustosLong, "line {n}");
                assert_eq!(last.pitch, 5.0);
            }
            for glyph in &line.glyphs {
                assert!(glyph.x + glyph.glyph.metrics().width <= 8.0 + 1e-3, "line {n}: {glyph:?}");
            }
        }
        // Every syllable is on some line, once.
        let syllables: usize = e.lines.iter().map(|l| l.lyrics.len()).sum();
        assert_eq!(syllables, 40);
    }

    #[test]
    fn text_is_read_with_its_styles_and_special_characters() {
        let mut styles = Styles::default();
        let read = styles.read("<i>Ps.</i> <sp>V/</sp>. a{e}$(x<v>\\tex</v>");
        let texts: Vec<(&str, bool)> = read.spans.iter().map(|s| (s.text.as_str(), s.italic)).collect();
        assert_eq!(texts, [("Ps.", true), (" \u{2123}. ae(x", false)]);
        // The part in braces, in the plain text: the `e`.
        let plain = read.plain();
        let (start, len) = read.forced.unwrap();
        assert_eq!(&plain[start..start + len], "e");
        // A code that the format's description does not name is printed as written.
        assert_eq!(styles.read("<sp>A/</sp>").spans[0].text, "A/");
        // A style lasts into the next syllable.
        styles.read("<b>one");
        assert!(styles.read("two").spans[0].bold);
    }

    #[test]
    fn the_letters_under_the_first_note() {
        fn centre(raw: &str) -> String {
            centred_part(raw).1
        }
        // Every pair is what Gregorio centres for that syllable.
        for (syllable, centred) in [
            ("Dó", "ó"), ("mi", "i"), ("nus", "u"), ("quo", "o"), ("qui", "i"), ("guis", "i"), ("ius", "u"),
            ("iam", "a"), ("ia", "a"), ("cui", "i"), ("cuí", "í"), ("uí", "í"), ("huic", "i"), ("cae", "ae"),
            ("lau", "au"), ("coe", "oe"), ("eu", "eu"), ("seu", "eu"), ("hei", "ei"), ("áu", "áu"), ("aé", "aé"),
            ("oé", "oé"), ("quae", "ae"), ("guae", "ae"), ("cœ", "œ"), ("præ", "æ"), ("ǽ", "ǽ"), ("Æ", "Æ"),
            ("y", "y"), ("Ie", "e"), ("ju", "u"), ("um", "u"), ("AL", "A"), ("Ísra", "Í"), ("qu", "u"),
            ("et.", "e"),
            // Any run of vowels, in any language.
            ("thou", "ou"), ("teous", "eou"), ("praise", "ai"), ("ya.", "ya"), ("Chúa", "úa"), ("muôn", "ô"),
            ("lê", "ê"), ("thương", "ươ"),
            // No vowel: all of it, punctuation too.
            ("str", "str"), ("*", "*"), ("Ps.", "Ps."), ("2.", "2."), ("T. P.", "T. P."),
            // A special character is no vowel, so this syllable has none ...
            ("s<sp>'ae</sp>", "sǽ"),
            // ... and here the `u` is the only one.
            ("qu<sp>ae</sp>", "u"),
            // What is written in braces is taken as it is.
            ("lú{ia}.", "ia"),
        ] {
            assert_eq!(centre(syllable), centred, "{syllable}");
        }
        assert_eq!(centre(""), "");
    }

    #[test]
    fn syllables_of_a_word_meet_or_get_a_hyphen() {
        // Wide syllables under single notes meet, and are set right together.
        let e = engraved("%%\n(c4) strens(f)strens(g)strens(h) (::)", 18.0);
        let texts: Vec<&str> = e.lines[0].lyrics.iter().map(|l| l.spans[0].text.as_str()).collect();
        assert_eq!(texts, ["strens", "strens", "strens"]);
        let lyrics = &e.lines[0].lyrics;
        let width = estimate_width("strens") * e.lyric_size;
        assert!((lyrics[1].x - lyrics[0].x - width).abs() < 1e-4);
        // Many notes on the first syllable: the next one is far away, and a hyphen says so.
        let e = engraved("%%\n(c4) Dó(fgfgfgfgfg//fgfgfgfg)mi(g)nus(h) (::)", 18.0);
        let texts: Vec<&str> = e.lines[0].lyrics.iter().map(|l| l.spans[0].text.as_str()).collect();
        assert_eq!(texts, ["Dó-", "mi-", "nus"]);
        // Between two words there is room.
        let e = engraved("%%\n(c4) et(f) in(g) (::)", 18.0);
        let (et, inn) = (&e.lines[0].lyrics[0], &e.lines[0].lyrics[1]);
        assert!(inn.x > et.x + estimate_width("et") * e.lyric_size, "{et:?} {inn:?}");
    }

    #[test]
    fn dots_strokes_and_the_custos_stand_where_exsurge_puts_them() {
        // A dot beside a note on a line (h) moves up into the space; beside one in a space (g) it stays.
        let e = engraved("%%\n(c4) A(h.) B(g.)", 18.0);
        let dots: Vec<f32> = e.lines[0].glyphs.iter().filter(|g| g.glyph == Glyph::Mora).map(|g| g.pitch).collect();
        assert_eq!(dots, [7.75, 6.0]);
        // The vertical episema under a note in a space is a step and a half below it.
        let e = engraved("%%\n(c4) A(g')", 18.0);
        let ictus = e.lines[0].glyphs.iter().find(|g| g.glyph == Glyph::VerticalEpisemaBelow).unwrap();
        assert_eq!(ictus.pitch, 4.5);
        // The horizontal episema lies in the next space above the note that is clear of it.
        let e = engraved("%%\n(c4) A(g_)", 18.0);
        let stroke = e.lines[0].rules.iter().find(|r| r.width > 0.1 && r.height < 0.5).unwrap();
        assert_eq!(stroke.top, 8.0);
        assert!((stroke.width - STEP).abs() < 1e-6);
        // Written twice, it lies over two notes.
        let e = engraved("%%\n(c4) A(gh__)", 18.0);
        assert_eq!(e.lines[0].rules.iter().filter(|r| r.width > 0.1 && r.height < 0.5).count(), 2);
        // The custos: stem up and long on a line in the lower part, short in a space, down from the top.
        assert_eq!(custos_glyph(5), Glyph::CustosLong);
        assert_eq!(custos_glyph(6), Glyph::CustosShort);
        assert_eq!(custos_glyph(9), Glyph::CustosDescLong);
        assert_eq!(custos_glyph(10), Glyph::CustosDescShort);
    }

    #[test]
    fn a_note_low_under_the_staff_pushes_the_lyrics_down() {
        let high = engraved("%%\n(c4) A(h)", 18.0);
        let low = engraved("%%\n(c4) A(a)", 18.0);
        assert!(low.lines[0].lyric_y() > high.lines[0].lyric_y());
    }
}
