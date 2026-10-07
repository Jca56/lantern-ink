//! One segment of a path with where it starts: a piece of line that can
//! say where it goes, be cut in two, and be walked from its other end.
//! Path editing and boolean operations are made of these. A piece cut
//! in two is still the kind it was (an arc's two halves are arcs of the
//! same ellipse), as everything is here (ARCHITECTURE §3.1).

use std::f64::consts::PI;

use lntrn_math::Vec2;

use crate::arc::{Centered, Shape, shape};
use crate::path::{ArcTo, Seg};

/// A segment, and the point it's drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub from: Vec2,
    pub seg: Seg,
}

fn lerp(a: Vec2, b: Vec2, t: f64) -> Vec2 {
    a + (b - a) * t
}

impl Piece {
    pub fn new(from: Vec2, seg: Seg) -> Piece {
        Piece { from, seg }
    }

    /// Where it ends.
    pub fn to(&self) -> Vec2 {
        self.seg.to()
    }

    /// Its arc as a centre and a sweep, when it's an arc that draws one
    /// (not a point, and not the straight line a radius of nothing is).
    fn centered(&self) -> Option<Centered> {
        match self.seg {
            Seg::Arc { ref arc, to } => match shape(self.from, arc, to) {
                Shape::Arc(c) => Some(c),
                _ => None,
            },
            _ => None,
        }
    }

    /// The point `t` of the way along it, 0 to 1: a curve's own
    /// parameter, an arc's share of its sweep.
    pub fn at(&self, t: f64) -> Vec2 {
        let u = 1.0 - t;
        match self.seg {
            Seg::Line { to } => lerp(self.from, to, t),
            Seg::Quad { c, to } => self.from * (u * u) + c * (2.0 * u * t) + to * (t * t),
            Seg::Cubic { c1, c2, to } => self.from * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + to * (t * t * t),
            Seg::Arc { to, .. } => match self.centered() {
                Some(c) => c.at(c.theta + c.delta * t, 1.0),
                None => lerp(self.from, to, t),
            },
        }
    }

    /// The way it's heading at `t`: not of any set length, and nothing
    /// where it stands still.
    pub fn heading(&self, t: f64) -> Vec2 {
        let u = 1.0 - t;
        match self.seg {
            Seg::Line { to } => to - self.from,
            Seg::Quad { c, to } => (c - self.from) * (2.0 * u) + (to - c) * (2.0 * t),
            Seg::Cubic { c1, c2, to } => (c1 - self.from) * (3.0 * u * u) + (c2 - c1) * (6.0 * u * t) + (to - c2) * (3.0 * t * t),
            Seg::Arc { to, .. } => match self.centered() {
                Some(c) => {
                    let angle = c.theta + c.delta * t;
                    let (sin, cos) = c.phi.sin_cos();
                    let (dx, dy) = (-c.rx * angle.sin(), c.ry * angle.cos());
                    Vec2::new(cos * dx - sin * dy, sin * dx + cos * dy) * c.delta
                }
                None => to - self.from,
            },
        }
    }

    /// It in two at `t`: together they draw exactly what it drew.
    pub fn split(&self, t: f64) -> (Piece, Piece) {
        let mid = self.at(t);
        let (first, second) = match self.seg {
            Seg::Line { to } => (Seg::Line { to: mid }, Seg::Line { to }),
            Seg::Quad { c, to } => (Seg::Quad { c: lerp(self.from, c, t), to: mid }, Seg::Quad { c: lerp(c, to, t), to }),
            Seg::Cubic { c1, c2, to } => {
                // De Casteljau: the points between the points between.
                let (a, b, c) = (lerp(self.from, c1, t), lerp(c1, c2, t), lerp(c2, to, t));
                (Seg::Cubic { c1: a, c2: lerp(a, b, t), to: mid }, Seg::Cubic { c1: lerp(b, c, t), c2: c, to })
            }
            Seg::Arc { arc, to } => {
                // Each part with the radii it's drawn with (ones too
                // small to reach have grown), and the long way round
                // only if its own share of the sweep is.
                let part = |share: f64| match self.centered() {
                    Some(c) => ArcTo { rx: c.rx, ry: c.ry, large: (c.delta * share).abs() > PI, ..arc },
                    None => arc,
                };
                (Seg::Arc { arc: part(t), to: mid }, Seg::Arc { arc: part(1.0 - t), to })
            }
        };
        (Piece::new(self.from, first), Piece::new(mid, second))
    }

    /// The part of it from `t0` to `t1`.
    pub fn part(&self, t0: f64, t1: f64) -> Piece {
        let (_, rest) = self.split(t0);
        if t0 >= 1.0 {
            return rest;
        }
        rest.split((t1 - t0) / (1.0 - t0)).0
    }

    /// The same line, drawn from its other end.
    pub fn reversed(&self) -> Piece {
        let to = self.from;
        let seg = match self.seg {
            Seg::Line { .. } => Seg::Line { to },
            Seg::Quad { c, .. } => Seg::Quad { c, to },
            Seg::Cubic { c1, c2, .. } => Seg::Cubic { c1: c2, c2: c1, to },
            Seg::Arc { arc, .. } => Seg::Arc { arc: ArcTo { sweep: !arc.sweep, ..arc }, to },
        };
        Piece::new(self.to(), seg)
    }

    /// The `t` of the point on it nearest `p`.
    pub fn nearest(&self, p: Vec2) -> f64 {
        if let Seg::Line { to } = self.seg {
            let along = to - self.from;
            return if along.length_squared() > 0.0 { ((p - self.from).dot(along) / along.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
        }
        // The nearest of a row of points along it, then closer and
        // closer between its neighbours.
        const STEPS: usize = 48;
        let far = |t: f64| self.at(t).distance_squared(p);
        let best = (0..=STEPS).map(|i| i as f64 / STEPS as f64).min_by(|a, b| far(*a).total_cmp(&far(*b))).unwrap_or(0.0);
        let (mut lo, mut hi) = ((best - 1.0 / STEPS as f64).max(0.0), (best + 1.0 / STEPS as f64).min(1.0));
        for _ in 0..48 {
            let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
            if far(a) <= far(b) { hi = b } else { lo = a }
        }
        (lo + hi) * 0.5
    }
}

/// The circle through three points: its centre and radius. `None` when
/// they're in a line.
pub fn circle_through(a: Vec2, b: Vec2, c: Vec2) -> Option<(Vec2, f64)> {
    let (ab, ac) = (b - a, c - a);
    let cross = ab.perp_dot(ac);
    if cross.abs() <= 1e-12 * ab.length() * ac.length() {
        return None;
    }
    // Where the two chords' perpendicular bisectors meet.
    let (d1, d2) = (ab.length_squared(), ac.length_squared());
    let centre = a + Vec2::new(ac.y * d1 - ab.y * d2, ab.x * d2 - ac.x * d1) * (0.5 / cross);
    Some((centre, centre.distance(a))).filter(|(c, r)| c.is_finite() && r.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Path;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    /// The pieces of `d`'s first subpath.
    fn pieces(d: &str) -> Vec<Piece> {
        let path = Path::parse(d).path;
        let sub = &path.subpaths[0];
        let mut from = sub.start;
        sub.segs.iter().map(|seg| Piece::new(std::mem::replace(&mut from, seg.to()), *seg)).collect()
    }

    const KINDS: [&str; 5] = ["M1 2 L9 -4", "M0 0 Q10 12 20 0", "M0 0 C0 10 20 10 20 -4", "M10 0 A10 6 30 0 1 -4 9", "M10 0 A10 10 0 1 0 0 10"];

    #[test]
    fn a_piece_goes_from_its_start_to_its_end() {
        for d in KINDS {
            let piece = pieces(d)[0];
            assert!(piece.at(0.0).distance(piece.from) < 1e-9 && piece.at(1.0).distance(piece.to()) < 1e-9, "{d}");
            // Its heading is the way its points go.
            for t in [0.1, 0.5, 0.9] {
                let step = (piece.at(t + 1e-6) - piece.at(t - 1e-6)) * (1.0 / 2e-6);
                assert!(step.distance(piece.heading(t)) < 1e-4 * step.length().max(1.0), "{d} at {t}: {step:?} vs {:?}", piece.heading(t));
            }
        }
        assert_eq!(pieces("M0 0 Q10 12 20 0")[0].at(0.5), v(10.0, 6.0));
    }

    #[test]
    fn a_piece_cut_in_two_draws_what_it_drew() {
        for d in KINDS {
            let piece = pieces(d)[0];
            for cut in [0.25, 0.5, 0.8] {
                let (a, b) = piece.split(cut);
                assert_eq!(std::mem::discriminant(&a.seg), std::mem::discriminant(&piece.seg), "{d}: still the kind it was");
                assert!(a.to().distance(b.from) < 1e-12 && a.from == piece.from && b.to() == piece.to());
                for i in 0..=10 {
                    let t = i as f64 / 10.0;
                    let on = if t <= cut { a.at(t / cut) } else { b.at((t - cut) / (1.0 - cut)) };
                    assert!(on.distance(piece.at(t)) < 1e-9, "{d} cut at {cut}, at {t}: {on:?} vs {:?}", piece.at(t));
                }
                let middle = piece.part(cut * 0.5, cut);
                assert!(middle.from.distance(piece.at(cut * 0.5)) < 1e-9 && middle.to().distance(piece.at(cut)) < 1e-9 && middle.at(0.5).distance(piece.at(cut * 0.75)) < 1e-9, "{d}");
            }
        }
        // An arc's long way round is long only for the part that is.
        let (first, second) = pieces("M10 0 A10 10 0 1 0 0 10")[0].split(0.9);
        assert!(matches!(first.seg, Seg::Arc { arc, .. } if arc.large) && matches!(second.seg, Seg::Arc { arc, .. } if !arc.large));
        // Radii too small to reach are the ones it's drawn with.
        let (half, _) = pieces("M0 0 A1 1 0 0 1 10 0")[0].split(0.5);
        assert!(matches!(half.seg, Seg::Arc { arc, to } if (arc.rx - 5.0).abs() < 1e-9 && to.distance(v(5.0, -5.0)) < 1e-9), "{half:?}");
    }

    #[test]
    fn a_piece_walked_backwards_is_the_same_line() {
        for d in KINDS {
            let piece = pieces(d)[0];
            let back = piece.reversed();
            assert_eq!((back.from, back.to()), (piece.to(), piece.from));
            for t in [0.0, 0.3, 0.5, 1.0] {
                assert!(back.at(t).distance(piece.at(1.0 - t)) < 1e-9, "{d} at {t}");
            }
            assert_eq!(back.reversed(), piece);
        }
    }

    #[test]
    fn the_nearest_point_on_a_piece() {
        assert_eq!(pieces("M0 0 L10 0")[0].nearest(v(3.0, 7.0)), 0.3);
        assert_eq!(pieces("M0 0 L10 0")[0].nearest(v(-5.0, 1.0)), 0.0);
        for d in KINDS {
            let piece = pieces(d)[0];
            for want in [0.0, 0.2, 0.55, 1.0] {
                // A point a little off the piece, square to it there.
                let off = piece.heading(want).perp();
                let p = piece.at(want) + off * (0.01 / off.length().max(1e-9));
                let t = piece.nearest(p);
                assert!(piece.at(t).distance(piece.at(want)) < 1e-3, "{d}: wanted {want}, got {t}");
            }
        }
    }

    #[test]
    fn three_points_have_a_circle_unless_they_are_in_a_line() {
        let (centre, r) = circle_through(v(5.0, 0.0), v(0.0, 5.0), v(-5.0, 0.0)).unwrap();
        assert!(centre.distance(v(0.0, 0.0)) < 1e-12 && (r - 5.0).abs() < 1e-12);
        let (centre, r) = circle_through(v(1.0, 1.0), v(4.0, 5.0), v(9.0, 1.0)).unwrap();
        for p in [v(1.0, 1.0), v(4.0, 5.0), v(9.0, 1.0)] {
            assert!((centre.distance(p) - r).abs() < 1e-9);
        }
        assert_eq!(circle_through(v(0.0, 0.0), v(1.0, 1.0), v(2.0, 2.0)), None);
        assert_eq!(circle_through(v(0.0, 0.0), v(0.0, 0.0), v(2.0, 2.0)), None);
    }
}
