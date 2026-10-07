//! 2D affine transforms, laid out as SVG writes them.

use lntrn_math::{Rect, Vec2};

/// `matrix(a b c d e f)`: x' = a·x + c·y + e, y' = b·x + d·y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Affine {
        Affine { a, b, c, d, e, f }
    }

    pub const fn translate(x: f64, y: f64) -> Affine {
        Affine::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    pub const fn scale(x: f64, y: f64) -> Affine {
        Affine::new(x, 0.0, 0.0, y, 0.0, 0.0)
    }

    /// A turn by `radians` about the origin: clockwise on screen (y down),
    /// as SVG's `rotate` goes.
    pub fn rotate(radians: f64) -> Affine {
        let (s, c) = radians.sin_cos();
        Affine::new(c, s, -s, c, 0.0, 0.0)
    }

    /// Columns lean sideways by `radians` (SVG's `skewX`).
    pub fn skew_x(radians: f64) -> Affine {
        Affine::new(1.0, 0.0, radians.tan(), 1.0, 0.0, 0.0)
    }

    /// Rows lean by `radians` (SVG's `skewY`).
    pub fn skew_y(radians: f64) -> Affine {
        Affine::new(1.0, radians.tan(), 0.0, 1.0, 0.0, 0.0)
    }

    /// This transform, with `pivot` staying where it is.
    pub fn about(self, pivot: Vec2) -> Affine {
        Affine::translate(-pivot.x, -pivot.y).then(&self).then(&Affine::translate(pivot.x, pivot.y))
    }

    pub fn apply(&self, p: Vec2) -> Vec2 {
        Vec2::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    /// A direction through this: the translation left out.
    pub fn linear(&self, v: Vec2) -> Vec2 {
        Vec2::new(self.a * v.x + self.c * v.y, self.b * v.x + self.d * v.y)
    }

    /// This applied first, then `outer`.
    pub fn then(&self, outer: &Affine) -> Affine {
        let (i, o) = (self, outer);
        Affine::new(o.a * i.a + o.c * i.b, o.b * i.a + o.d * i.b, o.a * i.c + o.c * i.d, o.b * i.c + o.d * i.d, o.a * i.e + o.c * i.f + o.e, o.b * i.e + o.d * i.f + o.f)
    }

    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// The transform that undoes this one; `None` when it squashes the
    /// plane flat.
    pub fn inverse(&self) -> Option<Affine> {
        let det = self.determinant();
        if !det.is_finite() || det.abs() < 1e-300 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        let inv = Affine::new(a, b, c, d, -(a * self.e + c * self.f), -(b * self.e + d * self.f));
        inv.is_finite().then_some(inv)
    }

    /// The most this lengthens any direction (its larger singular value).
    pub fn max_stretch(&self) -> f64 {
        let s = self.a * self.a + self.b * self.b + self.c * self.c + self.d * self.d;
        let det = self.determinant();
        ((s + (s * s - 4.0 * det * det).max(0.0).sqrt()) * 0.5).sqrt()
    }

    /// Every coefficient is a real number (no NaN or infinity).
    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f].iter().all(|v| v.is_finite())
    }

    pub fn is_identity(&self) -> bool {
        *self == Affine::IDENTITY
    }

    /// Only a move.
    pub fn is_translation(&self) -> bool {
        (self.a, self.b, self.c, self.d) == (1.0, 0.0, 0.0, 1.0)
    }

    /// The box around `r` once mapped (the bounds of its four corners).
    pub fn bounds(&self, r: &Rect) -> Rect {
        let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)].map(|p| self.apply(p));
        let (lo, hi) = corners.iter().fold((corners[0], corners[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        Rect::new(lo, hi)
    }
}

/// A transform's linear part as a scale along x and y, then a turn:
/// what a shape with sides of its own (a rect, an ellipse) can take
/// into its own numbers and one `rotate`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axes {
    /// Radians, clockwise on screen.
    pub angle: f64,
    /// Along x: more than nothing.
    pub sx: f64,
    /// Along y: less than nothing when the transform mirrors.
    pub sy: f64,
}

impl Axes {
    /// The transform this is (with no move).
    pub fn linear(&self) -> Affine {
        Affine::scale(self.sx, self.sy).then(&Affine::rotate(self.angle))
    }

    /// The same scale each way (a mirror aside).
    pub fn is_uniform(&self, reach: f64, within: f64) -> bool {
        (self.sx - self.sy.abs()).abs() * reach <= within
    }
}

impl Affine {
    /// The scale-then-turn nearest this transform's linear part: exactly
    /// it when the transform doesn't skew ([`Affine::gap`] tells). `None`
    /// when it squashes x flat.
    pub fn axes(&self) -> Option<Axes> {
        let sx = self.a.hypot(self.b);
        if !(sx > 0.0 && sx.is_finite()) {
            return None;
        }
        let (sin, cos) = (self.b / sx, self.a / sx);
        Some(Axes { angle: self.b.atan2(self.a), sx, sy: self.d * cos - self.c * sin })
    }

    /// The most this and `other` can differ in where they put a point
    /// no further than `reach` from the origin.
    pub fn gap(&self, other: &Affine, reach: f64) -> f64 {
        let linear = (self.a - other.a).abs() + (self.b - other.b).abs() + (self.c - other.c).abs() + (self.d - other.d).abs();
        linear * reach + (self.e - other.e).abs() + (self.f - other.f).abs()
    }

    /// This transform's linear part alone: where it leaves the origin
    /// left out.
    pub fn without_move(&self) -> Affine {
        Affine::new(self.a, self.b, self.c, self.d, 0.0, 0.0)
    }
}

impl Default for Affine {
    fn default() -> Affine {
        Affine::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use super::*;

    fn near(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-12
    }

    #[test]
    fn transforms_compose_inside_out() {
        // SVG's "translate(10,0) scale(2)": the scale is applied first.
        let t = Affine::scale(2.0, 2.0).then(&Affine::translate(10.0, 0.0));
        assert_eq!(t.apply(Vec2::new(1.0, 1.0)), Vec2::new(12.0, 2.0));
        assert!(near(Affine::rotate(FRAC_PI_2).apply(Vec2::X), Vec2::Y), "a quarter turn takes +x to +y: clockwise on screen");
        let turn = Affine::rotate(FRAC_PI_2).about(Vec2::new(10.0, 10.0));
        assert!(near(turn.apply(Vec2::new(20.0, 10.0)), Vec2::new(10.0, 20.0)));
        assert!(near(turn.apply(Vec2::new(10.0, 10.0)), Vec2::new(10.0, 10.0)), "the pivot stays put");
    }

    #[test]
    fn inverts_and_measures_its_stretch() {
        let t = Affine::scale(2.0, 5.0).then(&Affine::rotate(0.5)).then(&Affine::translate(7.0, -3.0));
        let back = t.inverse().unwrap();
        assert!(near(back.apply(t.apply(Vec2::new(1.5, -2.5))), Vec2::new(1.5, -2.5)));
        assert!((t.max_stretch() - 5.0).abs() < 1e-12);
        assert!((Affine::skew_x(0.3).max_stretch() - 1.0).abs() > 0.1, "a skew stretches along its lean");
        assert!(Affine::scale(1.0, 0.0).inverse().is_none(), "flat");
        assert!(Affine::scale(f64::NAN, 1.0).inverse().is_none());
        assert!(t.linear(Vec2::ZERO) == Vec2::ZERO && t.apply(Vec2::ZERO) == Vec2::new(7.0, -3.0));
    }

    #[test]
    fn a_transform_that_does_not_skew_is_a_scale_then_a_turn() {
        let t = Affine::scale(2.0, -3.0).then(&Affine::rotate(0.5)).then(&Affine::translate(7.0, 1.0));
        let axes = t.axes().unwrap();
        assert!((axes.angle - 0.5).abs() < 1e-12 && (axes.sx - 2.0).abs() < 1e-12 && (axes.sy + 3.0).abs() < 1e-12, "{axes:?}");
        assert!(axes.linear().gap(&t.without_move(), 100.0) < 1e-9);
        assert!(!axes.is_uniform(1.0, 1e-9) && Affine::scale(-2.0, 2.0).axes().unwrap().is_uniform(1.0, 1e-9));
        // A mirror in x is a half turn and a mirror in y.
        let mirror = Affine::scale(-1.0, 1.0).axes().unwrap();
        assert!((mirror.angle.abs() - std::f64::consts::PI).abs() < 1e-12 && mirror.sy == -1.0);
        // A skew has a nearest one, and a gap to it.
        let skew = Affine::skew_x(0.3);
        assert!(skew.axes().unwrap().linear().gap(&skew, 1.0) > 0.1);
        assert!(Affine::scale(0.0, 1.0).axes().is_none());
        assert_eq!(Affine::translate(3.0, 4.0).gap(&Affine::IDENTITY, 100.0), 7.0);
    }

    #[test]
    fn a_mapped_box_is_the_box_around_its_corners() {
        let r = Rect::from_xywh(0.0, 0.0, 10.0, 4.0);
        let b = Affine::rotate(FRAC_PI_2).bounds(&r);
        assert!(near(b.min, Vec2::new(-4.0, 0.0)) && near(b.max, Vec2::new(0.0, 10.0)), "{b:?}");
        assert!(Affine::translate(3.0, 4.0).is_translation() && !Affine::scale(2.0, 2.0).is_translation());
        assert!(Affine::default().is_identity());
    }
}
