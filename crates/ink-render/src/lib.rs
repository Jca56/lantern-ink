//! Lantern Ink's renderer (ARCHITECTURE §5): a document to pixels, on
//! the CPU alone. One renderer for the window's canvas, Claude's
//! previews and exports, so they can't disagree; pure and deterministic,
//! so its pictures can be tested to the byte.
//!
//! [`render`] draws a [`Document`] as a [`View`] says: at what size, and
//! how the page sits in the picture.

mod coverage;
mod draw;
mod filter;
mod paint;
mod par;
mod raster;
mod scene;

use core::fmt;

use ink_doc::{Document, Viewport};
use ink_geom::Affine;
use lntrn_image::Image;

/// What part of a drawing a picture shows, and how big.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The picture's size, px.
    pub width: u32,
    pub height: u32,
    /// The page's px (see [`Viewport`]) to the picture's.
    pub page_to_px: Affine,
    /// Show nothing past the page's edges, as a browser or an app
    /// showing the file wouldn't.
    pub clip_to_page: bool,
}

impl View {
    /// The whole page and nothing else, `scale` px to each of its own.
    pub fn page(viewport: &Viewport, scale: f64) -> View {
        let size = viewport.size * scale;
        View { width: size.x.ceil().max(1.0) as u32, height: size.y.ceil().max(1.0) as u32, page_to_px: Affine::scale(scale, scale), clip_to_page: true }
    }

    /// The page fitted into a square of `size` px and centred: an icon,
    /// as Lantern's apps (`lntrn_svg::render`) draw one.
    pub fn icon(viewport: &Viewport, size: u32) -> View {
        let side = size as f64;
        let scale = side / viewport.size.x.max(viewport.size.y);
        let (x, y) = ((side - viewport.size.x * scale) * 0.5, (side - viewport.size.y * scale) * 0.5);
        View { width: size, height: size, page_to_px: Affine::scale(scale, scale).then(&Affine::translate(x, y)), clip_to_page: true }
    }
}

/// A picture that can't be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadSize {
    pub width: u32,
    pub height: u32,
}

impl fmt::Display for BadSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a picture can't be {} × {} px (at least 1 × 1, at most {} px in all)", self.width, self.height, lntrn_image::MAX_PIXELS)
    }
}

impl std::error::Error for BadSize {}

/// Draw `doc` as `view` says: straight-alpha RGBA8, clear where nothing
/// is drawn.
pub fn render(doc: &Document, view: &View) -> Result<Image, BadSize> {
    let pixels = view.width as u64 * view.height as u64;
    if pixels == 0 || pixels > lntrn_image::MAX_PIXELS || !view.page_to_px.is_finite() {
        return Err(BadSize { width: view.width, height: view.height });
    }
    Ok(Image::new(view.width, view.height, draw::Scene::build(doc, view).render()))
}
