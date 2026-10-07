//! How many times an outline winds round a point, by its curves
//! themselves: no flattening, so a point a hair off a curve is on the
//! side of it that it is on.

use lntrn_math::Vec2;

use crate::piece::Piece;
use crate::path::{Path, Seg};

/// The pieces a path's fill is bounded by: every segment that goes
/// anywhere, and the line that closes each subpath that doesn't come
/// back to its start (a fill closes them all). An arc that is a
/// straight line (a radius of nothing) is one.
pub fn outline(path: &Path) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for sub in &path.subpaths {
        let mut at = sub.start;
        for seg in &sub.segs {
            let piece = Piece::new(at, *seg);
            at = seg.to();
            let straight = matches!(seg, Seg::Arc { .. }) && piece.bulge() == 0.0;
            let piece = if straight { Piece::new(piece.from, Seg::Line { to: at }) } else { piece };
            if piece.from != at || !matches!(piece.seg, Seg::Line { .. } | Seg::Arc { .. }) {
                pieces.push(piece);
            }
        }
        if at != sub.start {
            pieces.push(Piece::new(at, Seg::Line { to: sub.start }));
        }
    }
    pieces
}

/// The area between a piece and the origin (the wedge its ends make
/// with it), exactly: positive when it goes round the origin the way
/// x turns to y. Round a closed outline these add up to what it holds.
fn swept(piece: &Piece) -> f64 {
    let (p0, p3) = (piece.from, piece.to());
    match piece.seg {
        Seg::Line { .. } => p0.perp_dot(p3) * 0.5,
        Seg::Quad { c, .. } => (p0.perp_dot(c) * 2.0 + p0.perp_dot(p3) + c.perp_dot(p3) * 2.0) / 6.0,
        Seg::Cubic { c1, c2, .. } => (p0.perp_dot(c1) * 6.0 + p0.perp_dot(c2) * 3.0 + p0.perp_dot(p3) + c1.perp_dot(c2) * 3.0 + c1.perp_dot(p3) * 3.0 + c2.perp_dot(p3) * 6.0) / 20.0,
        // Its slice of the ellipse, and the wedge from the origin to
        // the ellipse's middle.
        Seg::Arc { .. } => match piece.round() {
            Some((centre, rx, ry, sweep)) => (rx * ry * sweep + centre.perp_dot(p3 - p0)) * 0.5,
            None => p0.perp_dot(p3) * 0.5,
        },
    }
}

impl Path {
    /// The area the path's outline holds, each subpath closed as a fill
    /// closes it: exact for every kind of segment, and signed (positive
    /// drawn the way x turns to y, clockwise on screen). Where the
    /// outline goes round twice the area counts twice, and a hole drawn
    /// the other way round takes its own away.
    pub fn area(&self) -> f64 {
        outline(self).iter().map(swept).sum()
    }
}

/// Whether going from `from` to `to` crosses the level line through
/// `p`, and which way: up it (1), down it (-1), or not (0). A point on
/// the line counts as under it, so a corner on it is crossed once.
fn crossing(from: Vec2, to: Vec2, p: Vec2) -> i32 {
    i32::from(from.y <= p.y && to.y > p.y) - i32::from(to.y <= p.y && from.y > p.y)
}

/// How often `piece` crosses the level ray from `p` to the right, up
/// less down. Curves are told from their chords down to `fine`.
fn turns(piece: &Piece, p: Vec2, fine: f64, depth: u32) -> i32 {
    let ends = crossing(piece.from, piece.to(), p);
    let straight = matches!(piece.seg, Seg::Line { .. });
    let bounds = piece.bounds();
    // Not level with it, or all to its left: the ray never reaches it.
    if p.y < bounds.min.y || p.y > bounds.max.y || bounds.max.x < p.x {
        return 0;
    }
    // All to its right: however it wanders, it crosses as its ends do.
    if bounds.min.x > p.x {
        return ends;
    }
    if straight || depth >= 64 || piece.bulge() <= fine {
        // A straight line: crossed where it is level with `p`, if that
        // is to the right.
        let (from, to) = (piece.from, piece.to());
        return if ends != 0 && from.x + (p.y - from.y) * (to.x - from.x) / (to.y - from.y) > p.x { ends } else { 0 };
    }
    let (first, second) = piece.split(0.5);
    turns(&first, p, fine, depth + 1) + turns(&second, p, fine, depth + 1)
}

/// How many times the outline `pieces` (closed: see [`outline`]) winds
/// round `p`. A curve is taken for its chord once it's within `fine`
/// of it: a point further than that from the outline is counted right.
pub fn winding(pieces: &[Piece], p: Vec2, fine: f64) -> i32 {
    pieces.iter().map(|piece| turns(piece, p, fine, 0)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn wind(d: &str, p: Vec2) -> i32 {
        winding(&outline(&Path::parse(d).path), p, 1e-12)
    }

    #[test]
    fn an_outline_winds_round_what_is_inside_it() {
        let square = "M0 0 H10 V10 H0 Z";
        assert_eq!((wind(square, v(5.0, 5.0)), wind(square, v(15.0, 5.0)), wind(square, v(-1.0, 5.0)), wind(square, v(5.0, 11.0))), (1, 0, 0, 0));
        // The other way round, the other sign; open, it's closed.
        assert_eq!(wind("M0 0 V10 H10 V0 Z", v(5.0, 5.0)), -1);
        assert_eq!(wind("M0 0 H10 V10 H0", v(5.0, 5.0)), 1);
        // Twice round is two; a hole drawn the same way is two deep.
        assert_eq!(wind("M0 0 H10 V10 H0 Z M2 2 H8 V8 H2 Z", v(5.0, 5.0)), 2);
        assert_eq!(wind("M0 0 H10 V10 H0 Z M2 2 V8 H8 V2 Z", v(5.0, 5.0)), 0);
        // Level with a corner, it's still one crossing.
        assert_eq!(wind("M0 0 L10 5 L0 10 Z", v(2.0, 5.0)), 1);
        assert_eq!(wind("M0 0 L10 5 L0 10 Z", v(-2.0, 5.0)), 0);
    }

    #[test]
    fn a_curve_is_the_curve_however_near_the_point() {
        // A circle of radius 5: a billionth inside it and a billionth
        // out, all the way round.
        let circle = "M10 5 A5 5 0 0 1 0 5 A5 5 0 0 1 10 5 Z";
        for i in 0..360 {
            let way = Vec2::from_angle((i as f64 + 0.37).to_radians());
            assert_eq!(wind(circle, v(5.0, 5.0) + way * (5.0 - 1e-9)), 1, "{i}° inside");
            assert_eq!(wind(circle, v(5.0, 5.0) + way * (5.0 + 1e-9)), 0, "{i}° outside");
        }
        // A cubic's bulge, and a quadratic's.
        let leaf = "M0 0 C0 8 12 8 12 0 Q6 -6 0 0 Z";
        assert_eq!((wind(leaf, v(6.0, 6.0 - 1e-9)), wind(leaf, v(6.0, 6.0 + 1e-9))), (-1, 0));
        assert_eq!((wind(leaf, v(6.0, -3.0 + 1e-9)), wind(leaf, v(6.0, -3.0 - 1e-9))), (-1, 0));
    }

    #[test]
    fn the_area_an_outline_holds_is_exact() {
        let area = |d: &str| Path::parse(d).path.area();
        assert_eq!(area("M0 0 H10 V10 H0 Z"), 100.0);
        assert_eq!(area("M0 0 V10 H10 V0 Z"), -100.0, "the other way round");
        assert_eq!(area("M0 0 H10 V10 H0"), 100.0, "closed as a fill closes it");
        assert_eq!(area("M0 0 H10 V10 H0 Z M2 2 V8 H8 V2 Z"), 64.0, "less its hole");
        // A circle, an ellipse turned, and half of one.
        let pi = std::f64::consts::PI;
        assert!((area("M10 5 A5 5 0 0 1 0 5 A5 5 0 0 1 10 5 Z") - 25.0 * pi).abs() < 1e-12);
        assert!((Path::ellipse(v(7.0, -3.0), 4.0, 2.5).transformed(&crate::Affine::rotate(0.7)).area().abs() - 10.0 * pi).abs() < 1e-11);
        assert!((area("M10 5 A5 5 0 0 1 0 5 Z") - 12.5 * pi).abs() < 1e-12);
        // Under a parabola: two thirds of its box (out along the curve
        // and back along the level: the other way round). A cubic that
        // is the same parabola holds the same.
        assert!((area("M0 0 Q5 10 10 0 Z") + 100.0 / 3.0).abs() < 1e-12);
        assert!((area("M0 0 C3.333333333333333 6.666666666666667 6.666666666666667 6.666666666666667 10 0 Z") + 100.0 / 3.0).abs() < 1e-12);
        // Wherever it is: moving it doesn't change what it holds.
        let leaf = Path::parse("M0 0 C0 8 12 8 12 0 Q6 -6 0 0 Z").path;
        assert!((leaf.area() - leaf.transformed(&crate::Affine::translate(-40.0, 17.0)).area()).abs() < 1e-10);
        // And it is what a fine flattening holds.
        let flat: f64 = leaf.flatten(1e-7).iter().map(|l| (0..l.points.len()).map(|i| l.points[i].perp_dot(l.points[(i + 1) % l.points.len()]) * 0.5).sum::<f64>()).sum();
        assert!((leaf.area() - flat).abs() < 1e-5, "{} and {flat}", leaf.area());
    }

    #[test]
    fn an_outline_is_what_a_fill_would_close() {
        let pieces = outline(&Path::parse("M0 0 H10 V10 M20 0 L20 0 A0 5 0 0 1 30 0 Z M5 5").path);
        // Two sides and the line home; a line that goes nowhere left
        // out, an arc of no radius as the line it is, closed already.
        assert_eq!(pieces.len(), 5);
        assert!(matches!(pieces[2].seg, Seg::Line { to } if to == v(0.0, 0.0)));
        assert!(matches!(pieces[3].seg, Seg::Line { to } if to == v(30.0, 0.0)));
        assert!(matches!(pieces[4].seg, Seg::Line { to } if to == v(20.0, 0.0)));
    }
}
