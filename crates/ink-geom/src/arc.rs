//! Elliptical arcs worked out from how SVG writes them (two end points,
//! radii, a turn and two flags) into a centre and a sweep of angle, as
//! the SVG spec's implementation notes convert them.

use std::f64::consts::TAU;

use lntrn_math::Vec2;

use crate::path::ArcTo;

/// An arc as a centre and a range of angle on its ellipse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Centered {
    pub c: Vec2,
    pub rx: f64,
    pub ry: f64,
    /// The ellipse's turn, radians.
    pub phi: f64,
    /// The angle it starts at, on the unturned ellipse.
    pub theta: f64,
    /// How far round it goes from there: positive is clockwise on screen.
    pub delta: f64,
}

/// What an `A` segment comes to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Shape {
    /// Its ends are the same point: SVG leaves the segment out.
    Nothing,
    /// A radius of nothing: a straight line to its end.
    Line,
    Arc(Centered),
}

pub(crate) fn shape(from: Vec2, arc: &ArcTo, to: Vec2) -> Shape {
    if from == to {
        return Shape::Nothing;
    }
    let (mut rx, mut ry) = (arc.rx.abs(), arc.ry.abs());
    if !(rx > 0.0 && ry > 0.0 && rx.is_finite() && ry.is_finite() && arc.rotation.is_finite()) {
        return Shape::Line;
    }
    let phi = arc.rotation.to_radians();
    let (sin, cos) = phi.sin_cos();
    // The ends' midpoint as the origin, the ellipse unturned.
    let half = (from - to) * 0.5;
    let (x1, y1) = (cos * half.x + sin * half.y, -sin * half.x + cos * half.y);
    // Radii too small to reach both ends grow until they just do.
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = if den > 0.0 { (num / den).sqrt() } else { 0.0 };
    if arc.large == arc.sweep {
        coef = -coef;
    }
    let (cx1, cy1) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
    let mid = (from + to) * 0.5;
    let c = Vec2::new(cos * cx1 - sin * cy1 + mid.x, sin * cx1 + cos * cy1 + mid.y);
    // The turn from u to v, by atan2: it keeps a turn of a few millionths
    // (a full circle drawn as one arc that stops just short of its
    // start), which an arc cosine would round to none.
    let angle = |u: Vec2, v: Vec2| u.perp_dot(v).atan2(u.dot(v));
    let u = Vec2::new((x1 - cx1) / rx, (y1 - cy1) / ry);
    let v = Vec2::new((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let theta = angle(Vec2::X, u);
    let mut delta = angle(u, v);
    if !arc.sweep && delta > 0.0 {
        delta -= TAU;
    } else if arc.sweep && delta < 0.0 {
        delta += TAU;
    }
    if !(c.is_finite() && theta.is_finite() && delta.is_finite()) {
        return Shape::Line;
    }
    Shape::Arc(Centered { c, rx, ry, phi, theta, delta })
}

impl Centered {
    /// The point at angle `t` on the unturned ellipse, its radii
    /// stretched by `k`.
    pub fn at(&self, t: f64, k: f64) -> Vec2 {
        let (sin, cos) = self.phi.sin_cos();
        let (x, y) = (k * self.rx * t.cos(), k * self.ry * t.sin());
        Vec2::new(cos * x - sin * y + self.c.x, sin * x + cos * y + self.c.y)
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    fn arc(rx: f64, ry: f64, rotation: f64, large: bool, sweep: bool) -> ArcTo {
        ArcTo { rx, ry, rotation, large, sweep }
    }

    fn centered(from: Vec2, a: ArcTo, to: Vec2) -> Centered {
        match shape(from, &a, to) {
            Shape::Arc(c) => c,
            other => panic!("not an arc: {other:?}"),
        }
    }

    #[test]
    fn a_half_circle_has_its_centre_between_its_ends() {
        let c = centered(Vec2::ZERO, arc(5.0, 5.0, 0.0, false, true), Vec2::new(10.0, 0.0));
        assert!(c.c.distance(Vec2::new(5.0, 0.0)) < 1e-12);
        assert!((c.theta.abs() - PI).abs() < 1e-12 && (c.delta - PI).abs() < 1e-12, "{c:?}");
        // Sweeping clockwise from the left end goes over the top (y up
        // the screen is negative).
        assert!(c.at(c.theta + c.delta / 2.0, 1.0).distance(Vec2::new(5.0, -5.0)) < 1e-12);
        assert!(c.at(c.theta, 1.0).distance(Vec2::ZERO) < 1e-12 && c.at(c.theta + c.delta, 1.0).distance(Vec2::new(10.0, 0.0)) < 1e-12);
    }

    #[test]
    fn the_flags_pick_among_the_four_arcs() {
        let (from, to) = (Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0));
        let small = centered(from, arc(10.0, 10.0, 0.0, false, true), to);
        assert!(small.c.distance(Vec2::ZERO) < 1e-9 && (small.delta - FRAC_PI_2).abs() < 1e-12);
        let large = centered(from, arc(10.0, 10.0, 0.0, true, true), to);
        assert!(large.c.distance(Vec2::new(10.0, 10.0)) < 1e-9 && (large.delta - 3.0 * FRAC_PI_2).abs() < 1e-12);
        let back = centered(from, arc(10.0, 10.0, 0.0, false, false), to);
        assert!(back.c.distance(Vec2::new(10.0, 10.0)) < 1e-9 && (back.delta + FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn radii_too_small_grow_and_odd_ones_are_lines() {
        // Ends 10 apart with a radius of 1: a half circle of radius 5.
        let c = centered(Vec2::ZERO, arc(1.0, 1.0, 0.0, false, true), Vec2::new(10.0, 0.0));
        assert!((c.rx - 5.0).abs() < 1e-12 && (c.ry - 5.0).abs() < 1e-12);
        assert_eq!(shape(Vec2::ZERO, &arc(0.0, 5.0, 0.0, false, true), Vec2::X), Shape::Line);
        assert_eq!(shape(Vec2::ZERO, &arc(f64::NAN, 5.0, 0.0, false, true), Vec2::X), Shape::Line);
        assert_eq!(shape(Vec2::X, &arc(5.0, 5.0, 0.0, true, true), Vec2::X), Shape::Nothing);
        // A negative radius is its size.
        assert!(matches!(shape(Vec2::ZERO, &arc(-5.0, -5.0, 0.0, false, true), Vec2::new(10.0, 0.0)), Shape::Arc(c) if c.rx == 5.0));
    }

    #[test]
    fn an_arc_just_short_of_a_full_turn_goes_all_the_way_round() {
        let c = centered(Vec2::new(128.0, 8.0), arc(120.0, 120.0, 0.0, true, true), Vec2::new(127.99, 8.0));
        assert!(c.delta > TAU - 1e-3, "{}", c.delta);
        let small = centered(Vec2::new(128.0, 8.0), arc(120.0, 120.0, 0.0, false, false), Vec2::new(127.99, 8.0));
        assert!(small.delta.abs() < 1e-3);
    }

    #[test]
    fn a_turned_ellipse_still_passes_through_both_ends() {
        let (from, to) = (Vec2::new(3.0, 1.0), Vec2::new(-2.0, 6.0));
        let c = centered(from, arc(16.0, 6.0, 30.0, true, false), to);
        assert!(c.at(c.theta, 1.0).distance(from) < 1e-9 && c.at(c.theta + c.delta, 1.0).distance(to) < 1e-9);
        assert!(c.delta < -PI, "the long way, anticlockwise");
    }
}
