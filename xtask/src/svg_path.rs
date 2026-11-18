//! A reader for the SVG path data the glyph outlines are stored as.
//!
//! It covers the commands outlines use: `M L H V C S Z` and their relative forms. Quadratic
//! curves and arcs do not occur in the glyph table and are reported as errors, so that a new
//! glyph that needs them does not get silently drawn wrong.

/// A point in the path's own coordinate system (y pointing down).
pub type Point = (f64, f64);

/// One drawn piece of a contour, ending at `to`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line { to: Point },
    Cubic { ctrl1: Point, ctrl2: Point, to: Point },
}

impl Segment {
    pub fn to(&self) -> Point {
        match *self {
            Segment::Line { to } | Segment::Cubic { to, .. } => to,
        }
    }
}

/// A closed contour: where it starts, and the pieces that lead back there.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub start: Point,
    pub segments: Vec<Segment>,
}

impl Contour {
    /// The area enclosed by the contour's points (control points count as corners). The sign
    /// tells the direction; only the sign and rough size are used.
    pub fn signed_area(&self) -> f64 {
        let mut points = vec![self.start];
        for segment in &self.segments {
            match *segment {
                Segment::Line { to } => points.push(to),
                Segment::Cubic { ctrl1, ctrl2, to } => points.extend([ctrl1, ctrl2, to]),
            }
        }
        let mut doubled = 0.0;
        for (i, a) in points.iter().enumerate() {
            let b = points[(i + 1) % points.len()];
            doubled += a.0 * b.1 - b.0 * a.1;
        }
        doubled / 2.0
    }

    /// The same contour drawn the other way round.
    pub fn reversed(&self) -> Contour {
        let mut ends = vec![self.start];
        ends.extend(self.segments.iter().map(Segment::to));
        let start = *ends.last().unwrap_or(&self.start);
        let segments = self
            .segments
            .iter()
            .enumerate()
            .rev()
            .map(|(i, segment)| match *segment {
                Segment::Line { .. } => Segment::Line { to: ends[i] },
                Segment::Cubic { ctrl1, ctrl2, .. } => Segment::Cubic {
                    ctrl1: ctrl2,
                    ctrl2: ctrl1,
                    to: ends[i],
                },
            })
            .collect();
        Contour { start, segments }
    }

    /// Every point of the contour passed through `f`.
    pub fn map(&self, f: impl Fn(Point) -> Point) -> Contour {
        Contour {
            start: f(self.start),
            segments: self
                .segments
                .iter()
                .map(|segment| match *segment {
                    Segment::Line { to } => Segment::Line { to: f(to) },
                    Segment::Cubic { ctrl1, ctrl2, to } => Segment::Cubic {
                        ctrl1: f(ctrl1),
                        ctrl2: f(ctrl2),
                        to: f(to),
                    },
                })
                .collect(),
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn skip_separators(&mut self) {
        while let Some(b) = self.bytes.get(self.pos) {
            if b.is_ascii_whitespace() || *b == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// True if a number starts here, which after a command means the command repeats.
    fn at_number(&mut self) -> bool {
        self.skip_separators();
        matches!(self.bytes.get(self.pos), Some(b) if b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.'))
    }

    /// Reads one number. SVG lets numbers run into each other (`0-60.9`, `.5.5`), so a number
    /// ends at the first character that cannot continue it.
    fn number(&mut self) -> Result<f64, String> {
        self.skip_separators();
        let start = self.pos;
        if matches!(self.bytes.get(self.pos), Some(b'-' | b'+')) {
            self.pos += 1;
        }
        let mut seen_dot = false;
        let mut seen_digit = false;
        while let Some(b) = self.bytes.get(self.pos) {
            match b {
                b'0'..=b'9' => seen_digit = true,
                b'.' if !seen_dot => seen_dot = true,
                _ => break,
            }
            self.pos += 1;
        }
        if seen_digit && matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            let mark = self.pos;
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'-' | b'+')) {
                self.pos += 1;
            }
            let digits = self.pos;
            while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
            if self.pos == digits {
                self.pos = mark; // not an exponent after all
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|e| e.to_string())?;
        if !seen_digit {
            return Err(format!("expected a number at byte {start}, found {text:?}"));
        }
        text.parse::<f64>()
            .map_err(|e| format!("bad number {text:?} at byte {start}: {e}"))
    }

    fn pair(&mut self) -> Result<Point, String> {
        Ok((self.number()?, self.number()?))
    }
}

/// Reads SVG path data into closed contours. A contour that is not closed explicitly is closed
/// by the next `M` or the end of the data, as a fill does.
pub fn parse(data: &str) -> Result<Vec<Contour>, String> {
    let mut reader = Reader {
        bytes: data.as_bytes(),
        pos: 0,
    };
    let mut contours: Vec<Contour> = Vec::new();
    let mut current: Option<Contour> = None;
    let mut pen: Point = (0.0, 0.0);
    let mut subpath_start: Point = (0.0, 0.0);
    // The second control point of the cubic just drawn, which `S` mirrors.
    let mut last_ctrl2: Option<Point> = None;

    loop {
        reader.skip_separators();
        let Some(&command) = reader.bytes.get(reader.pos) else {
            break;
        };
        if !command.is_ascii_alphabetic() {
            return Err(format!(
                "expected a command at byte {}, found {:?}",
                reader.pos, command as char
            ));
        }
        reader.pos += 1;
        let relative = command.is_ascii_lowercase();
        let offset = |pen: Point, p: Point| if relative { (pen.0 + p.0, pen.1 + p.1) } else { p };

        let mut first = true;
        loop {
            match command.to_ascii_uppercase() {
                b'M' => {
                    let to = offset(pen, reader.pair()?);
                    if first {
                        if let Some(contour) = current.take() {
                            contours.push(contour);
                        }
                        current = Some(Contour {
                            start: to,
                            segments: Vec::new(),
                        });
                        subpath_start = to;
                    } else {
                        // Further pairs after a move are lines.
                        segment(&mut current, pen, Segment::Line { to });
                    }
                    pen = to;
                    last_ctrl2 = None;
                }
                b'L' => {
                    let to = offset(pen, reader.pair()?);
                    segment(&mut current, pen, Segment::Line { to });
                    pen = to;
                    last_ctrl2 = None;
                }
                b'H' => {
                    let x = reader.number()?;
                    let to = (if relative { pen.0 + x } else { x }, pen.1);
                    segment(&mut current, pen, Segment::Line { to });
                    pen = to;
                    last_ctrl2 = None;
                }
                b'V' => {
                    let y = reader.number()?;
                    let to = (pen.0, if relative { pen.1 + y } else { y });
                    segment(&mut current, pen, Segment::Line { to });
                    pen = to;
                    last_ctrl2 = None;
                }
                b'C' => {
                    let ctrl1 = offset(pen, reader.pair()?);
                    let ctrl2 = offset(pen, reader.pair()?);
                    let to = offset(pen, reader.pair()?);
                    segment(&mut current, pen, Segment::Cubic { ctrl1, ctrl2, to });
                    pen = to;
                    last_ctrl2 = Some(ctrl2);
                }
                b'S' => {
                    let ctrl1 = match last_ctrl2 {
                        Some(c) => (2.0 * pen.0 - c.0, 2.0 * pen.1 - c.1),
                        None => pen,
                    };
                    let ctrl2 = offset(pen, reader.pair()?);
                    let to = offset(pen, reader.pair()?);
                    segment(&mut current, pen, Segment::Cubic { ctrl1, ctrl2, to });
                    pen = to;
                    last_ctrl2 = Some(ctrl2);
                }
                b'Z' => {
                    if let Some(contour) = current.take() {
                        contours.push(contour);
                    }
                    pen = subpath_start;
                    last_ctrl2 = None;
                    break; // `Z` takes no numbers, so it cannot repeat
                }
                other => {
                    return Err(format!(
                        "path command {:?} is not supported by the font build",
                        other as char
                    ));
                }
            }
            first = false;
            if !reader.at_number() {
                break;
            }
        }
    }
    if let Some(contour) = current.take() {
        contours.push(contour);
    }
    contours.retain(|contour| !contour.segments.is_empty());
    Ok(contours)
}

/// Adds a piece to the contour being drawn, starting one at the pen if a `Z` just ended the
/// last one (a path may go on drawing from there without a new `M`).
fn segment(current: &mut Option<Contour>, pen: Point, piece: Segment) {
    current
        .get_or_insert_with(|| Contour {
            start: pen,
            segments: Vec::new(),
        })
        .segments
        .push(piece);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_that_run_into_each_other() {
        let contours = parse("M0-60.906c33.333 0 50 9.635 50 28.906v94.53z").unwrap();
        assert_eq!(contours.len(), 1);
        assert_eq!(contours[0].start, (0.0, -60.906));
        assert_eq!(contours[0].segments.len(), 2);
        assert_eq!(
            contours[0].segments[0],
            Segment::Cubic {
                ctrl1: (33.333, -60.906),
                ctrl2: (50.0, -60.906 + 9.635),
                to: (50.0, -60.906 + 28.906),
            }
        );
        match contours[0].segments[1] {
            Segment::Line { to } => assert!((to.1 - (-60.906 + 28.906 + 94.53)).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
        assert_eq!(parse("M.5.5L1 1 2 0z").unwrap()[0].segments.len(), 2);
    }

    #[test]
    fn smooth_curve_mirrors_the_control_point() {
        let contours = parse("M0 0C0 10 10 20 20 20S40 10 40 0Z").unwrap();
        assert_eq!(
            contours[0].segments[1],
            Segment::Cubic {
                ctrl1: (30.0, 20.0),
                ctrl2: (40.0, 10.0),
                to: (40.0, 0.0),
            }
        );
    }

    #[test]
    fn reversing_keeps_the_shape_and_flips_the_area() {
        let contour = parse("M0 0L10 0C10 5 5 10 0 10Z").unwrap().remove(0);
        let back = contour.reversed();
        assert!((contour.signed_area() + back.signed_area()).abs() < 1e-9);
        assert_eq!(back.reversed(), contour);
    }

    #[test]
    fn unsupported_commands_are_errors() {
        assert!(parse("M0 0A5 5 0 0 1 10 10").is_err());
        assert!(parse("M0 0Q5 5 10 0").is_err());
    }
}
