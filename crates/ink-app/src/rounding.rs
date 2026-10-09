//! A rectangle's own handle (M4c): one round dot just inside its first
//! corner (Alva's choice: one, not one a corner), dragged toward the
//! middle to round all four corners and back toward the corner to
//! square them. Apart from the window, so the geometry is tested by
//! itself; the Pointer has the drag (`pointer.rs`).
//!
//! The dot stands on the middle of the corner's arc, a little further
//! in, so that at no rounding at all it's clear of the box's own handle
//! on that corner; and it follows the pointer along the line it travels,
//! from wherever it was taken hold of. Corners that were rounder one way
//! than the other keep that shape.

use ink_core::{Document, NodeId};
use ink_doc::{Geometry, geometry};
use ink_geom::Affine;
use lntrn_math::Vec2;

/// The dot as drawn, how far round its middle a press still takes it,
/// and how far in from the arc's middle it stands, logical px.
pub const DOT: f64 = 18.0;
pub const REACH: f64 = 14.0;
pub const INSET: f64 = 28.0;

/// A rectangle's own numbers, its rounding as it's drawn (no rounder
/// than half a side, whatever the file says).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounded {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rx: f64,
    pub ry: f64,
}

impl Rounded {
    /// `node`, if it's a rectangle whose numbers can all be read, with
    /// some size.
    pub fn of(doc: &Document, node: NodeId) -> Option<Rounded> {
        match Geometry::of(doc.get(node)?)? {
            Geometry::Rect { x, y, width, height, rx, ry } if width > 0.0 && height > 0.0 => Some(Rounded { x, y, width, height, rx: rx.min(width / 2.0), ry: ry.min(height / 2.0) }),
            _ => None,
        }
    }

    /// The rounding one number says: across.
    pub fn radius(&self) -> f64 {
        self.rx
    }

    /// How much further down than across its corners are rounded: 1,
    /// unless the file made them otherwise.
    fn shape(&self) -> f64 {
        if self.rx > 0.0 && self.ry > 0.0 { self.ry / self.rx } else { 1.0 }
    }

    /// The most its corners can be rounded, across.
    pub fn most(&self) -> f64 {
        (self.width / 2.0).min(self.height / 2.0 / self.shape())
    }

    /// This rectangle with its corners rounded `radius` across, no
    /// further than they can be.
    pub fn with(&self, radius: f64) -> Rounded {
        let rx = radius.clamp(0.0, self.most());
        Rounded { rx, ry: rx * self.shape(), ..*self }
    }

    pub fn geometry(&self) -> Geometry {
        Geometry::Rect { x: self.x, y: self.y, width: self.width, height: self.height, rx: self.rx, ry: self.ry }
    }

    /// What a drag of the dot from `from` to `to` (the rectangle's own
    /// coordinates) makes of it: rounder by as far as the pointer has
    /// gone along the dot's own line. On whole steps of `grid`, where
    /// there is one.
    pub fn dragged(&self, from: Vec2, to: Vec2, grid: f64) -> Rounded {
        let (k, d) = (self.shape(), to - from);
        let mut radius = self.rx + (d.x + k * d.y) / (1.0 + k * k);
        if grid > 0.0 {
            radius = (radius / grid).round() * grid;
        }
        self.with(radius)
    }

    /// Where the dot shows, window px, through `to_window` (from the
    /// rectangle's own coordinates) at `scale` window px a logical one.
    /// None where the rectangle shows too small for a dot clear of its
    /// box's handles.
    pub fn dot(&self, to_window: &Affine, scale: f64) -> Option<Vec2> {
        let (across, down) = (to_window.linear(Vec2::X), to_window.linear(Vec2::Y));
        let (wide, tall) = (across.length() * self.width, down.length() * self.height);
        if wide.min(tall) < INSET * scale * 2.0 || across.length() <= 0.0 || down.length() <= 0.0 {
            return None;
        }
        let arc = to_window.apply(Vec2::new(self.x + self.rx, self.y + self.ry));
        Some(arc + (across * (1.0 / across.length()) + down * (1.0 / down.length())) * (INSET * scale))
    }
}

/// From the document's coordinates to `node`'s own.
pub fn to_own(doc: &Document, node: NodeId) -> Option<Affine> {
    geometry::to_doc(doc, node)?.inverse()
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    fn rect(markup: &str) -> Option<Rounded> {
        let d = Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 48 48\">{markup}</svg>")).unwrap();
        Rounded::of(&d, NodeId(2))
    }

    #[test]
    fn a_rectangle_is_read_as_its_drawn() {
        let plain = rect(r#"<rect x="4" y="6" width="20" height="10"/>"#).unwrap();
        assert_eq!((plain.radius(), plain.most()), (0.0, 5.0));
        // One radius is both; neither is more than half a side.
        assert_eq!(rect(r#"<rect width="20" height="10" ry="2"/>"#).map(|r| (r.rx, r.ry)), Some((2.0, 2.0)));
        assert_eq!(rect(r#"<rect width="20" height="10" rx="30"/>"#).map(|r| (r.rx, r.ry)), Some((10.0, 5.0)));
        // What isn't a rectangle with a size and numbers that read has
        // no dot.
        for none in [r#"<circle r="4"/>"#, r#"<rect width="0" height="4"/>"#, r#"<rect width="50%" height="4"/>"#, "<g/>"] {
            assert_eq!(rect(none), None, "{none}");
        }
        assert_eq!(plain.with(3.0).geometry(), Geometry::Rect { x: 4.0, y: 6.0, width: 20.0, height: 10.0, rx: 3.0, ry: 3.0 });
    }

    #[test]
    fn the_dot_rounds_by_how_far_its_dragged_along_its_line() {
        let r = rect(r#"<rect x="4" y="6" width="20" height="10" rx="1"/>"#).unwrap();
        let at = Vec2::new(10.0, 10.0);
        let to = |dx: f64, dy: f64, grid: f64| r.dragged(at, at + Vec2::new(dx, dy), grid).rx;
        // Down and in along its line: as far as it went. Straight
        // across: half as far. Across its line: nowhere.
        assert_eq!((to(2.0, 2.0, 0.0), to(2.0, 0.0, 0.0), to(1.5, -1.5, 0.0)), (3.0, 2.0, 1.0));
        // No rounder than half its shorter side, no squarer than square.
        assert_eq!((to(30.0, 30.0, 0.0), to(-9.0, -9.0, 0.0)), (5.0, 0.0));
        // On whole steps, where there's a grid.
        assert_eq!((to(1.3, 1.3, 1.0), to(1.6, 1.6, 1.0), to(0.3, 0.3, 0.5)), (2.0, 3.0, 1.5));
        let round = r.dragged(at, at + Vec2::new(2.0, 2.0), 0.0);
        assert_eq!((round.rx, round.ry), (3.0, 3.0));
        // Rounder down than across: it keeps that shape.
        let oval = rect(r#"<rect width="20" height="10" rx="1" ry="2"/>"#).unwrap();
        let more = oval.dragged(at, at + Vec2::new(1.0, 2.0), 0.0);
        assert_eq!((more.rx, more.ry, oval.most()), (2.0, 4.0, 2.5));
        assert_eq!(oval.with(9.0).ry, 5.0);
    }

    #[test]
    fn the_dot_stands_inside_the_first_corner() {
        let r = rect(r#"<rect x="4" y="6" width="20" height="10" rx="2"/>"#).unwrap();
        // Ten px a unit: the arc's middle at (60, 80), and 28 px in.
        let zoom = Affine::scale(10.0, 10.0);
        assert_eq!(r.dot(&zoom, 1.0), Some(Vec2::new(88.0, 108.0)));
        assert_eq!(r.dot(&zoom, 1.25), Some(Vec2::new(95.0, 115.0)));
        // Mirrored and turned, it's still inside that corner.
        let turned = Affine::scale(-10.0, 10.0).then(&Affine::rotate(std::f64::consts::FRAC_PI_2));
        let dot = r.dot(&turned, 1.0).unwrap();
        let own = turned.inverse().unwrap().apply(dot);
        assert!((own - Vec2::new(8.8, 10.8)).length() < 1e-9, "{own:?}");
        // Too small on the screen for one: 10 units tall at 5 px each.
        assert_eq!(r.dot(&Affine::scale(5.0, 5.0), 1.0), None);
        assert!(r.dot(&Affine::scale(6.0, 6.0), 1.0).is_some());
    }
}
