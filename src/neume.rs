//! From notes to neumes, and from neumes to glyphs with their places.
//!
//! Notes that stand together in a group form neumes: two rising notes are a podatus, drawn one
//! on top of the other; two falling notes are a clivis, drawn side by side with a line between
//! them. This module finds the neumes in a run of notes and sets each one: which glyph of the
//! neume font each note gets, how far from the left it stands, and where the thin connecting
//! lines and stems go.
//!
//! Both steps are a port of exsurge (<https://github.com/frmatthew/exsurge>, MIT licence,
//! Copyright (c) 2008-2016 Fr. Matthew Spencer, OSJ): the state machine of
//! `Gabc.createNeumesFromNotes` and the `NeumeBuilder` with the neume classes of
//! `Exsurge.Chant.Neumes.js`.
//!
//! Lengths are in em, with one em being four staff spaces. Heights are pitches as the parser
//! counts them (`a` = 0, one per staff step); a fraction is part of a step.

use crate::gabc::{Liquescence, Note, Shape};
use crate::glyphs::Glyph;

/// One staff step, from a line to the space next to it.
pub const STEP: f32 = 0.125;
/// The room between the neumes of one group.
pub const NEUME_SPACE: f32 = STEP / 2.0;
/// The weight of stems and of the lines that connect notes.
pub const LINE_WEIGHT: f32 = STEP / 8.0;

/// One note head. A bivirga in the source is two of these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Head {
    pub pitch: u8,
    pub shape: HeadShape,
    pub cavum: bool,
    /// An oriscus with a stem (`O`).
    pub stemmed: bool,
    pub initio_debilis: bool,
    /// `~`: the small liquescent form.
    pub small: bool,
    /// `<`
    pub ascending: bool,
    /// `>`
    pub descending: bool,
    /// Which note of the group this head belongs to.
    pub note: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadShape {
    Default,
    Virga,
    Inclinatum,
    Quilisma,
    Stropha,
    Oriscus,
}

/// The heads of a note: one, or two or three for the doubled shapes.
pub fn heads(note: &Note, index: usize) -> Vec<Head> {
    let (shape, times) = match note.shape {
        Shape::Punctum | Shape::Quadratum | Shape::Linea | Shape::LineaPunctum => (HeadShape::Default, 1),
        Shape::Inclinatum => (HeadShape::Inclinatum, 1),
        Shape::Virga | Shape::VirgaReversa => (HeadShape::Virga, 1),
        Shape::Bivirga => (HeadShape::Virga, 2),
        Shape::Trivirga => (HeadShape::Virga, 3),
        Shape::Stropha => (HeadShape::Stropha, 1),
        Shape::Distropha => (HeadShape::Stropha, 2),
        Shape::Tristropha => (HeadShape::Stropha, 3),
        Shape::Oriscus | Shape::OriscusScapus => (HeadShape::Oriscus, 1),
        Shape::Quilisma | Shape::QuilismaQuadratum => (HeadShape::Quilisma, 1),
    };
    let head = Head {
        pitch: note.pitch,
        shape,
        cavum: note.cavum,
        stemmed: note.shape == Shape::OriscusScapus,
        initio_debilis: note.initio_debilis,
        small: note.liquescence == Liquescence::Deminutus,
        ascending: note.liquescence == Liquescence::Augmented,
        descending: note.liquescence == Liquescence::Diminutive,
        note: index,
    };
    vec![head; times]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Punctum,
    Virga,
    Bivirga,
    Trivirga,
    Apostropha,
    Distropha,
    Tristropha,
    Oriscus,
    PunctaInclinata,
    Podatus,
    Clivis,
    PesQuassus,
    Climacus,
    Porrectus,
    PorrectusFlexus,
    PesSubpunctis,
    Salicus,
    SalicusFlexus,
    Scandicus,
    ScandicusFlexus,
    Torculus,
    TorculusResupinus,
    TorculusResupinusFlexus,
}

/// A neume: its kind and which heads of the run it takes, as `first..end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Neume {
    pub kind: Kind,
    pub heads: std::ops::Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Unknown,
    Punctum,
    PunctaInclinata,
    Oriscus,
    Podatus,
    Clivis,
    Climacus,
    Porrectus,
    PesSubpunctis,
    Salicus,
    SalicusFlexus,
    Scandicus,
    ScandicusFlexus,
    Virga,
    Bivirga,
    Apostropha,
    Distropha,
    Tristropha,
    Torculus,
    TorculusResupinus,
}

impl State {
    /// The neume this state stands for when the notes run out.
    fn neume(self) -> Kind {
        match self {
            State::Unknown | State::Punctum => Kind::Punctum,
            State::PunctaInclinata => Kind::PunctaInclinata,
            State::Oriscus => Kind::Oriscus,
            State::Podatus => Kind::Podatus,
            State::Clivis => Kind::Clivis,
            State::Climacus => Kind::Climacus,
            State::Porrectus => Kind::Porrectus,
            State::PesSubpunctis => Kind::PesSubpunctis,
            State::Salicus => Kind::Salicus,
            State::SalicusFlexus => Kind::SalicusFlexus,
            State::Scandicus => Kind::Scandicus,
            State::ScandicusFlexus => Kind::ScandicusFlexus,
            State::Virga => Kind::Virga,
            State::Bivirga => Kind::Bivirga,
            State::Apostropha => Kind::Apostropha,
            State::Distropha => Kind::Distropha,
            State::Tristropha => Kind::Tristropha,
            State::Torculus => Kind::Torculus,
            State::TorculusResupinus => Kind::TorculusResupinus,
        }
    }
}

/// What a state does with the next note.
enum Step {
    /// Go on in this state; the note belongs to the neume being built.
    To(State),
    /// The neume is complete with this note.
    EndWith(Kind),
    /// The neume ended before this note, which starts the next one.
    EndBefore(Kind),
    /// The neume ended one note earlier still: that note and this one start anew.
    EndTwoBefore(Kind),
}

/// Finds the neumes in a run of notes that stand together with no space between them.
pub fn neumes(run: &[Head]) -> Vec<Neume> {
    use std::cmp::Ordering::{Equal, Greater, Less};
    use HeadShape as S;
    let mut out = Vec::new();
    let mut first = 0usize;
    let mut at = 0usize;
    let mut state = State::Unknown;
    while at < run.len() {
        let note = run[at];
        let step = match (state, at.checked_sub(1).map(|k| run[k])) {
            (State::Unknown, _) | (_, None) => match note.shape {
                S::Virga => Step::To(State::Virga),
                S::Stropha => Step::To(State::Apostropha),
                S::Oriscus => Step::To(State::Oriscus),
                S::Inclinatum => Step::To(State::PunctaInclinata),
                _ if note.cavum => Step::EndWith(Kind::Punctum),
                _ => Step::To(State::Punctum),
            },
            (state, Some(before)) => {
                let moves = note.pitch.cmp(&before.pitch);
                let plain = note.shape == S::Default;
                match state {
                    State::Unknown => unreachable!("handled above"),
                    State::Punctum => match moves {
                        Greater => Step::To(State::Podatus),
                        Less if note.shape == S::Inclinatum => Step::To(State::Climacus),
                        Less => Step::To(State::Clivis),
                        Equal => Step::To(State::Distropha),
                    },
                    State::PunctaInclinata if note.shape != S::Inclinatum => Step::EndBefore(Kind::PunctaInclinata),
                    State::PunctaInclinata => Step::To(State::PunctaInclinata),
                    State::Oriscus => match moves {
                        Greater if plain => Step::EndWith(Kind::PesQuassus),
                        Less if plain => Step::EndWith(Kind::Clivis),
                        _ => Step::EndBefore(Kind::Oriscus),
                    },
                    State::Podatus => match moves {
                        Greater if before.shape == S::Oriscus => Step::To(State::Salicus),
                        Greater => Step::To(State::Scandicus),
                        Less if note.shape == S::Inclinatum => Step::To(State::PesSubpunctis),
                        Less => Step::To(State::Torculus),
                        Equal => Step::EndBefore(Kind::Podatus),
                    },
                    State::Clivis if plain && moves == Greater => Step::To(State::Porrectus),
                    State::Clivis => Step::EndBefore(Kind::Clivis),
                    State::Climacus if note.shape != S::Inclinatum => Step::EndBefore(Kind::Climacus),
                    State::Climacus => Step::To(State::Climacus),
                    State::Porrectus if plain && moves == Less => Step::EndWith(Kind::PorrectusFlexus),
                    State::Porrectus => Step::EndBefore(Kind::Porrectus),
                    State::PesSubpunctis if note.shape != S::Inclinatum => Step::EndBefore(Kind::PesSubpunctis),
                    State::PesSubpunctis => Step::To(State::PesSubpunctis),
                    State::Salicus if moves == Less => Step::To(State::SalicusFlexus),
                    State::Salicus => Step::EndBefore(Kind::Salicus),
                    State::SalicusFlexus => Step::EndBefore(Kind::SalicusFlexus),
                    // A podatus whose upper note is a virga, with diamonds falling from it, is a
                    // podatus and then a climacus.
                    State::Scandicus if before.shape == S::Virga && note.shape == S::Inclinatum && moves == Less => {
                        Step::EndTwoBefore(Kind::Podatus)
                    }
                    State::Scandicus if plain && moves == Less => Step::To(State::ScandicusFlexus),
                    State::Scandicus => Step::EndBefore(Kind::Scandicus),
                    State::ScandicusFlexus => Step::EndBefore(Kind::ScandicusFlexus),
                    State::Virga if note.shape == S::Inclinatum && moves == Less => Step::To(State::Climacus),
                    State::Virga if note.shape == S::Virga && moves == Equal => Step::To(State::Bivirga),
                    State::Virga => Step::EndBefore(Kind::Virga),
                    State::Bivirga if note.shape == S::Virga && moves == Equal => Step::EndWith(Kind::Trivirga),
                    State::Bivirga => Step::EndBefore(Kind::Bivirga),
                    State::Apostropha if moves == Equal => Step::To(State::Distropha),
                    State::Apostropha => Step::EndBefore(Kind::Apostropha),
                    State::Distropha if moves == Equal => Step::To(State::Tristropha),
                    // Two notes on one pitch and then another pitch: the first stands alone,
                    // the second starts what follows.
                    State::Distropha => Step::EndTwoBefore(Kind::Apostropha),
                    // Three on one pitch are only a tristropha at the end of the run.
                    State::Tristropha => Step::EndTwoBefore(Kind::Distropha),
                    State::Torculus if plain && moves == Greater => Step::To(State::TorculusResupinus),
                    State::Torculus => Step::EndBefore(Kind::Torculus),
                    State::TorculusResupinus if plain && moves == Less => {
                        Step::EndWith(Kind::TorculusResupinusFlexus)
                    }
                    State::TorculusResupinus => Step::EndBefore(Kind::TorculusResupinus),
                }
            }
        };
        let (kind, end) = match step {
            Step::To(next) => {
                state = next;
                // On the last note, what has been gathered is the neume.
                if at + 1 == run.len() {
                    out.push(Neume { kind: state.neume(), heads: first..run.len() });
                    first = run.len();
                }
                at += 1;
                continue;
            }
            Step::EndWith(kind) => (kind, at + 1),
            Step::EndBefore(kind) => (kind, at),
            Step::EndTwoBefore(kind) => (kind, at.saturating_sub(1)),
        };
        // A neume is never empty: an end that would take nothing takes the note.
        let end = end.max(first + 1);
        out.push(Neume { kind, heads: first..end });
        first = end;
        at = end;
        state = State::Unknown;
    }
    out
}

/// A glyph of the neume font at its place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub glyph: Glyph,
    /// The left edge of the glyph, in em from the left edge of the neume.
    pub x: f32,
    /// The pitch whose height the glyph's origin sits on.
    pub pitch: f32,
    /// Which head of the run it stands for.
    pub head: usize,
}

/// A thin vertical line: the stem of a virga, or the line that joins two notes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// The left edge of the line, in em. It is [`LINE_WEIGHT`] wide.
    pub x: f32,
    /// The upper end, as a pitch.
    pub top: f32,
    /// The lower end, as a pitch.
    pub bottom: f32,
}

/// A neume as it is drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Set {
    pub glyphs: Vec<Placed>,
    pub strokes: Vec<Stroke>,
    /// The width of everything in it, in em.
    pub width: f32,
}

/// Sets a neume: `run` is the run of heads it was found in.
pub fn set(neume: &Neume, run: &[Head]) -> Set {
    use Glyph as G;
    let mut b = Builder { run, out: Set::default(), x: 0.0, last: None, hanging: false };
    let n = |k: usize| neume.heads.start + k;
    let len = neume.heads.len();
    let head = |k: usize| run[n(k)];
    // The glyph of a note that ends a falling figure.
    let lower_end = |h: Head| {
        if h.small {
            G::TerminatingDesLiquescent
        } else if h.ascending {
            G::PunctumQuadratumAscLiquescent
        } else if h.descending {
            G::PunctumQuadratumDesLiquescent
        } else {
            G::PunctumQuadratum
        }
    };
    // The glyph of a note that ends a rising figure, on top of the one before.
    let upper_end = |h: Head| {
        if h.small {
            G::TerminatingAscLiquescent
        } else if h.descending {
            G::PunctumQuadratumDesLiquescent
        } else {
            G::PodatusUpper
        }
    };
    let first_of = |h: Head| {
        if h.initio_debilis {
            G::TerminatingDesLiquescent
        } else if h.shape == HeadShape::Quilisma {
            G::Quilisma
        } else {
            G::PunctumQuadratum
        }
    };
    match neume.kind {
        Kind::Punctum => {
            let h = head(0);
            let glyph = if h.small || h.ascending || h.descending {
                match h.shape {
                    HeadShape::Inclinatum => G::PunctumInclinatumLiquescent,
                    HeadShape::Oriscus => G::OriscusLiquescent,
                    _ if h.ascending => G::PunctumQuadratumAscLiquescent,
                    _ if h.descending => G::PunctumQuadratumDesLiquescent,
                    _ => G::PunctumQuadratum,
                }
            } else if h.cavum {
                G::PunctumCavum
            } else {
                match h.shape {
                    HeadShape::Inclinatum => G::PunctumInclinatum,
                    HeadShape::Quilisma => G::Quilisma,
                    _ => G::PunctumQuadratum,
                }
            };
            b.note(n(0), glyph, true);
        }
        Kind::Virga => b.virga(n(0)),
        Kind::Bivirga | Kind::Trivirga => {
            for k in 0..len {
                if k > 0 {
                    b.advance(NEUME_SPACE);
                }
                b.virga(n(k));
            }
        }
        Kind::Apostropha | Kind::Distropha | Kind::Tristropha => {
            for k in 0..len {
                if k > 0 {
                    b.advance(NEUME_SPACE);
                }
                let h = head(k);
                let glyph = if h.shape == HeadShape::Stropha {
                    G::Stropha
                } else if h.small || h.ascending || h.descending {
                    // exsurge has no glyph of its own for a liquescent stropha.
                    G::Stropha
                } else if h.cavum {
                    G::PunctumCavum
                } else {
                    G::PunctumQuadratum
                };
                b.note(n(k), glyph, true);
            }
        }
        Kind::Oriscus => {
            let h = head(0);
            let glyph = if h.small || h.ascending || h.descending {
                G::OriscusLiquescent
            } else {
                // Leaning towards the note that follows, if one does in this run.
                match run.get(n(0) + 1) {
                    Some(next) if next.pitch > h.pitch => G::OriscusAsc,
                    _ => G::OriscusDes,
                }
            };
            b.note(n(0), glyph, true);
        }
        Kind::PunctaInclinata => b.inclinati(neume.heads.clone()),
        Kind::Podatus => b.podatus(n(0), n(1)),
        Kind::Clivis => b.clivis(n(0), n(1)),
        Kind::PesQuassus => {
            let (lower, upper) = (head(0), head(1));
            let glyph = if lower.shape == HeadShape::Oriscus { G::OriscusAsc } else { G::PunctumQuadratum };
            b.note(n(0), glyph, true);
            if upper.pitch - lower.pitch == 1 {
                b.virga(n(1));
            } else {
                let glyph = if upper.descending { G::PunctumQuadratumDesLiquescent } else { G::PunctumQuadratum };
                b.note(n(1), glyph, true);
                b.line_ending_at(n(0));
            }
        }
        Kind::Climacus => {
            b.virga(n(0));
            b.advance(NEUME_SPACE / 2.0);
            b.inclinati(n(1)..neume.heads.end);
        }
        Kind::PesSubpunctis => {
            b.podatus(n(0), n(1));
            b.advance(NEUME_SPACE / 2.0);
            b.inclinati(n(2)..neume.heads.end);
        }
        Kind::Porrectus => {
            b.line_from(n(1));
            b.porrectus_swash(n(0), n(1));
            b.note(n(2), upper_end(head(2)), true);
        }
        Kind::PorrectusFlexus => {
            let last = head(3);
            b.line_from(n(1));
            b.porrectus_swash(n(0), n(1));
            let third = if last.small { G::PunctumQuadratumDesLiquescent } else { G::PunctumQuadratum };
            b.note(n(2), third, true);
            b.note(n(3), lower_end(last), true);
        }
        Kind::Salicus | Kind::SalicusFlexus => {
            b.note(n(0), G::PunctumQuadratum, true);
            if !head(1).stemmed {
                b.advance(NEUME_SPACE);
            }
            b.note(n(1), G::OriscusAsc, true);
            let third = head(2);
            if neume.kind == Kind::Salicus {
                if third.small {
                    b.note(n(2), G::TerminatingAscLiquescent, true);
                } else if third.ascending {
                    b.note(n(2), G::PunctumQuadratumAscLiquescent, true);
                } else if third.descending {
                    b.note(n(2), G::PunctumQuadratumDesLiquescent, true);
                } else {
                    b.virga(n(2));
                }
            } else {
                let fourth = head(3);
                let glyph = if fourth.small { G::PunctumQuadratumDesLiquescent } else { G::PunctumQuadratum };
                b.note(n(2), glyph, true);
                b.note(n(3), lower_end(fourth), true);
            }
        }
        Kind::Scandicus => {
            if head(2).shape == HeadShape::Virga {
                b.podatus(n(0), n(1));
                b.virga(n(2));
            } else {
                b.note(n(0), G::PunctumQuadratum, true);
                b.podatus(n(1), n(2));
            }
        }
        Kind::ScandicusFlexus => {
            if head(2).shape == HeadShape::Virga {
                b.podatus(n(0), n(1));
                b.advance(NEUME_SPACE);
                b.clivis(n(2), n(3));
            } else {
                let fourth = head(3);
                let glyph = if fourth.ascending {
                    G::PunctumQuadratumAscLiquescent
                } else if fourth.descending {
                    G::PunctumQuadratumDesLiquescent
                } else {
                    G::PunctumQuadratum
                };
                b.note(n(0), G::PunctumQuadratum, true);
                b.podatus(n(1), n(2));
                b.advance(NEUME_SPACE);
                b.note(n(3), glyph, true);
            }
        }
        Kind::Torculus => {
            b.note(n(0), first_of(head(0)), true);
            b.note(n(1), G::PunctumQuadratum, true);
            b.note(n(2), lower_end(head(2)), true);
        }
        Kind::TorculusResupinus => {
            b.note(n(0), first_of(head(0)), true);
            b.porrectus_swash(n(1), n(2));
            b.note(n(3), upper_end(head(3)), true);
        }
        Kind::TorculusResupinusFlexus => {
            let fifth = head(4);
            b.note(n(0), first_of(head(0)), true);
            b.porrectus_swash(n(1), n(2));
            let fourth = if fifth.small { G::PunctumQuadratumDesLiquescent } else { G::PunctumQuadratum };
            b.note(n(3), fourth, true);
            b.note(n(4), lower_end(fifth), true);
        }
    }
    let mut out = b.out;
    out.width = out
        .glyphs
        .iter()
        .map(|g| g.x + g.glyph.metrics().width)
        .chain(out.strokes.iter().map(|s| s.x + LINE_WEIGHT))
        .fold(0.0, f32::max);
    out
}

/// Sets notes from left to right, as exsurge's `NeumeBuilder` does.
struct Builder<'a> {
    run: &'a [Head],
    out: Set,
    x: f32,
    /// The note set last and whether its glyph is right-aligned.
    last: Option<(usize, bool)>,
    /// A line hangs down on the left of the next note.
    hanging: bool,
}

impl Builder<'_> {
    fn pitch(&self, head: usize) -> u8 {
        self.run[head].pitch
    }

    /// The line between two notes. A hanging one reaches below the lower note.
    fn stroke(&mut self, a: usize, b: usize, hanging: bool, x: f32) {
        let (upper, lower) = (self.pitch(a).max(self.pitch(b)), self.pitch(a).min(self.pitch(b)));
        let mut bottom = f32::from(lower);
        if hanging {
            // A step apart with the upper note on a line: the line goes a whole step further,
            // unless that leaves the staff.
            if upper - lower == 1 && upper % 2 == 1 && lower > 3 {
                bottom -= 1.0;
            }
            // And past the middle of the lower note by a bit less than half a note's height.
            let punctum = Glyph::PunctumQuadratum.metrics();
            bottom -= (punctum.above + punctum.below) / 2.2 / STEP;
        }
        self.out.strokes.push(Stroke { x, top: f32::from(upper), bottom });
    }

    /// Makes the next note start with a line hanging on its left that reaches to `to`.
    fn line_from(&mut self, to: usize) {
        self.last = Some((to, false));
        self.hanging = true;
    }

    /// Adds a note, with a connecting line on its left if one is due.
    fn note(&mut self, head: usize, glyph: Glyph, with_line: bool) {
        let metrics = glyph.metrics();
        if let Some((last, last_right)) = self.last.filter(|_| with_line) {
            let far = self.pitch(last).abs_diff(self.pitch(head)) > 1;
            if self.hanging || last_right || far {
                let x = (self.x - LINE_WEIGHT).max(0.0);
                self.stroke(last, head, self.hanging, x);
                if !metrics.align_right {
                    self.x = x;
                }
            }
        }
        // A right-aligned glyph that is not the first sits on top of what is before it.
        let x = if metrics.align_right && self.last.is_some() {
            self.x - metrics.width
        } else {
            let x = self.x;
            self.x += metrics.width;
            x
        };
        self.out.glyphs.push(Placed { glyph, x, pitch: f32::from(self.pitch(head)), head });
        self.last = Some((head, metrics.align_right));
        self.hanging = false;
    }

    /// A virga: a punctum with a stem on its right.
    fn virga(&mut self, head: usize) {
        self.note(head, Glyph::PunctumQuadratum, true);
        let pitch = self.pitch(head);
        // The stem is longer for a note on a line.
        let length = if pitch % 2 == 0 { 1.8 } else { 2.7 };
        self.x -= LINE_WEIGHT;
        self.out.strokes.push(Stroke { x: self.x, top: f32::from(pitch), bottom: f32::from(pitch) - length });
        self.last = Some((head, false));
        self.hanging = false;
    }

    fn advance(&mut self, by: f32) {
        self.last = None;
        self.hanging = false;
        self.x += by;
    }

    /// Ends a hanging line that has no lower note of its own.
    fn line_ending_at(&mut self, head: usize) {
        let Some((last, _)) = self.last else { return };
        self.x -= LINE_WEIGHT;
        self.stroke(last, head, true, self.x);
        self.last = Some((head, false));
    }

    fn podatus(&mut self, lower: usize, upper: usize) {
        use Glyph as G;
        let (l, u) = (self.run[lower], self.run[upper]);
        let (mut lower_glyph, upper_glyph) = if l.initio_debilis {
            let up = if u.small || u.ascending || u.descending { G::PunctumQuadratumDesLiquescent } else { G::PunctumQuadratum };
            (G::TerminatingDesLiquescent, up)
        } else if u.small {
            (G::BeginningAscLiquescent, G::TerminatingAscLiquescent)
        } else if u.ascending {
            (G::PunctumQuadratum, G::PunctumQuadratumAscLiquescent)
        } else if u.descending {
            (G::PunctumQuadratum, G::PunctumQuadratumDesLiquescent)
        } else {
            (G::PodatusLower, G::PodatusUpper)
        };
        if l.shape == HeadShape::Quilisma {
            lower_glyph = G::Quilisma;
        }
        self.note(lower, lower_glyph, true);
        self.note(upper, upper_glyph, true);
        // No line leads on from a podatus.
        self.last = None;
    }

    fn clivis(&mut self, upper: usize, lower: usize) {
        use Glyph as G;
        if self.run[upper].shape == HeadShape::Oriscus {
            self.note(upper, G::OriscusDes, false);
        } else {
            self.line_from(lower);
            self.note(upper, G::PunctumQuadratum, true);
        }
        let l = self.run[lower];
        let glyph = if l.small {
            G::TerminatingDesLiquescent
        } else if l.ascending {
            G::PunctumQuadratumAscLiquescent
        } else if l.descending {
            G::PunctumQuadratumDesLiquescent
        } else {
            G::PunctumQuadratum
        };
        self.note(lower, glyph, true);
        self.last = None;
    }

    /// A row of diamond notes, as in a climacus. They are spaced by the width of the diamond
    /// and by how far each falls, so that liquescent ones sit like the others.
    fn inclinati(&mut self, heads: std::ops::Range<usize>) {
        use Glyph as G;
        let advance = G::PunctumInclinatum.metrics().width;
        let mut before: Option<u8> = None;
        for head in heads {
            let h = self.run[head];
            let glyph = if h.small {
                G::PunctumInclinatumLiquescent
            } else if h.ascending || h.descending {
                G::Stropha
            } else {
                G::PunctumInclinatum
            };
            if let Some(previous) = before {
                self.x += advance
                    * match previous.abs_diff(h.pitch) {
                        0 => 1.1,
                        1 => 0.8,
                        _ => 1.2,
                    };
            }
            self.out.glyphs.push(Placed { glyph, x: self.x, pitch: f32::from(h.pitch), head });
            before = Some(h.pitch);
        }
    }

    /// The slanted stroke of a porrectus from `start` down to `end`. The second note has no
    /// glyph of its own.
    fn porrectus_swash(&mut self, start: usize, end: usize) {
        use Glyph as G;
        if let Some((last, last_right)) = self.last {
            let far = self.pitch(last).abs_diff(self.pitch(start)) > 1;
            if self.hanging || last_right || far {
                self.x = (self.x - LINE_WEIGHT).max(0.0);
                self.stroke(last, start, self.hanging, self.x);
            }
        }
        let glyph = match self.pitch(start).saturating_sub(self.pitch(end)) {
            0 | 1 => G::Porrectus1,
            2 => G::Porrectus2,
            3 => G::Porrectus3,
            _ => G::Porrectus4,
        };
        self.out.glyphs.push(Placed { glyph, x: self.x, pitch: f32::from(self.pitch(start)), head: start });
        self.x += glyph.metrics().width;
        self.last = Some((end, false));
        self.hanging = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gabc::{parse, ElementKind};

    /// The heads of the first group after the clef.
    fn run(notes: &str) -> Vec<Head> {
        let parsed = parse(&format!("%%\n(c4) A({notes})"));
        assert!(parsed.is_valid(), "{:?}", parsed.diagnostics);
        let group = parsed.score.syllables[1].notes.clone().unwrap();
        let mut out = Vec::new();
        for (k, element) in group.elements.iter().enumerate() {
            if let ElementKind::Note(note) = &element.kind {
                out.extend(heads(note, k));
            }
        }
        out
    }

    fn kinds(notes: &str) -> Vec<(Kind, usize)> {
        neumes(&run(notes)).into_iter().map(|n| (n.kind, n.heads.len())).collect()
    }

    #[test]
    fn the_common_neumes_are_found() {
        use Kind::*;
        for (notes, expected) in [
            ("f", vec![(Punctum, 1)]),
            ("fv", vec![(Virga, 1)]),
            ("fg", vec![(Podatus, 2)]),
            ("gf", vec![(Clivis, 2)]),
            ("fgf", vec![(Torculus, 3)]),
            ("gfg", vec![(Porrectus, 3)]),
            ("fgh", vec![(Scandicus, 3)]),
            ("hvGF", vec![(Climacus, 3)]),
            ("gfgf", vec![(PorrectusFlexus, 4)]),
            ("fgfg", vec![(TorculusResupinus, 4)]),
            ("fgfgf", vec![(TorculusResupinusFlexus, 5)]),
            ("fghg", vec![(ScandicusFlexus, 4)]),
            ("ghGF", vec![(PesSubpunctis, 4)]),
            ("fvv", vec![(Bivirga, 2)]),
            ("fvvv", vec![(Trivirga, 3)]),
            ("fs", vec![(Apostropha, 1)]),
            ("fss", vec![(Distropha, 2)]),
            ("fsss", vec![(Tristropha, 3)]),
            ("fo", vec![(Oriscus, 1)]),
            ("fog", vec![(PesQuassus, 2)]),
            ("GFE", vec![(PunctaInclinata, 3)]),
            ("fgoh", vec![(Salicus, 3)]),
            // A run goes on into the next neume.
            ("fgfe", vec![(Torculus, 3), (Punctum, 1)]),
            ("ghhg", vec![(Podatus, 2), (Clivis, 2)]),
            ("fgwh", vec![(Scandicus, 3)]),
            // Two on one pitch and then a step: the first stands alone.
            ("ffg", vec![(Apostropha, 1), (Podatus, 2)]),
            ("ghvGF", vec![(PesSubpunctis, 4)]),
            // Three rising notes with a virga on top and diamonds after it.
            ("fghvGF", vec![(Podatus, 2), (Climacus, 3)]),
        ] {
            assert_eq!(kinds(notes), expected, "{notes}");
        }
    }

    #[test]
    fn every_head_is_in_one_neume() {
        for notes in ["fghgfgfedcdefgh", "ffffgggg", "fvgvhv", "GFEdefG", "fogohofo", "gwhihgvFED", "fssgsshs"] {
            let run = run(notes);
            let found = neumes(&run);
            let mut next = 0;
            for neume in &found {
                assert_eq!(neume.heads.start, next, "{notes}: {found:?}");
                assert!(!neume.heads.is_empty(), "{notes}");
                next = neume.heads.end;
            }
            assert_eq!(next, run.len(), "{notes}");
            // And every one can be set, with each visible note at its own pitch.
            for neume in &found {
                let set = set(neume, &run);
                assert!(set.width > 0.0, "{notes}: {neume:?}");
                for glyph in &set.glyphs {
                    assert_eq!(glyph.pitch, f32::from(run[glyph.head].pitch), "{notes}");
                }
            }
        }
    }

    #[test]
    fn a_podatus_is_stacked_and_a_clivis_stands_side_by_side() {
        let heads = run("fg");
        let podatus = set(&neumes(&heads)[0], &heads);
        assert_eq!(podatus.glyphs[0].glyph, Glyph::PodatusLower);
        assert_eq!(podatus.glyphs[1].glyph, Glyph::PodatusUpper);
        // The upper note ends where the lower one ends.
        let right = |p: &Placed| p.x + p.glyph.metrics().width;
        assert!((right(&podatus.glyphs[0]) - right(&podatus.glyphs[1])).abs() < 1e-6);
        assert!((podatus.width - STEP).abs() < 1e-6);

        let heads = run("gf");
        let clivis = set(&neumes(&heads)[0], &heads);
        assert_eq!(clivis.glyphs.len(), 2);
        assert!(clivis.glyphs[1].x > clivis.glyphs[0].x);
        // The line hangs on the left of the upper note, from it to below the lower one.
        assert_eq!(clivis.strokes.len(), 1);
        assert_eq!(clivis.strokes[0].x, 0.0);
        assert_eq!(clivis.strokes[0].top, 6.0);
        assert!(clivis.strokes[0].bottom < 5.0);
    }

    #[test]
    fn a_virga_has_its_stem_and_a_porrectus_its_stroke() {
        let heads = run("hv");
        let virga = set(&neumes(&heads)[0], &heads);
        assert_eq!(virga.glyphs[0].glyph, Glyph::PunctumQuadratum);
        assert_eq!(virga.strokes.len(), 1);
        // `h` is on a line, so the stem is the long one, on the right of the note.
        assert!((virga.strokes[0].top - virga.strokes[0].bottom - 2.7).abs() < 1e-6);
        assert!((virga.strokes[0].x - (STEP - LINE_WEIGHT)).abs() < 1e-6);

        let heads = run("hfg");
        let porrectus = set(&neumes(&heads)[0], &heads);
        let glyphs: Vec<Glyph> = porrectus.glyphs.iter().map(|g| g.glyph).collect();
        assert_eq!(glyphs, [Glyph::Porrectus2, Glyph::PodatusUpper]);
    }
}
