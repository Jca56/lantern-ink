//! What's drawn over the canvas in screen px, with LUI2's own lines, so
//! it's the same size at every zoom (ARCHITECTURE §8): the selection's
//! boxes. (The Pointer's handles join them in M4b's next piece.)

use std::collections::HashMap;

use ink_doc::{NodeId, Viewport};
use lntrn_math::{Rect, Vec2};
use lntrn_ui::Ui;

use crate::camera::Camera;
use crate::theme::ACCENT;

/// The selection's line, logical px.
const LINE: f64 = 2.0;

/// Where `b` (a box in the drawing's coordinates) shows in the window.
pub fn window_box(area: Rect, cam: &Camera, viewport: &Viewport, b: &Rect) -> Rect {
    let page = viewport.to_page.bounds(b);
    Rect::new(cam.window_at(area, page.min), cam.window_at(area, page.max))
}

/// A gold box round each of `selected` that draws anything, just
/// outside it.
pub fn selection(ui: &mut Ui, area: Rect, cam: &Camera, viewport: &Viewport, boxes: &HashMap<NodeId, Rect>, selected: &[NodeId]) {
    let line = (LINE * ui.m.scale).round().max(1.0);
    ui.draw.push_clip(area);
    for b in selected.iter().filter_map(|id| boxes.get(id)) {
        let r = window_box(area, cam, viewport, b);
        let r = Rect::new(Vec2::new(r.min.x.round(), r.min.y.round()), Vec2::new(r.max.x.round(), r.max.y.round())).expand(line);
        if r.intersects(&area) {
            ui.draw.stroke_rect(r, line, 0.0, ACCENT);
        }
    }
    ui.draw.pop_clip();
}

#[cfg(test)]
mod tests {
    use ink_doc::{DocId, Document};

    use super::*;

    #[test]
    fn a_box_in_the_drawing_shows_where_the_camera_puts_it() {
        // A 24-unit drawing on a 48 px page: a unit is 2 px of it.
        let doc = Document::parse(DocId(1), r#"<svg width="48" height="48" viewBox="0 0 24 24"/>"#).unwrap();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        let area = Rect::from_xywh(100.0, 50.0, 800.0, 600.0);
        let cam = Camera::centred(area, viewport.size, 4.0);
        let corner = cam.window_at(area, Vec2::ZERO);
        let r = window_box(area, &cam, &viewport, &Rect::from_xywh(6.0, 3.0, 12.0, 9.0));
        // 8 window px to a unit, from the page's corner.
        assert_eq!((r.min - corner, r.size()), (Vec2::new(48.0, 24.0), Vec2::new(96.0, 72.0)));
    }
}
