//! What a parser has to say about its input, and where.

/// A range of bytes in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The source is not valid notation. It is rejected.
    Error,
    /// The source is valid, but something in it will not be drawn as written.
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// A stable identifier: `E` and three digits for an error, `W` for a warning.
    pub code: &'static str,
    pub message: String,
    pub span: Span,
}

/// Line and column (both from 1, the column in characters) of a byte offset.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = &src[..floor_char_boundary(src, offset)];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, col)
}

fn floor_char_boundary(src: &str, mut offset: usize) -> usize {
    while offset > 0 && !src.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

impl Diagnostic {
    /// `line:col: error[E011]: message`, with the position counted in `src`.
    pub fn render(&self, src: &str) -> String {
        let (line, col) = line_col(src, self.span.start);
        let kind = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        format!("{line}:{col}: {kind}[{}]: {}", self.code, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_lines_and_characters() {
        let src = "a\nbé(c";
        assert_eq!(line_col(src, 0), (1, 1));
        assert_eq!(line_col(src, 2), (2, 1));
        // `é` is two bytes and one column
        assert_eq!(line_col(src, 5), (2, 3));
        assert_eq!(line_col(src, 999), (2, 5));
    }
}
