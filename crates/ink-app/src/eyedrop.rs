//! The Eyedropper (ARCHITECTURE §8): a press on the canvas takes the
//! colour of the drawing there into the fill (Shift: the stroke), of
//! what's selected and of the next shape drawn; held and dragged, it
//! goes on taking, and lands as one step.
//!
//! **The colour you see there** (Alva's choice): it's worked out from
//! the drawing itself, a speck of it drawn eight times finer than the
//! screen shows it, so it's the colour at that point through whatever
//! gradient, opacity or shadow is there, and a blend of two colours
//! only where the press is within a sixteenth of a pixel of the edge
//! between them (the screen's own pixel there is a blend all through).
//! Not finer still: a shadow's blur costs by how finely it's drawn.
//! Where the drawing is see-through, so is the colour taken; where
//! there's nothing, nothing is taken.

use ink_core::{DocId, Document};
use ink_doc::Viewport;
use ink_geom::Affine;
use ink_render::View as Picture;
use lntrn_math::{Color, Vec2};
use lntrn_ui::Ui;

use crate::canvas::CanvasInput;
use crate::ink::Ink;
use crate::paint::{Paint, Set, Which};
use crate::pointer::View;

/// How many times finer than the screen the speck is drawn.
const FINER: f64 = 8.0;

/// The colour of `drawing` at `point` (its own coordinates), where it
/// shows `zoom` px of the screen to a px of its page. None where
/// nothing is drawn.
pub fn colour_at(drawing: &Document, viewport: &Viewport, point: Vec2, zoom: f64) -> Option<Color> {
    let (at, fine) = (viewport.to_page.apply(point), (zoom * FINER).max(1e-6));
    // One pixel, with that point of the page in its middle; what's
    // drawn past the page's edge shows in the window, so counts here.
    let speck = Picture { width: 1, height: 1, page_to_px: Affine::scale(fine, fine).then(&Affine::translate(0.5 - at.x * fine, 0.5 - at.y * fine)), clip_to_page: false };
    let [r, g, b, a] = ink_render::render(drawing, &speck).ok()?.pixel(0, 0);
    let channel = |v: u8| f64::from(v) / 255.0;
    (a > 0).then(|| Color::rgba(channel(r), channel(g), channel(b), channel(a)))
}

impl Ink {
    /// One frame of the Eyedropper on `doc`.
    pub(crate) fn eyedrop_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput) {
        // Held, it goes on taking (the paint section lands it when the
        // button comes up, as it lands a colour dragged to).
        if !(input.pressed || (input.held && self.painting.is_some())) {
            return;
        }
        let which = if ui.state.mods.shift() { Which::Stroke } else { Which::Fill };
        let at = view.to_doc.apply(ui.state.pointer);
        let taken = self.core.doc(doc).ok().zip(self.core.viewport(doc).ok()).and_then(|(drawing, viewport)| {
            // How big a px of the page shows: across the way it's stretched least.
            let zoom = viewport.to_page.inverse().map_or(1.0, |to_doc| view.to_window.linear(to_doc.linear(Vec2::X)).length());
            colour_at(drawing, &viewport, at, zoom)
        });
        if let Some(colour) = taken {
            self.set_paint(Set::Paint(which, Paint::Color(colour)), true);
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId as Id;

    use super::*;

    #[test]
    fn the_colour_taken_is_the_one_drawn_at_that_very_point() {
        let drawing = Document::parse(Id(1), r##"<svg viewBox="0 0 24 24"><defs><linearGradient id="g" x1="0" x2="1" y1="0" y2="0"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient></defs><rect width="10" height="10" fill="#08f"/><rect x="12" width="10" height="10" fill="#f00" opacity="0.5"/><rect y="12" width="20" height="8" fill="url(#g)"/><circle cx="5" cy="5" r="2" fill="#fc0"/></svg>"##).unwrap();
        let viewport = Viewport::of(drawing.get(drawing.root()).unwrap());
        let hex = |x: f64, y: f64, zoom: f64| colour_at(&drawing, &viewport, Vec2::new(x, y), zoom).map(|c| format!("{:02x}{:02x}{:02x} {:.2}", (c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8, c.a));
        // A flat fill, and what's drawn over it.
        assert_eq!((hex(1.0, 1.0, 10.0).as_deref(), hex(5.0, 5.0, 10.0).as_deref()), (Some("0088ff 1.00"), Some("ffcc00 1.00")));
        // A fifth of a screen pixel from an edge, at any zoom: the
        // colour on this side of it, not a blend of the two.
        for zoom in [0.5, 10.0, 200.0] {
            let near = 0.2 / zoom;
            assert_eq!((hex(7.0 - near, 5.0, zoom).as_deref(), hex(7.0 + near, 5.0, zoom).as_deref()), (Some("ffcc00 1.00"), Some("0088ff 1.00")), "at {zoom}");
        }
        // See-through, it's taken see-through; through a gradient, the
        // colour at that point of it.
        assert_eq!(hex(15.0, 5.0, 10.0).as_deref(), Some("ff0000 0.50"));
        let grey = colour_at(&drawing, &viewport, Vec2::new(10.0, 16.0), 10.0).unwrap();
        assert!((grey.r - grey.b).abs() < 0.01 && (grey.r - 0.5).abs() < 0.01 && grey.a == 1.0, "{grey:?}");
        // Where nothing is drawn, nothing is taken.
        assert_eq!((hex(11.0, 5.0, 10.0), hex(40.0, 40.0, 10.0)), (None, None));
    }
}
