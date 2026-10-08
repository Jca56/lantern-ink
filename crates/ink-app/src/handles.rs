//! The selection's box and what dragging it does (ARCHITECTURE §8;
//! LS3's `handles.rs`): a handle at each corner and each side to scale
//! by, the box's inside to move by, and just outside a corner to turn
//! by. Apart from the window, so the geometry is tested by itself.
//!
//! A corner scales both ways about the corner opposite, a side one way
//! about the side opposite; Shift keeps the shape, Alt scales about
//! the middle. A turn is about the middle, in steps of 15° with Shift.
//! A move goes along one axis with Shift.

use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};

/// A handle as drawn, how far round its middle a press still takes it,
/// and how far out from a corner the pointer still turns, logical px.
pub const SIZE: f64 = 16.0;
pub const HIT: f64 = 13.0;
pub const TURN: f64 = 34.0;
/// A turn's steps with Shift, degrees.
const STEP: f64 = 15.0;
/// The least a box may be scaled to, of what it was: never flat, which
/// nothing comes back from.
const LEAST: f64 = 1e-3;

/// What a press on the selection's box takes hold of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    /// The inside: a move.
    Body,
    /// 0 top left, then clockwise.
    Corner(usize),
    /// 0 the top, then clockwise.
    Side(usize),
    /// Just outside this corner: a turn.
    Turn(usize),
}

/// The keys that change what a drag does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub shift: bool,
    pub alt: bool,
}

/// A box's corners: top left, then clockwise.
pub fn corners(r: Rect) -> [Vec2; 4] {
    [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)]
}

/// The middles of a four-cornered box's sides: the top's, then
/// clockwise.
pub fn sides(quad: [Vec2; 4]) -> [Vec2; 4] {
    [0, 1, 2, 3].map(|i| (quad[i] + quad[(i + 1) % 4]) * 0.5)
}

/// What of the box `r` (window px) is under `pointer`: a corner, a
/// side, the inside, or the turning ground outside a corner. `reach`:
/// how far from a handle's middle still takes it; `turn`: how far out
/// from a corner still turns. A box too small for its side handles to
/// be told from its corners has none on that side.
pub fn hit(r: Rect, pointer: Vec2, reach: f64, turn: f64) -> Option<Handle> {
    let near = |at: Vec2| (pointer.x - at.x).abs() <= reach && (pointer.y - at.y).abs() <= reach;
    let quad = corners(r);
    if let Some(i) = (0..4).find(|&i| near(quad[i])) {
        return Some(Handle::Corner(i));
    }
    let mid = sides(quad);
    // The top and bottom need room across, the left and right room down.
    let room = |i: usize| if i.is_multiple_of(2) { r.width() } else { r.height() } >= reach * 4.0;
    if let Some(i) = (0..4).find(|&i| room(i) && near(mid[i])) {
        return Some(Handle::Side(i));
    }
    if r.contains(pointer) || (r.is_empty() && r.expand(reach).contains(pointer)) {
        return Some(Handle::Body);
    }
    // Outside it, near a corner: the nearest one's turning ground.
    let far = |i: &usize| (pointer - quad[*i]).length();
    (0..4).filter(|i| far(i) <= turn).min_by(|a, b| far(a).total_cmp(&far(b))).map(Handle::Turn)
}

/// How far `v` is scaled when what was `was` from the pivot is now
/// `now` from it; 1 where there was nothing to scale.
fn factor(was: f64, now: f64) -> f64 {
    if was.abs() < 1e-12 {
        return 1.0;
    }
    let f = now / was;
    if f.abs() < LEAST { LEAST.copysign(if f == 0.0 { 1.0 } else { f }) } else { f }
}

/// The transform a drag of `handle` from `from` to `to` makes of what
/// the box `was` holds. All in the drawing's coordinates.
pub fn dragged(was: Rect, handle: Handle, from: Vec2, to: Vec2, keys: Keys) -> Affine {
    let d = to - from;
    let quad = corners(was);
    let centre = was.center();
    match handle {
        Handle::Body => {
            let (mut dx, mut dy) = (d.x, d.y);
            if keys.shift {
                if dx.abs() >= dy.abs() { dy = 0.0 } else { dx = 0.0 }
            }
            Affine::translate(dx, dy)
        }
        Handle::Corner(i) => {
            let (corner, pivot) = (quad[i % 4], if keys.alt { centre } else { quad[(i + 2) % 4] });
            let (mut sx, mut sy) = (factor(corner.x - pivot.x, corner.x + d.x - pivot.x), factor(corner.y - pivot.y, corner.y + d.y - pivot.y));
            if keys.shift {
                // The shape kept: as far as the further way goes.
                let s = sx.abs().max(sy.abs());
                (sx, sy) = (s.copysign(sx), s.copysign(sy));
            }
            Affine::scale(sx, sy).about(pivot)
        }
        Handle::Side(i) => {
            let mid = sides(quad);
            let (side, pivot) = (mid[i % 4], if keys.alt { centre } else { mid[(i + 2) % 4] });
            // The top and bottom scale down the page, the others across.
            let down = i.is_multiple_of(2);
            let s = if down { factor(side.y - pivot.y, side.y + d.y - pivot.y) } else { factor(side.x - pivot.x, side.x + d.x - pivot.x) };
            // Kept in shape, it grows the other way by as much, about
            // the pivot's line: mirrored only the way it was dragged.
            let other = if keys.shift { s.abs() } else { 1.0 };
            let (sx, sy) = if down { (other, s) } else { (s, other) };
            Affine::scale(sx, sy).about(pivot)
        }
        Handle::Turn(_) => Affine::rotate(turn(centre, from, to, keys).to_radians()).about(centre),
    }
}

/// How far a drag from `from` to `to` turns about `centre`, degrees
/// clockwise (the page's y runs down), between -180 and 180: in steps
/// of 15° with Shift.
pub fn turn(centre: Vec2, from: Vec2, to: Vec2, keys: Keys) -> f64 {
    let angle = |p: Vec2| (p.y - centre.y).atan2(p.x - centre.x).to_degrees();
    let turn = (angle(to) - angle(from) + 540.0).rem_euclid(360.0) - 180.0;
    if keys.shift { (turn / STEP).round() * STEP } else { turn }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Rect {
        Rect::from_xywh(10.0, 20.0, 40.0, 20.0)
    }

    fn at(by: &Affine, x: f64, y: f64) -> Vec2 {
        let p = by.apply(Vec2::new(x, y));
        Vec2::new((p.x * 1e6).round() / 1e6, (p.y * 1e6).round() / 1e6)
    }

    const PLAIN: Keys = Keys { shift: false, alt: false };
    const SHIFT: Keys = Keys { shift: true, alt: false };
    const ALT: Keys = Keys { shift: false, alt: true };

    #[test]
    fn a_press_takes_the_handle_it_is_on_then_the_inside_then_the_turning_ground() {
        let r = Rect::from_xywh(100.0, 100.0, 200.0, 100.0);
        let on = |x: f64, y: f64| hit(r, Vec2::new(x, y), 13.0, 34.0);
        assert_eq!((on(100.0, 100.0), on(305.0, 96.0), on(300.0, 200.0), on(95.0, 205.0)), (Some(Handle::Corner(0)), Some(Handle::Corner(1)), Some(Handle::Corner(2)), Some(Handle::Corner(3))));
        assert_eq!((on(200.0, 100.0), on(300.0, 150.0), on(200.0, 204.0), on(92.0, 150.0)), (Some(Handle::Side(0)), Some(Handle::Side(1)), Some(Handle::Side(2)), Some(Handle::Side(3))));
        assert_eq!((on(200.0, 150.0), on(120.0, 180.0)), (Some(Handle::Body), Some(Handle::Body)));
        // Outside a corner, out to 34 px from it; not beside the box,
        // and not further off.
        assert_eq!((on(80.0, 80.0), on(325.0, 85.0), on(320.0, 222.0), on(78.0, 215.0)), (Some(Handle::Turn(0)), Some(Handle::Turn(1)), Some(Handle::Turn(2)), Some(Handle::Turn(3))));
        assert_eq!((on(200.0, 70.0), on(60.0, 60.0), on(340.0, 150.0)), (None, None, None));
        // A box too short for a handle between its corners has none
        // there: the corners, and the top and bottom.
        let low = Rect::from_xywh(100.0, 100.0, 200.0, 30.0);
        let on = |x: f64, y: f64| hit(low, Vec2::new(x, y), 13.0, 34.0);
        assert_eq!((on(100.0, 112.0), on(300.0, 118.0), on(200.0, 100.0), on(100.0, 115.0), on(200.0, 115.0)), (Some(Handle::Corner(0)), Some(Handle::Corner(2)), Some(Handle::Side(0)), Some(Handle::Body), Some(Handle::Body)));
    }

    #[test]
    fn the_inside_moves_and_shift_keeps_it_to_one_axis() {
        let by = dragged(frame(), Handle::Body, Vec2::new(5.0, 5.0), Vec2::new(8.5, 3.0), PLAIN);
        assert_eq!(at(&by, 10.0, 20.0), Vec2::new(13.5, 18.0));
        let by = dragged(frame(), Handle::Body, Vec2::new(5.0, 5.0), Vec2::new(8.5, 3.0), SHIFT);
        assert_eq!(at(&by, 10.0, 20.0), Vec2::new(13.5, 20.0));
    }

    #[test]
    fn a_corner_scales_about_the_one_opposite_and_a_side_about_its_own() {
        // The bottom right corner, dragged out 20 and down 5: the top
        // left stays, the corner follows the pointer.
        let by = dragged(frame(), Handle::Corner(2), Vec2::new(50.0, 40.0), Vec2::new(70.0, 45.0), PLAIN);
        assert_eq!((at(&by, 10.0, 20.0), at(&by, 50.0, 40.0)), (Vec2::new(10.0, 20.0), Vec2::new(70.0, 45.0)));
        // Shift keeps the shape: half as wide again, so half as tall again.
        let by = dragged(frame(), Handle::Corner(2), Vec2::new(50.0, 40.0), Vec2::new(70.0, 45.0), SHIFT);
        assert_eq!(at(&by, 50.0, 40.0), Vec2::new(70.0, 50.0));
        // Alt: about the middle, the corner still under the pointer.
        let by = dragged(frame(), Handle::Corner(2), Vec2::new(50.0, 40.0), Vec2::new(60.0, 45.0), ALT);
        assert_eq!((at(&by, 30.0, 30.0), at(&by, 50.0, 40.0), at(&by, 10.0, 20.0)), (Vec2::new(30.0, 30.0), Vec2::new(60.0, 45.0), Vec2::new(0.0, 15.0)));
        // The top's handle, dragged up 10: the bottom stays, nothing
        // moves across.
        let by = dragged(frame(), Handle::Side(0), Vec2::new(30.0, 20.0), Vec2::new(33.0, 10.0), PLAIN);
        assert_eq!((at(&by, 10.0, 40.0), at(&by, 10.0, 20.0), at(&by, 50.0, 20.0)), (Vec2::new(10.0, 40.0), Vec2::new(10.0, 10.0), Vec2::new(50.0, 10.0)));
        // The left's, with Shift: both ways by as much, about the
        // right side's middle.
        let by = dragged(frame(), Handle::Side(3), Vec2::new(10.0, 30.0), Vec2::new(-10.0, 30.0), SHIFT);
        assert_eq!((at(&by, 50.0, 30.0), at(&by, 10.0, 20.0)), (Vec2::new(50.0, 30.0), Vec2::new(-10.0, 15.0)));
        // Dragged through the side opposite, it's mirrored.
        let by = dragged(frame(), Handle::Side(1), Vec2::new(50.0, 30.0), Vec2::new(0.0, 30.0), PLAIN);
        assert_eq!((at(&by, 50.0, 20.0), at(&by, 10.0, 20.0)), (Vec2::new(0.0, 20.0), Vec2::new(10.0, 20.0)));
        // Onto it, it's never flat.
        let by = dragged(frame(), Handle::Side(1), Vec2::new(50.0, 30.0), Vec2::new(10.0, 30.0), PLAIN);
        assert!(by.inverse().is_some() && at(&by, 50.0, 20.0).x > 10.0);
        // A box with no width has nothing across to scale.
        let line = Rect::from_xywh(10.0, 20.0, 0.0, 20.0);
        let by = dragged(line, Handle::Corner(2), Vec2::new(10.0, 40.0), Vec2::new(30.0, 50.0), PLAIN);
        assert_eq!(at(&by, 10.0, 40.0), Vec2::new(10.0, 50.0));
    }

    #[test]
    fn outside_a_corner_turns_about_the_middle() {
        let centre = frame().center();
        // A quarter turn clockwise: from due right of the middle to
        // straight below it.
        let (right, below) = (centre + Vec2::new(30.0, 0.0), centre + Vec2::new(0.0, 30.0));
        assert_eq!(turn(centre, right, below, PLAIN), 90.0);
        let by = dragged(frame(), Handle::Turn(1), right, below, PLAIN);
        assert_eq!((at(&by, 30.0, 30.0), at(&by, 50.0, 30.0)), (Vec2::new(30.0, 30.0), Vec2::new(30.0, 50.0)));
        // Shift: in steps of 15°.
        let nearly = centre + Vec2::new(30.0 * 40f64.to_radians().cos(), 30.0 * 40f64.to_radians().sin());
        assert_eq!((turn(centre, right, nearly, SHIFT), turn(centre, right, nearly, PLAIN).round()), (45.0, 40.0));
        // Round the back, it doesn't jump.
        let (up_left, down_left) = (centre + Vec2::new(-30.0, -1.0), centre + Vec2::new(-30.0, 1.0));
        assert!(turn(centre, up_left, down_left, PLAIN).abs() < 5.0);
    }
}
