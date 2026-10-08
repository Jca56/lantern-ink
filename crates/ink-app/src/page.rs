//! What the canvas shows under everything else: the tan ground, the
//! page on transparency checks with its edge marked, and the drawing's
//! tiles over them. What's drawn past the page's edge shows too: it's
//! there to be grabbed.

use ink_core::DocId;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{ImageHandle, Ui};

use crate::camera::Camera;
use crate::theme::{ACTIVE, GROUND};
use crate::tiles::Tiles;

/// The line round the page, logical px.
const EDGE: f64 = 2.0;

/// The page (`size` px at zoom 1) as `cam` shows it in `area`, window
/// px.
pub fn rect(area: Rect, cam: &Camera, size: Vec2) -> Rect {
    let min = cam.window_at(area, Vec2::ZERO);
    let max = cam.window_at(area, size);
    Rect::new(min, Vec2::new(max.x.round().max(min.x + 1.0), max.y.round().max(min.y + 1.0)))
}

pub fn draw(ui: &mut Ui, area: Rect, cam: &Camera, size: Vec2, checks: Option<ImageHandle>, tiles: &Tiles, doc: DocId) {
    ui.draw.rect(area, GROUND);
    let page = rect(area, cam, size);
    ui.draw.push_clip(area);
    // The edge, just outside the page: the drawing lies over it where
    // it runs past.
    let line = (EDGE * ui.m.scale).round().max(1.0);
    ui.draw.stroke_rect(page.expand(line), line, 0.0, ACTIVE);
    // The checks keep their size on the screen, and go with the page.
    let shown = page.intersection(&area);
    if let (Some(sheet), false) = (checks, shown.is_empty()) {
        let (w, h) = (f64::from(sheet.width), f64::from(sheet.height));
        ui.draw.push_clip(shown);
        let (first_x, first_y) = (((shown.min.x - page.min.x) / w).floor(), ((shown.min.y - page.min.y) / h).floor());
        let mut y = page.min.y + first_y * h;
        while y < shown.max.y {
            let mut x = page.min.x + first_x * w;
            while x < shown.max.x {
                ui.draw.image(Rect::from_xywh(x, y, w, h), sheet, 0.0, Color::WHITE);
                x += w;
            }
            y += h;
        }
        ui.draw.pop_clip();
    }
    ui.draw.pop_clip();
    tiles.draw(ui, doc, area, page.min, cam.zoom);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_lies_on_whole_pixels_and_never_vanishes() {
        let area = Rect::from_xywh(82.0, 89.0, 1436.0, 939.0);
        let cam = Camera::fit(area, Vec2::new(24.0, 24.0));
        let r = rect(area, &cam, Vec2::new(24.0, 24.0));
        assert!(r.min.x.fract() == 0.0 && r.min.y.fract() == 0.0 && r.max.x.fract() == 0.0 && r.max.y.fract() == 0.0);
        assert!((r.width() - 24.0 * cam.zoom).abs() <= 0.5 && (r.center() - area.center()).length() <= 1.0);
        // Zoomed out until it's less than a pixel, it's a pixel still.
        let tiny = rect(area, &Camera::centred(area, Vec2::new(24.0, 24.0), 1.0 / 64.0), Vec2::new(24.0, 24.0));
        assert_eq!((tiny.width(), tiny.height()), (1.0, 1.0));
    }
}
