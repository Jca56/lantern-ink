//! Strokes as polygons, the SVG spec's way: a rectangle along each
//! segment, a join at each corner and a cap at each open end. Every piece
//! winds the same way, so they fill together non-zero, and where two meet
//! they share an edge, which an exact-area rasterizer cancels: no seams.
//! Dashes cut the line into shorter ones first. All in the shape's own
//! coordinates: the outline goes through the element's transform
//! afterwards, so a stretched stroke stretches.

use std::f64::consts::{PI, TAU};

use lntrn_math::Vec2;

use crate::dash::dashed;
use crate::flatten::{Polyline, sane, turn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    /// Flat at the end.
    Butt,
    Round,
    /// Flat, half the width past the end.
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// Pointed, until the point is `miter_limit` stroke widths long; a
    /// bevel past that.
    Miter,
    Round,
    Bevel,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    pub miter_limit: f64,
    /// On and off lengths in turn; empty for a solid line.
    pub dashes: Vec<f64>,
    pub dash_offset: f64,
}

impl Default for Stroke {
    /// SVG's initial values: 1 wide, butt caps, mitred to 4 widths, solid.
    fn default() -> Stroke {
        Stroke { width: 1.0, cap: Cap::Butt, join: Join::Miter, miter_limit: 4.0, dashes: Vec::new(), dash_offset: 0.0 }
    }
}

/// The most points one stroke's outline takes; what's left of the line
/// past it isn't stroked.
const MAX_OUTLINE: usize = 2_000_000;

/// The outline of `lines` stroked with `st`: polygons that all wind the
/// same way, to fill non-zero. Round joins and caps are cut within `tol`.
pub fn stroke(lines: &[Polyline], st: &Stroke, tol: f64) -> Vec<Vec<Vec2>> {
    let mut out = Out { polys: Vec::new(), points: 0 };
    if !(st.width.is_finite() && st.width > 0.0) {
        return out.polys;
    }
    let pen = Pen { h: st.width / 2.0, cap: st.cap, join: st.join, miter_limit: st.miter_limit, tol: sane(tol) };
    for line in lines {
        let pts = clean(&line.points, line.closed, pen.tol * 1e-3);
        match dashed(&pts, line.closed, st) {
            Some(pieces) => pieces.iter().for_each(|piece| pen.polyline(&piece.points, piece.closed, &mut out)),
            None => pen.polyline(&pts, line.closed, &mut out),
        }
    }
    out.polys
}

struct Out {
    polys: Vec<Vec<Vec2>>,
    points: usize,
}

impl Out {
    /// Take `poly`, turned to wind the way every piece does. One with no
    /// area adds nothing.
    fn push(&mut self, mut poly: Vec<Vec2>) {
        if self.points + poly.len() > MAX_OUTLINE {
            return;
        }
        let area: f64 = poly.iter().zip(poly.iter().cycle().skip(1)).map(|(a, b)| a.perp_dot(*b)).sum();
        if area < 0.0 {
            poly.reverse();
        }
        if area != 0.0 && area.is_finite() {
            self.points += poly.len();
            self.polys.push(poly);
        }
    }
}

/// `pts` without the points that repeat the one before (within `eps`), a
/// closed one's last against its first too, and without any that aren't
/// real numbers.
fn clean(pts: &[Vec2], closed: bool, eps: f64) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::with_capacity(pts.len());
    for &p in pts.iter().filter(|p| p.is_finite()) {
        if out.last().is_none_or(|q| p.distance(*q) > eps) {
            out.push(p);
        }
    }
    while closed && out.len() > 1 && out[out.len() - 1].distance(out[0]) <= eps {
        out.pop();
    }
    out
}

struct Pen {
    /// Half the width.
    h: f64,
    cap: Cap,
    join: Join,
    miter_limit: f64,
    tol: f64,
}

impl Pen {
    fn polyline(&self, raw: &[Vec2], closed: bool, out: &mut Out) {
        let pts = clean(raw, closed, self.tol * 1e-3);
        let n = pts.len();
        if n == 0 {
            return;
        }
        if n == 1 {
            // A line of no length is a dot, if the caps give it a shape
            // (a square one sits level: there's no direction to turn to).
            let (p, h) = (pts[0], self.h);
            match self.cap {
                Cap::Butt => {}
                Cap::Round => out.push(self.sector(p, Vec2::new(h, 0.0), TAU)),
                Cap::Square => out.push(vec![p + Vec2::new(-h, -h), p + Vec2::new(h, -h), p + Vec2::new(h, h), p + Vec2::new(-h, h)]),
            }
            return;
        }
        let segs = if closed { n } else { n - 1 };
        let dirs: Vec<Vec2> = (0..segs).map(|i| (pts[(i + 1) % n] - pts[i]).normalize()).collect();
        for i in 0..segs {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let o = dirs[i].perp() * self.h;
            out.push(vec![a + o, b + o, b - o, a - o]);
        }
        for i in 1..segs {
            self.corner(pts[i], dirs[i - 1], dirs[i], out);
        }
        if closed {
            self.corner(pts[0], dirs[segs - 1], dirs[0], out);
        } else {
            self.end(pts[0], -dirs[0], out);
            self.end(pts[n - 1], dirs[segs - 1], out);
        }
    }

    /// The corner at `v`, arriving along `d1` and leaving along `d2`: the
    /// gap the two rectangles leave on the outside of the turn.
    fn corner(&self, v: Vec2, d1: Vec2, d2: Vec2, out: &mut Out) {
        let (cross, dot) = (d1.perp_dot(d2), d1.dot(d2));
        if cross.abs() < 1e-12 && dot > 0.0 {
            return; // straight on
        }
        // The outside is the side turned away from.
        let side = if cross > 0.0 { -self.h } else { self.h };
        let (o1, o2) = (d1.perp() * side, d2.perp() * side);
        match self.join {
            Join::Bevel => out.push(vec![v, v + o1, v + o2]),
            Join::Miter => {
                // The point's length over the width is 1 / cos(half the
                // turn).
                let half = ((1.0 + dot) * 0.5).max(0.0).sqrt();
                if half > 1e-9 && 1.0 / half <= self.miter_limit {
                    out.push(vec![v, v + o1, v + (o1 + o2) / (1.0 + dot), v + o2]);
                } else {
                    out.push(vec![v, v + o1, v + o2]);
                }
            }
            Join::Round => {
                // Round the outside, whichever way that is: by the side,
                // not the sign of a cross product that is nothing when
                // the line doubles straight back.
                let turn = o1.perp_dot(o2).atan2(o1.dot(o2)).abs();
                out.push(self.sector(v, o1, if side > 0.0 { -turn } else { turn }));
            }
        }
    }

    /// The end at `p`, where the line runs out along `d`.
    fn end(&self, p: Vec2, d: Vec2, out: &mut Out) {
        let o = d.perp() * self.h;
        match self.cap {
            Cap::Butt => {}
            Cap::Square => {
                let e = d * self.h;
                out.push(vec![p + o, p + o + e, p - o + e, p - o]);
            }
            // From one side round through `d` to the other.
            Cap::Round => out.push(self.sector(p, o, -PI)),
        }
    }

    /// A slice of the pen's circle about `c`: from `c + from` round by
    /// `sweep` radians, with `c` itself.
    fn sector(&self, c: Vec2, from: Vec2, sweep: f64) -> Vec<Vec2> {
        let mut poly = vec![c, c + from];
        turn(c, from, sweep, self.tol, &mut poly);
        poly
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const TOL: f64 = 0.05;

    pub(crate) fn line(pts: &[(f64, f64)], closed: bool) -> Vec<Polyline> {
        vec![Polyline { points: pts.iter().map(|&(x, y)| Vec2::new(x, y)).collect(), closed }]
    }

    fn pen(width: f64, cap: Cap, join: Join) -> Stroke {
        Stroke { width, cap, join, ..Stroke::default() }
    }

    /// Whether the pieces' union covers a point: its winding number
    /// isn't 0.
    pub(crate) fn covers(polys: &[Vec<Vec2>], x: f64, y: f64) -> bool {
        let mut wind = 0;
        for poly in polys {
            for (a, b) in poly.iter().zip(poly.iter().cycle().skip(1)) {
                if (a.y <= y) != (b.y <= y) && a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y) > x {
                    wind += if b.y > a.y { 1 } else { -1 };
                }
            }
        }
        wind != 0
    }

    fn area(polys: &[Vec<Vec2>]) -> f64 {
        polys.iter().map(|p| p.iter().zip(p.iter().cycle().skip(1)).map(|(a, b)| a.perp_dot(*b)).sum::<f64>() / 2.0).sum()
    }

    #[test]
    fn a_segment_is_a_rectangle_and_caps_reach_past_it() {
        let l = line(&[(10.0, 10.0), (30.0, 10.0)], false);
        let butt = stroke(&l, &pen(4.0, Cap::Butt, Join::Miter), TOL);
        assert_eq!(butt.len(), 1);
        assert!(covers(&butt, 20.0, 11.9) && !covers(&butt, 20.0, 12.1), "half the width each side");
        assert!(!covers(&butt, 9.5, 10.0) && !covers(&butt, 30.5, 10.0), "butt ends stop at the points");
        let square = stroke(&l, &pen(4.0, Cap::Square, Join::Miter), TOL);
        assert!(covers(&square, 8.1, 11.9) && covers(&square, 31.9, 8.1), "square caps: half a width on, corners and all");
        assert!(!covers(&square, 7.9, 10.0));
        let round = stroke(&l, &pen(4.0, Cap::Round, Join::Miter), TOL);
        assert!(covers(&round, 8.1, 10.0) && covers(&round, 31.9, 10.0), "round caps reach half a width on");
        assert!(!covers(&round, 8.3, 11.7) && !covers(&round, 31.7, 8.3), "and are round");
        // A round-capped line holds exactly its area: the rectangle and
        // one whole disc.
        assert!((area(&round) - (20.0 * 4.0 + PI * 4.0)).abs() < 1e-9, "{}", area(&round));
    }

    #[test]
    fn corners_join_as_asked() {
        // Right along the top, then down: the outside of the turn is the
        // top-right, and the miter's tip is at (22, 8).
        let l = line(&[(0.0, 10.0), (20.0, 10.0), (20.0, 30.0)], false);
        let (tip, rounded) = ((21.8, 8.2), (21.2, 8.8));
        let miter = stroke(&l, &pen(4.0, Cap::Butt, Join::Miter), TOL);
        assert!(covers(&miter, tip.0, tip.1), "a miter fills the corner to its tip");
        let bevel = stroke(&l, &pen(4.0, Cap::Butt, Join::Bevel), TOL);
        assert!(!covers(&bevel, rounded.0, rounded.1) && covers(&bevel, 20.9, 9.1), "a bevel cuts it straight across");
        let round = stroke(&l, &pen(4.0, Cap::Butt, Join::Round), TOL);
        assert!(covers(&round, rounded.0, rounded.1) && !covers(&round, tip.0, tip.1), "a round join fills the arc and no more");
        // The inside of the turn is covered by the rectangles themselves.
        for o in [&miter, &bevel, &round] {
            assert!(covers(o, 19.0, 11.0));
        }
        // A closed line joins at its start as well.
        let tri = line(&[(0.0, 0.0), (20.0, 0.0), (20.0, 20.0)], true);
        let closed = stroke(&tri, &pen(2.0, Cap::Butt, Join::Round), TOL);
        assert!(covers(&closed, -0.6, -0.3), "the start's corner is joined");
    }

    #[test]
    fn a_sharp_miter_falls_back_to_a_bevel() {
        // About 11 degrees: its miter would be ten widths long.
        let spike = line(&[(0.0, 0.0), (40.0, 4.0), (0.0, 8.0)], false);
        let limited = stroke(&spike, &pen(2.0, Cap::Butt, Join::Miter), TOL);
        assert!(!covers(&limited, 43.0, 4.0), "past the limit: bevelled");
        let long = Stroke { miter_limit: 20.0, ..pen(2.0, Cap::Butt, Join::Miter) };
        assert!(covers(&stroke(&spike, &long, TOL), 43.0, 4.0), "within a larger limit: mitred");
        // Straight back on itself has no miter at all, and doesn't divide by zero.
        let back = line(&[(0.0, 0.0), (10.0, 0.0), (0.0, 0.0)], false);
        assert!(stroke(&back, &long, TOL).iter().flatten().all(|p| p.is_finite()));
    }

    #[test]
    fn every_piece_winds_the_same_way() {
        let zig = line(&[(0.0, 0.0), (10.0, 5.0), (0.0, 10.0), (10.0, 15.0), (-3.0, 4.0)], true);
        for join in [Join::Miter, Join::Round, Join::Bevel] {
            for cap in [Cap::Butt, Cap::Round, Cap::Square] {
                for poly in stroke(&zig, &pen(3.0, cap, join), TOL) {
                    assert!(area(&[poly]) > 0.0, "{join:?} {cap:?}");
                }
            }
        }
    }

    #[test]
    fn bad_strokes_are_no_stroke() {
        let l = line(&[(0.0, 0.0), (100.0, 0.0)], false);
        for w in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(stroke(&l, &pen(w, Cap::Round, Join::Round), TOL).is_empty(), "{w}");
        }
        // A point that repeats makes no corner of its own, and one that
        // isn't a number is passed over.
        let dup = line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 0.0), (f64::NAN, 3.0), (20.0, 0.0)], false);
        assert_eq!(stroke(&dup, &pen(2.0, Cap::Butt, Join::Round), TOL).len(), 2);
        // A dot: only round and square caps give it a shape.
        let dot = line(&[(5.0, 5.0)], true);
        assert!(stroke(&dot, &pen(4.0, Cap::Butt, Join::Round), TOL).is_empty());
        assert!(covers(&stroke(&dot, &pen(4.0, Cap::Round, Join::Round), TOL), 6.3, 6.3));
        assert!(covers(&stroke(&dot, &pen(4.0, Cap::Square, Join::Round), TOL), 6.9, 6.9));
        // There and back, closed: joined at both ends, not capped.
        let back = line(&[(0.0, 0.0), (10.0, 0.0)], true);
        let joined = stroke(&back, &pen(4.0, Cap::Square, Join::Round), TOL);
        assert!(covers(&joined, -1.5, 0.0) && !covers(&joined, -1.9, 1.9), "round joins, though the caps are square");
    }
}
