//! Curves as polylines, never further than a tolerance from the true
//! curve. Béziers are split by Wang's formula. Arcs are sampled on the
//! ellipse itself. An arc's two ends stay on the curve, where the path
//! goes on from them; the vertices between are set a little outside it,
//! just enough that the polygon holds the curve's exact area (an
//! inscribed one is always short: 6 % on a 1 px round cap).

use lntrn_math::Vec2;

use crate::arc::{Centered, Shape, shape};
use crate::path::{Path, Seg};

/// The most segments one curve becomes, however big.
const MAX_SEGMENTS: usize = 1 << 16;
/// The most points one path becomes; what's left of it past that isn't
/// drawn (a hostile file can't ask for the memory).
const MAX_POINTS: usize = 1 << 21;

/// A run of points: a polygon when `closed` (its first point not
/// repeated at the end), else an open line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline {
    pub points: Vec<Vec2>,
    pub closed: bool,
}

/// A tolerance that can be used: `tol`, or a small default when it's
/// nothing, negative or not a number.
pub(crate) fn sane(tol: f64) -> f64 {
    if tol.is_finite() && tol > 0.0 { tol } else { 0.05 }
}

/// How many pieces keep a Bézier within `tol`, by Wang's formula:
/// n² ≥ `factor` · (its largest second difference) / `tol`.
fn pieces(bend: f64, factor: f64, tol: f64) -> usize {
    let n = (factor * bend / tol).sqrt().ceil();
    if n.is_finite() { (n as usize).clamp(1, MAX_SEGMENTS) } else { MAX_SEGMENTS }
}

/// The quadratic `p0 c p1` as the points after `p0`, onto `out`.
pub(crate) fn quad(p0: Vec2, c: Vec2, p1: Vec2, tol: f64, out: &mut Vec<Vec2>) {
    let n = pieces((p0 - c * 2.0 + p1).length(), 0.25, tol);
    for i in 1..n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        out.push(p0 * (u * u) + c * (2.0 * u * t) + p1 * (t * t));
    }
    out.push(p1);
}

/// The cubic `p0 c1 c2 p1` as the points after `p0`, onto `out`.
pub(crate) fn cubic(p0: Vec2, c1: Vec2, c2: Vec2, p1: Vec2, tol: f64, out: &mut Vec<Vec2>) {
    let bend = (p0 - c1 * 2.0 + c2).length().max((c1 - c2 * 2.0 + p1).length());
    let n = pieces(bend, 0.75, tol);
    for i in 1..n {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        out.push(p0 * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + p1 * (t * t * t));
    }
    out.push(p1);
}

/// How many chords an arc of `sweep` radians at radius `r` takes to stay
/// within `tol`, and the factor `k` that sets the vertices between its
/// ends out so the polygon holds the curve's area. Seen from the centre,
/// the curve's n slices hold r² step / 2 each; the polygon's two end
/// slices (one vertex on the curve) hold k r² sin(step) / 2 and the rest
/// k² r² sin(step) / 2, so k solves (n - 2) k² + 2 k = n step / sin(step).
/// One chord has no vertex to set out, and is short by less than `tol`.
pub(crate) fn chords(sweep: f64, r: f64, tol: f64) -> (usize, f64) {
    // The widest angle a chord may span while its middle stays within
    // the tolerance of the arc (its sagitta).
    let widest = if r <= tol { std::f64::consts::FRAC_PI_2 } else { 2.0 * (1.0 - tol / r).acos() };
    let n = (sweep.abs() / widest).ceil();
    let mut n = if n.is_finite() { (n as usize).clamp(1, MAX_SEGMENTS) } else { MAX_SEGMENTS };
    loop {
        let step = sweep.abs() / n as f64;
        let s = if step > 1e-12 { step / step.sin() } else { 1.0 };
        let k = match n {
            1 => 1.0,
            2 => s,
            _ => (((n - 2) as f64 * n as f64 * s + 1.0).sqrt() - 1.0) / (n - 2) as f64,
        };
        // Set out further than the tolerance (two or three chords at
        // their widest): one more chord brings it back in.
        if (k - 1.0) * r <= tol || n >= MAX_SEGMENTS {
            return (n, k);
        }
        n += 1;
    }
}

/// The arc `a` as the points after its start, onto `out`, ending on `to`
/// exactly.
pub(crate) fn ellipse_arc(a: &Centered, to: Vec2, tol: f64, out: &mut Vec<Vec2>) {
    let (n, k) = chords(a.delta, a.rx.max(a.ry), tol);
    for i in 1..n {
        out.push(a.at(a.theta + a.delta * (i as f64 / n as f64), k));
    }
    out.push(to);
}

/// A slice of the circle about `c` through `c + from`, round by `sweep`
/// radians (clockwise on screen when positive): the points after the
/// first, onto `out`, the last on the circle and the ones between set
/// out to hold its area.
pub(crate) fn turn(c: Vec2, from: Vec2, sweep: f64, tol: f64, out: &mut Vec<Vec2>) {
    let (n, k) = chords(sweep, from.length(), tol);
    for i in 1..=n {
        let k = if i == n { 1.0 } else { k };
        out.push(c + from.rotate(sweep * (i as f64 / n as f64)) * k);
    }
}

impl Path {
    /// The path as polylines, its curves within `tol` of the true ones.
    /// A subpath that is one point is kept only when closed (a dot, to
    /// round or square caps); one with a point that isn't a real number
    /// is left out.
    pub fn flatten(&self, tol: f64) -> Vec<Polyline> {
        let tol = sane(tol);
        let mut out = Vec::new();
        let mut budget = MAX_POINTS;
        for sub in &self.subpaths {
            let mut points = vec![sub.start];
            let mut at = sub.start;
            for seg in &sub.segs {
                match *seg {
                    Seg::Line { to } => points.push(to),
                    Seg::Quad { c, to } => quad(at, c, to, tol, &mut points),
                    Seg::Cubic { c1, c2, to } => cubic(at, c1, c2, to, tol, &mut points),
                    Seg::Arc { ref arc, to } => match shape(at, arc, to) {
                        Shape::Nothing => {}
                        Shape::Line => points.push(to),
                        Shape::Arc(centered) => ellipse_arc(&centered, to, tol, &mut points),
                    },
                }
                at = seg.to();
                if points.len() > budget {
                    break;
                }
            }
            // A closed one doesn't repeat its first point.
            while sub.closed && points.len() > 1 && points.last() == points.first() {
                points.pop();
            }
            let over = points.len() > budget;
            budget = budget.saturating_sub(points.len());
            if (points.len() >= 2 || sub.closed) && points.iter().all(|p| p.is_finite()) {
                out.push(Polyline { points, closed: sub.closed });
            }
            if over {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;
    use crate::path::ArcTo;

    const TOL: f64 = 0.05;

    /// Twice the signed area of a polygon.
    fn area2(p: &[Vec2]) -> f64 {
        p.iter().zip(p.iter().cycle().skip(1)).map(|(a, b)| a.perp_dot(*b)).sum()
    }

    #[test]
    fn a_cubic_stays_within_tolerance_and_ends_where_it_should() {
        let (p0, p1, p2, p3) = (Vec2::new(0.0, 0.0), Vec2::new(300.0, 900.0), Vec2::new(600.0, -900.0), Vec2::new(900.0, 0.0));
        let mut pts = vec![p0];
        cubic(p0, p1, p2, p3, TOL, &mut pts);
        assert_eq!(*pts.last().unwrap(), p3);
        // Midpoints of the chords against the curve at the same parameter.
        let n = pts.len() - 1;
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            let u = 1.0 - t;
            let on = p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t);
            let mid = (pts[i] + pts[i + 1]) * 0.5;
            assert!(mid.distance(on) <= TOL * 1.01, "segment {i} strays {}", mid.distance(on));
        }
        // A finer tolerance takes more points, a straight "curve" one.
        let mut fine = vec![p0];
        cubic(p0, p1, p2, p3, TOL / 100.0, &mut fine);
        assert!(fine.len() > pts.len() * 5);
        let mut straight = Vec::new();
        cubic(Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(20.0, 0.0), Vec2::new(30.0, 0.0), TOL, &mut straight);
        assert_eq!(straight, vec![Vec2::new(30.0, 0.0)]);
    }

    #[test]
    fn a_quadratic_stays_within_tolerance() {
        let (p0, c, p1) = (Vec2::ZERO, Vec2::new(500.0, 800.0), Vec2::new(1000.0, 0.0));
        let mut pts = vec![p0];
        quad(p0, c, p1, TOL, &mut pts);
        let n = pts.len() - 1;
        for i in 0..n {
            let t = (i as f64 + 0.5) / n as f64;
            let on = p0 * ((1.0 - t) * (1.0 - t)) + c * (2.0 * (1.0 - t) * t) + p1 * (t * t);
            assert!(((pts[i] + pts[i + 1]) * 0.5).distance(on) <= TOL * 1.01);
        }
        assert_eq!(*pts.last().unwrap(), p1);
    }

    #[test]
    fn a_huge_ellipse_is_within_tolerance_and_holds_its_area() {
        let (c, rx, ry) = (Vec2::new(10.0, -4.0), 6000.0, 2500.0);
        let polys = Path::ellipse(c, rx, ry).flatten(TOL);
        assert_eq!(polys.len(), 1);
        let pts = &polys[0].points;
        assert!(polys[0].closed && pts.first() != pts.last(), "closed, its first point not repeated");
        for p in pts {
            let on = ((p.x - c.x) / rx).hypot((p.y - c.y) / ry);
            assert!((on - 1.0).abs() * ry <= TOL, "a vertex strays {}", (on - 1.0).abs() * ry);
        }
        assert!((area2(pts) / 2.0 / (PI * rx * ry) - 1.0).abs() < 1e-9, "area {}", area2(pts) / 2.0);
        assert!(area2(pts) > 0.0, "clockwise on screen, as SVG draws it");
    }

    #[test]
    fn a_rounded_rect_holds_its_area_too() {
        let pts = &Path::rect(0.0, 0.0, 40.0, 20.0, 6.0, 4.0).flatten(TOL)[0].points;
        let exact = 40.0 * 20.0 - (4.0 - PI) * 6.0 * 4.0;
        assert!((area2(pts) / 2.0 - exact).abs() < 1e-9, "{} vs {exact}", area2(pts) / 2.0);
        let (lo, hi) = pts.iter().fold((pts[0], pts[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        assert!(lo.distance(Vec2::ZERO) < 1e-9 && hi.distance(Vec2::new(40.0, 20.0)) < 1e-9, "the corners' ends are on the edges");
    }

    #[test]
    fn a_turned_arc_ends_on_its_end_point() {
        let mut p = Path::new();
        let to = Vec2::new(-2.0, 6.0);
        p.move_to(Vec2::new(3.0, 1.0)).arc_to(ArcTo { rx: 8.0, ry: 3.0, rotation: 30.0, large: true, sweep: false }, to);
        let line = &p.flatten(0.01)[0];
        assert!(!line.closed && *line.points.last().unwrap() == to && line.points.len() > 20);
    }

    #[test]
    fn subpaths_that_draw_nothing_are_left_out() {
        let mut p = Path::new();
        p.move_to(Vec2::new(5.0, 5.0));
        assert!(p.flatten(TOL).is_empty(), "a lone move");
        p.close();
        let dot = p.flatten(TOL);
        assert_eq!((dot[0].points.len(), dot[0].closed), (1, true), "a closed point is a dot");
        let mut bad = Path::new();
        bad.move_to(Vec2::ZERO).line_to(Vec2::new(f64::NAN, 1.0)).move_to(Vec2::ONE).line_to(Vec2::X);
        assert_eq!(bad.flatten(TOL).len(), 1, "the subpath with a point that isn't a number is dropped");
        // A closing point that repeats the start isn't kept twice.
        let sq = Path::polyline(&[Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::ZERO], true).flatten(TOL);
        assert_eq!(sq[0].points.len(), 3);
    }

    #[test]
    fn a_hostile_path_stops_at_the_point_budget() {
        let mut p = Path::new();
        p.move_to(Vec2::ZERO);
        for _ in 0..200 {
            p.cubic_to(Vec2::new(3e30, 0.0), Vec2::new(0.0, 3e30), Vec2::ONE);
        }
        let n: usize = p.flatten(1e-6).iter().map(|l| l.points.len()).sum();
        assert!(n <= MAX_POINTS + MAX_SEGMENTS + 1, "{n}");
    }

    #[test]
    fn a_turn_ends_on_the_circle_and_bulges_between() {
        let mut pts = Vec::new();
        turn(Vec2::ZERO, Vec2::new(3.0, 0.0), PI / 2.0, TOL, &mut pts);
        assert!(pts.last().unwrap().distance(Vec2::new(0.0, 3.0)) < 1e-12);
        assert!(pts[0].length() > 3.0, "set out between");
        assert!(pts.iter().all(|p| p.x >= -1e-9 && p.y >= -1e-9), "clockwise on screen: toward +y");
    }
}
