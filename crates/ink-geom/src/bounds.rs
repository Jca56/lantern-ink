//! The exact box around a path: its curves' true extremes, not their
//! control points and not a flattening.

use std::f64::consts::TAU;

use lntrn_math::{Rect, Vec2};

use crate::affine::Affine;
use crate::arc::{Centered, Shape, shape};
use crate::path::{Path, Seg};
use crate::piece::Piece;

/// A box that grows to hold the points it's given.
struct Grow(Option<Rect>);

impl Grow {
    fn add(&mut self, p: Vec2) {
        self.0 = Some(match self.0 {
            Some(r) => Rect::new(r.min.min(p), r.max.max(p)),
            None => Rect::new(p, p),
        });
    }
}

/// The roots of a·t² + b·t + c strictly between 0 and 1.
fn roots(a: f64, b: f64, c: f64) -> [Option<f64>; 2] {
    let inside = |t: f64| (t > 0.0 && t < 1.0).then_some(t);
    if a.abs() < 1e-12 * (b.abs() + c.abs()).max(1e-300) {
        return [if b != 0.0 { inside(-c / b) } else { None }, None];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [None, None];
    }
    // The stable form: no subtracting of near-equal numbers.
    let q = -0.5 * (b + b.signum() * disc.sqrt());
    [inside(q / a), if q != 0.0 { inside(c / q) } else { None }]
}

fn quad_at(p0: Vec2, c: Vec2, p1: Vec2, t: f64) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t)
}

fn cubic_at(p0: Vec2, c1: Vec2, c2: Vec2, p1: Vec2, t: f64) -> Vec2 {
    let u = 1.0 - t;
    p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t)
}

/// Where an arc, seen through `t`, turns back in x or in y, within its
/// sweep.
fn arc_extremes(a: &Centered, t: &Affine, grow: &mut Grow) {
    let (sin, cos) = a.phi.sin_cos();
    // Through `t` the arc is M (cos θ, sin θ) plus a move: the ellipse's
    // turn and radii, then `t`'s own stretch and turn.
    let (m00, m01) = ((t.a * cos + t.c * sin) * a.rx, (t.c * cos - t.a * sin) * a.ry);
    let (m10, m11) = ((t.b * cos + t.d * sin) * a.rx, (t.d * cos - t.b * sin) * a.ry);
    // dx/dθ = 0 and dy/dθ = 0, each twice a turn.
    let turning = [m01.atan2(m00), m11.atan2(m10)];
    let way = if a.delta < 0.0 { -1.0 } else { 1.0 };
    for angle in turning.into_iter().flat_map(|angle| [angle, angle + std::f64::consts::PI]) {
        // How far round from the start, the way the arc goes.
        let along = ((angle - a.theta) * way).rem_euclid(TAU);
        if along < a.delta.abs() {
            grow.add(t.apply(a.at(a.theta + way * along, 1.0)));
        }
    }
}

impl Path {
    /// The smallest box holding everything the path draws; `None` when it
    /// draws nothing (lone moves aren't drawn) or has a point that isn't
    /// a real number.
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds_through(&Affine::IDENTITY)
    }

    /// The smallest box holding the path once it's through `t`: around
    /// the turned or skewed outline itself, not around its own box's
    /// corners.
    pub fn bounds_through(&self, t: &Affine) -> Option<Rect> {
        let mut grow = Grow(None);
        for sub in self.subpaths.iter().filter(|s| !s.segs.is_empty() || s.closed) {
            grow.add(t.apply(sub.start));
            let mut at = sub.start;
            for seg in &sub.segs {
                reach(at, seg, t, &mut grow);
                at = seg.to();
            }
        }
        grow.0.filter(|r| r.min.is_finite() && r.max.is_finite())
    }
}

/// Grow a box to hold the segment `seg` drawn from `at`, seen through
/// `t`: its end, and wherever it turns back in x or in y on the way.
/// `at` is in the path's own coordinates (an arc is worked out there);
/// a Bézier's control points go through `t` with it.
fn reach(at: Vec2, seg: &Seg, t: &Affine, grow: &mut Grow) {
    let from = t.apply(at);
    match *seg {
        Seg::Line { .. } => {}
        Seg::Quad { c, to } => {
            let (c, to) = (t.apply(c), t.apply(to));
            let d = from - c * 2.0 + to;
            for (num, den) in [(from.x - c.x, d.x), (from.y - c.y, d.y)] {
                let at = num / den;
                if den != 0.0 && at > 0.0 && at < 1.0 {
                    grow.add(quad_at(from, c, to, at));
                }
            }
        }
        Seg::Cubic { c1, c2, to } => {
            let (c1, c2, to) = (t.apply(c1), t.apply(c2), t.apply(to));
            // The derivative, a quadratic, per axis.
            let (a, b, c) = (to - c2 * 3.0 + c1 * 3.0 - from, (c2 - c1 * 2.0 + from) * 2.0, c1 - from);
            for at in roots(a.x, b.x, c.x).into_iter().chain(roots(a.y, b.y, c.y)).flatten() {
                grow.add(cubic_at(from, c1, c2, to, at));
            }
        }
        Seg::Arc { ref arc, to } => {
            if let Shape::Arc(centered) = shape(at, arc, to) {
                arc_extremes(&centered, t, grow);
            }
        }
    }
    grow.add(t.apply(seg.to()));
}

impl Piece {
    /// The smallest box holding all of it.
    pub fn bounds(&self) -> Rect {
        let mut grow = Grow(None);
        grow.add(self.from);
        match self.centered() {
            // By the ellipse the piece is on, not one worked out again
            // from its ends.
            Some(arc) => {
                arc_extremes(&arc, &Affine::IDENTITY, &mut grow);
                grow.add(self.to());
            }
            None => reach(self.from, &self.seg, &Affine::IDENTITY, &mut grow),
        }
        grow.0.unwrap_or(Rect::new(self.from, self.from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::ArcTo;

    fn close(r: Rect, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
        r.min.distance(Vec2::new(x0, y0)) < 1e-9 && r.max.distance(Vec2::new(x1, y1)) < 1e-9
    }

    #[test]
    fn lines_and_shapes() {
        let b = Path::polyline(&[Vec2::new(3.0, 4.0), Vec2::new(-1.0, 9.0), Vec2::new(2.0, -2.0)], false).bounds().unwrap();
        assert!(close(b, -1.0, -2.0, 3.0, 9.0));
        assert!(close(Path::rect(1.0, 2.0, 10.0, 4.0, 3.0, 1.0).bounds().unwrap(), 1.0, 2.0, 11.0, 6.0), "rounded corners stay inside");
        assert!(close(Path::ellipse(Vec2::new(5.0, 5.0), 3.0, 2.0).bounds().unwrap(), 2.0, 3.0, 8.0, 7.0));
        assert_eq!(Path::new().bounds(), None);
        let mut lone = Path::new();
        lone.move_to(Vec2::new(5.0, 5.0));
        assert_eq!(lone.bounds(), None, "a lone move draws nothing");
        lone.close();
        assert!(close(lone.bounds().unwrap(), 5.0, 5.0, 5.0, 5.0), "a closed point is a dot");
    }

    #[test]
    fn curves_reach_only_as_far_as_they_go() {
        // The control points are at y = 100, the curve tops out at 75.
        let cubic = Path::parse("M0 0 C0 100 100 100 100 0").path.bounds().unwrap();
        assert!(close(cubic, 0.0, 0.0, 100.0, 75.0), "{cubic:?}");
        let quad = Path::parse("M0 0 Q50 100 100 0").path.bounds().unwrap();
        assert!(close(quad, 0.0, 0.0, 100.0, 50.0), "{quad:?}");
        // An S-bend overshoots both ways in x.
        let s = Path::parse("M0 0 C400 0 -300 100 100 100").path.bounds().unwrap();
        assert!((s.max.x - 128.4).abs() < 0.1 && (s.min.x + 28.4).abs() < 0.1, "{s:?}");
        // A straight "curve" is its ends.
        assert!(close(Path::parse("M0 0 C10 0 20 0 30 0").path.bounds().unwrap(), 0.0, 0.0, 30.0, 0.0));
    }

    #[test]
    fn arcs_reach_only_as_far_as_they_sweep() {
        // A quarter of a circle of radius 10 about the origin.
        let quarter = Path::parse("M10 0 A10 10 0 0 1 0 10").path.bounds().unwrap();
        assert!(close(quarter, 0.0, 0.0, 10.0, 10.0), "{quarter:?}");
        // The long way round between the same two points.
        let long = Path::parse("M10 0 A10 10 0 1 0 0 10").path.bounds().unwrap();
        assert!(close(long, -10.0, -10.0, 10.0, 10.0), "{long:?}");
        // A turned ellipse: its box is wider than rx and taller than ry.
        let mut p = Path::new();
        let (from, to) = (Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0001));
        p.move_to(from).arc_to(ArcTo { rx: 10.0, ry: 4.0, rotation: 45.0, large: true, sweep: true }, to);
        let b = p.bounds().unwrap();
        let half = (10.0f64 * 10.0 / 2.0 + 4.0 * 4.0 / 2.0).sqrt();
        assert!((b.width() - 2.0 * half).abs() < 1e-3 && (b.height() - 2.0 * half).abs() < 1e-3, "{b:?} vs {half}");
        // Against a fine flattening, for any arc.
        let odd = Path::parse("M3 1 A8 3 30 1 0 -2 6").path;
        let (exact, flat) = (odd.bounds().unwrap(), odd.flatten(1e-6));
        let (lo, hi) = flat[0].points.iter().fold((flat[0].points[0], flat[0].points[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        assert!(exact.min.distance(lo) < 1e-4 && exact.max.distance(hi) < 1e-4, "{exact:?} vs {lo:?} {hi:?}");
    }

    #[test]
    fn a_turned_path_has_the_box_of_its_turned_outline() {
        // A circle is its own box's size however it's turned; its box's
        // corners, turned, would reach further.
        let circle = Path::ellipse(Vec2::new(10.0, 10.0), 5.0, 5.0);
        let turned = circle.bounds_through(&Affine::rotate(0.7).about(Vec2::new(10.0, 10.0))).unwrap();
        assert!(close(turned, 5.0, 5.0, 15.0, 15.0), "{turned:?}");
        // Stretched and skewed, against the same path flattened finely
        // and put through point by point.
        let t = Affine::scale(2.0, 0.5).then(&Affine::skew_x(0.4)).then(&Affine::rotate(-1.1)).then(&Affine::translate(3.0, -8.0));
        for d in ["M3 1 A8 3 30 1 0 -2 6", "M0 0 C40 0 -30 10 10 10 Q20 30 0 20 Z", "M1 2 L5 9 L-4 3"] {
            let path = Path::parse(d).path;
            let exact = path.bounds_through(&t).unwrap();
            let points: Vec<Vec2> = path.flatten(1e-7).into_iter().flat_map(|l| l.points).map(|p| t.apply(p)).collect();
            let (lo, hi) = points.iter().fold((points[0], points[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)));
            assert!(exact.min.distance(lo) < 1e-5 && exact.max.distance(hi) < 1e-5, "{d}: {exact:?} vs {lo:?} {hi:?}");
        }
        assert_eq!(Path::new().bounds_through(&t), None);
    }
}
