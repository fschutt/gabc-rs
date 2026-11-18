//! `gabc check [--summary] <file or directory>...`
//! `gabc html <file.gabc> [--width <em>] [--size <css size>] [--font <family>]`
//!
//! Checks `.gabc` and `.abc` files and the notation blocks of `.md` files. Prints one line
//! per problem as `path:line:col: error[E011]: message` and exits with 1 if any file has an
//! error. `--summary` prints counts instead, for a run over a whole corpus.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gabc_rs::diagnostics::line_col;
use gabc_rs::{abc, gabc, markdown, Diagnostic, Severity};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => {}
        Some("html") => return html(&args.collect::<Vec<_>>()),
        _ => {
            eprintln!("usage: gabc check [--summary] <file or directory>...");
            eprintln!("       gabc html <file.gabc> [--width <em>] [--size <css size>] [--font <family>]");
            return ExitCode::from(2);
        }
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

/// `gabc html <file.gabc>`: the score as an HTML document on standard output.
fn html(args: &[String]) -> ExitCode {
    let mut file = None;
    let (mut width, mut size, mut font) = (18.0f32, String::from("34px"), String::from("serif"));
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--width" => width = args.next().and_then(|v| v.parse().ok()).unwrap_or(width),
            "--size" => size = args.next().cloned().unwrap_or(size),
            "--font" => font = args.next().cloned().unwrap_or(font),
            path => file = Some(path.to_string()),
        }
    }
    let Some(file) = file else {
        eprintln!("usage: gabc html <file.gabc> [--width <em>] [--size <css size>] [--font <family>]");
        return ExitCode::from(2);
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        eprintln!("{file}: not readable as UTF-8");
        return ExitCode::from(2);
    };
    let parsed = gabc::parse(&text);
    for d in &parsed.diagnostics {
        eprintln!("{file}:{}", d.render(&text));
    }
    if !parsed.is_valid() {
        return ExitCode::from(1);
    }
    let options = gabc_rs::layout::Options { width, ..Default::default() };
    let engraving = gabc_rs::layout::engrave(&parsed.score, &options);
    let title = parsed.score.headers.iter().find(|h| h.name == "name").map_or("", |h| h.value.as_str());
    print!("{}", gabc_rs::html::document(&engraving, title, &font, &size));
    ExitCode::SUCCESS
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
/// score for a `.gabc` or `.abc` file, one per notation block for Markdown.
fn check(path: &Path, text: &str) -> Vec<Vec<Diagnostic>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("md") => markdown::check(text),
        Some("abc") => vec![abc::parse(text).diagnostics],
        _ => vec![gabc::parse(text).diagnostics],
    }
}
