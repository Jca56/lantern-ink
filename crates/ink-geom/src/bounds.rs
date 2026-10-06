//! The exact box around a path: its curves' true extremes, not their
//! control points and not a flattening.

use std::f64::consts::TAU;

use lntrn_math::{Rect, Vec2};

use crate::arc::{Centered, Shape, shape};
use crate::path::{Path, Seg};

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

/// Where an arc turns back in x or in y, within its sweep.
fn arc_extremes(a: &Centered, grow: &mut Grow) {
    let (sin, cos) = a.phi.sin_cos();
    // dx/dt = 0 and dy/dt = 0 on the turned ellipse, each twice a turn.
    let turning = [(-sin * a.ry).atan2(cos * a.rx), (cos * a.ry).atan2(sin * a.rx)];
    let way = if a.delta < 0.0 { -1.0 } else { 1.0 };
    for t in turning.into_iter().flat_map(|t| [t, t + std::f64::consts::PI]) {
        // How far round from the start, the way the arc goes.
        let along = ((t - a.theta) * way).rem_euclid(TAU);
        if along < a.delta.abs() {
            grow.add(a.at(a.theta + way * along, 1.0));
        }
    }
}

impl Path {
    /// The smallest box holding everything the path draws; `None` when it
    /// draws nothing (lone moves aren't drawn) or has a point that isn't
    /// a real number.
    pub fn bounds(&self) -> Option<Rect> {
        let mut grow = Grow(None);
        for sub in self.subpaths.iter().filter(|s| !s.segs.is_empty() || s.closed) {
            grow.add(sub.start);
            let mut at = sub.start;
            for seg in &sub.segs {
                match *seg {
                    Seg::Line { .. } => {}
                    Seg::Quad { c, to } => {
                        let d = at - c * 2.0 + to;
                        for (num, den) in [(at.x - c.x, d.x), (at.y - c.y, d.y)] {
                            let t = num / den;
                            if den != 0.0 && t > 0.0 && t < 1.0 {
                                grow.add(quad_at(at, c, to, t));
                            }
                        }
                    }
                    Seg::Cubic { c1, c2, to } => {
                        // The derivative, a quadratic in t, per axis.
                        let (a, b, c) = (to - c2 * 3.0 + c1 * 3.0 - at, (c2 - c1 * 2.0 + at) * 2.0, c1 - at);
                        for t in roots(a.x, b.x, c.x).into_iter().chain(roots(a.y, b.y, c.y)).flatten() {
                            grow.add(cubic_at(at, c1, c2, to, t));
                        }
                    }
                    Seg::Arc { ref arc, to } => {
                        if let Shape::Arc(centered) = shape(at, arc, to) {
                            arc_extremes(&centered, &mut grow);
                        }
                    }
                }
                grow.add(seg.to());
                at = seg.to();
            }
        }
        grow.0.filter(|r| r.min.is_finite() && r.max.is_finite())
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
}
