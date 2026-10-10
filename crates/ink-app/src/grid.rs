//! The pixel grid (ARCHITECTURE §8, "for icons"): a line at every
//! whole unit of the drawing, across the page, once the view is zoomed
//! in far enough for the units to be told apart. On an icon's page a
//! unit is a pixel of the icon at its own size, which is what the grid
//! is for: seeing where an edge will fall. View > Pixel Grid hides it.
//!
//! Drawn over the drawing in screen px, so its lines are a pixel wide
//! at every zoom.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::Ui;

use crate::pointer::View;
use crate::shapes;

/// How far apart, logical px, the grid's lines are before it shows.
pub const CELL_MIN: f64 = 8.0;

/// The grid's lines over the part of `page` (the drawing's
/// coordinates) that shows in `area`: where each line down is across
/// the window, where each line across is down it, and the page as it
/// shows. None while the view is too far out for a grid.
pub fn lines(view: &View, page: Rect, area: Rect) -> Option<(Vec<f64>, Vec<f64>, Rect)> {
    let unit = shapes::grid_for(page.width().max(page.height()));
    let per_unit = view.to_window.linear(Vec2::X).length();
    if unit * per_unit < CELL_MIN * view.scale {
        return None;
    }
    let shown = view.to_window.bounds(&page);
    let seen = shown.intersection(&area);
    if seen.is_empty() {
        return None;
    }
    // The units that show, in the drawing's coordinates.
    let (from, to) = (view.to_doc.apply(seen.min), view.to_doc.apply(seen.max));
    let along = |lo: f64, hi: f64, at: &dyn Fn(f64) -> f64| -> Vec<f64> {
        // (A line on the very edge of what shows is one that shows.)
        let (first, last) = ((lo.min(hi) / unit - 1e-6).ceil() as i64, (lo.max(hi) / unit + 1e-6).floor() as i64);
        (first..=last).map(|k| at(k as f64 * unit).round()).collect()
    };
    let xs = along(from.x, to.x, &|x| view.to_window.apply(Vec2::new(x, 0.0)).x);
    let ys = along(from.y, to.y, &|y| view.to_window.apply(Vec2::new(0.0, y)).y);
    Some((xs, ys, seen))
}

pub fn draw(ui: &mut Ui, area: Rect, view: &View, page: Rect, tint: Color) {
    let Some((xs, ys, seen)) = lines(view, page, area) else { return };
    let w = ui.m.scale.round().max(1.0);
    ui.draw.push_clip(seen);
    for x in xs {
        ui.draw.rect(Rect::from_xywh(x - (w / 2.0).floor(), seen.min.y, w, seen.height()), tint);
    }
    for y in ys {
        ui.draw.rect(Rect::from_xywh(seen.min.x, y - (w / 2.0).floor(), seen.width(), w), tint);
    }
    ui.draw.pop_clip();
}

#[cfg(test)]
mod tests {
    use ink_geom::Affine;

    use super::*;

    fn view(zoom: f64, origin: Vec2) -> View {
        let to_window = Affine::new(zoom, 0.0, 0.0, zoom, origin.x, origin.y);
        View { to_window, to_doc: to_window.inverse().unwrap(), scale: 1.0 }
    }

    #[test]
    fn a_line_at_every_unit_that_shows_once_they_can_be_told_apart() {
        let page = Rect::from_xywh(0.0, 0.0, 24.0, 24.0);
        let area = Rect::from_xywh(100.0, 50.0, 400.0, 300.0);
        // The page at (140, 80), ten px a unit: all of it across (140 to
        // 380), and down to the area's bottom at 350.
        let (xs, ys, seen) = lines(&view(10.0, Vec2::new(140.0, 80.0)), page, area).unwrap();
        assert_eq!((xs.len(), xs[0], xs[24], ys.len(), ys[0], ys[24]), (25, 140.0, 380.0, 25, 80.0, 320.0));
        assert_eq!(seen, Rect::new(Vec2::new(140.0, 80.0), Vec2::new(380.0, 320.0)));
        // Slid half off the area's left: only the units that show.
        let (xs, _, seen) = lines(&view(10.0, Vec2::new(45.0, 80.0)), page, area).unwrap();
        assert_eq!((xs[0], xs.len(), seen.min.x), (105.0, 19, 100.0));
        // Too far out: none. And none for a page out of sight.
        assert!(lines(&view(7.9, Vec2::new(140.0, 80.0)), page, area).is_none());
        assert!(lines(&view(10.0, Vec2::new(900.0, 80.0)), page, area).is_none());
        // At a bigger display scale the units must be further apart.
        assert!(lines(&View { scale: 1.3, ..view(10.0, Vec2::new(140.0, 80.0)) }, page, area).is_none());
        // A page of a unit or so has a grid of tenths.
        let tiny = Rect::from_xywh(0.0, 0.0, 2.0, 2.0);
        assert_eq!(lines(&view(100.0, Vec2::new(140.0, 80.0)), tiny, area).unwrap().0.len(), 21);
    }
}
