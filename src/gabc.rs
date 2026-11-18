//! A strict parser for GABC, the text notation for Gregorian chant.
//!
//! Written from the format's public description and from what real scores contain; it shares
//! no code with Gregorio. The parser never panics and never stops at the first problem: it
//! returns the score it could read together with every [`Diagnostic`] it has.
//!
//! A GABC file is header lines, a line that is only `%%`, and the body. In the body each
//! syllable of text is followed by its notes in parentheses:
//!
//! ```text
//! name: Kyrie;
//! mode: 8;
//! %%
//! (c4) KY(f)ri(gh)e(h.) (::)
//! ```

use crate::diagnostics::{Diagnostic, Severity, Span};

/// A parsed score.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Score {
    pub headers: Vec<Header>,
    pub syllables: Vec<Syllable>,
    /// How many NABC snippets follow each GABC snippet (`nabc-lines` header), 0 if none.
    pub nabc_lines: usize,
}

/// One `name: value;` line of the header.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: String,
    pub span: Span,
}

/// A piece of text and the notes sung on it. Either may be missing: a bar line has notes
/// and no text, text before the first notes has no notes.
#[derive(Debug, Clone, PartialEq)]
pub struct Syllable {
    /// The text as written, markup included, without the translation.
    pub text: String,
    /// Text in `[...]` after the syllable: a translation printed under it.
    pub translation: Option<String>,
    /// True if white space separates this syllable from the one before: a new word.
    pub starts_word: bool,
    pub notes: Option<NoteGroup>,
    pub span: Span,
}

/// What stands between one pair of parentheses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NoteGroup {
    pub elements: Vec<Element>,
    /// The NABC snippets of the group, unparsed. They are checked against `nabc-lines` and
    /// kept for the NABC extension.
    pub nabc: Vec<(String, Span)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub kind: ElementKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElementKind {
    Clef(Clef),
    Note(Note),
    /// A flat, natural or sharp drawn at a pitch.
    Accidental {
        pitch: u8,
        kind: AccidentalKind,
        cautionary: bool,
    },
    Bar(Bar),
    Space(Space),
    /// A guide note. `None` is the automatic one (`z0`), which takes the next pitch.
    Custos(Option<u8>),
    /// A forced line break: `z` justified, `Z` ragged.
    LineBreak {
        justified: bool,
    },
    /// `[name:value]`: braces, episema overrides, ledger lines, text above the staff.
    Attribute {
        name: String,
        value: String,
    },
    /// `!`: the notes on both sides belong to one neume.
    Join,
    /// `@`: fusion or a change of stem, depending on where it stands.
    At,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clef {
    /// True for a do (C) clef, false for a fa (F) clef.
    pub do_clef: bool,
    /// The staff line it sits on, 1 (bottom) to 4.
    pub line: u8,
    pub flat: bool,
}

/// Pitches are counted from `a` = 0 upwards; the four staff lines are 3, 5, 7 and 9
/// (`d f h j`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Note {
    pub pitch: u8,
    pub shape: Shape,
    /// `-` before the pitch: a small first note (initio debilis).
    pub initio_debilis: bool,
    pub liquescence: Liquescence,
    /// Number of dots after the note (`.`, `..`).
    pub mora: u8,
    /// Number of horizontal episema strokes requested (`_`, `__`, ...).
    pub episema: u8,
    /// A vertical episema (`'`).
    pub ictus: bool,
    /// The modifiers exactly as written, for whatever the fields above do not hold yet
    /// (position digits, sign numbers).
    pub raw_modifiers: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    #[default]
    Punctum,
    /// An uppercase pitch: the diamond-shaped note of descending groups.
    Inclinatum,
    Virga,
    VirgaReversa,
    Bivirga,
    Trivirga,
    Stropha,
    Distropha,
    Tristropha,
    Oriscus,
    OriscusScapus,
    Quilisma,
    QuilismaQuadratum,
    Quadratum,
    /// `r`: a hollow note.
    Cavum,
    /// `R` and `r0`: a note drawn as or with lines.
    Linea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Liquescence {
    #[default]
    None,
    /// `~`
    Deminutus,
    /// `<`
    Augmented,
    /// `>`
    Diminutive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccidentalKind {
    Flat,
    Natural,
    Sharp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    /// `` ` ``
    Virgula,
    /// `,`
    Quarter,
    /// `^`
    QuarterHigh,
    /// `;` and `;1` to `;8`
    Half,
    /// `:`
    Full,
    /// `::`
    Double,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    /// `/`
    Small,
    /// `//`
    Medium,
    /// A blank inside the parentheses
    Large,
    /// `/0`
    Half,
    /// `/!`: a small space that does not break the neume
    SmallJoined,
    /// `/[n]`: a space of a given size
    Sized,
}

/// The score and everything that was found wrong with its source.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub score: Score,
    pub diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    /// True if the source has no errors. Warnings do not count.
    pub fn is_valid(&self) -> bool {
        !self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }
}

const TEXT_TAGS: &[&str] = &[
    "b", "i", "sc", "ul", "u", "tt", "c", "e", "eu", "nlba", "sp", "v", "alt", "pr", "clear",
];

/// Tags that stand alone, with no closing tag.
const EMPTY_TAGS: &[&str] = &["clear"];

const ATTRIBUTES: &[&str] = &[
    "alt", "cn", "cs", "hl", "ll", "nm", "nocustos", "nv", "ob", "ocb", "ocba", "oh", "oll",
    "oslur", "shape", "ub", "ubb", "uh", "ull", "uslur",
];

/// Parses a GABC file.
pub fn parse(src: &str) -> Parsed {
    let mut p = Parser {
        src,
        out: Parsed::default(),
        tex_warned: false,
        nabc_warned: false,
    };
    let body_start = p.header();
    p.body(body_start);
    p.out
}

struct Parser<'a> {
    src: &'a str,
    out: Parsed,
    tex_warned: bool,
    nabc_warned: bool,
}

fn pitch_of(c: char) -> Option<u8> {
    match c.to_ascii_lowercase() {
        l @ 'a'..='n' => Some(l as u8 - b'a'),
        'p' => Some(14),
        _ => None,
    }
}

impl<'a> Parser<'a> {
    fn error(&mut self, code: &'static str, span: Span, message: impl Into<String>) {
        self.out.diagnostics.push(Diagnostic {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
        });
    }

    fn warning(&mut self, code: &'static str, span: Span, message: impl Into<String>) {
        self.out.diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code,
            message: message.into(),
            span,
        });
    }

    /// Reads the header and returns where the body starts.
    fn header(&mut self) -> usize {
        let src = self.src;
        if !src.lines().any(|line| line.trim() == "%%") {
            // Without the separator nothing says where the header ends. Reading the body's
            // lines as header lines would bury this one cause under a report per line, so
            // the whole source is read as the body.
            self.error(
                "E001",
                Span::new(0, 0),
                "no `%%` line: the header must be followed by a line that is only `%%`",
            );
            return 0;
        }
        let mut offset = 0;
        let mut lines = src.split_inclusive('\n');
        while let Some(raw) = lines.next() {
            let start = offset;
            offset += raw.len();
            let line = raw.trim_end_matches(['\n', '\r']);
            let trimmed = line.trim();
            if trimmed == "%%" {
                return offset;
            }
            if trimmed.is_empty() || trimmed.starts_with('%') {
                continue;
            }
            let Some(colon) = line.find(':') else {
                self.error(
                    "E002",
                    Span::new(start, start + line.len()),
                    "a header line is `name: value;`",
                );
                continue;
            };
            let name = line[..colon].trim();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                self.error(
                    "E002",
                    Span::new(start, start + colon),
                    format!("`{name}` is not a header name"),
                );
                continue;
            }
            // The value ends at the last `;` of the line. Without one it runs on over the
            // following lines, up to a line that ends with `;;`.
            let rest = &line[colon + 1..];
            let mut value = String::new();
            let mut end = start + line.len();
            if let Some(semi) = rest.trim_end().strip_suffix(';') {
                value.push_str(semi.trim_end_matches(';').trim());
            } else {
                value.push_str(rest.trim());
                let mut closed = false;
                for more in lines.by_ref() {
                    offset += more.len();
                    end = offset;
                    let more = more.trim_end_matches(['\n', '\r']);
                    if more.trim() == "%%" {
                        break;
                    }
                    if let Some(last) = more.trim_end().strip_suffix(";;") {
                        value.push('\n');
                        value.push_str(last);
                        closed = true;
                        break;
                    }
                    value.push('\n');
                    value.push_str(more);
                }
                if !closed {
                    self.error(
                        "E003",
                        Span::new(start, start + line.len()),
                        format!("the header `{name}` does not end with `;`"),
                    );
                    if src[..offset].trim_end().ends_with("%%") {
                        return offset;
                    }
                }
            }
            if name == "nabc-lines" {
                match value.parse::<usize>() {
                    Ok(n) if n <= 4 => self.out.score.nabc_lines = n,
                    _ => self.error(
                        "E004",
                        Span::new(start, end),
                        "`nabc-lines` is a number from 0 to 4",
                    ),
                }
            }
            self.out.score.headers.push(Header {
                name: name.to_string(),
                value,
                span: Span::new(start, end),
            });
        }
        // Not reached with a `%%` line in the source, which the check above made sure of.
        src.len()
    }

    fn body(&mut self, start: usize) {
        let src = self.src;
        let bytes = src.as_bytes();
        let mut i = start;
        let mut text = String::new();
        let mut text_start = start;
        let mut pending_space = true; // the first syllable starts a word
        let mut seen_clef = false;
        let mut seen_note = false;

        while i < bytes.len() {
            let c = bytes[i];
            match c {
                b'%' => {
                    // A comment runs to the end of the line.
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'(' => {
                    let open = i;
                    // The group ends at the next `)` that is not inside `[...]`.
                    let mut depth = 0usize;
                    let mut j = i + 1;
                    let mut close = None;
                    while j < bytes.len() {
                        match bytes[j] {
                            b'[' => depth += 1,
                            b']' => depth = depth.saturating_sub(1),
                            b')' if depth == 0 => {
                                close = Some(j);
                                break;
                            }
                            b'(' if depth == 0 => break,
                            _ => {}
                        }
                        j += 1;
                    }
                    let Some(close) = close else {
                        self.error(
                            "E010",
                            Span::new(open, open + 1),
                            "this `(` is never closed",
                        );
                        text.push('(');
                        i += 1;
                        continue;
                    };
                    let group = self.group(open + 1, close);
                    for el in &group.elements {
                        match el.kind {
                            ElementKind::Clef(_) => seen_clef = true,
                            ElementKind::Note(_) if !seen_clef && !seen_note => {
                                seen_note = true;
                                let span = el.span;
                                self.warning("W103", span, "notes before any clef");
                            }
                            _ => {}
                        }
                    }
                    let (plain, translation) = self.text(&text, text_start);
                    self.out.score.syllables.push(Syllable {
                        text: plain,
                        translation,
                        starts_word: pending_space,
                        notes: Some(group),
                        span: Span::new(text_start, close + 1),
                    });
                    text.clear();
                    pending_space = false;
                    i = close + 1;
                    text_start = i;
                }
                b' ' | b'\t' | b'\n' | b'\r' if text.trim().is_empty() => {
                    pending_space = true;
                    text.clear();
                    i += 1;
                    text_start = i;
                }
                _ => {
                    let ch = src[i..].chars().next().unwrap_or(' ');
                    text.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        if !text.trim().is_empty() {
            let (plain, translation) = self.text(&text, text_start);
            self.out.score.syllables.push(Syllable {
                text: plain,
                translation,
                starts_word: pending_space,
                notes: None,
                span: Span::new(text_start, bytes.len()),
            });
        }
    }

    /// Checks the markup of a syllable's text and splits off its translation.
    fn text(&mut self, raw: &str, at: usize) -> (String, Option<String>) {
        let mut plain = String::new();
        let mut translation = None;
        let mut open: Vec<(&str, usize)> = Vec::new();
        let mut i = 0;
        let bytes = raw.as_bytes();
        while i < bytes.len() {
            match bytes[i] {
                b'<' => {
                    let Some(len) = raw[i..].find('>') else {
                        self.error("E020", Span::new(at + i, at + i + 1), "this `<` starts no tag");
                        plain.push('<');
                        i += 1;
                        continue;
                    };
                    let tag = &raw[i + 1..i + len];
                    let span = Span::new(at + i, at + i + len + 1);
                    let (closing, name) = match tag.strip_prefix('/') {
                        Some(n) => (true, n),
                        None => (false, tag.split(':').next().unwrap_or(tag)),
                    };
                    if !TEXT_TAGS.contains(&name) {
                        self.error("E020", span, format!("`<{tag}>` is not a GABC tag"));
                    } else if closing {
                        match open.pop() {
                            Some((top, _)) if top == name => {}
                            Some((top, _)) => self.error(
                                "E021",
                                span,
                                format!("`</{name}>` closes `<{top}>`"),
                            ),
                            None => self.error(
                                "E021",
                                span,
                                format!("`</{name}>` has no opening tag"),
                            ),
                        }
                    } else if !EMPTY_TAGS.contains(&name) {
                        if name == "v" && !self.tex_warned {
                            self.tex_warned = true;
                            self.warning("W101", span, "TeX in `<v>` is not drawn");
                        }
                        open.push((name, at + i));
                    }
                    plain.push_str(&raw[i..i + len + 1]);
                    i += len + 1;
                }
                b'[' if open.is_empty() => {
                    let Some(len) = raw[i..].find(']') else {
                        self.error(
                            "E022",
                            Span::new(at + i, at + i + 1),
                            "this `[` is never closed",
                        );
                        plain.push('[');
                        i += 1;
                        continue;
                    };
                    translation = Some(raw[i + 1..i + len].to_string());
                    i += len + 1;
                }
                _ => {
                    let ch = raw[i..].chars().next().unwrap_or(' ');
                    plain.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        // A tag may stay open across syllables (`<i>` over several words), so an open tag at
        // the end of one syllable is not an error here.
        let _ = open;
        (plain.trim_end().to_string(), translation)
    }

    /// Parses what is between `(` at `start - 1` and `)` at `end`.
    fn group(&mut self, start: usize, end: usize) -> NoteGroup {
        let src = self.src;
        let mut group = NoteGroup {
            span: Span::new(start - 1, end + 1),
            ..NoteGroup::default()
        };
        // Split at `|` outside brackets: GABC, then `nabc-lines` NABC snippets, and again.
        let mut pieces = Vec::new();
        let (mut depth, mut from) = (0usize, start);
        for (k, b) in src.as_bytes()[start..end].iter().enumerate() {
            match b {
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'|' if depth == 0 => {
                    pieces.push((from, start + k));
                    from = start + k + 1;
                }
                _ => {}
            }
        }
        pieces.push((from, end));
        let cycle = self.out.score.nabc_lines + 1;
        if pieces.len() > 1 && cycle == 1 {
            self.error(
                "E030",
                Span::new(pieces[0].1, pieces[0].1 + 1),
                "`|` starts NABC neumes, but the header has no `nabc-lines`",
            );
        }
        for (n, (a, b)) in pieces.into_iter().enumerate() {
            if cycle == 1 && n > 0 || cycle > 1 && n % cycle != 0 {
                if !self.nabc_warned && cycle > 1 {
                    self.nabc_warned = true;
                    self.warning("W102", Span::new(a, b), "NABC neumes are not drawn yet");
                }
                group.nabc.push((src[a..b].to_string(), Span::new(a, b)));
            } else {
                self.snippet(a, b, &mut group.elements);
            }
        }
        group
    }

    fn snippet(&mut self, start: usize, end: usize, out: &mut Vec<Element>) {
        let src = self.src;
        let b = &src.as_bytes()[..end];
        let at = |i: usize| b.get(i).copied().unwrap_or(0);
        let mut i = start;
        while i < end {
            let from = i;
            let c = b[i];
            let kind = match c {
                b' ' => {
                    i += 1;
                    Some(ElementKind::Space(Space::Large))
                }
                b'\n' | b'\r' | b'\t' => {
                    i += 1;
                    None
                }
                b'/' => {
                    i += 1;
                    Some(ElementKind::Space(match at(i) {
                        b'/' => {
                            i += 1;
                            Space::Medium
                        }
                        b'0' => {
                            i += 1;
                            Space::Half
                        }
                        b'!' => {
                            i += 1;
                            Space::SmallJoined
                        }
                        b'[' => {
                            match src[i..end].find(']') {
                                Some(len) => i += len + 1,
                                None => {
                                    self.error("E014", Span::new(i, i + 1), "this `[` is never closed");
                                    i = end;
                                }
                            }
                            Space::Sized
                        }
                        _ => Space::Small,
                    }))
                }
                b'!' => {
                    i += 1;
                    Some(ElementKind::Join)
                }
                b'@' => {
                    i += 1;
                    Some(ElementKind::At)
                }
                b'`' | b',' | b'^' | b';' | b':' => {
                    i += 1;
                    let bar = match c {
                        b'`' => Bar::Virgula,
                        b',' => Bar::Quarter,
                        b'^' => Bar::QuarterHigh,
                        b';' => Bar::Half,
                        _ if at(i) == b':' => {
                            i += 1;
                            Bar::Double
                        }
                        _ => Bar::Full,
                    };
                    // A position or variant after the bar, then signs on it.
                    match (c, at(i)) {
                        (b'`' | b',' | b'^', b'0') | (b',', b'_') | (b':', b'?') => i += 1,
                        (b';', b'1'..=b'8') => i += 1,
                        _ => {}
                    }
                    while i < end && matches!(b[i], b'\'' | b'_') {
                        i += 1;
                    }
                    Some(ElementKind::Bar(bar))
                }
                b'z' | b'Z' => {
                    i += 1;
                    if c == b'z' && at(i) == b'0' {
                        i += 1;
                        Some(ElementKind::Custos(None))
                    } else {
                        if matches!(at(i), b'+' | b'-') && i < end {
                            i += 1;
                        }
                        Some(ElementKind::LineBreak { justified: c == b'z' })
                    }
                }
                b'[' => match src[i..end].find(']') {
                    Some(len) => {
                        let inner = &src[i + 1..i + len];
                        let (name, value) = inner.split_once(':').unwrap_or((inner, ""));
                        if !ATTRIBUTES.contains(&name) {
                            self.error(
                                "E012",
                                Span::new(i, i + len + 1),
                                format!("`[{name}]` is not a known attribute"),
                            );
                        }
                        i += len + 1;
                        Some(ElementKind::Attribute {
                            name: name.to_string(),
                            value: value.to_string(),
                        })
                    }
                    None => {
                        self.error("E014", Span::new(i, i + 1), "this `[` is never closed");
                        i = end;
                        None
                    }
                },
                // A clef: c or f, an optional b, a line number.
                b'c' | b'f'
                    if matches!(at(i + 1), b'1'..=b'5')
                        || at(i + 1) == b'b' && matches!(at(i + 2), b'1'..=b'5') =>
                {
                    let flat = at(i + 1) == b'b';
                    let line = at(i + 1 + usize::from(flat)) - b'0';
                    i += 2 + usize::from(flat);
                    if line > 4 {
                        self.error("E013", Span::new(from, i), "a clef sits on line 1 to 4");
                    }
                    Some(ElementKind::Clef(Clef {
                        do_clef: c == b'c',
                        line,
                        flat,
                    }))
                }
                _ => {
                    let initio = c == b'-';
                    let letter_at = i + usize::from(initio);
                    let letter = at(letter_at) as char;
                    match pitch_of(letter).filter(|_| letter_at < end) {
                        Some(pitch) => {
                            i = letter_at + 1;
                            Some(self.note(pitch, letter.is_ascii_uppercase(), initio, &mut i, end))
                        }
                        None => {
                            let ch = src[i..end].chars().next().unwrap_or(' ');
                            i += ch.len_utf8();
                            self.error(
                                "E011",
                                Span::new(from, i),
                                format!("`{ch}` is not part of the notation"),
                            );
                            None
                        }
                    }
                }
            };
            if let Some(kind) = kind {
                out.push(Element {
                    kind,
                    span: Span::new(from, i),
                });
            }
        }
    }

    /// Reads the modifiers after a pitch letter. `i` is just past the letter.
    fn note(&mut self, pitch: u8, upper: bool, initio: bool, i: &mut usize, end: usize) -> ElementKind {
        let b = self.src.as_bytes();
        let start = *i;
        let at = |k: usize| if k < end { b[k] } else { 0 };
        let mut note = Note {
            pitch,
            shape: if upper { Shape::Inclinatum } else { Shape::Punctum },
            initio_debilis: initio,
            ..Note::default()
        };
        // The lean of a diamond note.
        if upper && matches!(at(*i), b'0'..=b'2') {
            *i += 1;
        }
        loop {
            let c = at(*i);
            match c {
                b'v' => {
                    let mut n = 0;
                    while at(*i) == b'v' && n < 3 {
                        *i += 1;
                        n += 1;
                    }
                    note.shape = [Shape::Virga, Shape::Bivirga, Shape::Trivirga][n - 1];
                }
                b's' => {
                    let mut n = 0;
                    while at(*i) == b's' && n < 3 {
                        *i += 1;
                        n += 1;
                    }
                    note.shape = [Shape::Stropha, Shape::Distropha, Shape::Tristropha][n - 1];
                }
                b'V' => {
                    *i += 1;
                    note.shape = Shape::VirgaReversa;
                }
                b'o' | b'O' => {
                    *i += 1;
                    if matches!(at(*i), b'0' | b'1') {
                        *i += 1;
                    }
                    note.shape = if c == b'o' { Shape::Oriscus } else { Shape::OriscusScapus };
                }
                b'w' => {
                    *i += 1;
                    note.shape = Shape::Quilisma;
                }
                b'W' => {
                    *i += 1;
                    note.shape = Shape::QuilismaQuadratum;
                }
                b'q' => {
                    *i += 1;
                    note.shape = Shape::Quadratum;
                }
                b'r' => {
                    *i += 1;
                    match at(*i) {
                        b'0' => {
                            *i += 1;
                            note.shape = Shape::Linea;
                        }
                        // r1 to r8 put a sign above the note and leave its shape alone.
                        b'1'..=b'8' => *i += 1,
                        _ => note.shape = Shape::Cavum,
                    }
                }
                b'R' => {
                    *i += 1;
                    note.shape = Shape::Linea;
                }
                b'~' => {
                    *i += 1;
                    note.liquescence = Liquescence::Deminutus;
                }
                b'<' => {
                    *i += 1;
                    note.liquescence = Liquescence::Augmented;
                }
                b'>' => {
                    *i += 1;
                    note.liquescence = Liquescence::Diminutive;
                }
                b'.' => {
                    *i += 1;
                    note.mora = (note.mora + 1).min(2);
                    if matches!(at(*i), b'0' | b'1') {
                        *i += 1;
                    }
                }
                b'_' => {
                    *i += 1;
                    note.episema = note.episema.saturating_add(1);
                    while matches!(at(*i), b'0'..=b'5') {
                        *i += 1;
                    }
                }
                b'\'' => {
                    *i += 1;
                    note.ictus = true;
                    if matches!(at(*i), b'0' | b'1') {
                        *i += 1;
                    }
                }
                b'x' | b'y' | b'#' => {
                    // The letter was the height of an accidental, not a note.
                    *i += 1;
                    if c == b'#' && at(*i) == b'#' {
                        *i += 1;
                    }
                    let cautionary = at(*i) == b'?';
                    *i += usize::from(cautionary);
                    return ElementKind::Accidental {
                        pitch,
                        kind: match c {
                            b'x' => AccidentalKind::Flat,
                            b'y' => AccidentalKind::Natural,
                            _ => AccidentalKind::Sharp,
                        },
                        cautionary,
                    };
                }
                b'+' => {
                    *i += 1;
                    return ElementKind::Custos(Some(pitch));
                }
                _ => break,
            }
        }
        note.raw_modifiers = self.src[start..*i].to_string();
        ElementKind::Note(note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<ElementKind> {
        let parsed = parse(src);
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        parsed
            .score
            .syllables
            .iter()
            .flat_map(|s| s.notes.iter().flat_map(|g| g.elements.iter().map(|e| e.kind.clone())))
            .collect()
    }

    fn codes(src: &str) -> Vec<&'static str> {
        parse(src).diagnostics.iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_small_score() {
        let parsed = parse("name: Kyrie;\nmode: 8;\n%%\n(c4) KY(f)ri(gh)e(h.) (::)\n");
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        let score = parsed.score;
        assert_eq!(score.headers.len(), 2);
        assert_eq!(score.headers[1].value, "8");
        let texts: Vec<&str> = score.syllables.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["", "KY", "ri", "e", ""]);
        let words: Vec<bool> = score.syllables.iter().map(|s| s.starts_word).collect();
        assert_eq!(words, [true, true, false, false, true]);
    }

    #[test]
    fn clefs_notes_bars_and_spaces() {
        let k = kinds("%%\n(cb3) (f3) (fgwh/ixi//G'F_E.) (z0) (;3) (g+) (z)");
        assert!(matches!(k[0], ElementKind::Clef(Clef { do_clef: true, line: 3, flat: true })));
        assert!(matches!(k[1], ElementKind::Clef(Clef { do_clef: false, line: 3, flat: false })));
        let notes: Vec<&Note> = k
            .iter()
            .filter_map(|e| match e {
                ElementKind::Note(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(notes[0].pitch, 5);
        assert_eq!(notes[1].shape, Shape::Quilisma);
        assert_eq!(notes[4].shape, Shape::Inclinatum);
        assert!(notes[4].ictus);
        assert_eq!(notes[5].episema, 1);
        assert_eq!(notes[6].mora, 1);
        assert!(k.contains(&ElementKind::Accidental { pitch: 8, kind: AccidentalKind::Flat, cautionary: false }));
        assert!(k.contains(&ElementKind::Space(Space::Small)));
        assert!(k.contains(&ElementKind::Space(Space::Medium)));
        assert!(k.contains(&ElementKind::Custos(None)));
        assert!(k.contains(&ElementKind::Custos(Some(6))));
        assert!(k.contains(&ElementKind::Bar(Bar::Half)));
        assert!(k.contains(&ElementKind::LineBreak { justified: true }));
    }

    #[test]
    fn what_is_wrong_is_reported_and_the_rest_is_still_read() {
        assert_eq!(codes("name: x;\n(c4) A(f)"), ["E001"]);
        assert_eq!(codes("name x\n%%\n(c4)"), ["E002"]);
        assert_eq!(codes("%%\n(c4) A(f"), ["E010"]);
        assert_eq!(codes("%%\n(c4) A(f$g)"), ["E011"]);
        assert_eq!(codes("%%\n(c4) A(f[zz:1])"), ["E012"]);
        assert_eq!(codes("%%\n(c4) <x>A</x>(f)"), ["E020", "E020"]);
        assert_eq!(codes("%%\n(c4) A</i>(f)"), ["E021"]);
        assert_eq!(codes("%%\n(c4) A(f|vi)"), ["E030"]);
        let parsed = parse("%%\n(c4) A(f$g) B(h)");
        assert_eq!(parsed.score.syllables.len(), 3);
    }

    #[test]
    fn nabc_snippets_are_split_off_and_counted_by_the_header() {
        let parsed = parse("nabc-lines: 1;\n%%\n(c4) A(fg|pe|h|vi)");
        assert_eq!(codes("nabc-lines: 1;\n%%\n(c4) A(fg|pe|h|vi)"), ["W102"]);
        let group = parsed.score.syllables[1].notes.as_ref().unwrap();
        assert_eq!(group.nabc.len(), 2);
        assert_eq!(group.nabc[0].0, "pe");
        assert_eq!(group.elements.len(), 3);
    }

    #[test]
    fn text_markup_and_translation() {
        let parsed = parse("%%\n(c4) <i>Ps.</i>(f) Al[Praise](g)le<sp>'ae</sp>(h)");
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.score.syllables[2].translation.as_deref(), Some("Praise"));
        assert_eq!(parsed.score.syllables[2].text, "Al");
        assert_eq!(codes("%%\n(c4) <v>\\x</v>(f)"), ["W101"]);
    }
}
