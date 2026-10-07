//! Whether a point is on a path: inside it as it's filled, or within
//! reach of its line as it's stroked.

use lntrn_math::Vec2;

use crate::FillRule;
use crate::path::Path;

/// How far `p` is from the piece of line `a` to `b`.
fn from_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let along = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * along)
}

impl Path {
    /// Whether `p` is inside the path filled by `rule`, its curves taken
    /// to within `tol`. Every subpath counts as closed, as a fill closes
    /// it.
    pub fn contains(&self, p: Vec2, rule: FillRule, tol: f64) -> bool {
        // A ray from `p` to the right: each side crossing it winds
        // round `p` one way or the other.
        let (mut winding, mut crossings) = (0i32, 0u32);
        for line in self.flatten(tol) {
            let points = &line.points;
            for (i, &a) in points.iter().enumerate() {
                let b = points[(i + 1) % points.len()];
                if (a.y <= p.y) != (b.y <= p.y) && a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) > p.x {
                    winding += if b.y > a.y { 1 } else { -1 };
                    crossings += 1;
                }
            }
        }
        match rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => crossings % 2 == 1,
        }
    }

    /// How far `p` is from the path's line (open where the path is
    /// open), its curves taken to within `tol`. `None` when the path
    /// draws nothing.
    pub fn distance(&self, p: Vec2, tol: f64) -> Option<f64> {
        let mut nearest: Option<f64> = None;
        for line in self.flatten(tol) {
            let points = &line.points;
            let sides = if line.closed { points.len() } else { points.len().saturating_sub(1) };
            for i in 0..sides.max(usize::from(!points.is_empty())) {
                let d = from_segment(p, points[i], points[(i + 1) % points.len()]);
                nearest = Some(nearest.map_or(d, |n| n.min(d)));
            }
        }
        nearest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn a_point_is_inside_what_the_rule_fills() {
        let square = Path::rect(2.0, 2.0, 8.0, 8.0, 0.0, 0.0);
        assert!(square.contains(v(5.0, 5.0), FillRule::NonZero, 0.01));
        assert!(!square.contains(v(11.0, 5.0), FillRule::NonZero, 0.01) && !square.contains(v(5.0, 1.0), FillRule::NonZero, 0.01));
        // A rounded corner is cut off; a circle is round.
        let round = Path::rect(0.0, 0.0, 10.0, 10.0, 4.0, 4.0);
        assert!(!round.contains(v(0.5, 0.5), FillRule::NonZero, 0.01) && round.contains(v(2.0, 2.0), FillRule::NonZero, 0.01));
        let circle = Path::ellipse(v(0.0, 0.0), 5.0, 5.0);
        assert!(circle.contains(v(3.5, 3.5), FillRule::NonZero, 0.001) && !circle.contains(v(3.6, 3.6), FillRule::NonZero, 0.001));
        // A square in a square, both drawn the same way round: a hole
        // by one rule, solid by the other.
        let ring = Path::parse("M0 0 H10 V10 H0 Z M3 3 H7 V7 H3 Z").path;
        assert!(ring.contains(v(5.0, 5.0), FillRule::NonZero, 0.01) && !ring.contains(v(5.0, 5.0), FillRule::EvenOdd, 0.01));
        assert!(ring.contains(v(1.0, 5.0), FillRule::EvenOdd, 0.01));
        // An open path is filled as if closed; nothing is in nothing.
        assert!(Path::parse("M0 0 L10 0 L5 8").path.contains(v(5.0, 2.0), FillRule::NonZero, 0.01));
        assert!(!Path::new().contains(v(0.0, 0.0), FillRule::NonZero, 0.01));
    }

    #[test]
    fn a_point_is_as_far_from_a_path_as_from_its_nearest_piece() {
        let open = Path::parse("M0 0 L10 0 L10 10").path;
        let near = |p: Vec2, want: f64| assert!((open.distance(p, 0.01).unwrap() - want).abs() < 1e-9, "{p:?}");
        near(v(5.0, 3.0), 3.0);
        near(v(-4.0, 3.0), 5.0);
        near(v(4.0, 6.0), 6.0);
        // Closed, its last side is a side too.
        assert!((Path::parse("M0 0 L10 0 L10 10 Z").path.distance(v(4.0, 6.0), 0.01).unwrap() - 2f64.sqrt()).abs() < 1e-9);
        assert!((Path::ellipse(v(0.0, 0.0), 5.0, 5.0).distance(v(0.0, 0.0), 0.001).unwrap() - 5.0).abs() < 2e-3);
        assert_eq!(Path::new().distance(v(0.0, 0.0), 0.01), None);
        let mut dot = Path::new();
        dot.move_to(v(3.0, 4.0)).close();
        assert_eq!(dot.distance(v(0.0, 0.0), 0.01), Some(5.0));
    }
}
