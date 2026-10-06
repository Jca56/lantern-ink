//! A path: subpaths of segments, each kept as the kind it is. A line
//! stays a line and an arc an arc, so nothing a file said is turned into
//! something else behind anyone's back (ARCHITECTURE §3.1).

use lntrn_math::Vec2;

/// An elliptical arc as SVG's `A` gives it, without its end point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcTo {
    pub rx: f64,
    pub ry: f64,
    /// The ellipse's turn, degrees clockwise.
    pub rotation: f64,
    /// Take the long way round.
    pub large: bool,
    /// Go clockwise on screen.
    pub sweep: bool,
}

/// One segment, from where the one before it ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line { to: Vec2 },
    Quad { c: Vec2, to: Vec2 },
    Cubic { c1: Vec2, c2: Vec2, to: Vec2 },
    Arc { arc: ArcTo, to: Vec2 },
}

impl Seg {
    /// Where it ends.
    pub fn to(&self) -> Vec2 {
        match *self {
            Seg::Line { to } | Seg::Quad { to, .. } | Seg::Cubic { to, .. } | Seg::Arc { to, .. } => to,
        }
    }
}

/// A run of segments from `start`; `closed` joins its end back there.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subpath {
    pub start: Vec2,
    pub segs: Vec<Seg>,
    pub closed: bool,
}

impl Subpath {
    /// Where it has got to: its last segment's end, or its start.
    pub fn end(&self) -> Vec2 {
        self.segs.last().map_or(self.start, Seg::to)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub subpaths: Vec<Subpath>,
}

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.is_empty()
    }

    /// Start a new subpath at `p`.
    pub fn move_to(&mut self, p: Vec2) -> &mut Path {
        self.subpaths.push(Subpath { start: p, segs: Vec::new(), closed: false });
        self
    }

    /// The subpath being drawn. Drawing on after a close (or before any
    /// move) starts a new one where the last began, as SVG has it.
    fn current(&mut self) -> &mut Subpath {
        match self.subpaths.last() {
            Some(last) if !last.closed => {}
            last => {
                let start = last.map_or(Vec2::ZERO, |s| s.start);
                self.subpaths.push(Subpath { start, segs: Vec::new(), closed: false });
            }
        }
        self.subpaths.last_mut().expect("just made")
    }

    pub fn push(&mut self, seg: Seg) -> &mut Path {
        self.current().segs.push(seg);
        self
    }

    pub fn line_to(&mut self, to: Vec2) -> &mut Path {
        self.push(Seg::Line { to })
    }

    pub fn quad_to(&mut self, c: Vec2, to: Vec2) -> &mut Path {
        self.push(Seg::Quad { c, to })
    }

    pub fn cubic_to(&mut self, c1: Vec2, c2: Vec2, to: Vec2) -> &mut Path {
        self.push(Seg::Cubic { c1, c2, to })
    }

    pub fn arc_to(&mut self, arc: ArcTo, to: Vec2) -> &mut Path {
        self.push(Seg::Arc { arc, to })
    }

    /// Join the current subpath's end to its start.
    pub fn close(&mut self) -> &mut Path {
        if let Some(last) = self.subpaths.last_mut() {
            last.closed = true;
        }
        self
    }

    /// Where the next segment would start from.
    pub fn end(&self) -> Vec2 {
        match self.subpaths.last() {
            Some(last) if last.closed => last.start,
            Some(last) => last.end(),
            None => Vec2::ZERO,
        }
    }

    /// The rectangle `x, y, w, h` with corners rounded by `rx`, `ry`
    /// (each limited to half its side), drawn as SVG says a `<rect>` is:
    /// clockwise from the top edge's left end. Empty when it has no area.
    pub fn rect(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Path {
        let mut path = Path::new();
        if !(w > 0.0 && h > 0.0) {
            return path;
        }
        let (rx, ry) = (rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0));
        if !(rx > 0.0 && ry > 0.0) {
            path.move_to(Vec2::new(x, y)).line_to(Vec2::new(x + w, y)).line_to(Vec2::new(x + w, y + h)).line_to(Vec2::new(x, y + h)).close();
            return path;
        }
        let corner = ArcTo { rx, ry, rotation: 0.0, large: false, sweep: true };
        path.move_to(Vec2::new(x + rx, y))
            .line_to(Vec2::new(x + w - rx, y))
            .arc_to(corner, Vec2::new(x + w, y + ry))
            .line_to(Vec2::new(x + w, y + h - ry))
            .arc_to(corner, Vec2::new(x + w - rx, y + h))
            .line_to(Vec2::new(x + rx, y + h))
            .arc_to(corner, Vec2::new(x, y + h - ry))
            .line_to(Vec2::new(x, y + ry))
            .arc_to(corner, Vec2::new(x + rx, y))
            .close();
        path
    }

    /// The ellipse at `c`, as SVG says an `<ellipse>` is drawn: four
    /// quarter arcs, clockwise from its rightmost point. Empty when it
    /// has no area.
    pub fn ellipse(c: Vec2, rx: f64, ry: f64) -> Path {
        let mut path = Path::new();
        if !(rx > 0.0 && ry > 0.0) {
            return path;
        }
        let quarter = ArcTo { rx, ry, rotation: 0.0, large: false, sweep: true };
        path.move_to(Vec2::new(c.x + rx, c.y))
            .arc_to(quarter, Vec2::new(c.x, c.y + ry))
            .arc_to(quarter, Vec2::new(c.x - rx, c.y))
            .arc_to(quarter, Vec2::new(c.x, c.y - ry))
            .arc_to(quarter, Vec2::new(c.x + rx, c.y))
            .close();
        path
    }

    /// Straight lines through `points`, joined back to the first when
    /// `closed` (a `<polygon>`; open, a `<polyline>` or a `<line>`).
    pub fn polyline(points: &[Vec2], closed: bool) -> Path {
        let mut path = Path::new();
        let Some((&first, rest)) = points.split_first() else { return path };
        path.move_to(first);
        for &p in rest {
            path.line_to(p);
        }
        if closed {
            path.close();
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_on_after_a_close_starts_where_the_subpath_began() {
        let mut p = Path::new();
        p.move_to(Vec2::new(2.0, 3.0)).line_to(Vec2::new(6.0, 3.0)).line_to(Vec2::new(6.0, 7.0)).close();
        assert_eq!(p.end(), Vec2::new(2.0, 3.0));
        p.line_to(Vec2::new(-3.0, 3.0));
        assert_eq!(p.subpaths.len(), 2);
        assert_eq!((p.subpaths[1].start, p.subpaths[1].closed), (Vec2::new(2.0, 3.0), false));
        assert_eq!(p.end(), Vec2::new(-3.0, 3.0));
        // With no move at all, from the origin.
        let mut q = Path::new();
        q.line_to(Vec2::new(1.0, 1.0));
        assert_eq!(q.subpaths[0].start, Vec2::ZERO);
    }

    #[test]
    fn shapes_are_drawn_as_svg_says() {
        let plain = Path::rect(1.0, 2.0, 10.0, 4.0, 0.0, 3.0);
        assert_eq!(plain.subpaths[0].segs.len(), 3, "one radius of nothing: square corners");
        assert!(plain.subpaths[0].closed);
        let round = Path::rect(0.0, 0.0, 10.0, 4.0, 100.0, 1.0);
        let s = &round.subpaths[0];
        assert_eq!(s.start, Vec2::new(5.0, 0.0), "rx is limited to half the width");
        assert_eq!(s.segs.iter().filter(|g| matches!(g, Seg::Arc { .. })).count(), 4);
        assert_eq!(s.end(), s.start);
        assert!(Path::rect(0.0, 0.0, 0.0, 5.0, 1.0, 1.0).is_empty());
        let e = Path::ellipse(Vec2::new(5.0, 5.0), 3.0, 2.0);
        assert_eq!(e.subpaths[0].start, Vec2::new(8.0, 5.0));
        assert_eq!(e.subpaths[0].segs[0].to(), Vec2::new(5.0, 7.0), "clockwise: down first");
        assert!(Path::ellipse(Vec2::ZERO, 3.0, 0.0).is_empty());
        let tri = Path::polyline(&[Vec2::ZERO, Vec2::X, Vec2::Y], true);
        assert_eq!((tri.subpaths[0].segs.len(), tri.subpaths[0].closed), (2, true));
        assert!(Path::polyline(&[], true).is_empty());
    }
}
