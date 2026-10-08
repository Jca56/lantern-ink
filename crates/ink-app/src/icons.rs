//! The pictures the window's chrome is made of: the tools' icons, the
//! object tree's glyphs, the app's own, and the transparency checks
//! under a page. Each is drawn
//! by Ink's own renderer at exactly the pixels the screen's scale shows
//! it at, so none is ever stretched soft; made again when the scale
//! changes (LS3's `icons.rs`, which draws with `lntrn-svg`).

use ink_doc::{DocId, Document, Viewport};
use ink_render::View;
use lntrn_app::lntrn_render::{Gpu, Images};
use lntrn_image::Image;
use lntrn_ui::ImageHandle;

use crate::theme::TOOL_ICON;
use crate::tools::Tool;

/// The app icon beside the logo, logical px.
pub const LOGO_ICON: f64 = 30.0;
/// The checkerboard's squares, logical px, and how many it's made of
/// each way.
pub const CHECK: f64 = 8.0;
const CHECKS: (u32, u32) = (32, 16);

const APP_ICON: &[u8] = include_bytes!("../assets/lantern-ink.svg");

/// The small pictures in the object tree's rows: the eye, and what
/// kind of thing a row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Visible,
    Invisible,
    Group,
    Rect,
    Ellipse,
    Line,
    Polygon,
    Path,
    Text,
    Gradient,
}

impl Glyph {
    pub const ALL: [Glyph; 10] = [Glyph::Visible, Glyph::Invisible, Glyph::Group, Glyph::Rect, Glyph::Ellipse, Glyph::Line, Glyph::Polygon, Glyph::Path, Glyph::Text, Glyph::Gradient];

    /// LS3's eyes and its folder; the kinds are their tools' icons.
    fn svg(self) -> &'static [u8] {
        match self {
            Glyph::Visible => include_bytes!("../assets/icons/visible.svg"),
            Glyph::Invisible => include_bytes!("../assets/icons/invisible.svg"),
            Glyph::Group => include_bytes!("../assets/icons/folder.svg"),
            Glyph::Rect => Tool::Rect.icon(),
            Glyph::Ellipse => Tool::Ellipse.icon(),
            Glyph::Line => Tool::Line.icon(),
            Glyph::Polygon => Tool::Polygon.icon(),
            Glyph::Path => Tool::Pen.icon(),
            Glyph::Text => Tool::Text.icon(),
            Glyph::Gradient => Tool::Gradient.icon(),
        }
    }

    /// Its side, logical px.
    pub fn side(self) -> f64 {
        match self {
            Glyph::Visible | Glyph::Invisible => 28.0,
            _ => 24.0,
        }
    }
}

#[derive(Default)]
pub struct Icons {
    /// The scale they were made for; 0 before the first.
    scale: f64,
    /// By `Tool as usize`.
    tools: Vec<Option<ImageHandle>>,
    /// By `Glyph as usize`.
    glyphs: Vec<Option<ImageHandle>>,
    app: Option<ImageHandle>,
    checker: Option<ImageHandle>,
}

/// LS3's transparency checks (its viewport's greys), `square` px each.
fn checkerboard(square: u32) -> Image {
    let (w, h) = (CHECKS.0 * square, CHECKS.1 * square);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let v = if (x / square + y / square).is_multiple_of(2) { 194 } else { 166 };
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Image { width: w, height: h, rgba }
}

/// `svg` as an icon `px` a side: fitted and centred, as Lantern's apps
/// show one.
pub fn drawn(svg: &[u8], px: u32) -> Option<Image> {
    let doc = Document::parse(DocId(1), std::str::from_utf8(svg).ok()?).ok()?;
    let viewport = Viewport::of(doc.node(doc.root()).ok()?);
    ink_render::render(&doc, &View::icon(&viewport, px)).ok()
}

impl Icons {
    pub fn tool(&self, tool: Tool) -> Option<ImageHandle> {
        self.tools.get(tool as usize).copied().flatten()
    }

    pub fn glyph(&self, glyph: Glyph) -> Option<ImageHandle> {
        self.glyphs.get(glyph as usize).copied().flatten()
    }

    /// The app's icon, for the title bar.
    pub fn app(&self) -> Option<ImageHandle> {
        self.app
    }

    /// A sheet of transparency checks, `CHECK` logical px a square, for
    /// what's see-through to show against. Parts of it are drawn pixel
    /// for pixel (`image_uv`), never the whole stretched.
    pub fn checker(&self) -> Option<ImageHandle> {
        self.checker
    }

    /// Have the icons at `scale`. Whether any were made (so the frame
    /// wants building again to show them).
    pub fn ensure(&mut self, gpu: &Gpu, images: &mut Images, scale: f64) -> bool {
        if !(scale.is_finite() && scale > 0.0) || scale == self.scale {
            return false;
        }
        self.scale = scale;
        self.tools.resize(Tool::ALL.len(), None);
        let px = |side: f64| (side * scale).round().max(1.0) as u32;
        for tool in Tool::ALL {
            put(gpu, images, &mut self.tools[tool as usize], drawn(tool.icon(), px(TOOL_ICON)), tool.label());
        }
        self.glyphs.resize(Glyph::ALL.len(), None);
        for glyph in Glyph::ALL {
            put(gpu, images, &mut self.glyphs[glyph as usize], drawn(glyph.svg(), px(glyph.side())), &format!("{glyph:?}"));
        }
        put(gpu, images, &mut self.app, drawn(APP_ICON, px(LOGO_ICON)), "the app icon");
        put(gpu, images, &mut self.checker, Some(checkerboard(px(CHECK))), "the checks");
        true
    }
}

/// `image` into `slot`, behind the handle it has if it has one.
fn put(gpu: &Gpu, images: &mut Images, slot: &mut Option<ImageHandle>, image: Option<Image>, name: &str) {
    let Some(image) = image else {
        lntrn_core::log_error!("icon: {name} didn't draw");
        return;
    };
    *slot = Some(match *slot {
        Some(old) => images.replace(gpu, old, &image),
        None => images.add(gpu, &image),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_draws_at_its_size() {
        assert_eq!(Glyph::ALL.iter().map(|g| *g as usize).collect::<Vec<_>>(), (0..Glyph::ALL.len()).collect::<Vec<_>>());
        for (svg, name) in Tool::ALL.iter().map(|t| (t.icon(), t.label())).chain([(APP_ICON, "app"), (Glyph::Visible.svg(), "the eye"), (Glyph::Invisible.svg(), "the shut eye"), (Glyph::Group.svg(), "the folder")]) {
            for px in [34, 48] {
                let image = drawn(svg, px).unwrap_or_else(|| panic!("{name}"));
                assert_eq!((image.width, image.height), (px, px), "{name}");
                let inked = image.rgba.chunks_exact(4).filter(|p| p[3] > 128).count();
                assert!(inked > (px * px / 25) as usize, "{name} at {px}: {inked} px");
            }
        }
        let checks = checkerboard(3);
        assert_eq!((checks.width, checks.height, checks.rgba[0], checks.rgba[3 * 4]), (96, 48, 194, 166));
    }
}
