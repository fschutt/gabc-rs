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

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use gabc_rs::{gabc, Severity};

const DUMP_URL: &str =
    "https://raw.githubusercontent.com/gregorio-project/GregoBase/HEAD/gregobase_online.sql";
const TABLE: &str = "INSERT INTO `gregobase_chants`";

pub fn gregobase(root: &Path, oracle: Option<&str>) -> Result<(), String> {
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

    let mut report = Report { with_oracle: oracle.is_some(), ..Report::default() };
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
    let verdicts = match oracle {
        Some(program) => ask_oracle(program, &files)?,
        None => Vec::new(),
    };
    for (n, (id, _, score)) in files.iter().enumerate() {
        report.add(*id, score, verdicts.get(n).cloned());
    }

    let text = report.render();
    let path = dir.join("gregobase-report.md");
    std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    let tsv = dir.join("gregobase-verdicts.tsv");
    std::fs::write(&tsv, report.verdicts_tsv()).map_err(|e| format!("{}: {e}", tsv.display()))?;
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
                                    OracleVerdict {
                                        accepted: out.status.success() && !messages.contains("error"),
                                        message: line,
                                    }
                                }
                                Err(e) => OracleVerdict { accepted: false, message: format!("did not run: {e}") },
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

/// One score's result.
struct Outcome {
    id: u32,
    /// Our first error, if we reject the score.
    error: Option<(&'static str, String)>,
    degraded: bool,
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
        let said = |accepted, message: &str| Some(OracleVerdict { accepted, message: message.to_string() });
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
    }
}
