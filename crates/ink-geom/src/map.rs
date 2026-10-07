//! A path put through a transform, every segment still the kind it was:
//! a line a line, a curve a curve, and an arc of an ellipse an arc of
//! the ellipse the transform makes of it (ARCHITECTURE §3.1).

use lntrn_math::Vec2;

use crate::affine::Affine;
use crate::arc::{Shape, shape};
use crate::path::{ArcTo, Path, Seg, Subpath};

/// An ellipse's turn is the same every half turn: brought into
/// (-90°, 90°].
fn half_turn(degrees: f64) -> f64 {
    let d = degrees.rem_euclid(180.0);
    if d > 90.0 { d - 180.0 } else { d }
}

/// The arc from `from` to `to` once through `t`, its ends aside.
fn arc_through(from: Vec2, arc: &ArcTo, to: Vec2, t: &Affine) -> ArcTo {
    // Moved, it is the arc it was, to the digit.
    if t.is_translation() {
        return *arc;
    }
    // The radii it's drawn with: ones too small to reach have grown.
    let (rx, ry) = match shape(from, arc, to) {
        Shape::Arc(c) => (c.rx, c.ry),
        _ => (arc.rx.abs(), arc.ry.abs()),
    };
    // A mirror turns clockwise into anticlockwise.
    let sweep = arc.sweep != (t.determinant() < 0.0);
    let (sin, cos) = arc.rotation.to_radians().sin_cos();
    // Where the ellipse's two half axes go. They are half diameters of
    // the new ellipse, and its axes when still square to each other.
    let (u, v) = (t.linear(Vec2::new(cos, sin) * rx), t.linear(Vec2::new(-sin, cos) * ry));
    let (lu, lv) = (u.length(), v.length());
    if u.dot(v).abs() <= 1e-12 * lu * lv {
        // A circle has no turn to speak of: it keeps the one written.
        let rotation = if (lu - lv).abs() <= 1e-12 * lu { arc.rotation } else { half_turn(u.angle().to_degrees()) };
        return ArcTo { rx: lu, ry: lv, rotation, large: arc.large, sweep };
    }
    // Skewed: the new ellipse's axes are the principal ones of M·Mᵀ,
    // M being what takes the unit circle to it.
    let (xx, xy, yy) = (u.x * u.x + v.x * v.x, u.x * u.y + v.x * v.y, u.y * u.y + v.y * v.y);
    let turn = 0.5 * (2.0 * xy).atan2(xx - yy);
    let (s, c) = turn.sin_cos();
    let along = xx * c * c + 2.0 * xy * c * s + yy * s * s;
    let across = xx * s * s - 2.0 * xy * c * s + yy * c * c;
    ArcTo { rx: along.max(0.0).sqrt(), ry: across.max(0.0).sqrt(), rotation: half_turn(turn.to_degrees()), large: arc.large, sweep }
}

impl Path {
    /// The path once through `t`.
    pub fn transformed(&self, t: &Affine) -> Path {
        let subpaths = self
            .subpaths
            .iter()
            .map(|sub| {
                // An arc is worked out where it was drawn.
                let mut at = sub.start;
                let segs = sub
                    .segs
                    .iter()
                    .map(|seg| {
                        let from = std::mem::replace(&mut at, seg.to());
                        match *seg {
                            Seg::Line { to } => Seg::Line { to: t.apply(to) },
                            Seg::Quad { c, to } => Seg::Quad { c: t.apply(c), to: t.apply(to) },
                            Seg::Cubic { c1, c2, to } => Seg::Cubic { c1: t.apply(c1), c2: t.apply(c2), to: t.apply(to) },
                            Seg::Arc { ref arc, to } => Seg::Arc { arc: arc_through(from, arc, to, t), to: t.apply(to) },
                        }
                    })
                    .collect();
                Subpath { start: t.apply(sub.start), segs, closed: sub.closed }
            })
            .collect();
        Path { subpaths }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every point of the path `d` through `t`, against the path put
    /// through whole: the same outline, of the same kinds of segment.
    fn holds(d: &str, t: &Affine) {
        let path = Path::parse(d).path;
        let moved = path.transformed(t);
        let (own, mut by_point) = (moved.flatten(1e-5), path.flatten(1e-5));
        assert_eq!(own.len(), by_point.len(), "{d}");
        for (a, b) in own.iter().zip(&mut by_point) {
            // A polygon's last side runs back to its first point.
            let mut line: Vec<Vec2> = b.points.iter().map(|p| t.apply(*p)).collect();
            line.extend(b.closed.then(|| line[0]));
            let far = a.points.iter().map(|p| line.windows(2).map(|w| distance_to(*p, w[0], w[1])).fold(f64::MAX, f64::min)).fold(0.0, f64::max);
            assert!(far < 1e-3, "{d} through {t:?}: {far} off");
        }
        for (a, b) in path.subpaths.iter().zip(&moved.subpaths) {
            assert_eq!(a.segs.iter().map(std::mem::discriminant).collect::<Vec<_>>(), b.segs.iter().map(std::mem::discriminant).collect::<Vec<_>>());
        }
    }

    fn distance_to(p: Vec2, a: Vec2, b: Vec2) -> f64 {
        let ab = b - a;
        let t = if ab.dot(ab) > 0.0 { ((p - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0) } else { 0.0 };
        p.distance(a + ab * t)
    }

    #[test]
    fn a_path_goes_through_any_transform_as_the_kinds_it_was() {
        let transforms = [
            Affine::translate(3.0, -8.0),
            Affine::rotate(0.7).about(Vec2::new(5.0, 5.0)),
            Affine::scale(2.5, 2.5),
            Affine::scale(-1.0, 1.0),
            Affine::scale(2.0, 0.5).then(&Affine::rotate(-1.1)),
            Affine::skew_x(0.4).then(&Affine::scale(1.0, -3.0)).then(&Affine::translate(1.0, 2.0)),
        ];
        for d in ["M0 0 H10 V10 H0 Z", "M0 0 C40 0 -30 10 10 10 Q20 30 0 20 Z", "M3 1 A8 3 30 1 0 -2 6", "M10 0 A10 10 0 0 1 0 10 A10 10 0 1 0 10 0 Z", "M0 0 A1 1 0 0 1 10 0", "M0 0 A5 0 0 0 1 10 0 L4 4"] {
            for t in &transforms {
                holds(d, t);
            }
        }
    }

    #[test]
    fn arcs_keep_numbers_that_say_the_same() {
        let arc = |d: &str, t: &Affine| match Path::parse(d).path.transformed(t).subpaths[0].segs[0] {
            Seg::Arc { arc, to } => (arc, to),
            other => panic!("{other:?}"),
        };
        // Moved: the arc it was, though its radii are too small to reach.
        let (moved, to) = arc("M0 0 A1 2 200 0 1 10 0", &Affine::translate(5.0, 5.0));
        assert_eq!((moved, to), (ArcTo { rx: 1.0, ry: 2.0, rotation: 200.0, large: false, sweep: true }, Vec2::new(15.0, 5.0)));
        // Turned: an ellipse's turn goes with it; a circle's stays 0.
        let (turned, _) = arc("M3 1 A16 6 30 1 0 -2 6", &Affine::rotate(20f64.to_radians()));
        assert!((turned.rx - 16.0).abs() < 1e-9 && (turned.ry - 6.0).abs() < 1e-9 && (turned.rotation - 50.0).abs() < 1e-9, "{turned:?}");
        let (circle, _) = arc("M10 0 A10 10 0 0 1 0 10", &Affine::rotate(1.0).then(&Affine::scale(3.0, 3.0)));
        assert!((circle.rx - 30.0).abs() < 1e-9 && circle.rotation == 0.0 && circle.sweep);
        // Mirrored: it sweeps the other way, and its turn leans the
        // other way.
        let (mirrored, _) = arc("M3 1 A16 6 30 1 0 -2 6", &Affine::scale(-1.0, 1.0));
        assert!(mirrored.sweep && mirrored.large && (mirrored.rotation + 30.0).abs() < 1e-9, "{mirrored:?}");
        // Stretched along its axes: each radius by its own factor.
        let (wide, _) = arc("M5 0 A5 5 0 0 1 -5 0", &Affine::scale(2.0, 1.0));
        assert!((wide.rx - 10.0).abs() < 1e-9 && (wide.ry - 5.0).abs() < 1e-9 && wide.rotation == 0.0, "{wide:?}");
        // Radii too small are the ones drawn, once it's more than moved.
        let (grown, _) = arc("M0 0 A1 1 0 0 1 10 0", &Affine::scale(2.0, 2.0));
        assert!((grown.rx - 10.0).abs() < 1e-9, "{grown:?}");
        let line = Path::parse("M0 0 L4 4").path;
        assert_eq!(line.transformed(&Affine::IDENTITY), line);
    }
}
