//! What a score holds, as one line of tokens, from our parser and from Gregorio's dump.
//!
//! The reference run compares the two lines of every score that both parsers accept. A token
//! is close to the GABC it came from:
//!
//! - a clef as written: `c4`, `cb3`, `f3`
//! - a note: its pitch letter and one letter for its kind (`p` punctum, `i` inclinatum,
//!   `v` virga, `V` virga reversa, `s` stropha, `o` oriscus, `O` oriscus scapus,
//!   `w` quilisma, `l` linea, `L` linea punctum, `x` flat, `y` natural, `#` sharp)
//! - a bar as written: `` ` ``, `^`, `,`, `;`, `;3`, `:`, `:?`, `::`, with `0` for the high
//!   forms
//! - `+` for a custos and `z` for a line break
//!
//! Signs, spaces, text and attributes are left out: this is about which notes there are.

use gabc_rs::gabc::{AccidentalKind, Bar, ElementKind, Score, Shape};

/// The tokens of a score as our parser read it.
pub fn ours(score: &Score) -> String {
    let mut out: Vec<String> = Vec::new();
    for syllable in &score.syllables {
        let Some(group) = &syllable.notes else { continue };
        for element in &group.elements {
            match &element.kind {
                ElementKind::Clef(clef) => out.push(format!(
                    "{}{}{}",
                    if clef.do_clef { 'c' } else { 'f' },
                    if clef.flat { "b" } else { "" },
                    clef.line
                )),
                ElementKind::Note(note) => {
                    let (kind, times) = match note.shape {
                        Shape::Punctum | Shape::Quadratum => ('p', 1),
                        Shape::Inclinatum => ('i', 1),
                        Shape::Virga => ('v', 1),
                        Shape::Bivirga => ('v', 2),
                        Shape::Trivirga => ('v', 3),
                        Shape::VirgaReversa => ('V', 1),
                        Shape::Stropha => ('s', 1),
                        Shape::Distropha => ('s', 2),
                        Shape::Tristropha => ('s', 3),
                        Shape::Oriscus => ('o', 1),
                        Shape::OriscusScapus => ('O', 1),
                        Shape::Quilisma | Shape::QuilismaQuadratum => ('w', 1),
                        Shape::Linea => ('l', 1),
                        Shape::LineaPunctum => ('L', 1),
                    };
                    for _ in 0..times {
                        out.push(format!("{}{kind}", pitch_letter(note.pitch)));
                    }
                }
                ElementKind::Accidental { pitch, kind, .. } => {
                    let kind = match kind {
                        AccidentalKind::Flat => 'x',
                        AccidentalKind::Natural => 'y',
                        AccidentalKind::Sharp => '#',
                    };
                    out.push(format!("{}{kind}", pitch_letter(*pitch)));
                }
                ElementKind::Bar(bar) => out.push(match bar {
                    Bar::Virgula { high } => format!("`{}", if *high { "0" } else { "" }),
                    Bar::Minimis { high } => format!("^{}", if *high { "0" } else { "" }),
                    Bar::Minima { high } => format!(",{}", if *high { "0" } else { "" }),
                    Bar::Minor => String::from(";"),
                    Bar::Dominican(n) => format!(";{n}"),
                    Bar::Maior => String::from(":"),
                    Bar::MaiorDotted => String::from(":?"),
                    Bar::Finalis => String::from("::"),
                }),
                ElementKind::Custos(_) => out.push(String::from("+")),
                ElementKind::LineBreak { .. } => out.push(String::from("z")),
                _ => {}
            }
        }
    }
    // Gregorio reads a score that starts without a clef with a do clef on the third line.
    let starts_with_clef = out.first().is_some_and(|t| t.starts_with(['c', 'f']) && t.ends_with(|c: char| c.is_ascii_digit()));
    if !starts_with_clef {
        out.insert(0, String::from("c3"));
    }
    out.join(" ")
}

fn pitch_letter(pitch: u8) -> char {
    match pitch {
        0..=13 => (b'a' + pitch) as char,
        _ => 'p',
    }
}

/// The tokens of a score from the text of `gregorio -F dump`.
pub fn from_dump(dump: &str) -> String {
    #[derive(PartialEq)]
    enum Open {
        Nothing,
        Note,
        Bar,
        Clef,
    }
    let mut out: Vec<String> = Vec::new();
    let mut open = Open::Nothing;
    let mut pitch = '?';
    for line in dump.lines() {
        let line = line.trim();
        let (key, value) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let value = value.trim();
        // `15 (G_PUNCTUM)` -> `G_PUNCTUM`
        let name = value.rsplit_once('(').and_then(|(_, n)| n.strip_suffix(')')).unwrap_or(value);
        match key {
            // A `flatted_key` line may follow it, as it does after a clef change.
            "initial_key" => {
                out.push(name.to_string());
                open = Open::Clef;
            }
            "type" => {
                open = match name {
                    "GRE_NOTE" => Open::Note,
                    "GRE_BAR" => Open::Bar,
                    "GRE_CLEF" => Open::Clef,
                    "GRE_CUSTOS" => {
                        out.push(String::from("+"));
                        Open::Nothing
                    }
                    "GRE_END_OF_LINE" => {
                        out.push(String::from("z"));
                        Open::Nothing
                    }
                    _ => Open::Nothing,
                }
            }
            "pitch" if open == Open::Note => pitch = value.chars().next().unwrap_or('?'),
            "shape" if open == Open::Note => {
                out.push(format!("{pitch}{}", shape_kind(name)));
                open = Open::Nothing;
            }
            "bar" if open == Open::Bar => {
                out.push(bar_token(name));
                open = Open::Nothing;
            }
            "clef" if open == Open::Clef => out.push(name.to_string()),
            // `c3` -> `cb3`
            "flatted_key" if open == Open::Clef && value == "true" => {
                if let Some(clef) = out.last_mut().filter(|c| c.len() == 2) {
                    clef.insert(1, 'b');
                }
            }
            _ => {}
        }
    }
    out.join(" ")
}

/// One letter for the kind of a note, from Gregorio's name of its shape. Gregorio names many
/// forms of one kind (`S_PUNCTUM_INCLINATUM_DESCENDENS`); a name that is not known is kept
/// whole, so that the report shows it.
fn shape_kind(name: &str) -> String {
    let kinds: &[(&str, char)] = &[
        ("S_PUNCTUM_INCLINATUM", 'i'),
        ("S_ORISCUS_SCAPUS", 'O'),
        ("S_ORISCUS", 'o'),
        ("S_QUILISMA", 'w'),
        ("S_VIRGA_REVERSA", 'V'),
        ("S_VIRGA", 'v'),
        ("S_BIVIRGA", 'v'),
        ("S_TRIVIRGA", 'v'),
        ("S_STROPHA", 's'),
        ("S_DISTROPHA", 's'),
        ("S_TRISTROPHA", 's'),
        ("S_LINEA_PUNCTUM", 'L'),
        ("S_LINEA", 'l'),
        ("S_FLAT", 'x'),
        ("S_NATURAL", 'y'),
        ("S_SHARP", '#'),
        ("S_PUNCTUM", 'p'),
    ];
    match kinds.iter().find(|(prefix, _)| name.starts_with(prefix)) {
        Some((_, kind)) => kind.to_string(),
        None => format!("?{name}"),
    }
}

fn bar_token(name: &str) -> String {
    let token = match name {
        "B_VIRGULA" => "`",
        "B_VIRGULA_HIGH" => "`0",
        "B_DIVISIO_MINIMIS" => "^",
        "B_DIVISIO_MINIMIS_HIGH" => "^0",
        "B_DIVISIO_MINIMA" => ",",
        "B_DIVISIO_MINIMA_HIGH" => ",0",
        "B_DIVISIO_MINOR" => ";",
        "B_DIVISIO_MAIOR" => ":",
        "B_DIVISIO_MAIOR_DOTTED" => ":?",
        "B_DIVISIO_FINALIS" => "::",
        other => match other.strip_prefix("B_DIVISIO_MINOR_D") {
            Some(n) => return format!(";{n}"),
            None => return format!("?{other}"),
        },
    };
    token.to_string()
}

/// Where two token lines part, as a cause that scores can be grouped by, with the two
/// tokens. `None` if the lines are equal.
pub fn difference(ours: &str, theirs: &str) -> Option<(String, String)> {
    let mut a = ours.split(' ').filter(|t| !t.is_empty());
    let mut b = theirs.split(' ').filter(|t| !t.is_empty());
    loop {
        let (x, y) = (a.next(), b.next());
        if x == y {
            if x.is_none() {
                return None;
            }
            continue;
        }
        let kind = |t: Option<&str>| match t {
            None => "the end",
            Some("+") => "a custos",
            Some("z") => "a line break",
            Some(t) if t.starts_with(['`', '^', ',', ';', ':']) => "a bar",
            // `c4`, `cb3`: a note's second letter is never a digit.
            Some(t) if t.starts_with(['c', 'f']) && t.ends_with(|c: char| c.is_ascii_digit()) => "a clef",
            Some(_) => "a note",
        };
        let cause = match (x, y) {
            (Some(x), Some(y)) if kind(Some(x)) == "a note" && kind(Some(y)) == "a note" => {
                if x.chars().next() != y.chars().next() {
                    String::from("another pitch")
                } else {
                    format!("kind `{}` here, `{}` there", &x[1..], &y[1..])
                }
            }
            _ => format!("{} here, {} there", kind(x), kind(y)),
        };
        return Some((cause, format!("`{}` / `{}`", x.unwrap_or("-"), y.unwrap_or("-"))));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = "\
 VOICES INFOS
  Voice 1
   initial_key               7 (c4)
 SCORE
   type                      0 (GRE_SYLLABLE)
     character                 A
     type                    3 (GRE_ELEMENT)
       type                  2 (GRE_GLYPH)
       glyph_type            15 (G_PUNCTUM)
         type                   1 (GRE_NOTE)
         pitch                  f
         shape                  1 (S_PUNCTUM)
         type                   1 (GRE_NOTE)
         pitch                  i
         shape                  9 (S_FLAT)
         type                   1 (GRE_NOTE)
         pitch                  h
         shape                  3 (S_PUNCTUM_INCLINATUM_DESCENDENS)
   type                      0 (GRE_SYLLABLE)
     type                    7 (GRE_BAR)
     bar                     9 (B_DIVISIO_MINOR_D3)
   type                      0 (GRE_SYLLABLE)
     type                    4 (GRE_CLEF)
     clef                    5 (c3)
     flatted_key             true
   type                      0 (GRE_SYLLABLE)
     type                    6 (GRE_CUSTOS)
     pitch                   j
     type                    5 (GRE_END_OF_LINE)
     type                    7 (GRE_BAR)
     bar                     5 (B_DIVISIO_FINALIS)
";

    #[test]
    fn a_dump_and_our_parse_give_the_same_line() {
        let theirs = from_dump(DUMP);
        assert_eq!(theirs, "c4 fp ix hi ;3 cb3 + z ::");
        let parsed = gabc_rs::gabc::parse("%%\n(c4) A(fixH) (,3) (cb3) (z0) (z) (::)");
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        assert_eq!(ours(&parsed.score), theirs);
        assert_eq!(difference(&theirs, &theirs), None);
    }

    #[test]
    fn differences_are_named() {
        let cause = |a: &str, b: &str| difference(a, b).map(|d| d.0);
        assert_eq!(cause("c4 fp gp", "c4 fp hp").as_deref(), Some("another pitch"));
        assert_eq!(cause("c4 fp", "c4 fv").as_deref(), Some("kind `p` here, `v` there"));
        assert_eq!(cause("c4 fp ;", "c4 fp").as_deref(), Some("a bar here, the end there"));
        assert_eq!(cause("c4 fp", "c3 fp").as_deref(), Some("a clef here, a clef there"));
        assert_eq!(cause("c4 + fp", "c4 fp").as_deref(), Some("a custos here, a note there"));
        assert_eq!(cause("fp S_X", "fp f?S_X").as_deref(), Some("another pitch"));
    }
}
