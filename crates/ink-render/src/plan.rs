//! A drawing laid out once and drawn part by part (ARCHITECTURE §8):
//! the window's canvas is tiles of one picture too big to draw whole,
//! each drawn on a core of its own while the frame goes on without it.
//!
//! A [`Plan`] is the drawing flattened into the px of that picture at
//! one zoom: plain data, shared by every thread that draws from it.
//! [`Plan::part`] draws any rectangle of it, with as much of the
//! picture around it as its shadows look (the bands' margin, §5.1), so
//! parts laid edge to edge show no seam.

use ink_doc::Document;
use ink_geom::{Affine, Rect, Vec2};
use lntrn_image::Image;

use crate::BadSize;
use crate::draw::{Scene, lay};
use crate::scene::{Item, Polys};

/// The furthest past a part's own edges a shadow is followed, px. A
/// blur wider than this (one zoomed into until it's most of a screen)
/// is cut short at a part's edge: the room to follow it would be many
/// times the part itself.
pub const MAX_REACH: u32 = 1024;

/// A drawing ready to draw at one zoom.
pub struct Plan {
    items: Vec<Item<Polys>>,
    reach: u32,
    bounds: Option<Rect>,
}

/// Grow `bounds` to hold `polys`.
fn hold((polys, _): &Polys, bounds: &mut Option<Rect>) {
    for &p in polys.iter().flatten().filter(|p| p.is_finite()) {
        *bounds = Some(bounds.map_or(Rect::new(p, p), |b| Rect::new(b.min.min(p), b.max.max(p))));
    }
}

/// Grow `bounds` to hold everything `items` can paint.
fn extent(items: &[Item<Polys>], bounds: &mut Option<Rect>) {
    for item in items {
        match item {
            Item::Fill { shape, .. } => hold(shape, bounds),
            Item::Layer(layer) => {
                // A filter can paint anywhere in its region (a flood).
                if let (false, Some(region)) = (layer.filter.is_empty(), &layer.cut) {
                    hold(region, bounds);
                }
                extent(&layer.items, bounds);
            }
        }
    }
}

impl Plan {
    /// `doc` through `page_to_px` (the page's px to the picture's).
    /// With `clip_to_page`, nothing shows past the page's edges.
    pub fn new(doc: &Document, page_to_px: &Affine, clip_to_page: bool) -> Plan {
        if !page_to_px.is_finite() {
            return Plan { items: Vec::new(), reach: 0, bounds: None };
        }
        let (items, furthest) = lay(doc, page_to_px, |_| clip_to_page);
        let reach = if furthest.is_finite() { furthest.ceil().clamp(0.0, MAX_REACH as f64) as u32 } else { MAX_REACH };
        let mut bounds = None;
        extent(&items, &mut bounds);
        // A shadow falls as far as it looks; and a px for the rounding.
        Plan { items, reach, bounds: bounds.map(|b| b.expand(reach as f64 + 1.0)) }
    }

    /// The box around everything the drawing paints, in the picture's
    /// px, its shadows included: a part that lies outside it is clear,
    /// and needn't be drawn to know it. `None` for a drawing that
    /// paints nothing.
    pub fn bounds(&self) -> Option<Rect> {
        self.bounds
    }

    /// How much of the picture around a part is drawn to get the part
    /// right, px each way: what a part costs beyond its own size.
    pub fn reach(&self) -> u32 {
        self.reach
    }

    /// Whether the drawing draws nothing at all.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The `width` × `height` px of the picture whose top-left pixel
    /// is (`x`, `y`): straight-alpha RGBA8, clear where nothing is
    /// drawn. Drawn on the calling thread.
    pub fn part(&self, x: i64, y: i64, width: u32, height: u32) -> Result<Image, BadSize> {
        let pixels = width as u64 * height as u64;
        if pixels == 0 || pixels > lntrn_image::MAX_PIXELS {
            return Err(BadSize { width, height });
        }
        let scene = Scene::fit(&self.items, Vec2::new(x as f64, y as f64), (width as usize, height as usize), self.reach as usize, false);
        Ok(Image::new(width, height, scene.render_alone()))
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::{DocId, Viewport};

    use super::*;
    use crate::{View, render};

    fn doc(svg: &str) -> Document {
        Document::parse(DocId(1), svg).unwrap()
    }

    /// `plan`'s picture `w` × `h` from the corner (`x0`, `y0`), put
    /// together from parts `side` px square.
    fn tiled(plan: &Plan, (x0, y0): (i64, i64), (w, h): (u32, u32), side: u32) -> Vec<u8> {
        let mut out = vec![0u8; (w * h * 4) as usize];
        for ty in (0..h).step_by(side as usize) {
            for tx in (0..w).step_by(side as usize) {
                let (pw, ph) = (side.min(w - tx), side.min(h - ty));
                let part = plan.part(x0 + tx as i64, y0 + ty as i64, pw, ph).unwrap();
                for row in 0..ph {
                    let (from, to) = ((row * pw * 4) as usize, (((ty + row) * w + tx) * 4) as usize);
                    out[to..to + (pw * 4) as usize].copy_from_slice(&part.rgba[from..from + (pw * 4) as usize]);
                }
            }
        }
        out
    }

    /// The most any channel of any pixel differs by.
    fn furthest_apart(a: &[u8], b: &[u8]) -> u8 {
        assert_eq!(a.len(), b.len());
        a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0)
    }

    const BUSY: &str = r##"<svg viewBox="0 0 40 40"><defs><filter id="s"><feDropShadow dx="1.5" dy="2.5" stdDeviation="1.2" flood-opacity="0.7"/></filter><clipPath id="c"><circle cx="20" cy="20" r="17"/></clipPath><linearGradient id="g"><stop stop-color="#f80"/><stop offset="1" stop-color="#08f"/></linearGradient><radialGradient id="r"><stop stop-color="#fff"/><stop offset="1" stop-color="#a0f" stop-opacity="0.4"/></radialGradient></defs><g filter="url(#s)" opacity="0.9"><rect x="4" y="-3" width="20" height="18" rx="4" fill="url(#g)"/><g clip-path="url(#c)" filter="url(#s)"><path d="M2 38 L20 8 L38 38 Z" fill="#3a3" stroke="#000" stroke-width="2" stroke-dasharray="3 2" stroke-linejoin="round"/></g></g><ellipse cx="29" cy="12" rx="9" ry="6" fill="url(#r)" stroke="#fc0" stroke-width="0.7"/></svg>"##;

    #[test]
    fn parts_put_together_are_the_picture_drawn_whole() {
        let doc = doc(BUSY);
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        for scale in [1.0, 3.3, 8.0] {
            let view = View { clip_to_page: false, ..View::page(&viewport, scale) };
            let whole = render(&doc, &view).unwrap();
            let plan = Plan::new(&doc, &view.page_to_px, false);
            assert!(plan.reach() > 0 && plan.reach() < whole.width.max(whole.height), "its shadows reach {} px", plan.reach());
            for side in [whole.width.max(whole.height), 64, 37] {
                // A part is the same picture moved, so the same to
                // within a rounding of the last bit.
                let apart = furthest_apart(&tiled(&plan, (0, 0), (whole.width, whole.height), side), &whole.rgba);
                assert!(apart <= 1, "at {scale}, in parts of {side} px: {apart} levels apart");
            }
        }
    }

    #[test]
    fn a_part_can_lie_anywhere_on_the_picture_and_past_it() {
        let doc = doc(BUSY);
        let plan = Plan::new(&doc, &Affine::scale(4.0, 4.0), false);
        // The picture from 30 px up and left of the page's corner on:
        // the page's own pixels are where they'd be.
        let whole = tiled(&plan, (0, 0), (160, 160), 160);
        let wider = tiled(&plan, (-30, -30), (220, 220), 50);
        let mut worst = 0;
        for y in 0..160usize {
            let (a, b) = (&whole[y * 160 * 4..(y + 1) * 160 * 4], &wider[((y + 30) * 220 + 30) * 4..((y + 30) * 220 + 30 + 160) * 4]);
            worst = worst.max(furthest_apart(a, b));
        }
        assert!(worst <= 1, "{worst} levels apart");
        // Off the page, the rect that starts above it is there to see.
        assert!(wider[(22 * 220 + 30 + 56) * 4 + 3] > 200, "the rect above the page's top edge");
        // Far from everything there's nothing, and it costs nothing.
        let far = plan.part(1 << 40, -(1 << 40), 64, 64).unwrap();
        assert!(far.rgba.iter().all(|&b| b == 0));
        // Its bounds hold every pixel it paints: all that's outside
        // them is clear.
        let b = plan.bounds().unwrap();
        assert!(b.min.x < 0.0 && b.min.y < -12.0 && b.max.x > 150.0 && b.max.y > 150.0, "{b:?}");
        for (y, row) in wider.chunks_exact(220 * 4).enumerate() {
            for (x, px) in row.chunks_exact(4).enumerate() {
                let at = Vec2::new(x as f64 - 30.0 + 0.5, y as f64 - 30.0 + 0.5);
                assert!(px[3] == 0 || b.contains(at), "paint at {at:?}, outside {b:?}");
            }
        }
    }

    #[test]
    fn the_page_cuts_only_when_asked() {
        let doc = doc(r##"<svg viewBox="0 0 10 10"><rect x="-5" y="-5" width="20" height="20"/></svg>"##);
        let alpha = |clip: bool, x: i64| Plan::new(&doc, &Affine::scale(2.0, 2.0), clip).part(x, 4, 1, 1).unwrap().rgba[3];
        assert_eq!((alpha(false, -4), alpha(false, 4), alpha(true, -4), alpha(true, 4)), (255, 255, 0, 255));
    }

    #[test]
    fn a_plan_is_shared_between_threads_and_refuses_what_it_cant_draw() {
        fn shared<T: Send + Sync>() {}
        shared::<Plan>();
        let plan = Plan::new(&doc(BUSY), &Affine::scale(f64::NAN, 1.0), false);
        assert!(plan.is_empty() && plan.reach() == 0 && plan.bounds().is_none());
        assert!(plan.part(0, 0, 0, 5).is_err() && plan.part(0, 0, u32::MAX, u32::MAX).is_err());
        // A shadow that looks further than a part's margin may go is
        // followed that far and no further.
        let huge = doc(r##"<svg viewBox="0 0 8 8"><defs><filter id="s"><feDropShadow stdDeviation="5000"/></filter></defs><rect width="8" height="8" filter="url(#s)"/></svg>"##);
        assert_eq!(Plan::new(&huge, &Affine::IDENTITY, false).reach(), MAX_REACH);
    }
}
