//! From a parsed hymn (ABC) to lines of placed things.
//!
//! The staff has five lines with a treble clef. Every line starts with the clef and the key
//! signature, the first one also with the time signature. Notes are spaced by their length
//! and pushed apart where the text under them needs more room; the bars are filled into
//! lines of the given width, and each line is spread to that width. The verses stand in rows
//! under the staff, each syllable centred under its note.
//!
//! Heights are staff steps above middle C: the lines of the staff are 2, 4, 6, 8 and 10
//! (E G B d f). Lengths are in em, one em being four staff spaces, as in [`crate::layout`].
//!
//! Where the numbers come from. Weights and attachment points are the music font's own
//! ([`crate::music_glyphs::engraving`], the anchors of the note heads and flags). The places
//! of the key signature's sharps and flats and of the numbers of the time signature, the
//! direction of stems and the height of rests are the common rules of notation. What neither
//! gives is a choice of this crate and can be told by its plain numbers in the code: how much
//! room a note gets for its length ([`room`]), the gaps beside accidentals, dots, clef and
//! bars, how high a bow arches, where the breath mark and the fermata sit, and the room
//! between words.
//!
//! Short notes written without a space between them share a beam, which is drawn level.
//! Ties and slurs are bows from note to note; one that would cross a line end is left out.
//!
//! Not drawn yet: the bracket of a triplet, the numbers of first and second endings.

use crate::abc::{Bar, Decoration, ElementKind, Fraction, Key, Metre, Note, SlotKind, Tune};
use crate::layout::{Curve, Engraving, Line, Lyric, Options, PlacedMusic, Rule, Span};
use crate::music_glyphs::{engraving, MusicGlyph};
use crate::neume::STEP;

const STAFF: [f32; 5] = [2.0, 4.0, 6.0, 8.0, 10.0];
const TOP: f32 = 10.0;
const BOTTOM: f32 = 2.0;
const MIDDLE: f32 = 6.0;
/// The weight of a stem, from the font's own measures like the other weights used here.
const STEM: f32 = engraving::STEM;
/// A stem is three and a half staff spaces long: an octave, by the convention of engraving.
/// (The font's metadata gives weights and attachment points, not lengths.)
const STEM_LENGTH: f32 = 7.0;

/// One thing in the row of music with what belongs to it, placed from its own left edge.
#[derive(Default)]
struct Atom {
    /// Where it starts in its bar, before the line is spread.
    x: f32,
    /// How much room it takes.
    width: f32,
    music: Vec<PlacedMusic>,
    rules: Vec<Rule>,
    /// Text under it: the row, the text, and the middle of the note it belongs to.
    lyrics: Vec<(usize, String, f32)>,
    /// Verse numbers: the row and the label, which ends where the row's text starts.
    labels: Vec<(usize, String)>,
    /// A bar line: a line may end after it.
    ends_bar: bool,
    /// A note: where its head is and what hangs on it.
    note: Option<NoteParts>,
}

/// What of a note is decided only when its neighbours are known.
struct NoteParts {
    pitch: f32,
    /// The left and right edge of the head, from the atom's left edge.
    head: (f32, f32),
    /// Where a stem that points up, and one that points down, starts: its left edge from the
    /// atom's left edge, and the pitch at which it leaves the head.
    stem_up: (f32, f32),
    stem_down: (f32, f32),
    /// Its own stem and flag, drawn unless the note is under a beam.
    stem: Vec<Rule>,
    flag: Vec<PlacedMusic>,
    /// How many flags it has: 1 for an eighth, 2 for a sixteenth.
    flags: u8,
    /// The beam group it belongs to.
    beam: Option<usize>,
    tie: bool,
    slur_starts: u8,
    slur_ends: u8,
}

/// Lays a hymn out in lines.
pub fn engrave(tune: &Tune, options: &Options) -> Engraving {
    let estimate = |text: &str| crate::layout::estimate_width(text);
    let measure: &dyn Fn(&str) -> f32 = options.measure.unwrap_or(&estimate);
    let size = options.lyric_size;

    // The atoms of the whole tune, in bars.
    let mut bars: Vec<Vec<Atom>> = vec![Vec::new()];
    // The verse number each row showed last: it is printed where it first stands, not again
    // on every line of music.
    let mut shown: Vec<String> = Vec::new();
    // Beams: notes with flags that follow each other with no space between them.
    let mut groups = 0usize;
    let mut run: Vec<(usize, usize)> = Vec::new();
    fn close_run(run: &mut Vec<(usize, usize)>, bars: &mut [Vec<Atom>], groups: &mut usize) {
        if run.len() >= 2 {
            for (bar, atom) in run.iter() {
                if let Some(note) = bars[*bar][*atom].note.as_mut() {
                    note.beam = Some(*groups);
                }
            }
            *groups += 1;
        }
        run.clear();
    }
    let mut slurs_opening = 0u8;
    for line in &tune.lines {
        let mut note_index = 0usize;
        let mut pending: Vec<Decoration> = Vec::new();
        for element in &line.elements {
            if !matches!(element.kind, ElementKind::Note(_) | ElementKind::SlurStart | ElementKind::SlurEnd) {
                close_run(&mut run, &mut bars, &mut groups);
            }
            match &element.kind {
                ElementKind::Note(note) => {
                    let mut atom = note_atom(note, &pending);
                    pending.clear();
                    if let Some(parts) = atom.note.as_mut() {
                        parts.slur_starts = std::mem::take(&mut slurs_opening);
                        if parts.flags == 0 {
                            close_run(&mut run, &mut bars, &mut groups);
                        } else {
                            run.push((bars.len() - 1, bars.last().map_or(0, Vec::len)));
                        }
                    }
                    let centre = MusicGlyph::NoteheadBlack.metrics().advance / 2.0 + atom_lead(note);
                    for (row, lyrics) in line.lyrics.iter().enumerate() {
                        if let Some(slot) = lyrics.slots.get(note_index) {
                            if let SlotKind::Syllable { text, hyphen } = &slot.kind {
                                let (label, text) = split_label(text);
                                if let Some(label) = label {
                                    if shown.len() <= row {
                                        shown.resize(row + 1, String::new());
                                    }
                                    if shown[row] != label {
                                        shown[row] = label.to_string();
                                        atom.labels.push((row, label.to_string()));
                                    }
                                }
                                let text = if *hyphen { format!("{text}-") } else { text.to_string() };
                                atom.lyrics.push((row, text, centre));
                            }
                        }
                    }
                    note_index += 1;
                    push(&mut bars, atom);
                }
                ElementKind::Rest(length) => push(&mut bars, rest_atom(*length)),
                ElementKind::Bar(bar) => {
                    push(&mut bars, bar_atom(*bar));
                    bars.push(Vec::new());
                }
                ElementKind::Decoration(decoration) => {
                    if *decoration == Decoration::Breath {
                        // A breath mark stands after the note before it, above the staff.
                        let mut atom = Atom { width: 0.12, ..Atom::default() };
                        atom.music.push(PlacedMusic { glyph: MusicGlyph::BreathMarkComma, x: 0.0, pitch: TOP + 1.0 });
                        push(&mut bars, atom);
                    } else {
                        pending.push(*decoration);
                    }
                }
                ElementKind::Metre(metre) => push(&mut bars, metre_atom(*metre)),
                ElementKind::Key(new) => push(&mut bars, key_atom(*new)),
                ElementKind::SlurStart => slurs_opening += 1,
                ElementKind::SlurEnd => {
                    // It ends on the note before it.
                    let last = bars.iter_mut().rev().flat_map(|bar| bar.iter_mut().rev()).find_map(|a| a.note.as_mut());
                    if let Some(note) = last {
                        note.slur_ends += 1;
                    }
                }
                ElementKind::Ending(_) | ElementKind::Triplet | ElementKind::Space => {}
            }
        }
        close_run(&mut run, &mut bars, &mut groups);
    }
    bars.retain(|bar| !bar.is_empty());

    // Inside a bar: each atom after the one before, and far enough that no text runs into
    // the text of the note before it in its row.
    // Between two words there is a space and a little more, since widths may be estimates;
    // after a hyphen the next syllable may come close.
    let word_gap = measure(" ").max(0.25) * size * 1.5;
    let syllable_gap = 0.12 * size;
    let gap = word_gap;
    let mut widths = Vec::new();
    // Whether the text of each row ended with a hyphen: it carries over the bar line.
    let mut hyphen: Vec<bool> = Vec::new();
    for bar in &mut bars {
        let mut x = 0.0f32;
        let mut text_end: Vec<f32> = Vec::new();
        for atom in bar.iter_mut() {
            let mut start = x;
            for (row, text, centre) in &atom.lyrics {
                let label = atom.labels.iter().find(|(r, _)| r == row).map_or(0.0, |(_, l)| label_width(l, measure, size));
                let half = measure(text) * size / 2.0;
                let end = text_end.get(*row).copied().unwrap_or(f32::MIN);
                let gap = if hyphen.get(*row).copied().unwrap_or(false) { syllable_gap } else { word_gap };
                start = start.max(end + gap + label + half - centre);
            }
            atom.x = start;
            for (row, text, centre) in &atom.lyrics {
                let half = measure(text) * size / 2.0;
                if text_end.len() <= *row {
                    text_end.resize(*row + 1, f32::MIN);
                }
                if hyphen.len() <= *row {
                    hyphen.resize(*row + 1, false);
                }
                text_end[*row] = start + centre + half;
                hyphen[*row] = text.ends_with('-');
            }
            x = start + atom.width;
        }
        let text = text_end.iter().copied().fold(0.0, f32::max);
        widths.push(x.max(text + gap));
    }

    // Bars into lines.
    let mut engraving = Engraving { lines: Vec::new(), width: options.width, lyric_size: size };
    let mut first = 0usize;
    let mut on_first_line = true;
    while first < bars.len() {
        let prefix = prefix_atoms(tune.key, on_first_line.then_some(tune.metre));
        let prefix_width: f32 = prefix.iter().map(|a| a.width).sum();
        let room = options.width - prefix_width;
        let mut end = first;
        let mut natural = 0.0f32;
        while end < bars.len() && (end == first || natural + widths[end] <= room) {
            natural += widths[end];
            end += 1;
        }
        let last = end == bars.len();
        // Spread the bars over the line. The last line is only spread if it is nearly full.
        let stretch = if natural > 0.0 && (!last || natural > room * 0.8) { (room / natural).min(1.8) } else { 1.0 };
        let mut line = Line {
            staff: STAFF.to_vec(),
            staff_weight: engraving::STAFF_LINE,
            highest: TOP,
            lowest: BOTTOM,
            ..Line::default()
        };
        let mut x = 0.0f32;
        for atom in &prefix {
            place(&mut line, atom, x, measure, size);
            x += atom.width;
        }
        let mut bar_start = 0.0f32;
        // What spans notes: the stems under each beam, an open tie, the open slurs.
        let mut beams: Vec<(usize, Vec<Beamed>)> = Vec::new();
        let mut tie: Option<(f32, f32)> = None;
        let mut slurs: Vec<(f32, f32)> = Vec::new();
        for (bar, width) in bars[first..end].iter().zip(&widths[first..end]) {
            for atom in bar {
                // A bar line stands at the end of its bar, however far the bar is spread.
                let at = if atom.ends_bar { (bar_start + width) * stretch - atom.width } else { (bar_start + atom.x) * stretch };
                let x = prefix_width + at;
                place(&mut line, atom, x, measure, size);
                let Some(note) = &atom.note else { continue };
                let (left, right) = (x + note.head.0, x + note.head.1);
                // A bow lies on the side away from the stem.
                let above = note.pitch >= MIDDLE;
                let side = if above { 1.2 } else { -1.2 };
                if let Some((from, pitch)) = tie.take() {
                    if pitch == note.pitch {
                        line.curves.push(Curve {
                            x0: from,
                            pitch0: pitch + side,
                            x1: left - 0.02,
                            pitch1: pitch + side,
                            above,
                            thickness: engraving::TIE_MIDDLE,
                        });
                    }
                }
                if note.tie {
                    tie = Some((right + 0.02, note.pitch));
                }
                for _ in 0..note.slur_ends {
                    if let Some((from, pitch)) = slurs.pop() {
                        let centre = (left + right) / 2.0;
                        let above = (pitch + note.pitch) / 2.0 >= MIDDLE;
                        let side = if above { 1.4 } else { -1.4 };
                        line.curves.push(Curve {
                            x0: from,
                            pitch0: pitch + side,
                            x1: centre,
                            pitch1: note.pitch + side,
                            above,
                            thickness: engraving::TIE_MIDDLE,
                        });
                    }
                }
                for _ in 0..note.slur_starts {
                    slurs.push(((left + right) / 2.0, note.pitch));
                }
                match note.beam {
                    Some(group) => {
                        let stem = Beamed {
                            up: (x + note.stem_up.0, note.stem_up.1),
                            down: (x + note.stem_down.0, note.stem_down.1),
                            pitch: note.pitch,
                            flags: note.flags,
                        };
                        match beams.last_mut().filter(|(g, _)| *g == group) {
                            Some((_, stems)) => stems.push(stem),
                            None => beams.push((group, vec![stem])),
                        }
                    }
                    None => {
                        for rule in &note.stem {
                            line.rules.push(Rule { x: rule.x + x, ..*rule });
                            line.highest = line.highest.max(rule.top);
                            line.lowest = line.lowest.min(rule.top - rule.height);
                        }
                        for flag in &note.flag {
                            line.music.push(PlacedMusic { x: flag.x + x, ..*flag });
                        }
                    }
                }
            }
            bar_start += width;
        }
        for (_, stems) in &beams {
            beam(&mut line, stems);
        }
        for curve in &line.curves {
            line.highest = line.highest.max(curve.pitch0.max(curve.pitch1) + if curve.above { 2.0 } else { 0.0 });
            line.lowest = line.lowest.min(curve.pitch0.min(curve.pitch1) - if curve.above { 0.0 } else { 2.0 });
        }
        engraving.lines.push(line);
        first = end;
        on_first_line = false;
    }
    engraving
}

/// A note under a beam: where its stem starts if it points up or down (its left edge and the
/// pitch at which it leaves the head), and the note's pitch.
struct Beamed {
    up: (f32, f32),
    down: (f32, f32),
    pitch: f32,
    flags: u8,
}

/// Draws the stems of a group of notes and the level beam that joins them.
fn beam(line: &mut Line, stems: &[Beamed]) {
    let (Some(first), Some(last)) = (stems.first(), stems.last()) else { return };
    let mean = stems.iter().map(|s| s.pitch).sum::<f32>() / stems.len() as f32;
    let up = mean < MIDDLE;
    let thick = engraving::BEAM / STEP;
    let highest = stems.iter().map(|s| s.pitch).fold(f32::MIN, f32::max);
    let lowest = stems.iter().map(|s| s.pitch).fold(f32::MAX, f32::min);
    // The far edge of the beam, a stem's length from the note nearest to it.
    let edge = if up { highest + STEM_LENGTH } else { lowest - STEM_LENGTH };
    for stem in stems {
        let rule = if up {
            Rule { x: stem.up.0, top: edge, width: STEM, height: edge - stem.up.1 }
        } else {
            Rule { x: stem.down.0, top: stem.down.1, width: STEM, height: stem.down.1 - edge }
        };
        line.rules.push(rule);
    }
    let x_of = |s: &Beamed| if up { s.up.0 } else { s.down.0 };
    // From one beam to the next: its thickness and the room between two.
    let step = (engraving::BEAM + engraving::BEAM_SPACING) / STEP;
    let mut bar = |from: &Beamed, to: &Beamed, level: f32| {
        let top = if up { edge - level * step } else { edge + thick + level * step };
        line.rules.push(Rule { x: x_of(from), top, width: x_of(to) + STEM - x_of(from), height: thick });
    };
    bar(first, last, 0.0);
    // A second beam over each stretch of sixteenths.
    let mut start: Option<usize> = None;
    for (k, stem) in stems.iter().enumerate() {
        let short = stem.flags >= 2;
        if short && start.is_none() {
            start = Some(k);
        }
        let ends = !short || k + 1 == stems.len();
        if let Some(from) = start.filter(|_| ends) {
            let to = if short { k } else { k - 1 };
            if to > from {
                bar(&stems[from], &stems[to], 1.0);
            }
            start = None;
        }
    }
    line.highest = line.highest.max(edge.max(highest) + 1.0);
    line.lowest = line.lowest.min(edge.min(lowest) - 1.0);
}

fn push(bars: &mut Vec<Vec<Atom>>, atom: Atom) {
    if let Some(bar) = bars.last_mut() {
        bar.push(atom);
    }
}

/// Puts an atom's parts into the line at `x`.
fn place(line: &mut Line, atom: &Atom, x: f32, measure: &dyn Fn(&str) -> f32, size: f32) {
    for glyph in &atom.music {
        let metrics = glyph.glyph.metrics();
        line.music.push(PlacedMusic { x: glyph.x + x, ..*glyph });
        line.highest = line.highest.max(glyph.pitch + metrics.above / STEP);
        line.lowest = line.lowest.min(glyph.pitch - metrics.below / STEP);
    }
    for rule in &atom.rules {
        line.rules.push(Rule { x: rule.x + x, ..*rule });
        line.highest = line.highest.max(rule.top);
        line.lowest = line.lowest.min(rule.top - rule.height);
    }
    for (row, text, centre) in &atom.lyrics {
        let width = measure(text) * size;
        let left = x + centre - width / 2.0;
        if let Some((_, label)) = atom.labels.iter().find(|(r, _)| r == row) {
            line.lyrics.push(Lyric {
                x: (left - label_width(label, measure, size)).max(0.0),
                row: *row,
                spans: vec![Span { text: label.clone(), ..Span::default() }],
            });
        }
        line.lyrics.push(Lyric { x: left, row: *row, spans: vec![Span { text: text.clone(), ..Span::default() }] });
    }
}

/// Splits a verse number off a syllable: `1.\u{a0}Lo` is the label `1.` and the text `Lo`.
/// A label starts with a digit and ends at the last no-break space.
fn split_label(text: &str) -> (Option<&str>, &str) {
    match text.rfind('\u{a0}') {
        Some(at) if text.starts_with(|c: char| c.is_ascii_digit()) => (Some(&text[..at]), &text[at + '\u{a0}'.len_utf8()..]),
        _ => (None, text),
    }
}

/// The room a verse number takes before its syllable.
fn label_width(label: &str, measure: &dyn Fn(&str) -> f32, size: f32) -> f32 {
    (measure(label) + measure(" ")) * size
}

/// How far the note head stands from the left edge of its atom: the room of an accidental.
fn atom_lead(note: &Note) -> f32 {
    match note.accidental {
        Some(accidental) => accidental_glyph(accidental).metrics().advance + 0.03,
        None => 0.0,
    }
}

fn accidental_glyph(semitones: i8) -> MusicGlyph {
    match semitones {
        n if n < 0 => MusicGlyph::AccidentalFlat,
        0 => MusicGlyph::AccidentalNatural,
        _ => MusicGlyph::AccidentalSharp,
    }
}

/// The note value of a length: the head, whether it has a stem, how many flags and dots.
fn value(length: Fraction) -> (MusicGlyph, bool, u8, u8) {
    // A dotted note is three halves of a plain one, a double-dotted one seven quarters.
    let (plain, dots) = match length.num {
        3 => (Fraction::new(2, length.den), 1),
        7 => (Fraction::new(4, length.den), 2),
        _ => (length, 0),
    };
    let (head, stem, flags) = match (plain.num, plain.den) {
        (n, 1) if n >= 2 => (MusicGlyph::NoteheadDoubleWhole, false, 0),
        (1, 1) => (MusicGlyph::NoteheadWhole, false, 0),
        (1, 2) => (MusicGlyph::NoteheadHalf, true, 0),
        (1, 4) => (MusicGlyph::NoteheadBlack, true, 0),
        (1, 8) => (MusicGlyph::NoteheadBlack, true, 1),
        _ => (MusicGlyph::NoteheadBlack, true, 2),
    };
    (head, stem, flags, dots)
}

/// Where the stem of a note head starts: its left edge, from the atom's left edge, and the
/// pitch at which it leaves the head. The font names the corner of the head for each direction.
fn stem_start(head: MusicGlyph, lead: f32, pitch: f32, up: bool) -> (f32, f32) {
    if up {
        let (ax, ay) = head.stem_up().unwrap_or((head.metrics().advance, 0.0));
        (lead + ax - STEM, pitch + ay / STEP)
    } else {
        let (ax, ay) = head.stem_down().unwrap_or((0.0, 0.0));
        (lead + ax, pitch + ay / STEP)
    }
}

/// The room a note or rest of this length takes: longer ones more, but not in proportion.
fn room(length: Fraction) -> f32 {
    let quarters = 4.0 * length.num as f32 / length.den as f32;
    0.32 + 0.34 * quarters.sqrt()
}

fn note_atom(note: &Note, decorations: &[Decoration]) -> Atom {
    let mut atom = Atom::default();
    let pitch = f32::from(note.step);
    let (head, has_stem, flags, dots) = value(note.length);
    let lead = atom_lead(note);
    if let Some(accidental) = note.accidental {
        atom.music.push(PlacedMusic { glyph: accidental_glyph(accidental), x: 0.0, pitch });
    }
    let head_width = head.metrics().advance;
    atom.music.push(PlacedMusic { glyph: head, x: lead, pitch });

    // Ledger lines, below the staff and above it.
    let mut ledger = |at: f32| {
        let weight = engraving::LEDGER_LINE / STEP;
        atom.rules.push(Rule {
            x: lead - engraving::LEDGER_EXTENSION,
            top: at + weight / 2.0,
            width: head_width + 2.0 * engraving::LEDGER_EXTENSION,
            height: weight,
        });
    };
    let mut at = BOTTOM - 2.0;
    while at >= pitch {
        ledger(at);
        at -= 2.0;
    }
    let mut at = TOP + 2.0;
    while at <= pitch {
        ledger(at);
        at += 2.0;
    }

    let mut parts = NoteParts {
        pitch,
        head: (lead, lead + head_width),
        stem_up: (0.0, pitch),
        stem_down: (0.0, pitch),
        stem: Vec::new(),
        flag: Vec::new(),
        flags: if has_stem { flags } else { 0 },
        beam: None,
        tie: note.tie,
        slur_starts: 0,
        slur_ends: 0,
    };
    if has_stem {
        // Up for notes below the middle line, down from there on.
        let up = pitch < MIDDLE;
        let (x, from) = stem_start(head, lead, pitch, up);
        let (end, glyphs) = if up {
            (pitch + STEM_LENGTH, [MusicGlyph::Flag8thUp, MusicGlyph::Flag16thUp])
        } else {
            (pitch - STEM_LENGTH, [MusicGlyph::Flag8thDown, MusicGlyph::Flag16thDown])
        };
        parts.stem.push(Rule { x, top: from.max(end), width: STEM, height: (from - end).abs() });
        if flags > 0 {
            // The flag hangs on the end of the stem by the point the font names for it.
            let glyph = glyphs[usize::from(flags > 1)];
            let (ax, ay) = glyph.on_stem().unwrap_or((0.0, 0.0));
            parts.flag.push(PlacedMusic { glyph, x: x - ax, pitch: end - ay / STEP });
        }
        parts.stem_up = stem_start(head, lead, pitch, true);
        parts.stem_down = stem_start(head, lead, pitch, false);
    }
    atom.note = Some(parts);
    // A dot beside a note on a line stands in the space above it.
    let dot_pitch = if note.step.rem_euclid(2) == 0 { pitch + 1.0 } else { pitch };
    for dot in 0..dots {
        atom.music.push(PlacedMusic {
            glyph: MusicGlyph::AugmentationDot,
            x: lead + head_width + 0.05 + f32::from(dot) * 0.14,
            pitch: dot_pitch,
        });
    }
    if decorations.contains(&Decoration::Fermata) {
        let width = MusicGlyph::FermataAbove.metrics().advance;
        atom.music.push(PlacedMusic {
            glyph: MusicGlyph::FermataAbove,
            x: lead + head_width / 2.0 - width / 2.0,
            pitch: (pitch + STEM_LENGTH + 1.0).max(TOP + 1.5),
        });
    }
    atom.width = lead + room(note.length) + f32::from(dots) * 0.1;
    atom
}

fn rest_atom(length: Fraction) -> Atom {
    let (plain, dots): (Fraction, u8) = match length.num {
        3 => (Fraction::new(2, length.den), 1),
        7 => (Fraction::new(4, length.den), 2),
        _ => (length, 0),
    };
    // A whole rest hangs from the fourth line, a half rest sits on the middle one.
    let (glyph, pitch) = match (plain.num, plain.den) {
        (n, 1) if n >= 2 => (MusicGlyph::RestDoubleWhole, MIDDLE),
        (1, 1) => (MusicGlyph::RestWhole, 8.0),
        (1, 2) => (MusicGlyph::RestHalf, MIDDLE),
        (1, 4) => (MusicGlyph::RestQuarter, MIDDLE),
        (1, 8) => (MusicGlyph::Rest8th, MIDDLE),
        _ => (MusicGlyph::Rest16th, MIDDLE),
    };
    let mut atom = Atom { width: room(length), ..Atom::default() };
    atom.music.push(PlacedMusic { glyph, x: 0.0, pitch });
    for dot in 0..dots {
        atom.music.push(PlacedMusic {
            glyph: MusicGlyph::AugmentationDot,
            x: glyph.metrics().advance + 0.05 + f32::from(dot) * 0.14,
            pitch: MIDDLE + 1.0,
        });
    }
    atom
}

fn bar_atom(bar: Bar) -> Atom {
    let mut atom = Atom { ends_bar: true, ..Atom::default() };
    let thin = engraving::THIN_BARLINE;
    let thick = engraving::THICK_BARLINE;
    let between = engraving::BARLINE_SEPARATION;
    let to_thick = engraving::THIN_THICK_SEPARATION;
    let to_dots = engraving::REPEAT_DOT_SEPARATION;
    let mut x = 0.12f32;
    let mut line = |x: &mut f32, weight: f32| {
        atom.rules.push(Rule { x: *x, top: TOP, width: weight, height: TOP - BOTTOM });
        *x += weight;
    };
    let dots = |atom: &mut Atom, x: f32| {
        for pitch in [5.0, 7.0] {
            atom.music.push(PlacedMusic { glyph: MusicGlyph::AugmentationDot, x, pitch });
        }
    };
    let dot = MusicGlyph::AugmentationDot.metrics().advance;
    match bar {
        Bar::Single => line(&mut x, thin),
        Bar::Double => {
            line(&mut x, thin);
            x += between;
            line(&mut x, thin);
        }
        Bar::Final => {
            line(&mut x, thin);
            x += to_thick;
            line(&mut x, thick);
        }
        Bar::RepeatStart => {
            line(&mut x, thick);
            x += to_thick;
            line(&mut x, thin);
            x += to_dots;
            let at = x;
            drop(line);
            dots(&mut atom, at);
            x += dot;
        }
        Bar::RepeatEnd => {
            let at = x;
            x += dot + to_dots;
            line(&mut x, thin);
            x += to_thick;
            line(&mut x, thick);
            drop(line);
            dots(&mut atom, at);
        }
        Bar::RepeatBoth => {
            let left = x;
            x += dot + to_dots;
            line(&mut x, thin);
            x += to_thick;
            line(&mut x, thick);
            x += to_thick;
            line(&mut x, thin);
            x += to_dots;
            let right = x;
            drop(line);
            dots(&mut atom, left);
            dots(&mut atom, right);
            x += dot;
        }
    }
    atom.width = x + 0.16;
    atom
}

/// The sharps or flats of a key at their places on the treble staff.
fn key_atom(key: Key) -> Atom {
    const SHARPS: [f32; 7] = [10.0, 7.0, 11.0, 8.0, 5.0, 9.0, 6.0];
    const FLATS: [f32; 7] = [6.0, 9.0, 5.0, 8.0, 4.0, 7.0, 3.0];
    let signature = key.signature();
    let (glyph, places) = if signature >= 0 {
        (MusicGlyph::AccidentalSharp, &SHARPS)
    } else {
        (MusicGlyph::AccidentalFlat, &FLATS)
    };
    let mut atom = Atom::default();
    let mut x = 0.0;
    for pitch in places.iter().take(usize::from(signature.unsigned_abs()).min(7)) {
        atom.music.push(PlacedMusic { glyph, x, pitch: *pitch });
        x += glyph.metrics().advance + 0.02;
    }
    atom.width = if x > 0.0 { x + 0.12 } else { 0.0 };
    atom
}

fn metre_atom(metre: Metre) -> Atom {
    let mut atom = Atom::default();
    let digit = |n: u32| match n {
        0 => MusicGlyph::TimeSig0,
        1 => MusicGlyph::TimeSig1,
        2 => MusicGlyph::TimeSig2,
        3 => MusicGlyph::TimeSig3,
        4 => MusicGlyph::TimeSig4,
        5 => MusicGlyph::TimeSig5,
        6 => MusicGlyph::TimeSig6,
        7 => MusicGlyph::TimeSig7,
        8 => MusicGlyph::TimeSig8,
        _ => MusicGlyph::TimeSig9,
    };
    // A number in its half of the staff, its digits side by side.
    let mut number = |n: u32, pitch: f32| {
        let mut x = 0.0;
        for c in n.to_string().chars() {
            let glyph = digit(c.to_digit(10).unwrap_or(0));
            atom.music.push(PlacedMusic { glyph, x, pitch });
            x += glyph.metrics().advance;
        }
        x
    };
    let width = match metre {
        Metre::None => 0.0,
        Metre::Common => {
            atom.music.push(PlacedMusic { glyph: MusicGlyph::TimeSigCommon, x: 0.0, pitch: MIDDLE });
            MusicGlyph::TimeSigCommon.metrics().advance
        }
        Metre::Cut => {
            atom.music.push(PlacedMusic { glyph: MusicGlyph::TimeSigCutCommon, x: 0.0, pitch: MIDDLE });
            MusicGlyph::TimeSigCutCommon.metrics().advance
        }
        Metre::Simple { num, den } => number(num, 8.0).max(number(den, 4.0)),
    };
    atom.width = if width > 0.0 { width + 0.2 } else { 0.0 };
    atom
}

/// What every line starts with: the clef and the key, and on the first line the metre.
fn prefix_atoms(key: Key, metre: Option<Metre>) -> Vec<Atom> {
    let mut clef = Atom { width: MusicGlyph::GClef.metrics().advance + 0.14, ..Atom::default() };
    // The curl of the clef goes around the G line.
    clef.music.push(PlacedMusic { glyph: MusicGlyph::GClef, x: 0.04, pitch: 4.0 });
    let mut out = vec![clef, key_atom(key)];
    if let Some(metre) = metre {
        out.push(metre_atom(metre));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abc::parse;

    fn engraved(music: &str, width: f32) -> Engraving {
        let parsed = parse(&format!("X:1\nT:Test\nM:3/4\nL:1/4\nK:F\n{music}\n"));
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        engrave(&parsed.tune, &Options { width, ..Options::default() })
    }

    fn glyphs(line: &Line) -> Vec<MusicGlyph> {
        line.music.iter().map(|g| g.glyph).collect()
    }

    #[test]
    fn a_line_starts_with_clef_key_and_metre() {
        let e = engraved("F G A | (B A) G | F2 z |]\nw: 1.~Lo-bet den Her-_ren all.", 18.0);
        assert_eq!(e.lines.len(), 1);
        let line = &e.lines[0];
        assert_eq!(line.staff.len(), 5);
        use MusicGlyph::*;
        assert_eq!(glyphs(line)[..4], [GClef, AccidentalFlat, TimeSig3, TimeSig4]);
        // The flat of F major is on the middle line, the numbers in their halves.
        assert_eq!(line.music[1].pitch, 6.0);
        assert_eq!((line.music[2].pitch, line.music[3].pitch), (8.0, 4.0));
        // Six quarter notes, a half note, a rest.
        let heads = glyphs(line).iter().filter(|g| **g == NoteheadBlack).count();
        assert_eq!(heads, 6);
        assert!(glyphs(line).contains(&NoteheadHalf));
        assert!(glyphs(line).contains(&RestQuarter));
        // One syllable per sung note, in the first row; the held note has none.
        let texts: Vec<&str> = line.lyrics.iter().map(|l| l.spans[0].text.as_str()).collect();
        assert_eq!(texts, ["1.", "Lo-", "bet", "den", "Her-", "ren", "all."]);
        assert!(line.lyrics.iter().all(|l| l.row == 0));
        // The number stands before its syllable.
        assert!(line.lyrics[0].x < line.lyrics[1].x);
        // Three single bars drawn, the last one with a thick line.
        let full: Vec<&Rule> = line.rules.iter().filter(|r| r.height == 8.0 && r.top == 10.0).collect();
        assert_eq!(full.len(), 4);
    }

    #[test]
    fn stems_dots_flags_and_ledger_lines() {
        let e = engraved("C3/2 D/2 c | e3 |]", 18.0);
        let line = &e.lines[0];
        use MusicGlyph::*;
        let found = glyphs(line);
        assert!(found.contains(&AugmentationDot));
        assert!(found.contains(&Flag8thUp));
        // Middle C has a ledger line under the staff, at its own height.
        assert!(line.rules.iter().any(|r| r.height < 1.0 && (r.top - 0.0).abs() < 0.2));
        // A note above the middle line has its stem down: the stem starts near the head.
        let high = line.music.iter().find(|g| g.pitch == 9.0).unwrap();
        assert!(line.rules.iter().any(|r| (r.x - high.x).abs() < 1e-4 && r.top < 9.0 && r.height > 6.0), "{:?}", line.rules);
    }

    #[test]
    fn verses_stand_in_rows_and_long_tunes_break_at_bars() {
        let bar = "F G A | ";
        let music = format!("{}F3 |]\nw: {}aus.\nw: {}ein.", bar.repeat(11), "ei-ne Silb ".repeat(11), "zwei-te Zeil ".repeat(11));
        let e = engraved(&music, 12.0);
        assert!(e.lines.len() >= 3, "{} lines", e.lines.len());
        for (n, line) in e.lines.iter().enumerate() {
            assert_eq!(line.music[0].glyph, MusicGlyph::GClef, "line {n}");
            // Only the first line has the time signature.
            assert_eq!(glyphs(line).contains(&MusicGlyph::TimeSig3), n == 0, "line {n}");
            assert!(line.lyrics.iter().any(|l| l.row == 1), "line {n}");
            for glyph in &line.music {
                assert!(glyph.x <= 12.0 + 1e-3, "line {n}: {glyph:?}");
            }
            assert!(line.height(e.lyric_size) > line.lyric_row_y(1, e.lyric_size));
        }
        let syllables: usize = e.lines.iter().map(|l| l.lyrics.len()).sum();
        assert_eq!(syllables, 2 * 34);
    }

    #[test]
    fn short_notes_without_a_space_share_a_beam() {
        use MusicGlyph::*;
        // Two eighths together, then two apart.
        let e = engraved("F/G/ A/ B/ c |]", 18.0);
        let line = &e.lines[0];
        // The pair has no flags, the two single ones have one each.
        assert_eq!(glyphs(line).iter().filter(|g| matches!(g, Flag8thUp | Flag8thDown)).count(), 2);
        // One beam: a rule half a staff space thick that reaches from the first stem to the second.
        let beams: Vec<&Rule> = line.rules.iter().filter(|r| r.height == 1.0 && r.width > 0.1).collect();
        assert_eq!(beams.len(), 1, "{:?}", line.rules);
        let heads: Vec<&PlacedMusic> = line.music.iter().filter(|g| g.glyph == NoteheadBlack).collect();
        assert!(beams[0].x > heads[0].x && beams[0].x + beams[0].width > heads[1].x);
        // Sixteenths get a second one.
        let e = engraved("F/4G/4A/4B/4 c2 |]", 18.0);
        assert_eq!(e.lines[0].rules.iter().filter(|r| r.height == 1.0 && r.width > 0.1).count(), 2);
    }

    #[test]
    fn ties_and_slurs_are_bows() {
        let e = engraved("F2 A- | A (G F) |]", 18.0);
        let line = &e.lines[0];
        assert_eq!(line.curves.len(), 2);
        let heads: Vec<&PlacedMusic> =
            line.music.iter().filter(|g| matches!(g.glyph, MusicGlyph::NoteheadBlack | MusicGlyph::NoteheadHalf)).collect();
        // The tie runs from the second note to the third, level, under notes with stems up.
        let tie = &line.curves[0];
        assert!(tie.x0 > heads[1].x && tie.x1 < heads[2].x + 0.01, "{tie:?}");
        assert_eq!(tie.pitch0, tie.pitch1);
        assert!(!tie.above);
        // The slur runs from the fourth to the fifth.
        let slur = &line.curves[1];
        assert!(slur.x0 > heads[3].x && slur.x1 > heads[4].x, "{slur:?}");
        let html = crate::html::score(&e);
        assert_eq!(html.matches("<svg class=\"gabc-bow\"").count(), 2);
    }

    #[test]
    fn a_verse_number_is_printed_once() {
        // The number is written on each line of music in the source.
        let e = engraved("F G A |\nw: 1.~Lo-bet den\nB A G |\nw: 1.~Her-ren all,\nF3 |]\nw: 2.~ja.", 18.0);
        let texts: Vec<&str> = e.lines.iter().flat_map(|l| &l.lyrics).map(|l| l.spans[0].text.as_str()).collect();
        assert_eq!(texts, ["1.", "Lo-", "bet", "den", "Her-", "ren", "all,", "2.", "ja."]);
    }
}
