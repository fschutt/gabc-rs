//! A strict parser for metered hymns in ABC notation.
//!
//! It reads a subset of ABC 2.1 (<https://abcnotation.com/wiki/abc:standard:v2.1>): one tune,
//! one voice, with lyrics. What the subset leaves out (chords, grace notes, a second voice,
//! chord symbols, directives) is valid ABC, and is rejected here with its own code so that the
//! message can say so. Like the GABC parser it never panics and reports every problem it
//! finds.
//!
//! ```text
//! X:1
//! T:Lobet den Herren
//! M:3/4
//! L:1/4
//! K:F
//! F G A | (B A) G | F2 z |]
//! w: 1.~Lo-bet den Her-_ren all.
//! ```
//!
//! Beyond the syntax three things are checked: every bar adds up to the metre (an upbeat and
//! the bars next to a repeat or double bar may be shorter), a tie leads to a note of the same
//! pitch, and every `w:` line has exactly one syllable, `_` or `*` for each note of the line
//! of music above it.

use crate::diagnostics::{Diagnostic, Severity, Span};

/// A parsed tune.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tune {
    /// The header lines as written.
    pub fields: Vec<Field>,
    pub title: String,
    pub composer: Option<String>,
    pub metre: Metre,
    /// `L:`, the length that a note without a number has.
    pub unit: Fraction,
    pub key: Key,
    pub lines: Vec<MusicLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: char,
    pub value: String,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Metre {
    /// `M:none`: free rhythm. Bar lines only mark divisions.
    #[default]
    None,
    /// `M:C`, drawn as the C sign, counted as 4/4.
    Common,
    /// `M:C|`, counted as 2/2.
    Cut,
    Simple { num: u32, den: u32 },
}

impl Metre {
    /// The length of a full bar, or `None` for free rhythm.
    pub fn bar(self) -> Option<Fraction> {
        match self {
            Metre::None => None,
            Metre::Common | Metre::Cut => Some(Fraction::new(1, 1)),
            Metre::Simple { num, den } => Some(Fraction::new(num, den)),
        }
    }
}

/// A length as a fraction of a whole note, always reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fraction {
    pub num: u32,
    pub den: u32,
}

impl Default for Fraction {
    fn default() -> Self {
        Fraction { num: 0, den: 1 }
    }
}

impl Fraction {
    pub fn new(num: u32, den: u32) -> Self {
        let (mut a, mut b) = (num, den.max(1));
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let g = a.max(1);
        Fraction { num: num / g, den: den.max(1) / g }
    }

    #[must_use]
    pub fn plus(self, other: Fraction) -> Fraction {
        Fraction::new(
            self.num.saturating_mul(other.den).saturating_add(other.num.saturating_mul(self.den)),
            self.den.saturating_mul(other.den),
        )
    }

    #[must_use]
    pub fn times(self, other: Fraction) -> Fraction {
        Fraction::new(self.num.saturating_mul(other.num), self.den.saturating_mul(other.den))
    }

    fn less_than(self, other: Fraction) -> bool {
        u64::from(self.num) * u64::from(other.den) < u64::from(other.num) * u64::from(self.den)
    }

    /// True if one note head can show this length: a plain, dotted or double-dotted value
    /// from a sixty-fourth note up to a longa.
    fn is_one_note(self) -> bool {
        let power_of_two = |n: u32| n.is_power_of_two();
        match (self.num, self.den) {
            (1 | 3 | 7, den) => power_of_two(den) && den <= 64,
            (2 | 4, 1) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for Fraction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.num, self.den)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Key {
    /// 0 = C, 1 = D, ... 6 = B
    pub tonic: u8,
    /// -1 flat, 0, 1 sharp
    pub accidental: i8,
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
}

impl Key {
    /// The key signature: the number of sharps, negative for flats.
    pub fn signature(self) -> i8 {
        let major = [0, 2, 4, -1, 1, 3, 5][usize::from(self.tonic.min(6))];
        let mode = match self.mode {
            Mode::Lydian => 1,
            Mode::Major => 0,
            Mode::Mixolydian => -1,
            Mode::Dorian => -2,
            Mode::Minor => -3,
            Mode::Phrygian => -4,
            Mode::Locrian => -5,
        };
        major + 7 * self.accidental + mode
    }
}

/// One source line of music with the lyric lines under it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MusicLine {
    pub elements: Vec<Element>,
    /// The `w:` lines directly below, in order: the verses, or the passes of a repeat.
    pub lyrics: Vec<LyricLine>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub kind: ElementKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    Note(Note),
    /// `z` with its length
    Rest(Fraction),
    Bar(Bar),
    /// `[1`, `[2`: the start of a first or second ending
    Ending(u8),
    SlurStart,
    SlurEnd,
    /// `(3`: the next three notes are played in the time of two
    Triplet,
    Decoration(Decoration),
    /// `[M:...]`
    Metre(Metre),
    /// `[K:...]`
    Key(Key),
    /// White space between notes: it breaks the beam.
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Note {
    /// Steps of the scale above middle C: `C` is 0, `B` is 6, `c` is 7, `B,` is -1.
    pub step: i8,
    /// A written accidental: -2 to 2 semitones, 0 for a natural sign.
    pub accidental: Option<i8>,
    /// The written length. In a triplet the note sounds for two thirds of it.
    pub length: Fraction,
    pub in_triplet: bool,
    /// `-` after the note: tied to the next one.
    pub tie: bool,
}

impl Note {
    pub fn duration(&self) -> Fraction {
        if self.in_triplet {
            self.length.times(Fraction::new(2, 3))
        } else {
            self.length
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    /// `|`
    Single,
    /// `||` and `[|`
    Double,
    /// `|]`
    Final,
    /// `|:`
    RepeatStart,
    /// `:|`
    RepeatEnd,
    /// `::` and `:|:`
    RepeatBoth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    Fermata,
    Breath,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LyricLine {
    /// One per note of the music line.
    pub slots: Vec<LyricSlot>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LyricSlot {
    pub kind: SlotKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotKind {
    /// Text under a note. `hyphen` is true if the word goes on with the next syllable. A `~`
    /// in the source is a no-break space here.
    Syllable { text: String, hyphen: bool },
    /// `_`: the syllable before is held for this note too.
    Hold,
    /// `*`: no text under this note.
    Skip,
}

/// The tune and everything that was found wrong with its source.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub tune: Tune,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    /// True if the source has no errors.
    pub fn is_valid(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }
}

/// The accepted header fields, in the order they must come.
const HEADER_ORDER: &[char] = &['X', 'T', 'C', 'M', 'L', 'K'];

/// Parses one ABC tune.
pub fn parse(src: &str) -> Parsed {
    let mut p = Parser { src, out: Parsed::default(), unit: Fraction::new(1, 8), triplet_left: 0 };
    p.run();
    p.check_bars();
    p.check_ties_and_slurs();
    p.check_lyrics();
    p.out
}

struct Parser<'a> {
    src: &'a str,
    out: Parsed,
    unit: Fraction,
    /// How many of the next notes still belong to a `(3`.
    triplet_left: u8,
}

impl Parser<'_> {
    fn error(&mut self, code: &'static str, span: Span, message: impl Into<String>) {
        self.out.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
        });
    }

    fn run(&mut self) {
        let src = self.src;
        let mut offset = 0;
        let mut in_header = true;
        let mut seen: Vec<char> = Vec::new();
        let mut blank_at: Option<Span> = None;
        let mut started = false;
        for raw in src.split_inclusive('\n') {
            let start = offset;
            offset += raw.len();
            let line = raw.trim_end_matches(['\n', '\r']);
            let span = Span::new(start, start + line.len());
            if line.trim().is_empty() {
                if started && blank_at.is_none() {
                    blank_at = Some(span);
                }
                continue;
            }
            if let Some(blank) = blank_at.take() {
                self.error("E207", blank, "an empty line ends an ABC tune; there is more below it");
            }
            started = true;
            if line.starts_with("%%") {
                self.error("E211", span, "`%%` directives are not accepted");
                continue;
            }
            if line.starts_with('%') {
                continue;
            }
            let field = field_of(line);
            if in_header {
                let Some((name, value)) = field else {
                    self.error(
                        "E206",
                        span,
                        "the music starts before the header is complete: `K:` is its last line",
                    );
                    in_header = false;
                    self.music_line(start, start + line.len());
                    continue;
                };
                self.header_field(name, value, span, &mut seen);
                if name == 'K' {
                    for required in ['T', 'M', 'L'] {
                        if !seen.contains(&required) {
                            self.error("E206", span, format!("the header has no `{required}:` line"));
                        }
                    }
                    in_header = false;
                }
                continue;
            }
            match field {
                Some(('w', text)) => {
                    let at = start + line.len() - text.len();
                    let lyrics = self.lyric_line(text, at, span);
                    match self.out.tune.lines.last_mut() {
                        Some(music) => music.lyrics.push(lyrics),
                        None => self.error("E230", span, "this `w:` line has no line of music above it"),
                    }
                }
                Some(('V', _)) => self.error("E211", span, "a second voice (`V:`) is not accepted"),
                Some((name @ ('M' | 'K' | 'L'), value)) => self.error(
                    "E211",
                    span,
                    format!("a change inside the tune is written in the music line: `[{name}:{}]`", value.trim()),
                ),
                Some((name, _)) => {
                    self.error("E211", span, format!("`{name}:` is not accepted inside the tune"))
                }
                None => self.music_line(start, start + line.len()),
            }
        }
        if !started {
            self.error("E201", Span::new(0, 0), "the block is empty; an ABC tune starts with `X:1`");
        } else if in_header {
            self.error("E206", Span::new(src.len(), src.len()), "the header has no `K:` line, and no music follows");
        }
    }

    fn header_field(&mut self, name: char, value: &str, span: Span, seen: &mut Vec<char>) {
        let value = value.trim();
        if seen.is_empty() && name != 'X' {
            self.error("E201", span, "an ABC tune starts with `X:1`");
        }
        let Some(rank) = HEADER_ORDER.iter().position(|c| *c == name) else {
            self.error("E202", span, format!("`{name}:` is not an accepted header field (X, T, C, M, L, K)"));
            return;
        };
        if seen.contains(&name) && name != 'T' {
            self.error("E206", span, format!("a second `{name}:` line"));
        } else if seen.iter().any(|s| HEADER_ORDER.iter().position(|c| c == s) > Some(rank)) {
            self.error("E206", span, "the header lines come in the order X, T, C, M, L, K");
        }
        seen.push(name);
        self.out.tune.fields.push(Field { name, value: value.to_string(), span });
        match name {
            'X' => {
                if value.parse::<u32>().is_err() {
                    self.error("E201", span, "`X:` is followed by the tune number: `X:1`");
                }
            }
            'T' => {
                if value.is_empty() {
                    self.error("E202", span, "`T:` has no title");
                } else if self.out.tune.title.is_empty() {
                    self.out.tune.title = value.to_string();
                }
            }
            'C' => self.out.tune.composer = Some(value.to_string()),
            'M' => match metre_of(value) {
                Some(metre) => self.out.tune.metre = metre,
                None => self.error("E203", span, format!("`{value}` is not a metre (`3/4`, `C`, `C|`, `none`)")),
            },
            'L' => match unit_of(value) {
                Some(unit) => {
                    self.unit = unit;
                    self.out.tune.unit = unit;
                }
                None => self.error("E204", span, format!("`{value}` is not a unit note length (`1/4`, `1/8`)")),
            },
            _ => match key_of(value) {
                Some(key) => self.out.tune.key = key,
                None => self.error("E205", span, format!("`{value}` is not a key (`F`, `Dm`, `Bb`, `D dor`)")),
            },
        }
    }

    fn music_line(&mut self, start: usize, end: usize) {
        let src = self.src;
        let b = &src.as_bytes()[..end];
        let at = |i: usize| b.get(i).copied().unwrap_or(0);
        let mut line = MusicLine { span: Span::new(start, end), ..MusicLine::default() };
        let mut i = start;
        while i < end {
            let from = i;
            let c = b[i];
            let kind = match c {
                b' ' | b'\t' => {
                    while matches!(at(i), b' ' | b'\t') {
                        i += 1;
                    }
                    Some(ElementKind::Space)
                }
                b'%' => {
                    i = end;
                    None
                }
                b'|' => {
                    i += 1;
                    Some(ElementKind::Bar(match at(i) {
                        b']' => {
                            i += 1;
                            Bar::Final
                        }
                        b'|' => {
                            i += 1;
                            Bar::Double
                        }
                        b':' => {
                            i += 1;
                            Bar::RepeatStart
                        }
                        _ => Bar::Single,
                    }))
                }
                b':' => {
                    i += 1;
                    match at(i) {
                        b':' => {
                            i += 1;
                            Some(ElementKind::Bar(Bar::RepeatBoth))
                        }
                        b'|' => {
                            i += 1;
                            if at(i) == b':' {
                                i += 1;
                                Some(ElementKind::Bar(Bar::RepeatBoth))
                            } else {
                                if matches!(at(i), b']' | b'|') {
                                    i += 1;
                                }
                                Some(ElementKind::Bar(Bar::RepeatEnd))
                            }
                        }
                        _ => {
                            self.error("E210", Span::new(from, i), "a `:` belongs to a repeat sign: `|:` or `:|`");
                            None
                        }
                    }
                }
                b'[' => match at(i + 1) {
                    b'|' => {
                        i += 2;
                        Some(ElementKind::Bar(Bar::Double))
                    }
                    n @ b'1'..=b'9' => {
                        i += 2;
                        Some(ElementKind::Ending(n - b'0'))
                    }
                    letter if letter.is_ascii_alphabetic() && at(i + 2) == b':' => {
                        let close = src[i..end].find(']').map_or(end, |n| i + n);
                        let value = src[i + 3..close].trim();
                        let span = Span::new(from, (close + 1).min(end));
                        if close == end {
                            self.error("E216", Span::new(from, from + 1), "this `[` is never closed");
                        }
                        i = (close + 1).min(end);
                        match letter {
                            b'M' => match metre_of(value) {
                                Some(metre) => Some(ElementKind::Metre(metre)),
                                None => {
                                    self.error("E203", span, format!("`{value}` is not a metre"));
                                    None
                                }
                            },
                            b'K' => match key_of(value) {
                                Some(key) => Some(ElementKind::Key(key)),
                                None => {
                                    self.error("E205", span, format!("`{value}` is not a key"));
                                    None
                                }
                            },
                            other => {
                                self.error(
                                    "E216",
                                    span,
                                    format!("`[{}:]` is not accepted in the music; only `[M:]` and `[K:]` are", other as char),
                                );
                                None
                            }
                        }
                    }
                    _ => {
                        let close = src[i..end].find(']').map_or(end, |n| i + n + 1);
                        self.error("E211", Span::new(from, close), "chords are not accepted: one voice only");
                        i = close;
                        None
                    }
                },
                // `|1` and `:|2` are the short forms of `| [1` and `:| [2`.
                n @ b'1'..=b'9' if matches!(line.elements.last(), Some(Element { kind: ElementKind::Bar(_), .. })) => {
                    i += 1;
                    Some(ElementKind::Ending(n - b'0'))
                }
                b'(' => {
                    i += 1;
                    if at(i).is_ascii_digit() && i < end {
                        let digit = at(i);
                        i += 1;
                        if digit == b'3' && at(i) != b':' {
                            self.triplet_left = 3;
                            Some(ElementKind::Triplet)
                        } else {
                            self.error("E211", Span::new(from, i), "the only tuplet accepted is the triplet `(3`");
                            None
                        }
                    } else {
                        Some(ElementKind::SlurStart)
                    }
                }
                b')' => {
                    i += 1;
                    Some(ElementKind::SlurEnd)
                }
                b'!' => match src[i + 1..end].find('!') {
                    Some(len) => {
                        let name = &src[i + 1..i + 1 + len];
                        i += len + 2;
                        match name {
                            "fermata" => Some(ElementKind::Decoration(Decoration::Fermata)),
                            "breath" => Some(ElementKind::Decoration(Decoration::Breath)),
                            _ => {
                                self.error(
                                    "E213",
                                    Span::new(from, i),
                                    format!("`!{name}!` is not accepted; the decorations are `!fermata!` and `!breath!`"),
                                );
                                None
                            }
                        }
                    }
                    None => {
                        self.error("E213", Span::new(from, from + 1), "this `!` starts a decoration that is never closed");
                        i += 1;
                        None
                    }
                },
                b'"' => {
                    let close = src[i + 1..end].find('"').map_or(end, |n| i + n + 2);
                    self.error("E211", Span::new(from, close), "chord symbols and annotations are not accepted");
                    i = close;
                    None
                }
                b'{' => {
                    let close = src[i..end].find('}').map_or(end, |n| i + n + 1);
                    self.error("E211", Span::new(from, close), "grace notes are not accepted");
                    i = close;
                    None
                }
                b'>' | b'<' => {
                    while matches!(at(i), b'>' | b'<') {
                        i += 1;
                    }
                    self.error(
                        "E211",
                        Span::new(from, i),
                        "broken rhythm (`>`, `<`) is not accepted; write the two lengths out",
                    );
                    None
                }
                b'z' => {
                    i += 1;
                    let length = self.length(&mut i, end);
                    Some(ElementKind::Rest(length))
                }
                b'^' | b'_' | b'=' | b'A'..=b'G' | b'a'..=b'g' => self.note(&mut i, end).map(ElementKind::Note),
                _ => {
                    let ch = src[i..end].chars().next().unwrap_or(' ');
                    i += ch.len_utf8();
                    self.error("E210", Span::new(from, i), format!("`{ch}` is not part of the notation"));
                    None
                }
            };
            if let Some(kind) = kind {
                line.elements.push(Element { kind, span: Span::new(from, i) });
            }
        }
        self.out.tune.lines.push(line);
    }

    /// Reads a note with its accidental, octave marks, length and tie. `i` is at its start.
    fn note(&mut self, i: &mut usize, end: usize) -> Option<Note> {
        let b = self.src.as_bytes();
        let at = |k: usize| if k < end { b[k] } else { 0 };
        let from = *i;
        let accidental = match at(*i) {
            b'^' => {
                *i += 1;
                if at(*i) == b'^' {
                    *i += 1;
                    Some(2)
                } else {
                    Some(1)
                }
            }
            b'_' => {
                *i += 1;
                if at(*i) == b'_' {
                    *i += 1;
                    Some(-2)
                } else {
                    Some(-1)
                }
            }
            b'=' => {
                *i += 1;
                Some(0)
            }
            _ => None,
        };
        let letter = at(*i);
        let mut step: i8 = match letter {
            b'C'..=b'G' => (letter - b'C') as i8,
            b'A' | b'B' => (letter - b'A') as i8 + 5,
            b'c'..=b'g' => (letter - b'c') as i8 + 7,
            b'a' | b'b' => (letter - b'a') as i8 + 12,
            _ => {
                self.error("E210", Span::new(from, *i), "an accidental stands before a note");
                return None;
            }
        };
        *i += 1;
        loop {
            match at(*i) {
                b',' => step = step.saturating_sub(7),
                b'\'' => step = step.saturating_add(7),
                _ => break,
            }
            *i += 1;
        }
        let length = self.length(i, end);
        let tie = at(*i) == b'-';
        *i += usize::from(tie);
        let in_triplet = self.triplet_left > 0;
        self.triplet_left = self.triplet_left.saturating_sub(1);
        Some(Note { step, accidental, length, in_triplet, tie })
    }

    /// Reads the length after a note or rest (`2`, `/2`, `3/2`, `/`) as a length in whole
    /// notes.
    fn length(&mut self, i: &mut usize, end: usize) -> Fraction {
        let b = self.src.as_bytes();
        let at = |k: usize| if k < end { b[k] } else { 0 };
        let from = *i;
        let number = |i: &mut usize| {
            let mut n: Option<u32> = None;
            while at(*i).is_ascii_digit() {
                n = Some(n.unwrap_or(0).saturating_mul(10).saturating_add(u32::from(at(*i) - b'0')));
                *i += 1;
            }
            n
        };
        let num = number(i).unwrap_or(1);
        let mut den: u32 = 1;
        while at(*i) == b'/' {
            *i += 1;
            den = den.saturating_mul(number(i).unwrap_or(2));
        }
        let length = Fraction::new(num, den).times(self.unit);
        if num == 0 || den == 0 || !length.is_one_note() {
            self.error(
                "E212",
                Span::new(from.saturating_sub(1), *i),
                format!(
                    "a length of {} of the unit ({length} of a whole note) is not one note; tie two notes",
                    Fraction::new(num, den)
                ),
            );
        }
        length
    }

    fn lyric_line(&mut self, text: &str, at: usize, span: Span) -> LyricLine {
        let mut line = LyricLine { span, ..LyricLine::default() };
        let mut word = String::new();
        let mut word_at = at;
        let mut chars = text.char_indices().peekable();
        fn flush(line: &mut LyricLine, word: &mut String, from: usize, to: usize, hyphen: bool) {
            if !word.is_empty() {
                line.slots.push(LyricSlot {
                    kind: SlotKind::Syllable { text: std::mem::take(word), hyphen },
                    span: Span::new(from, to),
                });
            }
        }
        while let Some((k, c)) = chars.next() {
            let here = at + k;
            if word.is_empty() {
                word_at = here;
            }
            match c {
                ' ' | '\t' => flush(&mut line, &mut word, word_at, here, false),
                '-' => {
                    if word.is_empty() {
                        // A hyphen on its own takes a note, like `_`.
                        line.slots.push(LyricSlot { kind: SlotKind::Hold, span: Span::new(here, here + 1) });
                    } else {
                        flush(&mut line, &mut word, word_at, here + 1, true);
                    }
                }
                '_' | '*' => {
                    flush(&mut line, &mut word, word_at, here, false);
                    let kind = if c == '_' { SlotKind::Hold } else { SlotKind::Skip };
                    line.slots.push(LyricSlot { kind, span: Span::new(here, here + 1) });
                }
                '~' => word.push('\u{a0}'),
                '\\' if chars.peek().is_some_and(|(_, n)| *n == '-') => {
                    chars.next();
                    word.push('-');
                }
                '|' => self.error(
                    "E232",
                    Span::new(here, here + 1),
                    "`|` is not accepted in a `w:` line; give every note its syllable, `_` or `*`",
                ),
                _ => word.push(c),
            }
        }
        flush(&mut line, &mut word, word_at, at + text.len(), false);
        line
    }

    /// Every bar must add up to the metre. A bar may be shorter if it is the first or the
    /// last of the tune, or if a repeat sign, a double bar or an ending stands on one of its
    /// sides: that is where an upbeat and the bar that completes it are.
    fn check_bars(&mut self) {
        let mut metre = self.out.tune.metre;
        let mut length = Fraction::default();
        let mut bar_start: Option<usize> = None;
        let mut bar_end = 0;
        let mut first = true;
        let mut loose_before = true;
        let mut problems = Vec::new();
        let close = |length: Fraction, metre: Metre, span: Span, loose: bool, problems: &mut Vec<(Span, String)>| {
            let Some(full) = metre.bar() else { return };
            if full.less_than(length) {
                problems.push((span, format!("this bar is {length} of a whole note long, the metre allows {full}")));
            } else if length.less_than(full) && !loose {
                problems.push((span, format!("this bar is only {length} of a whole note long, the metre needs {full}")));
            }
        };
        for line in &self.out.tune.lines {
            for el in &line.elements {
                match &el.kind {
                    ElementKind::Note(note) => {
                        bar_start.get_or_insert(el.span.start);
                        bar_end = el.span.end;
                        length = length.plus(note.duration());
                    }
                    ElementKind::Rest(rest) => {
                        bar_start.get_or_insert(el.span.start);
                        bar_end = el.span.end;
                        length = length.plus(*rest);
                    }
                    ElementKind::Bar(bar) => {
                        let loose_after = *bar != Bar::Single;
                        if let Some(start) = bar_start.take() {
                            let loose = first || loose_before || loose_after;
                            close(length, metre, Span::new(start, bar_end), loose, &mut problems);
                            first = false;
                        }
                        length = Fraction::default();
                        loose_before = loose_after;
                    }
                    ElementKind::Ending(_) => loose_before = true,
                    ElementKind::Metre(new) => {
                        // What stands before the change is a bar of the old metre.
                        if let Some(start) = bar_start.take() {
                            close(length, metre, Span::new(start, bar_end), true, &mut problems);
                            length = Fraction::default();
                        }
                        metre = *new;
                    }
                    _ => {}
                }
            }
        }
        // The last bar may be short, never long.
        if let Some(start) = bar_start {
            close(length, metre, Span::new(start, bar_end), true, &mut problems);
        }
        for (span, message) in problems {
            self.error("E220", span, message);
        }
    }

    fn check_ties_and_slurs(&mut self) {
        let mut problems: Vec<(&'static str, Span, String)> = Vec::new();
        let mut tied: Option<(i8, Span)> = None;
        let mut slurs: Vec<Span> = Vec::new();
        for line in &self.out.tune.lines {
            for el in &line.elements {
                match &el.kind {
                    ElementKind::Note(note) => {
                        if let Some((step, span)) = tied.take() {
                            if step != note.step {
                                problems.push(("E215", span, String::from("a tie joins two notes of the same pitch; this is a slur: `(A B)`")));
                            }
                        }
                        if note.tie {
                            tied = Some((note.step, el.span));
                        }
                    }
                    ElementKind::Rest(_) => {
                        if let Some((_, span)) = tied.take() {
                            problems.push(("E215", span, String::from("a tie leads to a rest")));
                        }
                    }
                    ElementKind::SlurStart => slurs.push(el.span),
                    ElementKind::SlurEnd => {
                        if slurs.pop().is_none() {
                            problems.push(("E214", el.span, String::from("this `)` closes no slur")));
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some((_, span)) = tied {
            problems.push(("E215", span, String::from("a tie at the end of the tune leads nowhere")));
        }
        for span in slurs {
            problems.push(("E214", span, String::from("this slur is never closed")));
        }
        for (code, span, message) in problems {
            self.error(code, span, message);
        }
    }

    fn check_lyrics(&mut self) {
        let mut problems = Vec::new();
        for line in &self.out.tune.lines {
            let notes = line.elements.iter().filter(|e| matches!(e.kind, ElementKind::Note(_))).count();
            for lyrics in &line.lyrics {
                let slots = lyrics.slots.len();
                if slots != notes {
                    let hint = if slots < notes {
                        "fill the rest with `_` (held) or `*` (no text)"
                    } else {
                        "a syllable too many, or a `-` where the print has a hyphen in one syllable (`\\-`)"
                    };
                    problems.push((
                        lyrics.span,
                        format!("{slots} syllables for {notes} notes in the line of music above: {hint}"),
                    ));
                }
            }
        }
        for (span, message) in problems {
            self.error("E231", span, message);
        }
    }
}

/// `X:...` at the start of a line: the letter and the rest.
fn field_of(line: &str) -> Option<(char, &str)> {
    let mut chars = line.chars();
    let name = chars.next().filter(char::is_ascii_alphabetic)?;
    (chars.next() == Some(':')).then(|| (name, &line[2..]))
}

fn metre_of(value: &str) -> Option<Metre> {
    match value.trim() {
        "none" => Some(Metre::None),
        "C" => Some(Metre::Common),
        "C|" => Some(Metre::Cut),
        other => {
            let (num, den) = other.split_once('/')?;
            let (num, den) = (num.trim().parse::<u32>().ok()?, den.trim().parse::<u32>().ok()?);
            (num > 0 && num <= 32 && den.is_power_of_two() && den <= 64).then_some(Metre::Simple { num, den })
        }
    }
}

fn unit_of(value: &str) -> Option<Fraction> {
    let (num, den) = value.trim().split_once('/')?;
    let den = den.trim().parse::<u32>().ok()?;
    (num.trim() == "1" && den.is_power_of_two() && den <= 64).then(|| Fraction::new(1, den))
}

fn key_of(value: &str) -> Option<Key> {
    let value = value.trim();
    let mut chars = value.chars();
    let tonic = match chars.next()? {
        c @ 'C'..='G' => c as u8 - b'C',
        c @ ('A' | 'B') => c as u8 - b'A' + 5,
        _ => return None,
    };
    let rest = chars.as_str();
    let (accidental, rest) = match rest.chars().next() {
        Some('#') => (1, &rest[1..]),
        Some('b') => (-1, &rest[1..]),
        _ => (0, rest),
    };
    let mode = rest.trim().to_ascii_lowercase();
    let mode = match mode.as_str() {
        "" | "maj" | "major" | "ion" | "ionian" => Mode::Major,
        "m" | "min" | "minor" | "aeo" | "aeolian" => Mode::Minor,
        "dor" | "dorian" => Mode::Dorian,
        "phr" | "phrygian" => Mode::Phrygian,
        "lyd" | "lydian" => Mode::Lydian,
        "mix" | "mixolydian" => Mode::Mixolydian,
        "loc" | "locrian" => Mode::Locrian,
        _ => return None,
    };
    let key = Key { tonic, accidental, mode };
    (key.signature().abs() <= 7).then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "X:1\nT:Test\nM:3/4\nL:1/4\nK:F\n";

    fn codes(src: &str) -> Vec<&'static str> {
        parse(src).diagnostics.iter().map(|d| d.code).collect()
    }

    fn valid(src: &str) -> Tune {
        let parsed = parse(src);
        assert!(parsed.is_valid(), "{:#?}", parsed.diagnostics);
        parsed.tune
    }

    #[test]
    fn a_small_tune() {
        let tune = valid(&format!("{HEAD}F G A | (B A) G | F2 z |]\nw: 1.~Lo-bet den Her-_ren all.\nw: 2.~Prei-set den Na-_men sein.\n"));
        assert_eq!(tune.title, "Test");
        assert_eq!(tune.metre, Metre::Simple { num: 3, den: 4 });
        assert_eq!(tune.key.signature(), -1);
        let line = &tune.lines[0];
        assert_eq!(line.lyrics.len(), 2);
        let slots = &line.lyrics[0].slots;
        assert_eq!(slots.len(), 7);
        assert_eq!(slots[0].kind, SlotKind::Syllable { text: "1.\u{a0}Lo".into(), hyphen: true });
        assert_eq!(slots[4].kind, SlotKind::Hold);
        let notes: Vec<&Note> = line
            .elements
            .iter()
            .filter_map(|e| if let ElementKind::Note(n) = &e.kind { Some(n) } else { None })
            .collect();
        assert_eq!(notes[0].step, 3);
        assert_eq!(notes[6].length, Fraction::new(1, 2));
    }

    #[test]
    fn pitches_accidentals_and_lengths() {
        let tune = valid("X:1\nT:T\nM:none\nL:1/4\nK:C\nC B, c' ^F _B =B A3/2 G/2 A/ A8 z2\n");
        let steps: Vec<(i8, Option<i8>, Fraction)> = tune.lines[0]
            .elements
            .iter()
            .filter_map(|e| if let ElementKind::Note(n) = &e.kind { Some((n.step, n.accidental, n.length)) } else { None })
            .collect();
        assert_eq!(steps[0], (0, None, Fraction::new(1, 4)));
        assert_eq!(steps[1].0, -1);
        assert_eq!(steps[2].0, 14);
        assert_eq!((steps[3].0, steps[3].1), (3, Some(1)));
        assert_eq!(steps[4].1, Some(-1));
        assert_eq!(steps[5].1, Some(0));
        assert_eq!(steps[6].2, Fraction::new(3, 8));
        assert_eq!(steps[7].2, Fraction::new(1, 8));
        assert_eq!(steps[8].2, Fraction::new(1, 8));
        assert_eq!(steps[9].2, Fraction::new(2, 1));
        assert_eq!(codes("X:1\nT:T\nM:none\nL:1/4\nK:C\nA5\n"), ["E212"]);
    }

    #[test]
    fn keys() {
        let sig = |k: &str| key_of(k).map(Key::signature);
        assert_eq!(sig("C"), Some(0));
        assert_eq!(sig("Em"), Some(1));
        assert_eq!(sig("Bb"), Some(-2));
        assert_eq!(sig("D dor"), Some(0));
        assert_eq!(sig("F#m"), Some(3));
        assert_eq!(sig("G mix"), Some(0));
        assert_eq!(sig("H"), None);
        assert_eq!(sig("C blues"), None);
    }

    #[test]
    fn the_header_is_checked() {
        assert_eq!(codes("T:Test\nM:3/4\nL:1/4\nK:F\nF G A |]\n"), ["E201"]);
        assert_eq!(codes("X:1\nT:Test\nL:1/4\nK:F\nF G A |]\n"), ["E206"]);
        assert_eq!(codes("X:1\nT:Test\nL:1/4\nM:3/4\nK:F\nF G A |]\n"), ["E206"]);
        assert_eq!(codes("X:1\nT:Test\nR:reel\nM:3/4\nL:1/4\nK:F\nF G A |]\n"), ["E202"]);
        assert_eq!(codes("X:1\nT:Test\nM:3/5\nL:1/4\nK:F\nF G A |]\n"), ["E203"]);
        assert_eq!(codes("X:1\nT:Test\nM:3/4\nL:1/3\nK:F\nF G A |]\n"), ["E204"]);
        assert_eq!(codes("X:1\nT:Test\nM:3/4\nL:1/4\nK:H\nF G A |]\n"), ["E205"]);
        assert_eq!(codes("X:1\nT:Test\nM:3/4\nL:1/4\n"), ["E206"]);
        assert_eq!(codes(""), ["E201"]);
    }

    #[test]
    fn what_the_subset_leaves_out_is_named() {
        for music in ["[CEG]3 |]", "\"C\"F G A |]", "{g}F G A |]", "F>G A |]", "(5FGFGF A2 |]"] {
            assert!(codes(&format!("{HEAD}{music}\n")).contains(&"E211"), "{music}");
        }
        assert_eq!(codes(&format!("{HEAD}F G A |]\nV:2\n")), ["E211"]);
        assert_eq!(codes(&format!("{HEAD}%%score 1\nF G A |]\n")), ["E211"]);
        assert_eq!(codes(&format!("{HEAD}F G A |\nK:G\nF G A |]\n")), ["E211"]);
        assert_eq!(codes(&format!("{HEAD}F G !trill!A |]\n")), ["E213"]);
        assert_eq!(codes(&format!("{HEAD}F G ? |]\n")), ["E210"]);
    }

    #[test]
    fn bars_add_up_to_the_metre() {
        // An upbeat, and the bar before the repeat sign that completes it.
        valid(&format!("{HEAD}C | F G A | B A :| G | F3 |]\n"));
        // A bar may run over a line end.
        valid(&format!("{HEAD}F G A | B A\nG | F3 |]\n"));
        assert_eq!(codes(&format!("{HEAD}F G A | B A | F3 |]\n")), ["E220"]);
        assert_eq!(codes(&format!("{HEAD}F G A B | F3 |]\n")), ["E220"]);
        // The last bar may be short, not long.
        valid(&format!("{HEAD}F G A | F2\n"));
        assert_eq!(codes(&format!("{HEAD}F G A | F4\n")), ["E220"]);
        // A triplet takes the time of two of its notes.
        valid(&format!("{HEAD}(3FGA B | F3 |]\n"));
        // A change of metre, and free rhythm.
        valid(&format!("{HEAD}F G A | [M:4/4] F G A B | F4 |]\n"));
        valid("X:1\nT:T\nM:none\nL:1/4\nK:C\nC D E F G | A8 |]\n");
    }

    #[test]
    fn ties_and_slurs() {
        valid(&format!("{HEAD}F2 A- | A (G F) |]\n"));
        assert_eq!(codes(&format!("{HEAD}F2 A- | G G F |]\n")), ["E215"]);
        assert_eq!(codes(&format!("{HEAD}F2 (A | G G F |]\n")), ["E214"]);
        assert_eq!(codes(&format!("{HEAD}F2 A) | G G F |]\n")), ["E214"]);
    }

    #[test]
    fn every_note_has_its_syllable() {
        let src = |w: &str| format!("{HEAD}F G A | B A G |]\nw: {w}\n");
        valid(&src("Lo-bet den Her-ren all"));
        valid(&src("Lo-bet den Herrn__"));
        valid(&src("* * den Lieb\\-Frau o~wie schön"));
        assert_eq!(codes(&src("Lo-bet den Her-ren")), ["E231"]);
        assert_eq!(codes(&src("Lo-bet den Her-ren al-le-samt")), ["E231"]);
        assert_eq!(codes(&src("Lo-bet den | Her-ren all")), ["E232"]);
        assert_eq!(codes(&format!("{HEAD}w: Lo-bet\nF G A |]\n")), ["E230"]);
    }

    #[test]
    fn an_empty_line_inside_the_tune() {
        assert_eq!(codes(&format!("{HEAD}F G A |\n\nB A G |]\n")), ["E207"]);
        valid(&format!("\n{HEAD}F G A |]\n\n"));
    }
}
