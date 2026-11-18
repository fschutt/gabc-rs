//! A strict parser for GABC, the text notation for Gregorian chant.
//!
//! Written from the format's public description, from what real scores contain, and from what
//! Gregorio does when it is given a piece of GABC (`docs/gabc-observed.md` lists those
//! observations). It shares no code with Gregorio. The parser never panics and never stops
//! at the first problem: it returns the score it could read together with every
//! [`Diagnostic`] it has.
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
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub headers: Vec<Header>,
    pub syllables: Vec<Syllable>,
    /// How many NABC snippets follow each GABC snippet (`nabc-lines` header), 0 if none.
    pub nabc_lines: usize,
    /// The number of staff lines (`staff-lines` header), 4 unless the header says otherwise.
    pub staff_lines: u8,
}

impl Default for Score {
    fn default() -> Self {
        Score { headers: Vec::new(), syllables: Vec::new(), nabc_lines: 0, staff_lines: 4 }
    }
}

/// One `name: value;` entry of the header.
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
    /// The text as written, markup and `$` escapes included, without the translation.
    pub text: String,
    /// Text in `[...]` at the end of the syllable: a translation printed under it.
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
    /// The second clef of `c4@f2`, drawn together with the first.
    SecondaryClef(Clef),
    Note(Note),
    /// A flat, natural or sharp drawn at a pitch.
    Accidental {
        pitch: u8,
        kind: AccidentalKind,
        /// `?`: drawn in parentheses.
        cautionary: bool,
        /// `##`: the soft form.
        soft: bool,
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
    /// `{`: the notes up to [`ElementKind::VoiceEnd`] are a second voice. They are drawn at
    /// the place of the notes that follow and take no room of their own.
    VoiceStart,
    /// `}`
    VoiceEnd,
    /// `<nlba>` and `</nlba>` inside the notes: no line may be broken in between.
    NoBreak {
        start: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clef {
    /// True for a do (C) clef, false for a fa (F) clef.
    pub do_clef: bool,
    /// The staff line it sits on, 1 (bottom) to 4, or 5 on a staff of five lines.
    pub line: u8,
    pub flat: bool,
}

/// Pitches are counted from `a` = 0 upwards; the four staff lines are 3, 5, 7 and 9
/// (`d f h j`). `n` (13) and `p` (14) exist on a staff of five lines only.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Note {
    pub pitch: u8,
    pub shape: Shape,
    /// `r`: drawn hollow.
    pub cavum: bool,
    /// `-` before the pitch: a small first note (initio debilis).
    pub initio_debilis: bool,
    pub liquescence: Liquescence,
    /// Number of dots after the note (`.`, `..`).
    pub mora: u8,
    /// How many `_` follow the note. Written n times, the horizontal episema lies over this
    /// note and the n - 1 notes before it.
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
    /// `=`: a line through the staff space.
    Linea,
    /// `R` and `r0`: a punctum between two lines.
    LineaPunctum,
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
    /// `` ` ``, and `` `0 `` above the staff
    Virgula { high: bool },
    /// `^`, and `^0` above the staff: the shortest stroke
    Minimis { high: bool },
    /// `,`, and `,0` above the staff: the quarter bar
    Minima { high: bool },
    /// `;`: the half bar
    Minor,
    /// `;1` to `;8`, also written `,1` to `,8`: the half bars of Dominican books at
    /// different heights. 7 and 8 need a staff of five lines.
    Dominican(u8),
    /// `:`: the full bar
    Maior,
    /// `:?`: a dotted full bar
    MaiorDotted,
    /// `::`: the double bar
    Finalis,
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
    /// `!/`: a small space at which no line is broken
    SmallNoBreak,
    /// `/[n]` and `//[n]`: a space of a given size
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

/// The tags of the text. Anything else in angle brackets is printed as it stands.
const TEXT_TAGS: &[&str] = &[
    "b", "i", "sc", "ul", "tt", "c", "e", "eu", "nlba", "sp", "v", "alt", "pr", "clear",
];

/// Tags that stand alone, with no closing tag.
const EMPTY_TAGS: &[&str] = &["clear", "pr"];

/// Tags whose content is taken as it stands: a `(` in it starts no notes and a `[` no
/// translation. Each must be closed.
const LITERAL_TAGS: &[(&str, &str, &str)] =
    &[("v", "<v>", "</v>"), ("sp", "<sp>", "</sp>"), ("alt", "<alt>", "</alt>")];

const ATTRIBUTES: &[&str] = &[
    "alt", "cn", "cs", "ev", "gv", "hl", "ll", "nocustos", "nv", "ob", "ocb", "ocba", "oh",
    "oll", "oslur", "shape", "ub", "uh", "ull", "uslur",
];

/// Attributes that can open a stretch with `{` and end it with `}` on a later note:
/// `[ob:1{]` ... `[ob:1}]`.
const STRETCHES: &[&str] = &["ob", "ocb", "ocba", "oh", "oll", "oslur", "ub", "uh", "ull", "uslur"];

/// Parses a GABC file.
pub fn parse(src: &str) -> Parsed {
    let mut p = Parser {
        src,
        out: Parsed::default(),
        tex_warned: false,
        nabc_warned: false,
        open_tags: Vec::new(),
        stretches: Vec::new(),
        episemas: 0,
    };
    let body_start = p.header();
    p.body(body_start);
    // Without a `%%` the header was read as text, and its "first syllable" means nothing.
    if !p.out.diagnostics.iter().any(|d| d.code == "E001") {
        p.first_syllable();
    }
    p.out
}

struct Parser<'a> {
    src: &'a str,
    out: Parsed,
    tex_warned: bool,
    nabc_warned: bool,
    /// The text tags that are open, with where each was opened. A tag stays open over
    /// the syllables that follow until it is closed.
    open_tags: Vec<(&'static str, Span)>,
    /// The stretches that are open (`[ob:1{]`), by attribute name. One may end in a later
    /// syllable.
    stretches: Vec<(String, Span)>,
    /// How many `_` the note read last has.
    episemas: usize,
}

fn pitch_of(c: u8) -> Option<u8> {
    match c.to_ascii_lowercase() {
        l @ b'a'..=b'n' => Some(l - b'a'),
        b'p' => Some(14),
        _ => None,
    }
}

/// The offset of the `]` that closes the `[` at the start of `text`.
fn matching_bracket(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (k, c) in text.bytes().enumerate() {
        match c {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_note(kind: &ElementKind) -> bool {
    matches!(kind, ElementKind::Note(_) | ElementKind::Accidental { .. })
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
        let mut separator = None;
        let mut offset = 0;
        for raw in src.split_inclusive('\n') {
            if raw.trim() == "%%" {
                separator = Some((offset, offset + raw.len()));
                break;
            }
            offset += raw.len();
        }
        let Some((head_end, body_start)) = separator else {
            // Without the separator nothing says where the header ends. Reading the body's
            // lines as header lines would bury this one cause under a report per line, so
            // the whole source is read as the body.
            self.error(
                "E001",
                Span::new(0, 0),
                "no `%%` line: the header must be followed by a line that is only `%%`",
            );
            return 0;
        };
        let head = &src[..head_end];
        let mut i = 0;
        loop {
            // An entry starts at the next thing that is not white space.
            i += head[i..].len() - head[i..].trim_start().len();
            if i >= head.len() {
                break;
            }
            let line_end = i + head[i..].find('\n').unwrap_or(head.len() - i);
            if head[i..].starts_with('%') {
                i = line_end;
                continue;
            }
            // The name is what stands before the next `:`. Gregorio takes any name, even
            // one that is left over from a stray `;` on the line before.
            let Some(colon) = head[i..].find(':') else {
                self.error("E002", Span::new(i, line_end), "a header entry is `name: value;`");
                break;
            };
            let name = head[i..i + colon].trim();
            if name.is_empty() {
                self.error("E002", Span::new(i, line_end), "a header entry is `name: value;`");
                i += colon + 1;
                continue;
            }
            let value_start = i + colon + 1;
            // The value runs to the next `;`, over line ends if it must.
            let Some(length) = head[value_start..].find(';') else {
                self.error(
                    "E003",
                    Span::new(i, line_end),
                    format!("the header `{name}` is not ended by a `;`"),
                );
                break;
            };
            let value_end = value_start + length;
            let value = head[value_start..value_end].trim();
            let doubled = head[value_end + 1..].starts_with(';');
            let end = value_end + 1 + usize::from(doubled);
            let span = Span::new(i, end);
            if value.contains('\n') && !doubled {
                self.warning(
                    "W104",
                    Span::new(i, line_end),
                    format!("the header `{name}` has no `;` on its line, so its value runs on to the next `;`"),
                );
            }
            match name {
                "nabc-lines" => match value.parse::<usize>() {
                    Ok(n) if n <= 4 => self.out.score.nabc_lines = n,
                    _ => self.error("E004", span, "`nabc-lines` is a number from 0 to 4"),
                },
                "staff-lines" => match value.parse::<u8>() {
                    Ok(n @ 2..=5) => self.out.score.staff_lines = n,
                    _ => self.error("E004", span, "`staff-lines` is a number from 2 to 5"),
                },
                _ => {}
            }
            self.out.score.headers.push(Header { name: name.to_string(), value: value.to_string(), span });
            i = end;
        }
        body_start
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
        // The closing tag of the literal tag we are in, if any.
        let mut literal: Option<&'static str> = None;

        while i < bytes.len() {
            let c = bytes[i];
            // `$` takes the next character as it stands, everywhere in the text.
            if c == b'$' {
                text.push('$');
                i += 1;
                if let Some(next) = src[i..].chars().next() {
                    text.push(next);
                    i += next.len_utf8();
                }
                continue;
            }
            if let Some(close) = literal {
                if src[i..].starts_with(close) {
                    literal = None;
                    text.push_str(close);
                    i += close.len();
                } else {
                    let ch = src[i..].chars().next().unwrap_or(' ');
                    text.push(ch);
                    i += ch.len_utf8();
                }
                continue;
            }
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
                        self.error("E010", Span::new(open, open + 1), "this `(` is never closed");
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
                    if c == b'<' {
                        literal = LITERAL_TAGS
                            .iter()
                            .find(|(_, open, _)| src[i..].starts_with(open))
                            .map(|(_, _, close)| *close);
                    }
                    let ch = src[i..].chars().next().unwrap_or(' ');
                    text.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        if !text.trim().is_empty() {
            let (plain, translation) = self.text(&text, text_start);
            let lead = text.len() - text.trim_start().len();
            self.error(
                "E024",
                Span::new(text_start + lead, bytes.len()),
                "text after the last notes: every piece of text is followed by `(...)`, which may be empty",
            );
            self.out.score.syllables.push(Syllable {
                text: plain,
                translation,
                starts_word: pending_space,
                notes: None,
                span: Span::new(text_start, bytes.len()),
            });
        }
        for (name, span) in std::mem::take(&mut self.open_tags) {
            if LITERAL_TAGS.iter().any(|(tag, _, _)| *tag == name) {
                self.error("E023", span, format!("`<{name}>` is never closed"));
            } else {
                self.warning("W107", span, format!("`<{name}>` is never closed and lasts to the end"));
            }
        }
        for (name, span) in std::mem::take(&mut self.stretches) {
            self.error("E019", span, format!("the stretch that `[{name}:...{{]` opens is never ended with `[{name}:...}}]`"));
        }
    }

    /// Checks the markup of a syllable's text and splits off its translation.
    fn text(&mut self, raw: &str, at: usize) -> (String, Option<String>) {
        let mut plain = String::new();
        let mut translation: Option<String> = None;
        let mut after_translation = None;
        let mut i = 0;
        let bytes = raw.as_bytes();
        while i < bytes.len() {
            let c = bytes[i];
            if translation.is_some() && after_translation.is_none() && !c.is_ascii_whitespace() {
                after_translation = Some(at + i);
            }
            if c == b'$' {
                let next = raw[i + 1..].chars().next().map_or(0, char::len_utf8);
                plain.push_str(&raw[i..i + 1 + next]);
                i += 1 + next;
                continue;
            }
            let literal = self
                .open_tags
                .last()
                .and_then(|(top, _)| LITERAL_TAGS.iter().find(|(tag, _, _)| tag == top));
            if let Some((_, _, close)) = literal {
                if raw[i..].starts_with(close) {
                    self.open_tags.pop();
                    plain.push_str(close);
                    i += close.len();
                } else {
                    let ch = raw[i..].chars().next().unwrap_or(' ');
                    plain.push(ch);
                    i += ch.len_utf8();
                }
                continue;
            }
            match c {
                b'<' => {
                    let length = raw[i..].find('>');
                    let tag = length.map(|len| &raw[i + 1..i + len]);
                    let known = tag.and_then(|tag| {
                        let name = tag.strip_prefix('/').unwrap_or(tag);
                        let name = name.split(':').next().unwrap_or(name);
                        TEXT_TAGS.iter().copied().find(|t| *t == name)
                    });
                    let (Some(len), Some(tag), Some(name)) = (length, tag, known) else {
                        // Not a tag: it is printed as it stands.
                        if let (Some(len), Some(tag)) = (length, tag) {
                            let word = tag.strip_prefix('/').unwrap_or(tag);
                            if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric()) {
                                self.warning(
                                    "W106",
                                    Span::new(at + i, at + i + len + 1),
                                    format!("`<{tag}>` is not a GABC tag and is printed as text"),
                                );
                            }
                        }
                        plain.push('<');
                        i += 1;
                        continue;
                    };
                    let span = Span::new(at + i, at + i + len + 1);
                    if tag.starts_with('/') {
                        // Tags need not be closed in the order they were opened.
                        match self.open_tags.iter().rposition(|(open, _)| *open == name) {
                            Some(k) => {
                                self.open_tags.remove(k);
                            }
                            None => self.error("E021", span, format!("`</{name}>` has no opening tag")),
                        }
                    } else if !EMPTY_TAGS.contains(&name) {
                        if name == "v" && !self.tex_warned {
                            self.tex_warned = true;
                            self.warning("W101", span, "TeX in `<v>` is not drawn");
                        }
                        self.open_tags.push((name, span));
                    }
                    plain.push_str(&raw[i..i + len + 1]);
                    i += len + 1;
                }
                b'[' => {
                    let Some(len) = raw[i..].find(']') else {
                        self.error("E022", Span::new(at + i, at + i + 1), "this `[` is never closed");
                        plain.push('[');
                        i += 1;
                        continue;
                    };
                    if translation.is_none() {
                        translation = Some(raw[i + 1..i + len].to_string());
                    }
                    i += len + 1;
                }
                b'{' if self.open_tags.iter().any(|(open, _)| *open == "e") => {
                    self.error(
                        "E026",
                        Span::new(at + i, at + i + 1),
                        "`{...}` may not stand inside `<e>...</e>`; write `{<e>...</e>}`",
                    );
                    plain.push('{');
                    i += 1;
                }
                _ => {
                    let ch = raw[i..].chars().next().unwrap_or(' ');
                    plain.push(ch);
                    i += ch.len_utf8();
                }
            }
        }
        if let Some(offset) = after_translation {
            self.error(
                "E027",
                Span::new(offset, at + raw.len()),
                "nothing may follow the translation `[...]` in a syllable's text",
            );
        }
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
        // `!` joins the note before it to the one after it.
        if let Some(k) = group.elements.iter().rposition(|e| e.kind == ElementKind::Join) {
            if !group.elements[k..].iter().any(|e| is_note(&e.kind)) {
                let span = group.elements[k].span;
                self.error("E011", span, "`!` joins two notes, and no note follows it");
            }
        }
        group
    }

    /// A clef at `i`: `c` or `f`, an optional `b`, a line number.
    fn clef(&self, i: usize, end: usize) -> Option<(Clef, usize)> {
        let b = &self.src.as_bytes()[..end];
        let at = |k: usize| b.get(k).copied().unwrap_or(0);
        let do_clef = match at(i) {
            b'c' => true,
            b'f' => false,
            _ => return None,
        };
        let flat = at(i + 1) == b'b';
        let digit = at(i + 1 + usize::from(flat));
        matches!(digit, b'1'..=b'5').then(|| (Clef { do_clef, line: digit - b'0', flat }, 2 + usize::from(flat)))
    }

    fn snippet(&mut self, start: usize, end: usize, out: &mut Vec<Element>) {
        let src = self.src;
        let b = &src.as_bytes()[..end];
        let at = |i: usize| b.get(i).copied().unwrap_or(0);
        let five_lines = self.out.score.staff_lines >= 5;
        let mut i = start;
        // Where each open `{` is.
        let mut voices: Vec<usize> = Vec::new();
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
                // `!/` is a space at which no line is broken. The forms of `/` follow it too.
                b'/' | b'!' if c == b'/' || at(i + 1) == b'/' => {
                    let no_break = c == b'!';
                    i += 1 + usize::from(no_break);
                    let mut space = match at(i) {
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
                        _ if no_break => Space::SmallNoBreak,
                        _ => Space::Small,
                    };
                    // A size in brackets may follow any of them: `/[-0.5]`. Other brackets
                    // are an attribute of the note before the space.
                    if at(i) == b'[' && matches!(at(i + 1), b'0'..=b'9' | b'-' | b'+' | b'.') {
                        match src[i..end].find(']') {
                            Some(len) => i += len + 1,
                            None => {
                                self.error("E014", Span::new(i, i + 1), "this `[` is never closed");
                                i = end;
                            }
                        }
                        space = Space::Sized;
                    }
                    Some(ElementKind::Space(space))
                }
                b'!' => {
                    i += 1;
                    Some(ElementKind::Join)
                }
                b'@' => {
                    i += 1;
                    match at(i) {
                        // `@[...]`: the notes in the brackets are fused. They may carry
                        // attributes, which have brackets of their own.
                        b'[' => match matching_bracket(&src[i..end]) {
                            Some(len) => {
                                out.push(Element { kind: ElementKind::At, span: Span::new(from, i) });
                                self.snippet(i + 1, i + len, out);
                                i += len + 1;
                                None
                            }
                            None => {
                                self.error("E014", Span::new(i, i + 1), "this `[` is never closed");
                                i = end;
                                None
                            }
                        },
                        next if pitch_of(next).is_some() || next == b'-' => Some(ElementKind::At),
                        _ => {
                            self.error("E011", Span::new(from, i), "`@` stands before a note or before `[...]`");
                            None
                        }
                    }
                }
                b'`' | b',' | b'^' | b';' | b':' => {
                    i += 1;
                    let next = at(i);
                    let bar = match c {
                        b',' | b';' if matches!(next, b'1'..=b'8') => {
                            i += 1;
                            let n = next - b'0';
                            if n > 6 && !five_lines {
                                self.error(
                                    "E016",
                                    Span::new(from, i),
                                    format!("the bar `;{n}` needs a staff of five lines (`staff-lines: 5;`)"),
                                );
                            }
                            Bar::Dominican(n)
                        }
                        b'`' | b'^' | b',' => {
                            let high = next == b'0';
                            i += usize::from(high);
                            match c {
                                b'`' => Bar::Virgula { high },
                                b'^' => Bar::Minimis { high },
                                _ => Bar::Minima { high },
                            }
                        }
                        b';' => Bar::Minor,
                        _ if next == b':' => {
                            i += 1;
                            Bar::Finalis
                        }
                        _ if next == b'?' => {
                            i += 1;
                            Bar::MaiorDotted
                        }
                        _ => Bar::Maior,
                    };
                    // Signs on the bar.
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
                        if matches!(at(i), b'+' | b'-') {
                            i += 1;
                        }
                        Some(ElementKind::LineBreak { justified: c == b'z' })
                    }
                }
                b']' => {
                    i += 1;
                    self.warning("W108", Span::new(from, i), "this `]` closes nothing and is ignored");
                    None
                }
                b'[' => match src[i..end].find(']') {
                    Some(len) => {
                        let inner = &src[i + 1..i + len];
                        let span = Span::new(i, i + len + 1);
                        let name_end = inner.find([':', '{', '}']).unwrap_or(inner.len());
                        let name = &inner[..name_end];
                        let value = inner[name_end..].strip_prefix(':').unwrap_or(&inner[name_end..]);
                        if !ATTRIBUTES.contains(&name) {
                            self.error("E012", span, format!("`[{name}]` is not a known attribute"));
                        } else if STRETCHES.contains(&name) {
                            if value.contains('{') {
                                self.stretches.push((name.to_string(), span));
                            } else if value.contains('}') {
                                match self.stretches.iter().rposition(|(open, _)| open == name) {
                                    Some(k) => {
                                        self.stretches.remove(k);
                                    }
                                    None => self.error(
                                        "E019",
                                        span,
                                        format!("`[{inner}]` ends a stretch that no `[{name}:...{{]` opened"),
                                    ),
                                }
                            }
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
                b'<' if src[i..end].starts_with("<nlba>") => {
                    i += 6;
                    Some(ElementKind::NoBreak { start: true })
                }
                b'<' if src[i..end].starts_with("</nlba>") => {
                    i += 7;
                    Some(ElementKind::NoBreak { start: false })
                }
                b'{' => {
                    i += 1;
                    voices.push(from);
                    Some(ElementKind::VoiceStart)
                }
                b'}' => {
                    i += 1;
                    if voices.pop().is_none() {
                        self.warning("W109", Span::new(from, i), "this `}` closes no `{`");
                    }
                    Some(ElementKind::VoiceEnd)
                }
                // A sign that does not follow its note directly. The note itself reads the
                // signs that do.
                b'.' | b'\'' | b'_' => {
                    i += 1;
                    let digits = if c == b'_' { b'0'..=b'5' } else { b'0'..=b'1' };
                    while i < end && digits.contains(&b[i]) {
                        i += 1;
                    }
                    let sign = c as char;
                    let before = out.iter().rposition(|e| !matches!(e.kind, ElementKind::Attribute { .. }));
                    match before {
                        // After an attribute it still belongs to the note: `d_[oh:h].`
                        Some(k) if is_note(&out[k].kind) => {
                            if let ElementKind::Note(note) = &mut out[k].kind {
                                match c {
                                    b'.' => note.mora = (note.mora + 1).min(2),
                                    b'_' => note.episema = note.episema.saturating_add(1),
                                    _ => note.ictus = true,
                                }
                                note.raw_modifiers.push_str(&src[from..i]);
                            }
                        }
                        // After a space or `!`, Gregorio drops a dot or a vertical episema.
                        Some(_) if c != b'_' && out.iter().any(|e| is_note(&e.kind)) => self.warning(
                            "W108",
                            Span::new(from, i),
                            format!("this `{sign}` follows no note directly and is ignored"),
                        ),
                        _ => self.error("E011", Span::new(from, i), format!("`{sign}` needs a note before it")),
                    }
                    None
                }
                b'c' | b'f' if self.clef(i, end).is_some() => {
                    let (clef, len) = self.clef(i, end).unwrap_or((Clef { do_clef: true, line: 4, flat: false }, 2));
                    i += len;
                    let lines = self.out.score.staff_lines;
                    if clef.line > lines {
                        self.error(
                            "E013",
                            Span::new(from, i),
                            format!("a clef on line {} does not fit a staff of {lines} lines", clef.line),
                        );
                    }
                    // `c4@f2`: two clefs at once.
                    match (at(i) == b'@').then(|| self.clef(i + 1, end)).flatten() {
                        Some((second, len)) => {
                            out.push(Element { kind: ElementKind::Clef(clef), span: Span::new(from, i) });
                            i += 1 + len;
                            Some(ElementKind::SecondaryClef(second))
                        }
                        None => Some(ElementKind::Clef(clef)),
                    }
                }
                _ => {
                    let initio = c == b'-';
                    let letter_at = i + usize::from(initio);
                    let letter = at(letter_at);
                    match pitch_of(letter).filter(|_| letter_at < end) {
                        Some(pitch) => {
                            i = letter_at + 1;
                            if pitch > 12 && !five_lines {
                                self.error(
                                    "E016",
                                    Span::new(letter_at, i),
                                    format!(
                                        "the pitch `{}` needs a staff of five lines (`staff-lines: 5;`)",
                                        letter as char
                                    ),
                                );
                            }
                            let kind = self.note(pitch, letter.is_ascii_uppercase(), initio, &mut i, end);
                            // `__` lies over this note and the one before it, and so on.
                            let run = out
                                .iter()
                                .rev()
                                .filter(|e| !matches!(e.kind, ElementKind::Attribute { .. }))
                                .take_while(|e| is_note(&e.kind))
                                .count()
                                + 1;
                            if self.episemas > run {
                                self.error(
                                    "E017",
                                    Span::new(from, i),
                                    format!(
                                        "{} `_` need {} notes in a row to lie over, and only {run} stand here",
                                        self.episemas, self.episemas
                                    ),
                                );
                            }
                            Some(kind)
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
        for at in voices {
            self.warning("W109", Span::new(at, at + 1), "this `{` is never closed");
        }
    }

    /// Reads the modifiers after a pitch letter. `i` is just past the letter. Each modifier
    /// sets what it stands for and the last one wins: `gxw` is a quilisma, not a flat.
    fn note(&mut self, pitch: u8, upper: bool, initio: bool, i: &mut usize, end: usize) -> ElementKind {
        let src = self.src;
        let b = src.as_bytes();
        let start = *i;
        let at = |k: usize| if k < end { b[k] } else { 0 };
        let mut note = Note {
            pitch,
            shape: if upper { Shape::Inclinatum } else { Shape::Punctum },
            initio_debilis: initio,
            ..Note::default()
        };
        // Set while the letter stands for the height of an accidental.
        let mut accidental: Option<(AccidentalKind, bool, bool)> = None;
        let mut episemas = 0usize;
        // The lean of a diamond note.
        if upper && matches!(at(*i), b'0'..=b'2') {
            *i += 1;
        }
        loop {
            let c = at(*i);
            let shape_before = note.shape;
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
                b'=' => {
                    *i += 1;
                    note.shape = Shape::Linea;
                }
                b'r' => {
                    *i += 1;
                    match at(*i) {
                        b'0' => {
                            *i += 1;
                            note.cavum = true;
                            note.shape = Shape::LineaPunctum;
                        }
                        // r1 to r8 put a sign above the note and leave its shape alone.
                        b'1'..=b'8' => *i += 1,
                        _ => note.cavum = true,
                    }
                }
                b'R' => {
                    *i += 1;
                    note.shape = Shape::LineaPunctum;
                }
                b'~' => {
                    *i += 1;
                    note.liquescence = Liquescence::Deminutus;
                }
                // `<nlba>` and `</nlba>` may stand among the notes.
                b'<' if !src[*i..end].starts_with("<nlba>") && !src[*i..end].starts_with("</nlba>") => {
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
                    // `_0` and `_1` put the stroke below or above this note. Without them
                    // each further `_` reaches one note further back.
                    if !matches!(at(*i), b'0' | b'1') {
                        episemas += 1;
                    }
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
                    *i += 1;
                    let soft = c == b'#' && at(*i) == b'#';
                    *i += usize::from(soft);
                    let kind = match c {
                        b'x' => AccidentalKind::Flat,
                        b'y' => AccidentalKind::Natural,
                        _ => AccidentalKind::Sharp,
                    };
                    accidental = Some((kind, false, soft));
                }
                b'?' if accidental.is_some() => {
                    *i += 1;
                    if let Some(a) = &mut accidental {
                        a.1 = true;
                    }
                }
                b'+' => {
                    *i += 1;
                    self.episemas = 0;
                    return ElementKind::Custos(Some(pitch));
                }
                _ => break,
            }
            // A shape after `x` makes it a note again.
            if note.shape != shape_before || matches!(c, b'r' | b'R') {
                accidental = None;
            }
        }
        self.episemas = episemas;
        if let Some((kind, cautionary, soft)) = accidental {
            return ElementKind::Accidental { pitch, kind, cautionary, soft };
        }
        note.raw_modifiers = src[start..*i].to_string();
        ElementKind::Note(note)
    }

    /// Gregorio does not take a first syllable that holds only a clef change or only a line
    /// break. The first syllable is the one after the initial clef, and syllables that are
    /// empty altogether do not count.
    fn first_syllable(&mut self) {
        let syllables = &self.out.score.syllables;
        let only = |s: &Syllable, f: fn(&ElementKind) -> bool| {
            s.notes.as_ref().is_some_and(|g| !g.elements.is_empty() && g.elements.iter().all(|e| f(&e.kind)))
        };
        let is_clef = |k: &ElementKind| matches!(k, ElementKind::Clef(_) | ElementKind::SecondaryClef(_));
        let is_break = |k: &ElementKind| matches!(k, ElementKind::LineBreak { .. });
        // The initial clef is the very first group, if it holds a clef and nothing else.
        let skip = usize::from(syllables.first().is_some_and(|s| only(s, is_clef)));
        let first = syllables.iter().skip(skip).find(|s| {
            !s.text.is_empty() || s.notes.as_ref().is_some_and(|g| !g.elements.is_empty() || !g.nabc.is_empty())
        });
        let Some(first) = first else { return };
        let span = first.span;
        if only(first, is_clef) {
            self.error(
                "E018",
                span,
                "the first syllable may not be a clef change on its own; put the clef into the first `(...)`",
            );
        } else if only(first, is_break) {
            self.error("E018", span, "the first syllable may not be a line break on its own");
        }
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

    fn notes(src: &str) -> Vec<Note> {
        kinds(src)
            .into_iter()
            .filter_map(|e| match e {
                ElementKind::Note(n) => Some(n),
                _ => None,
            })
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
    fn a_header_value_runs_to_its_semicolon() {
        // Over line ends too: a line without `;` swallows the next entry.
        let parsed = parse("name: Angelus\nbook: Tomus I;\nmode: 5;;\ntext: a\nb;;\n%%\n(c4) A(f)");
        assert_eq!(parsed.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(), ["W104"]);
        let headers: Vec<(&str, &str)> =
            parsed.score.headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect();
        assert_eq!(headers, [("name", "Angelus\nbook: Tomus I"), ("mode", "5"), ("text", "a\nb")]);
        assert_eq!(codes("name: x\n%%\n(c4) A(f)"), ["E003"]);
        assert_eq!(codes("name x\n%%\n(c4) A(f)"), ["E002"]);
        assert!(parse("a: b; odd name.x: 12;\n%%\n(c4) A(f)").diagnostics.is_empty());
        assert_eq!(codes("name: x;\n(c4) A(f)"), ["E001"]);
        assert_eq!(codes("staff-lines: 7;\n%%\n(c4) A(f)"), ["E004"]);
    }

    #[test]
    fn clefs_notes_and_spaces() {
        let k = kinds("%%\n(cb3) A(f) (f3) (fgwh/ixi//G'F_E. g//[-0.5]h) (z0) (g+) (z) (c4@f2)");
        assert!(matches!(k[0], ElementKind::Clef(Clef { do_clef: true, line: 3, flat: true })));
        assert!(k.contains(&ElementKind::Clef(Clef { do_clef: false, line: 3, flat: false })));
        assert!(k.contains(&ElementKind::SecondaryClef(Clef { do_clef: false, line: 2, flat: false })));
        let n = notes("%%\n(c4) A(fgwh/ixi//G'F_E.)");
        assert_eq!(n[0].pitch, 5);
        assert_eq!(n[1].shape, Shape::Quilisma);
        assert_eq!(n[4].shape, Shape::Inclinatum);
        assert!(n[4].ictus);
        assert_eq!(n[5].episema, 1);
        assert_eq!(n[6].mora, 1);
        assert!(k.contains(&ElementKind::Accidental {
            pitch: 8,
            kind: AccidentalKind::Flat,
            cautionary: false,
            soft: false
        }));
        for space in [Space::Small, Space::Medium, Space::Large, Space::Sized] {
            assert!(k.contains(&ElementKind::Space(space)), "{space:?}");
        }
        assert!(k.contains(&ElementKind::Custos(None)));
        assert!(k.contains(&ElementKind::Custos(Some(6))));
        assert!(k.contains(&ElementKind::LineBreak { justified: true }));
        // A clef may stand among the notes.
        kinds("%%\n(c4) A(f c3 g)");
        assert_eq!(codes("%%\n(c5) A(f)"), ["E013"]);
        assert_eq!(codes("staff-lines: 5;\n%%\n(c5) A(f)"), [] as [&str; 0]);
    }

    #[test]
    fn bars() {
        let k = kinds("%%\n(c4) A(f) (`) (`0) (^) (^0) (,) (,0) (;) (;3) (,3) (:) (:?) (::) (::') (;_) B(g';h)");
        let bars: Vec<Bar> = k
            .iter()
            .filter_map(|e| if let ElementKind::Bar(b) = e { Some(*b) } else { None })
            .collect();
        assert_eq!(
            bars,
            [
                Bar::Virgula { high: false },
                Bar::Virgula { high: true },
                Bar::Minimis { high: false },
                Bar::Minimis { high: true },
                Bar::Minima { high: false },
                Bar::Minima { high: true },
                Bar::Minor,
                Bar::Dominican(3),
                Bar::Dominican(3),
                Bar::Maior,
                Bar::MaiorDotted,
                Bar::Finalis,
                Bar::Finalis,
                Bar::Minor,
                Bar::Minor,
            ]
        );
        assert_eq!(codes("%%\n(c4) A(f) (;7)"), ["E016"]);
        assert_eq!(codes("staff-lines: 5;\n%%\n(c4) A(n) (;7) B(p)"), [] as [&str; 0]);
        assert_eq!(codes("%%\n(c4) A(f) (;9)"), ["E011"]);
        assert_eq!(codes("%%\n(c4) A(n)"), ["E016"]);
    }

    #[test]
    fn shapes_and_accidentals() {
        let n = notes("%%\n(c4) A(fv) B(fV) C(fo1) D(fO) E(fw) F(fr) G(fR) H(fr0) I(f=) J(fss) K(F1) L(-f) M(f~) N(fr3) O(fvvv)");
        let shapes: Vec<Shape> = n.iter().map(|n| n.shape).collect();
        assert_eq!(
            shapes,
            [
                Shape::Virga,
                Shape::VirgaReversa,
                Shape::Oriscus,
                Shape::OriscusScapus,
                Shape::Quilisma,
                Shape::Punctum,
                Shape::LineaPunctum,
                Shape::LineaPunctum,
                Shape::Linea,
                Shape::Distropha,
                Shape::Inclinatum,
                Shape::Punctum,
                Shape::Punctum,
                Shape::Punctum,
                Shape::Trivirga,
            ]
        );
        assert!(n[5].cavum && n[7].cavum && !n[6].cavum);
        assert!(n[11].initio_debilis);
        assert_eq!(n[12].liquescence, Liquescence::Deminutus);

        let accidental = |src: &str| kinds(src).into_iter().find(|k| matches!(k, ElementKind::Accidental { .. }));
        let flat = |cautionary, soft| {
            Some(ElementKind::Accidental { pitch: 8, kind: AccidentalKind::Flat, cautionary, soft })
        };
        assert_eq!(accidental("%%\n(c4) A(ixi)"), flat(false, false));
        assert_eq!(accidental("%%\n(c4) A(ix?i)"), flat(true, false));
        // Signs after the letter stay with the accidental; a second `x` changes nothing.
        assert_eq!(accidental("%%\n(c4) A(ix_[oh:h]iv)"), flat(false, false));
        assert_eq!(accidental("%%\n(c4) A(ixxi)"), flat(false, false));
        assert_eq!(
            accidental("%%\n(c4) A(i##i)"),
            Some(ElementKind::Accidental { pitch: 8, kind: AccidentalKind::Sharp, cautionary: false, soft: true })
        );
        // The last modifier wins: this is a quilisma on g, no flat.
        assert_eq!(accidental("%%\n(c4) A(gxwf)"), None);
        assert_eq!(notes("%%\n(c4) A(gxwf)")[0].shape, Shape::Quilisma);
    }

    #[test]
    fn signs_and_what_they_need() {
        // A sign after an attribute belongs to the note before it.
        let n = notes("%%\n(c4) B(d_[oh:h].)");
        assert_eq!((n[0].mora, n[0].episema), (1, 1));
        // After a space or `!`, a dot or a vertical episema is dropped.
        assert_eq!(codes("%%\n(c4) A(fhf/.gv)"), ["W108"]);
        assert_eq!(codes("%%\n(c4) A(dh!'iv)"), ["W108"]);
        assert_eq!(codes("%%\n(c4) A(gh~])"), ["W108"]);
        // With no note before them, and for the other signs, it is an error.
        for group in ["(.)", "(')", "(_)", "(f/_)", "(f/~)", "(f/v)", "(f/x)", "(~)", "(g0h)", "(f-)", "(--f)"] {
            assert_eq!(codes(&format!("%%\n(c4) A{group}")), ["E011"], "{group}");
        }
        // `__` lies over two notes that stand together.
        kinds("%%\n(c4) A(gh__) B(g_h_) C(g_[oh:h]h__) D(gxh__) E(g_0_1h_)");
        for group in ["(g/h__)", "(gh___)", "(g!h__)"] {
            assert_eq!(codes(&format!("%%\n(c4) A{group}")), ["E017"], "{group}");
        }
    }

    #[test]
    fn joins_fusion_and_voices() {
        kinds("%%\n(c4) A(!f) B(f!/g) C(g!!h) D(f@g) E(@fg) F(f@[fdfdc~]) G(fgh/!ijk)");
        // `!/` is a space, so nothing has to follow it, and a fused group may hold attributes.
        kinds("%%\n(c4) A(ghg!//::) B(@[[uh:m{]d_e_0f][uh}]) C(f!/[-0.5]g) D(f!/0g)");
        assert!(kinds("%%\n(c4) A(f!/g)").contains(&ElementKind::Space(Space::SmallNoBreak)));
        for group in ["(g!)", "(!)", "(hV!g!h!)"] {
            assert_eq!(codes(&format!("%%\n(c4) A{group}")), ["E011"], "{group}");
        }
        assert_eq!(codes("%%\n(c4) A(g@@h)"), ["E011"]);
        assert_eq!(codes("%%\n(c4) A(f@)"), ["E011"]);
        // A second voice in braces. Gregorio takes a brace that is left open.
        let k = kinds("%%\n(c4) A(ff/[1]{ix}fdg)");
        assert!(matches!(k[4], ElementKind::VoiceStart), "{k:?}");
        assert!(matches!(k[5], ElementKind::Accidental { .. }), "{k:?}");
        assert!(matches!(k[6], ElementKind::VoiceEnd), "{k:?}");
        assert_eq!(codes("%%\n(c4) A(f{g)"), ["W109"]);
        assert_eq!(codes("%%\n(c4) A(f}g)"), ["W109"]);
        let k = kinds("%%\n(c4) A(f<nlba>gh</nlba>i)");
        assert!(k.contains(&ElementKind::NoBreak { start: true }));
        assert!(k.contains(&ElementKind::NoBreak { start: false }));
    }

    #[test]
    fn attributes_and_stretches() {
        for name in ATTRIBUTES {
            kinds(&format!("%%\n(c4) A(f[{name}:1]g)"));
        }
        for group in ["(f[zz:1])", "(f[nm:1])", "(f[ubb:1;2mm])", "(f[-0.5]g)"] {
            assert_eq!(codes(&format!("%%\n(c4) A{group}")), ["E012"], "{group}");
        }
        let k = kinds("%%\n(c4) A(g[oh:m{]hf[oh}])");
        assert!(k.contains(&ElementKind::Attribute { name: "oh".into(), value: "m{".into() }));
        assert!(k.contains(&ElementKind::Attribute { name: "oh".into(), value: "}".into() }));
        // A stretch may end in a later syllable, but it must end.
        kinds("%%\n(c4) A(f[ob:1{]) B(g[ob:1}])");
        kinds("%%\n(c4) A([oll:1{1]lvKJ[oll:}])");
        // After a space too, where brackets with a number would be the size of the space.
        kinds("%%\n(c4) A(e/![ull:0{1]fc/!cb[ull:}]//f)");
        assert_eq!(codes("%%\n(c4) A(f[ob:1{]g)"), ["E019"]);
        assert_eq!(codes("%%\n(c4) A(f[oh}])"), ["E019"]);
        // TeX in a value may hold braces of its own.
        kinds("%%\n(c4) A(f[nv:\\hbox{x}]g)");
    }

    #[test]
    fn what_is_wrong_is_reported_and_the_rest_is_still_read() {
        assert_eq!(codes("%%\n(c4) A(f"), ["E010", "E024"]);
        assert_eq!(codes("%%\n(c4) A(f$g)"), ["E011"]);
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
        // The translation ends the syllable's text.
        assert_eq!(codes("%%\n(c4) A[x]<i>B</i>(f)"), ["E027"]);
        assert_eq!(codes("%%\n(c4) B[tr]C[tr2](f)"), ["E027"]);
        assert_eq!(codes("%%\n(c4) A[open(f)"), ["E022"]);
        assert_eq!(codes("%%\n(c4) n<e>{u}m</e>(j)"), ["E026"]);
        assert!(parse("%%\n(c4) ct{<e>o</e>}(d) A{b}{c}(f) A{b(f) Ab}c(f)").is_valid());
    }

    #[test]
    fn tags_stay_open_and_need_no_order() {
        let parsed = parse("%%\n(c4) <eu>E(j) u(jji) o(hg) u(h) a(i) e.</eu>(gh..) (::)");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(parse("%%\n(c4) <b>A<i>B</b>C</i>(f)").diagnostics.is_empty());
        // Left open, a style lasts to the end.
        assert_eq!(codes("%%\n(c4) <i>E(j) u(j)"), ["W107"]);
        assert_eq!(codes("%%\n(c4) E(j) u</i>(j)"), ["E021"]);
        // What is not a tag is text.
        assert_eq!(codes("%%\n(c4) <foo>x</foo>(g)"), ["W106", "W106"]);
        assert!(parse("%%\n(c4) A(f) x<y(g) <u>z</u>(h) <(,) >(,)").is_valid());
        assert!(parse("%%\n(c4) A(f) <pr>B(g) <pr:1>C(h) <clear>D(i)").diagnostics.is_empty());
    }

    #[test]
    fn literal_tags_and_the_escape() {
        // In `<v>`, `<sp>` and `<alt>` a parenthesis is text, and so are `[`, `<` and `%`.
        let parsed = parse("%%\n(c4) A(f) <i><v>(</v>Non repetitur.<v>)</v></i>(g) <v>a<b %</v>(h)");
        assert_eq!(parsed.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>(), ["W101"]);
        assert_eq!(parsed.score.syllables.len(), 4);
        assert_eq!(parsed.score.syllables[2].text, "<i><v>(</v>Non repetitur.<v>)</v></i>");
        assert!(parse("%%\n(c4) <alt>All (standing):</alt>O(f) <sp>[</sp>(g) <sp>(</sp>(h)").is_valid());
        let parsed = parse("%%\n(c4) HIC[<v>\\textup{(Haec)}</v>](f)");
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.score.syllables[1].text, "HIC");
        // `$` takes the next character as it stands.
        let parsed = parse("%%\n(c4) A$(x$)(f) a$<i$>b(g)");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.score.syllables[1].text, "A$(x$)");
        // So `$<` before `/v>` leaves the `<v>` open, and a literal tag must be closed.
        assert_eq!(codes("%%\n(c4) <v>$\\star$</v>(,) B(g)"), ["W101", "E024", "E023"]);
        assert_eq!(codes("%%\n(c4) <sp>open A(f)"), ["E024", "E023"]);
        // Parentheses are not text inside the other tags.
        assert!(!parse("%%\n(c4) <c>x(y)</c>O(f)").is_valid());
    }

    #[test]
    fn text_needs_its_notes() {
        assert_eq!(codes("%%\n(c4) A(f) B"), ["E024"]);
        assert_eq!(codes("%%\n(c4) <eu>A(f) B(g)</eu>"), ["E024"]);
        assert!(parse("%%\n(c4) A(f) B() \n").is_valid());
        // A stray `)` is text.
        assert!(parse("%%\n(c4) A(f) B(g))C(h)").is_valid());
    }

    #[test]
    fn the_first_syllable() {
        for body in ["() (c4) A(f)", "(c3)(c3) A(f)", "(c4) (c3) A(f)", "(c4) A(c3) B(f)", "(c4) *(c3) A(f)", "(c4) A(z-) B(g)"] {
            assert_eq!(codes(&format!("%%\n{body}")), ["E018"], "{body}");
        }
        for body in ["(c4) A(f)", "(c4) (c3f) A(g)", "(c4) A(c3f)", "(c4) A(f) (c3) B(g)"] {
            assert!(parse(&format!("%%\n{body}")).is_valid(), "{body}");
        }
        // Text before the initial clef does not make it a clef change.
        assert!(parse("%%\nannotation: Ps.;\n(c3)\nA(f)").is_valid());
        // No clef at all is taken, with a warning.
        assert_eq!(codes("%%\nA(f) B(g)"), ["W103"]);
    }
}
