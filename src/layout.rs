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

impl Default for Options<'_> {
    fn default() -> Self {
        Options { width: 18.0, lyric_size: 0.42, measure: None }
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
    /// The width of the text, in em of the score.
    text: f32,
    /// How far the text starts to the left of the notes.
    text_lead: f32,
    starts_word: bool,
    /// The first sounding pitch, for the custos of the line before.
    first_pitch: Option<u8>,
    /// A clef that takes effect here, to be repeated on later lines.
    clef: Option<Clef>,
    /// Holds only a clef: it is drawn at the start of the line, not here.
    only_clef: bool,
    /// A forced line break after this block.
    breaks: bool,
}

/// Lays a score out in lines.
pub fn engrave(score: &Score, options: &Options) -> Engraving {
    let estimate = |text: &str| estimate_width(text);
    let measure: &dyn Fn(&str) -> f32 = options.measure.unwrap_or(&estimate);
    let size = options.lyric_size;

    // Gregorio reads a score without a clef with a do clef on the third line.
    let mut clef = Clef { do_clef: true, line: 3, flat: false };
    let mut styles = Styles::default();
    let mut blocks = Vec::new();
    for syllable in &score.syllables {
        let spans = styles.read(&syllable.text);
        let plain: String = spans.iter().map(|s| s.text.as_str()).collect();
        let empty = NoteGroup::default();
        let group = syllable.notes.as_ref().unwrap_or(&empty);
        let mut block = set_group(group);
        block.starts_word = syllable.starts_word;
        block.text = measure(&plain) * size;
        // The first vowel stands under the middle of the first note.
        let lead = measure(&plain[..vowel_at(&plain)]) * size;
        let vowel = plain[vowel_at(&plain)..].chars().next().map_or(0.0, |c| measure(&c.to_string()) * size);
        block.text_lead = if block.notes > 0.0 { (lead + vowel / 2.0 - STEP / 2.0).max(0.0) } else { 0.0 };
        block.spans = spans;
        if let Some(new) = block.clef {
            clef = new;
        }
        block.clef = Some(clef);
        blocks.push(block);
    }

    let clef_width = Glyph::DoClef.metrics().width + 3.0 * NEUME_SPACE;
    let custos_room = Glyph::CustosLong.metrics().width + 2.0 * NEUME_SPACE;
    let word_gap = measure(" ").max(0.2) * size;
    let syllable_gap = 3.0 * NEUME_SPACE;

    let mut engraving = Engraving { lines: Vec::new(), width: options.width, lyric_size: size };
    let mut line = Line::default();
    let mut notes_end = clef_width;
    let mut text_end = clef_width;
    let mut line_clef = blocks.first().and_then(|b| b.clef).unwrap_or(clef);
    let mut on_line: Vec<(usize, f32)> = Vec::new();

    let finish = |line: &mut Line, on_line: &mut Vec<(usize, f32)>, blocks: &[Block], clef: Clef, custos: Option<u8>, justify: bool, engraving: &mut Engraving| {
        // Spread the room that is left over the gaps between the syllables.
        let used = on_line.last().map_or(0.0, |(k, x)| x + blocks[*k].notes.max(blocks[*k].text - blocks[*k].text_lead));
        let room = options.width - used - if custos.is_some() { custos_room } else { 0.0 };
        let gaps = on_line.len().saturating_sub(1).max(1) as f32;
        let extra = if justify && room > 0.0 && room < options.width * 0.4 { room / gaps } else { 0.0 };
        line.highest = TOP_LINE;
        line.lowest = BOTTOM_LINE;
        place_clef(line, clef);
        for (n, (k, x)) in on_line.iter().enumerate() {
            let block = &blocks[*k];
            let x = x + extra * n as f32;
            for glyph in &block.glyphs {
                line.glyphs.push(PlacedGlyph { x: glyph.x + x, ..*glyph });
                line.highest = line.highest.max(glyph.pitch + 1.0);
                line.lowest = line.lowest.min(glyph.pitch - 1.0);
            }
            for rule in &block.rules {
                line.rules.push(Rule { x: rule.x + x, ..*rule });
                line.highest = line.highest.max(rule.top);
                line.lowest = line.lowest.min(rule.top - rule.height);
            }
            if !block.spans.is_empty() {
                line.lyrics.push(Lyric { x: x - block.text_lead, row: 0, spans: block.spans.clone() });
            }
        }
        if let Some(pitch) = custos {
            // The stem points away from the middle of the staff.
            let glyph = if f32::from(pitch) < 6.0 { Glyph::CustosLong } else { Glyph::CustosDescLong };
            line.glyphs.push(PlacedGlyph { glyph, x: options.width - glyph.metrics().width, pitch: f32::from(pitch) });
            line.highest = line.highest.max(f32::from(pitch) + 2.5);
            line.lowest = line.lowest.min(f32::from(pitch) - 2.5);
        }
        engraving.lines.push(std::mem::take(line));
        on_line.clear();
    };

    for (k, block) in blocks.iter().enumerate() {
        if block.only_clef {
            // At the start of a line the clef is drawn by the line itself.
            if on_line.is_empty() {
                line_clef = block.clef.unwrap_or(line_clef);
                continue;
            }
        }
        let gap = if on_line.is_empty() {
            0.0
        } else if block.starts_word {
            word_gap
        } else {
            0.0
        };
        let mut x = (notes_end).max(text_end + gap + block.text_lead);
        let right = x + block.notes.max(block.text - block.text_lead);
        if right > options.width - custos_room && !on_line.is_empty() {
            let custos = blocks[k..].iter().find_map(|b| b.first_pitch);
            finish(&mut line, &mut on_line, &blocks, line_clef, custos, true, &mut engraving);
            line_clef = block.clef.unwrap_or(line_clef);
            notes_end = clef_width;
            text_end = clef_width;
            x = notes_end.max(text_end + block.text_lead);
        }
        on_line.push((k, x));
        notes_end = x + block.notes + if block.notes > 0.0 { syllable_gap } else { 0.0 };
        text_end = x - block.text_lead + block.text;
        if block.breaks {
            let custos = blocks[k + 1..].iter().find_map(|b| b.first_pitch);
            finish(&mut line, &mut on_line, &blocks, line_clef, custos, false, &mut engraving);
            line_clef = blocks.get(k + 1).and_then(|b| b.clef).unwrap_or(line_clef);
            notes_end = clef_width;
            text_end = clef_width;
        }
    }
    if !on_line.is_empty() {
        finish(&mut line, &mut on_line, &blocks, line_clef, None, false, &mut engraving);
    }
    engraving
}

fn place_clef(line: &mut Line, clef: Clef) {
    // Line 1 is the bottom line.
    let pitch = BOTTOM_LINE + 2.0 * (f32::from(clef.line) - 1.0);
    let glyph = if clef.do_clef { Glyph::DoClef } else { Glyph::FaClef };
    let mut x = 0.0;
    if clef.flat {
        // The flat of a flat clef stands in the space below the clef's line.
        line.glyphs.push(PlacedGlyph { glyph: Glyph::Flat, x, pitch: pitch - 1.0 });
        x += Glyph::Flat.metrics().width + NEUME_SPACE / 2.0;
    }
    line.glyphs.push(PlacedGlyph { glyph, x, pitch });
    line.highest = line.highest.max(pitch + 2.0);
}

/// Sets what stands between one pair of parentheses.
fn set_group(group: &NoteGroup) -> Block {
    let mut block = Block {
        glyphs: Vec::new(),
        rules: Vec::new(),
        spans: Vec::new(),
        notes: 0.0,
        text: 0.0,
        text_lead: 0.0,
        starts_word: false,
        first_pitch: None,
        clef: None,
        only_clef: !group.elements.is_empty(),
        breaks: false,
    };
    let mut x = 0.0f32;
    // The notes that stand together, waiting to become neumes.
    let mut run: Vec<Head> = Vec::new();
    let mut pending_space = 0.0f32;

    fn flush(run: &mut Vec<Head>, group: &NoteGroup, block: &mut Block, x: &mut f32) {
        if run.is_empty() {
            return;
        }
        let found = neume::neumes(run);
        for (n, neume) in found.iter().enumerate() {
            if n > 0 {
                *x += NEUME_SPACE;
            }
            let set = neume::set(neume, run);
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
            // The signs of each note, by the glyph that stands for it.
            for (k, head) in run[neume.heads.clone()].iter().enumerate() {
                let index = neume.heads.start + k;
                // A doubled shape carries its signs on its last head.
                if run.get(index + 1).is_some_and(|next| next.note == head.note) {
                    continue;
                }
                let ElementKind::Note(note) = &group.elements[head.note].kind else { continue };
                let Some(placed) = set.glyphs.iter().rev().find(|g| g.head == index).or(set.glyphs.last()) else { continue };
                let metrics = placed.glyph.metrics();
                let (left, right) = (placed.x + *x, placed.x + *x + metrics.width);
                let pitch = f32::from(head.pitch);
                if note.mora > 0 {
                    // A dot beside a note on a line stands in the space above it.
                    let at = if head.pitch % 2 == 1 { pitch + 0.9 } else { pitch };
                    for dot in 0..note.mora {
                        block.glyphs.push(PlacedGlyph {
                            glyph: Glyph::Mora,
                            x: right + 0.02 + f32::from(dot) * 0.09,
                            pitch: at,
                        });
                    }
                    *x += 0.09 * f32::from(note.mora);
                }
                if note.ictus {
                    block.glyphs.push(PlacedGlyph {
                        glyph: Glyph::VerticalEpisemaBelow,
                        x: (left + right) / 2.0 - Glyph::VerticalEpisemaBelow.metrics().width / 2.0,
                        pitch: pitch - 1.1,
                    });
                }
                if note.episema > 0 {
                    block.rules.push(Rule { x: left, top: pitch + 1.6, width: right - left, height: LINE_WEIGHT * 1.5 / STEP });
                }
            }
            *x += set.width;
        }
        run.clear();
    }

    for (index, element) in group.elements.iter().enumerate() {
        if !matches!(element.kind, ElementKind::Clef(_) | ElementKind::SecondaryClef(_)) {
            block.only_clef = false;
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
                    flush(&mut run, group, &mut block, &mut x);
                }
            }
            ElementKind::Space(space) => {
                flush(&mut run, group, &mut block, &mut x);
                pending_space = match space {
                    Space::Small | Space::SmallJoined | Space::SmallNoBreak | Space::Half => NEUME_SPACE,
                    Space::Medium | Space::Large | Space::Sized => 2.0 * NEUME_SPACE,
                };
            }
            ElementKind::Accidental { pitch, kind, .. } => {
                flush(&mut run, group, &mut block, &mut x);
                x += pending_space;
                pending_space = NEUME_SPACE;
                // The font has no sharp: a natural stands in for it for now.
                let glyph = if *kind == AccidentalKind::Flat { Glyph::Flat } else { Glyph::Natural };
                block.glyphs.push(PlacedGlyph { glyph, x, pitch: f32::from(*pitch) });
                x += glyph.metrics().width;
            }
            ElementKind::Bar(bar) => {
                flush(&mut run, group, &mut block, &mut x);
                x += pending_space;
                if !block.glyphs.is_empty() || !block.rules.is_empty() {
                    x += 2.0 * NEUME_SPACE;
                }
                let weight = LINE_WEIGHT * 1.3;
                let mut rule = |x: f32, top: f32, bottom: f32| {
                    block.rules.push(Rule { x, top, width: weight, height: top - bottom });
                };
                match bar {
                    Bar::Virgula { high } => {
                        let pitch = if *high { TOP_LINE + 2.0 } else { TOP_LINE };
                        block.glyphs.push(PlacedGlyph { glyph: Glyph::Virgula, x, pitch });
                        x += Glyph::Virgula.metrics().width;
                    }
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
                        x += 0.07;
                        rule(x, TOP_LINE, BOTTOM_LINE);
                    }
                }
                x += weight;
                pending_space = 2.0 * NEUME_SPACE;
            }
            ElementKind::Custos(pitch) => {
                flush(&mut run, group, &mut block, &mut x);
                if let Some(pitch) = pitch {
                    x += pending_space + NEUME_SPACE;
                    pending_space = 0.0;
                    let glyph = if *pitch < 6 { Glyph::CustosLong } else { Glyph::CustosDescLong };
                    block.glyphs.push(PlacedGlyph { glyph, x, pitch: f32::from(*pitch) });
                    x += glyph.metrics().width;
                }
            }
            ElementKind::Clef(clef) => {
                flush(&mut run, group, &mut block, &mut x);
                block.clef = Some(*clef);
            }
            ElementKind::LineBreak { .. } => {
                flush(&mut run, group, &mut block, &mut x);
                block.breaks = true;
            }
            ElementKind::SecondaryClef(_)
            | ElementKind::VoiceStart
            | ElementKind::VoiceEnd
            | ElementKind::NoBreak { .. } => {}
        }
    }
    flush(&mut run, group, &mut block, &mut x);
    block.notes = x;
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
    /// Reads a syllable's text as written into spans of plain text.
    fn read(&mut self, raw: &str) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        let mut text = String::new();
        let mut i = 0;
        let flush = |text: &mut String, spans: &mut Vec<Span>, s: &Styles| {
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
                            text.push_str(special(&rest[end + 1..inner_end]));
                        }
                        i += (inner_end + close.len()).min(rest.len());
                        continue;
                    }
                    let known = matches!(name, "i" | "b" | "sc" | "c" | "ul" | "tt" | "e" | "eu" | "nlba" | "pr" | "clear");
                    if known {
                        flush(&mut text, &mut spans, self);
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
            // The braces that mark the vowel to centre on are not printed.
            if c != '{' && c != '}' {
                text.push(c);
            }
            i += c.len_utf8();
        }
        flush(&mut text, &mut spans, self);
        spans
    }
}

/// The character that a `<sp>` code stands for.
fn special(code: &str) -> &str {
    match code {
        "V/" => "\u{2123}",
        "R/" => "\u{211F}",
        "A/" => "\u{2C6F}",
        "ae" => "æ",
        "'ae" | "'æ" => "ǽ",
        "oe" => "œ",
        "'oe" | "'œ" => "œ\u{301}",
        "AE" => "Æ",
        "OE" => "Œ",
        "+" => "\u{2020}",
        "*" => "*",
        other => other,
    }
}

/// Where the first vowel of a piece of text starts, or 0 if it has none.
fn vowel_at(text: &str) -> usize {
    const VOWELS: &str = "aeiouyáéíóúýàèìòùäëïöüæœǽAEIOUYÁÉÍÓÚÝÄËÏÖÜÆŒ";
    text.char_indices().find(|(_, c)| VOWELS.contains(*c)).map_or(0, |(i, _)| i)
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
        let texts: Vec<String> = line.lyrics.iter().map(|l| l.spans[0].text.clone()).collect();
        assert_eq!(texts, ["KY", "ri", "e"]);
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
        let spans = styles.read("<i>Ps.</i> <sp>V/</sp>. a{e}$(x<v>\\tex</v>");
        let texts: Vec<(&str, bool)> = spans.iter().map(|s| (s.text.as_str(), s.italic)).collect();
        assert_eq!(texts, [("Ps.", true), (" \u{2123}. ae(x", false)]);
        // A style lasts into the next syllable.
        styles.read("<b>one");
        assert!(styles.read("two")[0].bold);
    }

    #[test]
    fn a_note_low_under_the_staff_pushes_the_lyrics_down() {
        let high = engraved("%%\n(c4) A(h)", 18.0);
        let low = engraved("%%\n(c4) A(a)", 18.0);
        assert!(low.lines[0].lyric_y() > high.lines[0].lyric_y());
    }
}
