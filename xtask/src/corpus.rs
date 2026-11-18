//! `cargo run -p xtask -- corpus gregobase [--oracle gregorio]`: the reference run over the
//! GregoBase library.
//!
//! GregoBase publishes its database as one SQL dump (the transcriptions are CC0). This task
//! downloads the dump once into `target/corpus`, writes every score as a `.gabc` file, checks
//! each one, and writes `target/corpus/gregobase-report.md`. Nothing of the corpus is
//! committed.
//!
//! With `--oracle <program>` every file is also given to that program, which is Gregorio
//! where it is installed (CI). Gregorio is run and never read: what it accepts and rejects is
//! the reference our verdicts are compared with, and the report lists the disagreements.
//! `--verdicts <file.tsv>` takes the oracle's verdicts from the `gregobase-verdicts.tsv` of an
//! earlier run instead, for working on the parser where Gregorio is not installed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use gabc_rs::{gabc, Severity};

use crate::structure;

const DUMP_URL: &str =
    "https://raw.githubusercontent.com/gregorio-project/GregoBase/HEAD/gregobase_online.sql";
const TABLE: &str = "INSERT INTO `gregobase_chants`";

/// Where the reference verdicts come from.
pub enum Oracle<'a> {
    None,
    /// A program to run on every file.
    Program(&'a str),
    /// The verdicts file of an earlier run.
    Saved(&'a Path),
}

pub fn gregobase(root: &Path, oracle: Oracle) -> Result<(), String> {
    let dir = root.join("target/corpus");
    let dump = dir.join("gregobase_online.sql");
    let out = dir.join("gregobase");
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    if !dump.exists() {
        println!("downloading {DUMP_URL}");
        let status = Command::new("curl")
            .args(["--location", "--fail", "--silent", "--show-error", "--output"])
            .arg(&dump)
            .arg(DUMP_URL)
            .status()
            .map_err(|e| format!("curl: {e}"))?;
        if !status.success() {
            return Err(format!("curl failed for {DUMP_URL}"));
        }
    }
    let sql = std::fs::read_to_string(&dump).map_err(|e| format!("{}: {e}", dump.display()))?;

    let mut report = Report { with_oracle: !matches!(oracle, Oracle::None), ..Report::default() };
    let mut files = Vec::new();
    for row in rows(&sql)? {
        let Some(score) = score_file(&row) else {
            report.without_gabc += 1;
            continue;
        };
        let path = out.join(format!("{:05}.gabc", row.id));
        std::fs::write(&path, &score.source).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push((row.id, path, score));
    }
    let verdicts: Vec<Option<OracleVerdict>> = match &oracle {
        Oracle::Program(program) => ask_oracle(program, &files)?.into_iter().map(Some).collect(),
        Oracle::Saved(path) => {
            let saved = saved_verdicts(path)?;
            files.iter().map(|(id, _, _)| saved.get(id).cloned()).collect()
        }
        Oracle::None => Vec::new(),
    };
    for (n, (id, _, score)) in files.iter().enumerate() {
        report.add(*id, score, verdicts.get(n).cloned().flatten());
    }

    let text = report.render();
    let path = dir.join("gregobase-report.md");
    std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    let tsv = dir.join("gregobase-verdicts.tsv");
    std::fs::write(&tsv, report.verdicts_tsv()).map_err(|e| format!("{}: {e}", tsv.display()))?;
    if matches!(oracle, Oracle::Program(_)) {
        let tsv = dir.join("gregobase-structure.tsv");
        std::fs::write(&tsv, report.structure_tsv()).map_err(|e| format!("{}: {e}", tsv.display()))?;
        let tsv = dir.join("gregobase-centres.tsv");
        std::fs::write(&tsv, report.centres_tsv()).map_err(|e| format!("{}: {e}", tsv.display()))?;
    }
    print!("{}", text.split("\n## Scores").next().unwrap_or(&text));
    println!("\nreport: {}", path.display());
    Ok(())
}

/// What the reference program said about one file.
#[derive(Clone)]
struct OracleVerdict {
    accepted: bool,
    /// Its first line of messages, if it had any.
    message: String,
    /// What it read, as a line of tokens (see [`crate::structure`]), if it accepted the file.
    structure: Option<String>,
    /// The text of each syllable and the letters it centres under the first note.
    centres: Option<Vec<(String, String)>>,
}

/// Reads the `id`, `oracle` and `oracle_message` columns of a verdicts file.
fn saved_verdicts(path: &Path) -> Result<BTreeMap<u32, OracleVerdict>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    for line in text.lines().skip(1) {
        let fields: Vec<&str> = line.split('\t').collect();
        if let (Some(id), Some(verdict)) = (fields.first().and_then(|f| f.parse().ok()), fields.get(3)) {
            if !verdict.is_empty() {
                let message = fields.get(4).copied().unwrap_or("").to_string();
                out.insert(id, OracleVerdict { accepted: *verdict == "accept", message, structure: None, centres: None });
            }
        }
    }
    // The token lines of the same run, if they lie next to the verdicts.
    if let Ok(text) = std::fs::read_to_string(path.with_file_name("gregobase-structure.tsv")) {
        for line in text.lines() {
            if let Some((id, tokens)) = line.split_once('\t') {
                if let Some(verdict) = id.parse().ok().and_then(|id: u32| out.get_mut(&id)) {
                    verdict.structure = Some(tokens.to_string());
                }
            }
        }
    }
    Ok(out)
}

/// Runs `program -S <file>` on every file, on all cores. The program is expected to write
/// its output to stdout, its messages to stderr, and to fail on input it does not accept.
fn ask_oracle(program: &str, files: &[(u32, PathBuf, ScoreFile)]) -> Result<Vec<OracleVerdict>, String> {
    Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| format!("the oracle `{program}` does not run: {e}"))?;
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
    let chunk = files.len().div_ceil(threads).max(1);
    let verdicts = std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|(_, path, _)| {
                            let run = Command::new(program).arg("-S").arg(path).output();
                            match run {
                                Ok(out) => {
                                    let messages = String::from_utf8_lossy(&out.stderr);
                                    // A line that says "error" counts even if the program
                                    // went on and exited with success.
                                    let line = messages
                                        .lines()
                                        .find(|l| l.contains("error"))
                                        .or_else(|| messages.lines().find(|l| !l.trim().is_empty()))
                                        .unwrap_or("")
                                        .trim()
                                        .to_string();
                                    let accepted = out.status.success() && !messages.contains("error");
                                    // What it read, from its structural dump.
                                    let dump = accepted
                                        .then(|| Command::new(program).args(["-F", "dump", "-S"]).arg(path).output().ok())
                                        .flatten()
                                        .map(|dump| String::from_utf8_lossy(&dump.stdout).into_owned());
                                    let structure = dump.as_deref().map(structure::from_dump);
                                    let centres = dump.as_deref().map(structure::centres_from_dump);
                                    OracleVerdict { accepted, message: line, structure, centres }
                                }
                                Err(e) => OracleVerdict {
                                    accepted: false,
                                    message: format!("did not run: {e}"),
                                    structure: None,
                                    centres: None,
                                },
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect::<Vec<_>>()
    });
    if verdicts.len() != files.len() {
        return Err(String::from("an oracle thread died"));
    }
    Ok(verdicts)
}

/// One row of the chants table, with the columns the `.gabc` file is made of.
struct Row {
    id: u32,
    incipit: String,
    office_part: String,
    mode: String,
    transcriber: String,
    /// JSON: a string, or a list of `["gabc" | "tex", text, {...}]` segments.
    gabc: Option<String>,
}

struct ScoreFile {
    source: String,
    /// TeX segments stood between the GABC segments and were left out.
    dropped_tex: bool,
}

/// The `.gabc` file of a row, or `None` if the row holds no notation.
fn score_file(row: &Row) -> Option<ScoreFile> {
    let json: serde_json::Value = serde_json::from_str(row.gabc.as_deref()?).ok()?;
    let mut dropped_tex = false;
    let body = match &json {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(segments) => {
            let mut body = String::new();
            for segment in segments {
                let kind = segment.get(0).and_then(|v| v.as_str()).unwrap_or("");
                let text = segment.get(1).and_then(|v| v.as_str()).unwrap_or("");
                if kind == "gabc" {
                    body.push_str(text);
                    body.push('\n');
                } else {
                    dropped_tex = true;
                }
            }
            body
        }
        _ => return None,
    };
    let body = body.replace("\r\n", "\n");
    if body.trim().is_empty() {
        return None;
    }
    // Some transcribers stored a whole file, header included.
    if body.lines().any(|line| line.trim() == "%%") {
        return Some(ScoreFile { source: body, dropped_tex });
    }
    let mut source = String::new();
    let mut header = |name: &str, value: &str| {
        let value = value.trim().replace(';', ",");
        if !value.is_empty() {
            let _ = writeln!(source, "{name}: {value};");
        }
    };
    header("name", &row.incipit);
    header("office-part", &row.office_part);
    header("mode", &row.mode);
    header("transcriber", &row.transcriber);
    // The database has no column for it: a score with NABC neumes gets the header from the
    // `|` in its notes.
    if has_nabc(&body) {
        header("nabc-lines", "1");
    }
    source.push_str("%%\n");
    source.push_str(&body);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    Some(ScoreFile { source, dropped_tex })
}

/// True if a `|` stands between a pair of parentheses.
fn has_nabc(body: &str) -> bool {
    let mut inside = false;
    for c in body.chars() {
        match c {
            '(' => inside = true,
            ')' => inside = false,
            '|' if inside => return true,
            _ => {}
        }
    }
    false
}

/// What drawing a score gave.
enum Drawn {
    /// Lines, and all of them right.
    Fine,
    /// The layout code panicked.
    Panicked,
    /// The number of note heads drawn is not the number in the score.
    NotesLost,
    /// Something lies beyond the right end of its line.
    TooWide,
}

/// Lays the score out at a common width and checks what comes back: every note head is
/// drawn once, and nothing lies beyond the end of its line.
fn draw(score: &gabc::Score) -> Drawn {
    use gabc_rs::glyphs::Glyph;
    let width = 18.0;
    let options = gabc_rs::layout::Options { width, ..Default::default() };
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gabc_rs::layout::engrave(score, &options)));
    let Ok(engraving) = attempt else {
        return Drawn::Panicked;
    };
    // The glyphs that stand for a note head. A porrectus stroke stands for two.
    let heads = |glyph: Glyph| match glyph {
        Glyph::Porrectus1 | Glyph::Porrectus2 | Glyph::Porrectus3 | Glyph::Porrectus4 => 2,
        Glyph::DoClef | Glyph::FaClef | Glyph::Flat | Glyph::Natural | Glyph::Mora | Glyph::Virgula => 0,
        Glyph::CustosLong | Glyph::CustosShort | Glyph::CustosDescLong | Glyph::CustosDescShort => 0,
        Glyph::VerticalEpisemaAbove | Glyph::VerticalEpisemaBelow | Glyph::AcuteAccent => 0,
        _ => 1,
    };
    let drawn: usize = engraving.lines.iter().flat_map(|l| &l.glyphs).map(|g| heads(g.glyph)).sum();
    let in_score: usize = score
        .syllables
        .iter()
        .filter_map(|s| s.notes.as_ref())
        .flat_map(|g| g.elements.iter().enumerate())
        .filter_map(|(k, e)| if let gabc::ElementKind::Note(n) = &e.kind { Some(gabc_rs::neume::heads(n, k).len()) } else { None })
        .sum();
    if drawn != in_score {
        return Drawn::NotesLost;
    }
    let too_wide = engraving
        .lines
        .iter()
        .any(|l| l.glyphs.iter().any(|g| g.x + g.glyph.metrics().width > width + 0.01));
    if too_wide {
        return Drawn::TooWide;
    }
    Drawn::Fine
}

/// One score's result.
struct Outcome {
    id: u32,
    /// Our first error, if we reject the score.
    error: Option<(&'static str, String)>,
    degraded: bool,
    /// What we read, as a line of tokens, if we accept the score.
    structure: Option<String>,
    /// How drawing the score went, if we accept it.
    drawn: Option<Drawn>,
    /// The text of each syllable and the letters we centre, if we accept it.
    centres: Option<Vec<(String, String)>>,
    oracle: Option<OracleVerdict>,
}

#[derive(Default)]
struct Report {
    with_oracle: bool,
    without_gabc: usize,
    dropped_tex: usize,
    /// code -> (problems, scores, the first example)
    by_code: BTreeMap<&'static str, (usize, usize, String)>,
    outcomes: Vec<Outcome>,
}

impl Report {
    fn add(&mut self, id: u32, score: &ScoreFile, oracle: Option<OracleVerdict>) {
        self.dropped_tex += usize::from(score.dropped_tex);
        let parsed = gabc::parse(&score.source);
        let mut seen: Vec<&str> = Vec::new();
        for d in &parsed.diagnostics {
            let entry = self.by_code.entry(d.code).or_default();
            entry.0 += 1;
            if !seen.contains(&d.code) {
                seen.push(d.code);
                entry.1 += 1;
            }
            if entry.2.is_empty() {
                entry.2 = format!("{id}: {}", d.render(&score.source));
            }
        }
        let error = parsed
            .diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .map(|d| (d.code, d.render(&score.source)));
        self.outcomes.push(Outcome {
            id,
            degraded: error.is_none() && (score.dropped_tex || !parsed.diagnostics.is_empty()),
            structure: error.is_none().then(|| structure::ours(&parsed.score)),
            drawn: error.is_none().then(|| draw(&parsed.score)),
            centres: error.is_none().then(|| structure::centres(&parsed.score)),
            error,
            oracle,
        });
    }

    /// One line per score: id, our verdict, our first error code, the oracle's verdict.
    fn verdicts_tsv(&self) -> String {
        let mut s = String::from("id\tours\tcode\toracle\toracle_message\n");
        for o in &self.outcomes {
            let ours = if o.error.is_some() { "reject" } else { "accept" };
            let code = o.error.as_ref().map_or("", |e| e.0);
            let (oracle, message) = match &o.oracle {
                Some(v) => (if v.accepted { "accept" } else { "reject" }, v.message.replace('\t', " ")),
                None => ("", String::new()),
            };
            let _ = writeln!(s, "{}\t{ours}\t{code}\t{oracle}\t{message}", o.id);
        }
        s
    }

    /// The oracle's token line of every score it accepted: id, tokens.
    fn structure_tsv(&self) -> String {
        let mut s = String::new();
        for o in &self.outcomes {
            if let Some(tokens) = o.oracle.as_ref().and_then(|v| v.structure.as_ref()) {
                let _ = writeln!(s, "{}\t{tokens}", o.id);
            }
        }
        s
    }

    /// Every different syllable of the library with the letters the oracle centres in it and
    /// how often: text, centre, times. The data the centring rule is derived from.
    fn centres_tsv(&self) -> String {
        let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for o in &self.outcomes {
            for (text, centre) in o.oracle.iter().filter_map(|v| v.centres.as_ref()).flatten() {
                if !centre.is_empty() {
                    *counts.entry((text, centre)).or_default() += 1;
                }
            }
        }
        let mut s = String::from("text\tcentre\ttimes\n");
        for ((text, centre), times) in counts {
            let _ = writeln!(s, "{}\t{}\t{times}", text.replace(['\t', '\n'], " "), centre.replace(['\t', '\n'], " "));
        }
        s
    }

    fn render(&self) -> String {
        let all = self.outcomes.len();
        let count = |f: &dyn Fn(&Outcome) -> bool| self.outcomes.iter().filter(|o| f(o)).count();
        let pct = |n: usize| 100.0 * n as f64 / all.max(1) as f64;
        let rejected = count(&|o| o.error.is_some());
        let degraded = count(&|o| o.degraded);
        let mut s = String::new();
        let _ = writeln!(s, "# GregoBase reference run\n");
        let _ = writeln!(s, "Parsing only so far. {} rows hold no GABC and are left out.\n", self.without_gabc);
        let _ = writeln!(s, "| | scores | share |\n|---|---:|---:|");
        let _ = writeln!(s, "| all | {all} | |");
        let _ = writeln!(s, "| clean | {} | {:.1}% |", all - rejected - degraded, pct(all - rejected - degraded));
        let _ = writeln!(s, "| valid, drawn in part | {degraded} | {:.1}% |", pct(degraded));
        let _ = writeln!(s, "| rejected | {rejected} | {:.1}% |", pct(rejected));
        let _ = writeln!(
            s,
            "\n\"Drawn in part\" is a warning or TeX between the GABC segments ({} scores).\n",
            self.dropped_tex
        );
        let _ = writeln!(s, "| code | problems | scores | first example |\n|---|---:|---:|---|");
        for (code, (problems, scores, example)) in &self.by_code {
            let _ = writeln!(s, "| {code} | {problems} | {scores} | `{}` |", example.replace('`', "'"));
        }

        if self.with_oracle {
            let both = |ours: bool, theirs: bool| {
                count(&|o| o.error.is_none() == ours && o.oracle.as_ref().is_some_and(|v| v.accepted == theirs))
            };
            let agree = both(true, true) + both(false, false);
            let _ = writeln!(s, "\n## Against Gregorio\n");
            let _ = writeln!(s, "The two agree on {agree} of {all} scores ({:.2}%).\n", pct(agree));
            let _ = writeln!(s, "| | Gregorio accepts | Gregorio rejects |\n|---|---:|---:|");
            let _ = writeln!(s, "| we accept | {} | {} |", both(true, true), both(true, false));
            let _ = writeln!(s, "| we reject | {} | {} |", both(false, true), both(false, false));

            // Our blind spots, by the code we reject with.
            let mut too_strict: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
            // Where we are too lenient, by the shape of Gregorio's message.
            let mut too_lenient: BTreeMap<String, Vec<&Outcome>> = BTreeMap::new();
            for o in &self.outcomes {
                match (&o.error, &o.oracle) {
                    (Some((code, _)), Some(v)) if v.accepted => too_strict.entry(code).or_default().push(o),
                    (None, Some(v)) if !v.accepted => {
                        too_lenient.entry(message_shape(&v.message)).or_default().push(o)
                    }
                    _ => {}
                }
            }
            let _ = writeln!(s, "\n### We reject, Gregorio accepts\n");
            let _ = writeln!(s, "| our code | scores | examples |\n|---|---:|---|");
            for (code, list) in &too_strict {
                let examples: Vec<String> = list
                    .iter()
                    .take(3)
                    .map(|o| format!("`{}`", o.error.as_ref().map_or("", |e| &e.1).replace('`', "'")))
                    .collect();
                let ids: Vec<String> = list.iter().take(3).map(|o| o.id.to_string()).collect();
                let _ = writeln!(s, "| {code} | {} | {} ({}) |", list.len(), examples.join(", "), ids.join(", "));
            }
            let _ = writeln!(s, "\n### We accept, Gregorio rejects\n");
            let _ = writeln!(s, "| Gregorio's message | scores | examples |\n|---|---:|---|");
            let mut shapes: Vec<_> = too_lenient.iter().collect();
            shapes.sort_by_key(|(_, list)| std::cmp::Reverse(list.len()));
            for (shape, list) in shapes {
                let ids: Vec<String> = list.iter().take(5).map(|o| o.id.to_string()).collect();
                let _ = writeln!(s, "| `{}` | {} | {} |", shape.replace('`', "'").replace('|', "\\|"), list.len(), ids.join(", "));
            }
        }

        // What both accept: do they hold the same notes, bars and clefs?
        let mut compared = 0usize;
        let mut causes: BTreeMap<String, Vec<(u32, String)>> = BTreeMap::new();
        for o in &self.outcomes {
            let theirs = o.oracle.as_ref().and_then(|v| v.structure.as_ref());
            if let (Some(ours), Some(theirs)) = (&o.structure, theirs) {
                compared += 1;
                if let Some((cause, tokens)) = structure::difference(ours, theirs) {
                    causes.entry(cause).or_default().push((o.id, tokens));
                }
            }
        }
        if compared > 0 {
            let different: usize = causes.values().map(Vec::len).sum();
            let same = compared - different;
            let _ = writeln!(s, "\n### The scores both accept\n");
            let _ = writeln!(
                s,
                "Clefs, notes with their pitch and kind, bars, custodes and line breaks, in order: {same} of {compared} scores are the same ({:.2}%).\n",
                100.0 * same as f64 / compared as f64
            );
            let _ = writeln!(s, "| where they first part | scores | examples (ours / Gregorio's) |\n|---|---:|---|");
            let mut causes: Vec<_> = causes.iter().collect();
            causes.sort_by_key(|(_, list)| std::cmp::Reverse(list.len()));
            for (cause, list) in causes {
                let examples: Vec<String> =
                    list.iter().take(4).map(|(id, tokens)| format!("{id}: {}", tokens.replace('|', "\\|"))).collect();
                let _ = writeln!(s, "| {} | {} | {} |", cause.replace('|', "\\|"), list.len(), examples.join(", "));
            }
        }

        // Centring: which letters of each syllable stand under its first note.
        let (mut scores, mut same_text, mut syllables, mut agree) = (0usize, 0usize, 0usize, 0usize);
        let mut differ: BTreeMap<(String, String, String), (usize, u32)> = BTreeMap::new();
        for o in &self.outcomes {
            let theirs = o.oracle.as_ref().and_then(|v| v.centres.as_ref());
            let (Some(ours), Some(theirs)) = (&o.centres, theirs) else { continue };
            scores += 1;
            // Only where both read the same syllables is there something to compare.
            if ours.len() != theirs.len() || ours.iter().zip(theirs).any(|(a, b)| a.0 != b.0) {
                continue;
            }
            same_text += 1;
            for (a, b) in ours.iter().zip(theirs) {
                // Gregorio centres nothing in the syllable of the initial.
                if b.1.is_empty() {
                    continue;
                }
                syllables += 1;
                if a.1 == b.1 {
                    agree += 1;
                } else {
                    let entry = differ.entry((a.0.clone(), a.1.clone(), b.1.clone())).or_insert((0, o.id));
                    entry.0 += 1;
                }
            }
        }
        if scores > 0 {
            let _ = writeln!(s, "\n### The letters under the first note\n");
            let _ = writeln!(
                s,
                "In {same_text} of {scores} scores both read the same text for every syllable. Of their {syllables} syllables, {agree} have the same letters centred ({:.3}%).\n",
                100.0 * agree as f64 / syllables.max(1) as f64
            );
            let _ = writeln!(s, "| syllable | we centre | Gregorio centres | times | first in |\n|---|---|---|---:|---:|");
            let mut rows: Vec<_> = differ.iter().collect();
            rows.sort_by_key(|(_, (times, _))| std::cmp::Reverse(*times));
            for ((text, ours, theirs), (times, id)) in rows.iter().take(40) {
                let cell = |t: &str| t.replace('|', "\\|").replace('`', "'");
                let _ = writeln!(s, "| `{}` | `{}` | `{}` | {times} | {id} |", cell(text), cell(ours), cell(theirs));
            }
        }

        // Drawing: what the layout makes of every score we accept.
        let accepted: Vec<&Outcome> = self.outcomes.iter().filter(|o| o.drawn.is_some()).collect();
        if !accepted.is_empty() {
            let ids = |f: &dyn Fn(&Drawn) -> bool| -> Vec<u32> {
                accepted.iter().filter(|o| o.drawn.as_ref().is_some_and(f)).map(|o| o.id).collect()
            };
            let fine = ids(&|d| matches!(d, Drawn::Fine)).len();
            let list = |ids: &[u32]| ids.iter().take(8).map(u32::to_string).collect::<Vec<_>>().join(", ");
            let _ = writeln!(s, "\n## Drawing\n");
            let _ = writeln!(
                s,
                "Every score we accept is laid out at a width of 18 em. {fine} of {} come out with every note head drawn once and nothing beyond the end of a line ({:.2}%).\n",
                accepted.len(),
                100.0 * fine as f64 / accepted.len() as f64
            );
            let _ = writeln!(s, "| what went wrong | scores | examples |\n|---|---:|---|");
            let panicked = ids(&|d| matches!(d, Drawn::Panicked));
            let lost = ids(&|d| matches!(d, Drawn::NotesLost));
            let wide = ids(&|d| matches!(d, Drawn::TooWide));
            let _ = writeln!(s, "| the layout panicked | {} | {} |", panicked.len(), list(&panicked));
            let _ = writeln!(s, "| note heads lost or doubled | {} | {} |", lost.len(), list(&lost));
            let _ = writeln!(s, "| something beyond the end of a line | {} | {} |", wide.len(), list(&wide));
        }

        let _ = writeln!(s, "\n## Scores we reject\n");
        for o in &self.outcomes {
            if let Some((_, error)) = &o.error {
                let theirs = match &o.oracle {
                    Some(v) if v.accepted => " (Gregorio accepts)",
                    Some(_) => " (Gregorio rejects)",
                    None => "",
                };
                let _ = writeln!(s, "- {}: {error}{theirs}", o.id);
            }
        }
        s
    }
}

/// A message with what varies from score to score taken out, so that equal causes group:
/// file names, numbers and quoted pieces of the input become `_`.
fn message_shape(message: &str) -> String {
    let mut out = String::new();
    let mut quoted = false;
    for word in message.split_whitespace() {
        let variable = quoted
            || word.contains(".gabc")
            || word.chars().any(|c| c.is_ascii_digit())
            || word.starts_with(['"', '\'', '`']);
        if word.starts_with(['"', '`']) && !word[1..].contains(['"', '`']) {
            quoted = true;
        } else if word.ends_with(['"', '`', ':', ',']) && quoted {
            quoted = false;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(if variable { "_" } else { word });
    }
    out
}

/// Reads the rows of every `INSERT INTO gregobase_chants` statement.
fn rows(sql: &str) -> Result<Vec<Row>, String> {
    let mut out = Vec::new();
    let mut rest = sql;
    while let Some(at) = rest.find(TABLE) {
        rest = &rest[at + TABLE.len()..];
        let values = rest.find("VALUES").ok_or("an INSERT without VALUES")?;
        let columns: Vec<&str> = rest[..values]
            .trim()
            .trim_matches(['(', ')'])
            .split(',')
            .map(|c| c.trim().trim_matches('`'))
            .collect();
        let column = |name: &str| {
            columns
                .iter()
                .position(|c| *c == name)
                .ok_or_else(|| format!("the dump has no `{name}` column"))
        };
        let (id, incipit, office_part, mode, transcriber, gabc) = (
            column("id")?,
            column("incipit")?,
            column("office-part")?,
            column("mode")?,
            column("transcriber")?,
            column("gabc")?,
        );
        let mut tuples = Tuples { s: rest[values + 6..].as_bytes(), i: 0 };
        while let Some(fields) = tuples.next()? {
            let text = |n: usize| fields.get(n).cloned().flatten().unwrap_or_default();
            out.push(Row {
                id: text(id).parse().map_err(|_| "a row without a numeric id")?,
                incipit: text(incipit),
                office_part: text(office_part),
                mode: text(mode),
                transcriber: text(transcriber),
                gabc: fields.get(gabc).cloned().flatten(),
            });
        }
        rest = &rest[values + 6 + tuples.i..];
    }
    if out.is_empty() {
        return Err(String::from("the dump holds no chants"));
    }
    Ok(out)
}

/// The `(...), (...), ...;` list of one INSERT statement.
struct Tuples<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tuples<'_> {
    /// The fields of the next tuple (`None` for NULL), or `None` at the statement's `;`.
    fn next(&mut self) -> Result<Option<Vec<Option<String>>>, String> {
        let s = self.s;
        while matches!(s.get(self.i), Some(b' ' | b'\r' | b'\n' | b',')) {
            self.i += 1;
        }
        match s.get(self.i) {
            Some(b'(') => self.i += 1,
            Some(b';') | None => return Ok(None),
            Some(other) => return Err(format!("unexpected `{}` between rows", *other as char)),
        }
        let mut fields = Vec::new();
        loop {
            while s.get(self.i) == Some(&b' ') {
                self.i += 1;
            }
            if s.get(self.i) == Some(&b'\'') {
                self.i += 1;
                let mut text = Vec::new();
                loop {
                    match s.get(self.i).copied().ok_or("the dump ends inside a string")? {
                        b'\\' => {
                            let next = s.get(self.i + 1).copied().ok_or("a lone backslash")?;
                            text.push(match next {
                                b'n' => b'\n',
                                b'r' => b'\r',
                                b't' => b'\t',
                                b'0' => 0,
                                other => other,
                            });
                            self.i += 2;
                        }
                        b'\'' if s.get(self.i + 1) == Some(&b'\'') => {
                            text.push(b'\'');
                            self.i += 2;
                        }
                        b'\'' => {
                            self.i += 1;
                            break;
                        }
                        other => {
                            text.push(other);
                            self.i += 1;
                        }
                    }
                }
                fields.push(Some(String::from_utf8_lossy(&text).into_owned()));
            } else {
                let from = self.i;
                while !matches!(s.get(self.i), Some(b',' | b')') | None) {
                    self.i += 1;
                }
                let token = String::from_utf8_lossy(&s[from..self.i]).trim().to_string();
                fields.push((token != "NULL").then_some(token));
            }
            while s.get(self.i) == Some(&b' ') {
                self.i += 1;
            }
            match s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b')') => {
                    self.i += 1;
                    return Ok(Some(fields));
                }
                _ => return Err(String::from("a row that does not end")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_read_with_their_escapes() {
        let sql = "INSERT INTO `gregobase_chants` (`id`, `incipit`, `office-part`, `mode`, \
                   `transcriber`, `gabc`) VALUES\n\
                   (1, 'It''s; \\'so\\'', 'an', '1', NULL, '\"(c4) A(f|vi) (::)\"'),\n\
                   (2, 'Two', NULL, NULL, 'x', NULL);\n";
        let rows = rows(sql).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].incipit, "It's; 'so'");
        assert!(rows[1].gabc.is_none());
        let file = score_file(&rows[0]).unwrap();
        assert_eq!(
            file.source,
            "name: It's, 'so';\noffice-part: an;\nmode: 1;\nnabc-lines: 1;\n%%\n(c4) A(f|vi) (::)\n"
        );
        assert!(score_file(&rows[1]).is_none());
    }

    #[test]
    fn tex_segments_are_left_out_and_noted() {
        let row = Row {
            id: 3,
            incipit: String::from("Three"),
            office_part: String::new(),
            mode: String::new(),
            transcriber: String::new(),
            gabc: Some(String::from(r#"[["tex","\\x",{}],["gabc","(c4) A(f)",{}]]"#)),
        };
        let file = score_file(&row).unwrap();
        assert!(file.dropped_tex);
        assert_eq!(file.source, "name: Three;\n%%\n(c4) A(f)\n");
    }

    #[test]
    fn disagreements_with_the_oracle_are_listed_by_cause() {
        let score = |source: &str| ScoreFile { source: source.to_string(), dropped_tex: false };
        let said = |accepted, message: &str| {
            Some(OracleVerdict {
                accepted,
                message: message.to_string(),
                structure: accepted.then(|| String::from("c4 fv")),
                centres: accepted.then(|| vec![(String::from("A"), String::from("A"))]),
            })
        };
        let mut report = Report { with_oracle: true, ..Report::default() };
        report.add(1, &score("%%\n(c4) A(f)"), said(true, ""));
        report.add(2, &score("%%\n(c4) A(?)"), said(true, ""));
        report.add(3, &score("%%\n(c4) A(f)"), said(false, "00003.gabc:2:5: error: unrecognized thing 12"));
        report.add(4, &score("%%\n(c4) A(?)"), said(false, "error"));
        let text = report.render();
        assert!(text.contains("The two agree on 2 of 4 scores"), "{text}");
        assert!(text.contains("| we accept | 1 | 1 |"), "{text}");
        assert!(text.contains("| we reject | 1 | 1 |"), "{text}");
        assert!(text.contains("| E011 | 1 |"), "{text}");
        assert!(text.contains("| `_ error: unrecognized thing _` | 1 | 3 |"), "{text}");
        assert!(report.verdicts_tsv().contains("2\treject\tE011\taccept\t\n"));
        // Score 1 is accepted by both: we read a punctum where the oracle says virga.
        assert!(text.contains("0 of 1 scores are the same"), "{text}");
        assert!(text.contains("| kind `p` here, `v` there | 1 | 1: `fp` / `fv` |"), "{text}");
        assert_eq!(report.structure_tsv(), "1\tc4 fv\n2\tc4 fv\n");
    }
}
