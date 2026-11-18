//! `gabc check [--summary] <file or directory>...`
//!
//! Checks `.gabc` and `.abc` files and the notation blocks of `.md` files. Prints one line
//! per problem as `path:line:col: error[E011]: message` and exits with 1 if any file has an
//! error. `--summary` prints counts instead, for a run over a whole corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gabc_rs::diagnostics::line_col;
use gabc_rs::markdown::{notation_blocks, Language};
use gabc_rs::{abc, gabc, Diagnostic, Severity};

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
        eprintln!("gabc: no .gabc, .abc or .md files to check");
        return ExitCode::from(2);
    }
    files.sort();

    let (mut clean, mut warned, mut rejected, mut scores) = (0usize, 0usize, 0usize, 0usize);
    // code -> (problems, scores, an example)
    let mut by_code: BTreeMap<&'static str, (usize, usize, String)> = BTreeMap::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{}: not readable as UTF-8", path.display());
            rejected += 1;
            continue;
        };
        for diagnostics in check(path, &text) {
            scores += 1;
            let mut seen = Vec::new();
            for d in &diagnostics {
                let (line, col) = line_col(&text, d.span.start);
                let kind = if d.severity == Severity::Error { "error" } else { "warning" };
                let rendered = format!("{}:{line}:{col}: {kind}[{}]: {}", path.display(), d.code, d.message);
                if summary {
                    let entry = by_code.entry(d.code).or_default();
                    entry.0 += 1;
                    if !seen.contains(&d.code) {
                        seen.push(d.code);
                        entry.1 += 1;
                    }
                    if entry.2.is_empty() {
                        entry.2 = rendered;
                    }
                } else {
                    println!("{rendered}");
                }
            }
            if diagnostics.iter().any(|d| d.severity == Severity::Error) {
                rejected += 1;
            } else if diagnostics.is_empty() {
                clean += 1;
            } else {
                warned += 1;
            }
        }
    }

    if summary {
        let pct = |n: usize| 100.0 * n as f64 / scores.max(1) as f64;
        println!("scores: {scores}");
        println!("  clean:          {clean:>6}  {:.1}%", pct(clean));
        println!("  with warnings:  {warned:>6}  {:.1}%", pct(warned));
        println!("  rejected:       {rejected:>6}  {:.1}%", pct(rejected));
        for (code, (problems, in_scores, example)) in &by_code {
            println!("{code}: {problems} in {in_scores} scores, e.g. {example}");
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
    } else if matches!(path.extension().and_then(|e| e.to_str()), Some("gabc" | "abc" | "md")) {
        out.push(path.to_path_buf());
    }
}

/// The diagnostics of each score in a file, with spans that are positions in the file: one
/// score for a `.gabc` or `.abc` file, one per notation block for Markdown. Problems with how
/// the blocks are wrapped count as one more score.
fn check(path: &Path, text: &str) -> Vec<Vec<Diagnostic>> {
    let parse = |language: Language, source: &str, offset: usize| {
        let mut diagnostics = match language {
            Language::Abc => abc::parse(source).diagnostics,
            Language::Gabc => gabc::parse(source).diagnostics,
        };
        for d in &mut diagnostics {
            d.span.start += offset;
            d.span.end += offset;
        }
        diagnostics
    };
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => {
            let (blocks, wrapping) = notation_blocks(text);
            let mut out: Vec<Vec<Diagnostic>> =
                blocks.iter().map(|b| parse(b.language, &b.source, b.offset)).collect();
            if !wrapping.is_empty() {
                out.push(wrapping);
            }
            out
        }
        Some("abc") => vec![parse(Language::Abc, text, 0)],
        _ => vec![parse(Language::Gabc, text, 0)],
    }
}
