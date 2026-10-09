//! A polygon read back (M4c): how many sides the Polygon tool drew it
//! with, whether it's a star, and how deep its points go. Nothing in the
//! file says so (a polygon is a plain `<polygon>`), so it's read off its
//! corners: they are a regular polygon's, or a star's, wherever one has
//! been moved, stretched, turned or leant since. Then the Box can give
//! it other sides on the same circle (Alva's choice: its middle and its
//! reach stay, so a regular one stays regular, and 5 to 6 and back is
//! what it was).
//!
//! The file's numbers are rounded ones, so the circle read off them is
//! the one that fits them best, and is the circle it was drawn on to
//! within the file's last decimal: other sides and back again is the
//! same polygon to that last decimal, not always to the byte (undo is).
//! The circle isn't put on rounder numbers than it reads as: a polygon
//! drawn to fill a box of whole units has a circle of awkward ones, and
//! would stop filling its box. A star's depth is a whole percent, as
//! the Box sets it, wherever its corners still fit that.
//!
//! Any three corners are a triangle's and any slanted box's a square's:
//! those read as 3 and 4 sides too, whoever drew them.

use lntrn_math::Vec2;

use crate::shapes::{SIDES, ring};

/// A polygon, as the circle it's drawn on and what's drawn on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Regular {
    /// Its sides (a star: its points).
    pub sides: usize,
    /// A star's depth: how far in its inner corners are.
    pub star: Option<f64>,
    /// The circle's middle, and where its one unit across and one unit
    /// down have gone: the same length and square to each other while
    /// it's still a circle.
    pub centre: Vec2,
    pub across: Vec2,
    pub down: Vec2,
}

/// The middle of `points`.
fn middle(points: &[Vec2]) -> Vec2 {
    points.iter().fold(Vec2::ZERO, |sum, p| sum + *p) * (1.0 / points.len().max(1) as f64)
}

/// Where the circle's two units go for `points` (about `centre`) to be
/// the corners `on` it, as nearly as can be.
fn frame(points: impl Iterator<Item = Vec2>, on: &[Vec2], centre: Vec2) -> (Vec2, Vec2) {
    let k = 2.0 / on.len() as f64;
    points.zip(on).fold((Vec2::ZERO, Vec2::ZERO), |(across, down), (p, u)| (across + (p - centre) * (u.x * k), down + (p - centre) * (u.y * k)))
}

impl Regular {
    /// Where the corner at `u` on the unit circle is.
    fn at(&self, u: Vec2) -> Vec2 {
        self.centre + self.across * u.x + self.down * u.y
    }

    /// How far it reaches from its middle.
    pub fn reach(&self) -> f64 {
        self.across.length().max(self.down.length())
    }

    /// The corners of one with `sides` sides (a star, `star` deep) on
    /// the same circle.
    pub fn points(&self, sides: usize, star: Option<f64>) -> Vec<Vec2> {
        ring(sides.clamp(SIDES.0 as usize, SIDES.1 as usize), star).into_iter().map(|u| self.at(u)).collect()
    }

    /// What `points` are the corners of, if each is within two of
    /// `unit` (one in the last place the file writes) of where a
    /// regular polygon's or a star's would be; or within a
    /// ten-thousandth of its size, where that's more.
    pub fn read(points: &[Vec2], unit: f64) -> Option<Regular> {
        let (least, most) = (SIDES.0 as usize, SIDES.1 as usize);
        let fits = |r: &Regular| {
            let room = (unit * 2.0).max(r.reach() * 1e-4);
            // Flat, it's the corners of nothing.
            let area = (r.across.x * r.down.y - r.across.y * r.down.x).abs();
            area > room * room && r.points(r.sides, r.star).iter().zip(points).all(|(a, b)| (*a - *b).length() <= room)
        };
        // A star as deep as a whole percent, where that's still its
        // corners.
        let settled = |r: Regular| {
            let whole = Regular { star: r.star.map(|d| (d * 100.0).round() / 100.0), ..r };
            [whole, r].into_iter().find(|r| fits(r))
        };
        let centre = middle(points);
        let n = points.len();
        if (least..=most).contains(&n) {
            let (across, down) = frame(points.iter().copied(), &ring(n, None), centre);
            if let Some(plain) = settled(Regular { sides: n, star: None, centre, across, down }) {
                return Some(plain);
            }
        }
        // A star: every other corner on the circle, the rest part of
        // the way in, half a step round.
        if !n.is_multiple_of(2) || !(least..=most).contains(&(n / 2)) {
            return None;
        }
        let on = ring(n / 2, Some(0.0));
        let outer: Vec<Vec2> = on.iter().copied().step_by(2).collect();
        let inner: Vec<Vec2> = on.iter().copied().skip(1).step_by(2).collect();
        let (across, down) = frame(points.iter().copied().step_by(2), &outer, centre);
        let (in_across, in_down) = frame(points.iter().copied().skip(1).step_by(2), &inner, centre);
        // The inner ring is the outer one, smaller.
        let size = across.dot(across) + down.dot(down);
        if size <= 0.0 {
            return None;
        }
        let share = (in_across.dot(across) + in_down.dot(down)) / size;
        if share <= 0.0 || share >= 1.0 {
            return None;
        }
        settled(Regular { sides: n / 2, star: Some(1.0 - share), centre, across, down })
    }
}

#[cfg(test)]
mod tests {
    use ink_geom::Affine;

    use super::*;

    /// A polygon as the tool draws it, put through `t` and written to
    /// three decimals.
    fn drawn(sides: usize, star: Option<f64>, t: &Affine) -> Vec<Vec2> {
        let round = |v: f64| (v * 1000.0).round() / 1000.0;
        ring(sides, star).into_iter().map(|u| t.apply(u)).map(|p| Vec2::new(round(p.x), round(p.y))).collect()
    }

    fn read(points: &[Vec2]) -> Option<(usize, Option<f64>)> {
        Regular::read(points, 0.001).map(|r| (r.sides, r.star.map(|d| (d * 100.0).round() / 100.0)))
    }

    #[test]
    fn a_polygon_says_how_it_was_drawn_wherever_its_been_put() {
        let plain = Affine::scale(10.0, 10.0).then(&Affine::translate(12.0, 12.0));
        // Stretched to a box, turned, leant, mirrored: the same count.
        let turned = Affine::scale(9.0, 4.0).then(&Affine::rotate(0.7)).then(&Affine::translate(-30.0, 8.0));
        let leant = Affine::scale(-7.0, 5.0).then(&Affine::skew_x(0.4));
        for t in [plain, turned, leant] {
            for sides in [3, 4, 5, 6, 7, 12, 64] {
                assert_eq!(read(&drawn(sides, None, &t)), Some((sides, None)), "{sides} sides");
                for depth in [0.05, 0.3, 0.5, 0.95] {
                    assert_eq!(read(&drawn(sides, Some(depth), &t)), Some((sides, Some(depth))), "a star of {sides}, {depth} deep");
                }
            }
        }
        // Its circle is the one it was drawn on, to what the file says;
        // a star's depth, the percent it was set to.
        let r = Regular::read(&drawn(5, None, &plain), 0.001).unwrap();
        assert!((r.centre - Vec2::new(12.0, 12.0)).length() < 0.001 && (r.across - Vec2::new(10.0, 0.0)).length() < 0.001 && (r.down - Vec2::new(0.0, 10.0)).length() < 0.001, "{r:?}");
        assert!((r.reach() - 10.0).abs() < 0.001);
        assert_eq!(Regular::read(&drawn(7, Some(0.35), &turned), 0.001).unwrap().star, Some(0.35));
    }

    #[test]
    fn what_isnt_one_reads_as_none() {
        let t = Affine::scale(10.0, 10.0);
        // A corner pulled away; too few, too many, or all in a line.
        let mut bent = drawn(6, None, &t);
        bent[2] += Vec2::new(0.5, 0.0);
        assert_eq!(read(&bent), None);
        let mut star = drawn(5, Some(0.5), &t);
        star[3] += Vec2::new(0.0, 0.3);
        assert_eq!(read(&star), None);
        assert_eq!(read(&drawn(5, None, &t)[..2]), None);
        assert_eq!(read(&[Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(8.0, 0.0)]), None);
        assert_eq!(read(&[Vec2::ZERO; 5]), None);
        assert_eq!(read(&[]), None);
        // An arrow's head, an L: five and six corners of nothing regular.
        assert_eq!(read(&[Vec2::ZERO, Vec2::new(6.0, 0.0), Vec2::new(6.0, 2.0), Vec2::new(2.0, 2.0), Vec2::new(2.0, 6.0), Vec2::new(0.0, 6.0)]), None);
        // But any triangle has three sides, and a level box four.
        assert_eq!(read(&[Vec2::ZERO, Vec2::new(9.0, 1.0), Vec2::new(2.0, 5.0)]), Some((3, None)));
        assert_eq!(read(&[Vec2::ZERO, Vec2::new(8.0, 0.0), Vec2::new(8.0, 3.0), Vec2::new(0.0, 3.0)]), Some((4, None)));
    }

    #[test]
    fn other_sides_go_on_the_same_circle_and_come_back() {
        let t = Affine::scale(10.0, 6.0).then(&Affine::rotate(0.3)).then(&Affine::translate(20.0, 20.0));
        let written = |points: Vec<Vec2>| -> Vec<Vec2> { points.into_iter().map(|p| Vec2::new((p.x * 1000.0).round() / 1000.0, (p.y * 1000.0).round() / 1000.0)).collect() };
        let five = drawn(5, None, &t);
        let was = Regular::read(&five, 0.001).unwrap();
        // Six, a star of six, and five again, each written to the file
        // and read back off it: the same circle to the file's last
        // decimal every time, and at the end the corners it began with,
        // to that decimal.
        let same = |a: &Regular, b: &Regular| (a.centre - b.centre).length() < 0.001 && (a.across - b.across).length() < 0.001 && (a.down - b.down).length() < 0.001;
        let six = written(was.points(6, None));
        let again = Regular::read(&six, 0.001).unwrap();
        assert_eq!((six.len(), again.sides), (6, 6));
        assert!(same(&again, &was), "{again:?} {was:?}");
        let star = written(again.points(6, Some(0.4)));
        assert_eq!(read(&star), Some((6, Some(0.4))));
        let last = Regular::read(&star, 0.001).unwrap();
        assert!(same(&last, &was), "{last:?} {was:?}");
        assert!(written(last.points(5, None)).iter().zip(&five).all(|(a, b)| (*a - *b).length() < 0.002), "{five:?}");
        // Never fewer than three, nor more than the tool draws.
        assert_eq!((was.points(1, None).len(), was.points(500, None).len()), (3, 64));
    }
}
