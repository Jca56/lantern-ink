//! A scene to pixels: its rows drawn in bands, on every core. Each band
//! is drawn with as many rows (and columns) beyond it as the scene's
//! shadows look, so a band's pixels are the same wherever the bands are
//! cut.

use ink_doc::{Document, Viewport};
use ink_geom::Vec2;

use crate::filter::drop_shadows;
use crate::coverage::Shape;
use crate::raster::{Canvas, Pixel, to_bytes};
use crate::scene::{Builder, Clip, Item, Polys, fitted};
use crate::{View, par};

/// Rows a core draws at a time, at least.
const BAND: usize = 32;
/// A picture under this many pixels is drawn in one go.
const SMALL: usize = 64 * 64;

pub(crate) struct Scene {
    width: usize,
    height: usize,
    /// How far past the picture's edges the frame goes, each way: far
    /// enough for what's out there to cast its shadows in.
    margin: usize,
    /// The shadows look further than the margin could go: only the
    /// picture drawn whole is the same every time.
    whole: bool,
    items: Vec<Item<Shape>>,
}

impl Scene {
    pub fn build(doc: &Document, view: &View) -> Scene {
        let (width, height) = (view.width as usize, view.height as usize);
        let Ok(root) = doc.node(doc.root()) else { return Scene { width, height, margin: 0, whole: true, items: Vec::new() } };
        let viewport = Viewport::of(root);
        let to_px = viewport.to_page.then(&view.page_to_px);
        // The page's edge, where the picture shows anything past it.
        let corners = [Vec2::ZERO, Vec2::new(viewport.size.x, 0.0), viewport.size, Vec2::new(0.0, viewport.size.y)].map(|p| view.page_to_px.apply(p));
        let (lo, hi) = corners.iter().fold((corners[0], corners[0]), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        let upright = view.page_to_px.b == 0.0 && view.page_to_px.c == 0.0;
        let fills = upright && lo.x <= 1e-3 && lo.y <= 1e-3 && hi.x >= width as f64 - 1e-3 && hi.y >= height as f64 - 1e-3;
        let mut builder = Builder::new(doc, viewport.view);
        let items = builder.root(root, &to_px, (view.clip_to_page && !fills).then_some(corners));
        // Shadows that look further than the picture is long are cut
        // off there.
        let (reach, longest) = (builder.furthest.ceil(), width.max(height) as f64);
        let (margin, whole) = if reach <= longest { (reach as usize, false) } else { (longest as usize, true) };
        let (frame_w, frame_h) = (width + 2 * margin, height + 2 * margin);
        let offset = Vec2::splat(margin as f64);
        let fit = |(polys, rule): Polys| {
            let moved: Vec<Vec<Vec2>> = polys.into_iter().map(|poly| poly.into_iter().map(|p| p + offset).collect()).collect();
            Shape::new(&moved, rule, frame_w, frame_h)
        };
        Scene { width, height, margin, whole, items: fitted(items, &fit) }
    }

    /// The picture, as straight-alpha RGBA8, top row first.
    pub fn render(&self) -> Vec<u8> {
        self.render_in_bands(if self.width * self.height <= SMALL { self.height } else { BAND.max(2 * self.margin) })
    }

    /// The picture, drawn `band` rows at a time (all at once when its
    /// shadows look further than its margin goes).
    fn render_in_bands(&self, band: usize) -> Vec<u8> {
        let band = if self.whole { self.height } else { band.clamp(1, self.height) };
        let bands: Vec<(usize, usize)> = (0..self.height).step_by(band).map(|y0| (y0, (y0 + band).min(self.height))).collect();
        let done = par::map(bands, |(y0, y1)| {
            let mut bytes = Vec::with_capacity(self.width * (y1 - y0) * 4);
            to_bytes(&self.band(y0, y1), &mut bytes);
            bytes
        });
        done.concat()
    }

    /// The picture's rows `y0..y1`.
    fn band(&self, y0: usize, y1: usize) -> Vec<Pixel> {
        let m = self.margin;
        // The frame's rows for these: the margin above and below too.
        let mut canvas = Canvas::new(self.width + 2 * m, y0, y1 - y0 + 2 * m);
        draw(&mut canvas, &self.items, Vec2::splat(-(m as f64)));
        let all = canvas.finish();
        if m == 0 {
            return all;
        }
        let frame_w = self.width + 2 * m;
        (m..m + y1 - y0).flat_map(|r| all[r * frame_w + m..r * frame_w + m + self.width].iter().copied()).collect()
    }
}

/// Draw `items` onto `canvas`'s top layer. `origin` is where the frame's
/// corner is in the picture's px.
fn draw(canvas: &mut Canvas, items: &[Item<Shape>], origin: Vec2) {
    for item in items {
        match item {
            Item::Fill { shape, paint, alpha } => canvas.fill(shape, paint, *alpha, origin),
            Item::Layer(layer) => {
                canvas.push_layer();
                draw(canvas, &layer.items, origin);
                let mut pixels = canvas.pop_layer();
                if !layer.shadows.is_empty() {
                    drop_shadows(&mut pixels, canvas.w, canvas.h, &layer.shadows);
                }
                if let Some(cut) = &layer.cut {
                    through(&mut pixels, &canvas.mask(cut));
                }
                if let Some(clip) = &layer.clip {
                    through(&mut pixels, &clip_mask(canvas, clip));
                }
                canvas.composite(&pixels, layer.opacity);
            }
        }
    }
}

/// Keep of each pixel what `mask` lets through.
fn through(pixels: &mut [Pixel], mask: &[f32]) {
    for (px, cover) in pixels.iter_mut().zip(mask) {
        *px = px.map(|v| v * cover);
    }
}

/// How much of each of the band's pixels `clip` lets through, 0..1.
fn clip_mask(canvas: &mut Canvas, clip: &Clip<Shape>) -> Vec<f32> {
    let mut mask: Option<Vec<f32>> = None;
    for (shape, within) in &clip.shapes {
        let mut cover = canvas.mask(shape);
        if let Some(within) = within {
            cover.iter_mut().zip(clip_mask(canvas, within)).for_each(|(c, w)| *c *= w);
        }
        // Shapes add up: what one lets through, the rest can't take
        // away.
        match &mut mask {
            Some(mask) => mask.iter_mut().zip(cover).for_each(|(m, c)| *m += c * (1.0 - *m)),
            None => mask = Some(cover),
        }
    }
    let mut mask = mask.unwrap_or_else(|| vec![0.0; canvas.w * canvas.h]);
    if let Some(outer) = &clip.outer {
        mask.iter_mut().zip(clip_mask(canvas, outer)).for_each(|(m, o)| *m *= o);
    }
    mask
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    fn scene(svg: &str, size: u32) -> Scene {
        let doc = Document::parse(DocId(1), svg).unwrap();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        Scene::build(&doc, &View::icon(&viewport, size))
    }

    const SHADOWED: &str = r##"<svg viewBox="0 0 40 40"><defs><filter id="s"><feDropShadow dx="1.5" dy="2.5" stdDeviation="1.2" flood-opacity="0.7"/></filter><clipPath id="c"><circle cx="20" cy="20" r="17"/></clipPath><linearGradient id="g"><stop stop-color="#f80"/><stop offset="1" stop-color="#08f"/></linearGradient></defs><g filter="url(#s)" opacity="0.9"><rect x="4" y="-3" width="20" height="18" rx="4" fill="url(#g)"/><g clip-path="url(#c)" filter="url(#s)"><path d="M2 38 L20 8 L38 38 Z" fill="#3a3" stroke="#000" stroke-width="2" stroke-dasharray="3 2" stroke-linejoin="round"/></g></g></svg>"##;

    #[test]
    fn bands_never_change_a_pixel() {
        let scene = scene(SHADOWED, 96);
        assert!(scene.margin > 0 && !scene.whole, "its shadows reach {} px", scene.margin);
        let whole = scene.render_in_bands(96);
        assert!(whole.chunks_exact(4).any(|p| p[3] > 0 && p[3] < 255) && whole.len() == 96 * 96 * 4);
        for band in [1, 7, 32, 50] {
            assert!(scene.render_in_bands(band) == whole, "{band}-row bands");
        }
        assert!(scene.render() == whole);
    }

    #[test]
    fn what_is_off_the_picture_still_casts_its_shadow_in() {
        // A square just above the picture's top edge, its shadow thrown
        // down into it.
        let s = scene(r##"<svg viewBox="0 0 20 20"><defs><filter id="s" x="-100%" y="-100%" width="300%" height="300%"><feDropShadow dx="0" dy="6" stdDeviation="0.01"/></filter></defs><rect x="5" y="-5" width="10" height="4" filter="url(#s)"/></svg>"##, 20);
        let px = s.render();
        assert_eq!(px[(2 * 20 + 10) * 4 + 3], 255, "the shadow of what's off the top");
        assert_eq!(px[(8 * 20 + 10) * 4 + 3], 0);
    }

    #[test]
    fn a_shadow_wider_than_the_picture_is_drawn_whole() {
        let s = scene(r##"<svg viewBox="0 0 8 8"><defs><filter id="s"><feDropShadow stdDeviation="500"/></filter></defs><rect width="8" height="8" filter="url(#s)"/></svg>"##, 8);
        assert!(s.whole && s.margin == 8);
        assert_eq!(s.render().len(), 8 * 8 * 4);
    }

    /// The alpha at pixel (x, y) of `svg` drawn 40 px square.
    fn alpha_at(svg: &str, x: usize, y: usize) -> u8 {
        scene(svg, 40).render()[(y * 40 + x) * 4 + 3]
    }

    #[test]
    fn a_clip_path_lets_through_its_shapes_together() {
        let clipped = |clips: &str| format!(r##"<svg viewBox="0 0 40 40"><defs>{clips}</defs><rect width="40" height="40" clip-path="url(#c)"/></svg>"##);
        // Two shapes: what either lets through.
        let two = clipped(r##"<clipPath id="c"><circle cx="10" cy="10" r="6"/><rect x="20" y="20" width="16" height="10"/></clipPath>"##);
        assert_eq!((alpha_at(&two, 10, 10), alpha_at(&two, 28, 25), alpha_at(&two, 28, 10), alpha_at(&two, 2, 2)), (255, 255, 0, 0));
        // A shape in a clip path cut by a clip of its own, measured
        // against its box: the middle half of the rect.
        let nested = clipped(r##"<clipPath id="c"><rect x="20" y="20" width="16" height="10" clip-path="url(#half)"/></clipPath><clipPath id="half" clipPathUnits="objectBoundingBox"><rect x="0.25" y="0" width="0.5" height="1"/></clipPath>"##);
        assert_eq!((alpha_at(&nested, 28, 25), alpha_at(&nested, 21, 25), alpha_at(&nested, 34, 25)), (255, 0, 0));
        // A clip path with a clip path of its own: what both let through.
        let outer = clipped(r##"<clipPath id="c" clip-path="url(#left)"><rect x="10" y="10" width="20" height="20"/></clipPath><clipPath id="left"><rect width="20" height="40"/></clipPath>"##);
        assert_eq!((alpha_at(&outer, 15, 20), alpha_at(&outer, 25, 20)), (255, 0));
        // Its own transform, and an even-odd hole.
        let moved = clipped(r##"<clipPath id="c" transform="translate(10 0)"><path d="M0 0h20v20h-20z M5 5h10v10h-10z" clip-rule="evenodd"/></clipPath>"##);
        assert_eq!((alpha_at(&moved, 12, 2), alpha_at(&moved, 20, 10), alpha_at(&moved, 5, 2)), (255, 0, 0));
        // Nothing to let anything through, or a ring of clips: nothing shows.
        for none in [r##"<clipPath id="c"/>"##, r##"<clipPath id="c" clip-path="url(#c)"><rect width="40" height="40"/></clipPath>"##, r##"<clipPath id="c"><rect width="40" height="40" display="none"/></clipPath>"##] {
            assert_eq!(alpha_at(&clipped(none), 20, 20), 0, "{none}");
        }
        // A name for nothing cuts nothing.
        assert_eq!(alpha_at(r##"<svg viewBox="0 0 40 40"><rect width="40" height="40" clip-path="url(#nowhere)"/></svg>"##, 20, 20), 255);
    }

    #[test]
    fn the_page_is_cut_only_where_the_picture_shows_past_it() {
        // A wide page in a square picture: the bars above and below it
        // stay clear, though the drawing runs over them.
        let wide = scene(r##"<svg viewBox="0 0 40 20"><rect x="-50" y="-50" width="200" height="200"/></svg>"##, 40);
        let px = wide.render();
        assert_eq!((px[(5 * 40 + 20) * 4 + 3], px[(20 * 40 + 20) * 4 + 3], px[(35 * 40 + 20) * 4 + 3]), (0, 255, 0));
        assert!(matches!(wide.items.as_slice(), [Item::Layer(_)]), "one layer, cut to the page");
        let square = scene(r##"<svg viewBox="0 0 40 40"><rect width="40" height="40"/></svg>"##, 40);
        assert!(matches!(square.items.as_slice(), [Item::Fill { .. }]), "a page that fills the picture needs no cutting");
    }
}
