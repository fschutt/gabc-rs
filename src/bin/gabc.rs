//! `gabc check [--summary] <file or directory>...`
//!
//! Checks `.gabc` files and the ```` ```gabc ```` blocks of `.md` files. Prints one line per
//! problem as `path:line:col: error[E011]: message` and exits with 1 if any file has an
//! error. `--summary` prints counts instead, for a run over a whole corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gabc_rs::{gabc, Severity};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("check") {
        eprintln!("usage: gabc check [--summary] <file or directory>...");
        return ExitCode::from(2);
    }
    let mut summary = false;
    let mut files = Vec::new();
    for arg in args {
        if arg == "--summary" {
            summary = true;
        } else {
            collect(Path::new(&arg), &mut files);
        }
    }
    if files.is_empty() {
        eprintln!("gabc: no .gabc or .md files to check");
        return ExitCode::from(2);
    }
    files.sort();

    let (mut clean, mut warned, mut rejected, mut scores) = (0usize, 0usize, 0usize, 0usize);
    // code -> (problems, files, an example)
    let mut by_code: BTreeMap<&'static str, (usize, usize, String)> = BTreeMap::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{}: not readable as UTF-8", path.display());
            rejected += 1;
            continue;
        };
        for (source, first_line) in sources(path, &text) {
            scores += 1;
            let parsed = gabc::parse(&source);
            let mut seen = Vec::new();
            for d in &parsed.diagnostics {
                if summary {
                    let entry = by_code.entry(d.code).or_default();
                    entry.0 += 1;
                    if !seen.contains(&d.code) {
                        seen.push(d.code);
                        entry.1 += 1;
                    }
                    if entry.2.is_empty() {
                        entry.2 = format!("{}: {}", path.display(), d.render(&source));
                    }
                } else {
                    // Positions inside a Markdown block are counted from the top of the file.
                    let rendered = d.render(&source);
                    let (line, rest) = rendered.split_once(':').unwrap_or(("1", &rendered));
                    let line = line.parse::<usize>().unwrap_or(1) + first_line;
                    println!("{}:{line}:{rest}", path.display());
                }
            }
            if !parsed.is_valid() {
                rejected += 1;
            } else if parsed.diagnostics.iter().any(|d| d.severity == Severity::Warning) {
                warned += 1;
            } else {
                clean += 1;
            }
        }
    }

    if summary {
        let pct = |n: usize| 100.0 * n as f64 / scores.max(1) as f64;
        println!("scores: {scores}");
        println!("  clean:          {clean:>6}  {:.1}%", pct(clean));
        println!("  with warnings:  {warned:>6}  {:.1}%", pct(warned));
        println!("  rejected:       {rejected:>6}  {:.1}%", pct(rejected));
        for (code, (problems, in_files, example)) in &by_code {
            println!("{code}: {problems} in {in_files} scores, e.g. {example}");
        }
    }
    if rejected > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                collect(&entry.path(), out);
            }
        }
    } else if matches!(path.extension().and_then(|e| e.to_str()), Some("gabc" | "md")) {
        out.push(path.to_path_buf());
    }
}

/// The GABC sources of a file with the number of lines before each: the file itself, or the
/// fenced `gabc` blocks of a Markdown file.
fn sources(path: &Path, text: &str) -> Vec<(String, usize)> {
    if path.extension().and_then(|e| e.to_str()) != Some("md") {
        return vec![(text.to_string(), 0)];
    }
    let mut out = Vec::new();
    let mut block: Option<(String, usize)> = None;
    for (n, line) in text.lines().enumerate() {
        match &mut block {
            None if line.trim() == "```gabc" => block = Some((String::new(), n + 1)),
            None => {}
            Some(_) if line.trim() == "```" => out.extend(block.take()),
            Some((source, _)) => {
                source.push_str(line);
                source.push('\n');
            }
        }
    }
    out
}
