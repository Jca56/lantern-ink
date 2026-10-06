//! Pictures for the model: a document drawn at the size asked for (a
//! vector is sharp at any), flattened over a background, encoded within
//! a byte budget. By default that's a lossless PNG when one fits;
//! otherwise JPEG, its quality stepping down 82 → 70 → 60, then the size
//! 768 → 512 → 256. And the Lantern preview (ARCHITECTURE §5.4): the
//! drawing as `lntrn-svg`, which Lantern's apps show icons with, draws
//! it at icon sizes.

use std::path::Path;

use ink_core::ink_doc::{Document, Kind, Viewport};
use ink_core::{Core, DocId, View};
use ink_geom::{Affine, Rect};
use lntrn_image::{Compression, Filter, Image};
use lntrn_mcp::{Picture, ToolError, fail};

use crate::describe::{n, rect};
use crate::env::Env;
use crate::input::{In, refused};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    /// PNG if it fits the budget at the asked size, else JPEG.
    Auto,
    Jpeg,
    Png,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Background {
    Checker,
    White,
    Black,
    /// Kept transparent: PNG only.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Renderer {
    Ink,
    /// `lntrn-svg`, at icon sizes.
    Lantern,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Options {
    pub max_edge: u32,
    pub background: Background,
    pub format: Format,
    pub quality: u8,
    pub max_bytes: usize,
    /// The part of the drawing to show, in its own coordinates.
    pub region: Option<Rect>,
    pub renderer: Renderer,
}

pub(crate) const MAX_EDGE: u32 = 2000;

impl Default for Options {
    fn default() -> Self {
        Options { max_edge: 512, background: Background::Checker, format: Format::Auto, quality: 82, max_bytes: 256 << 10, region: None, renderer: Renderer::Ink }
    }
}

impl Options {
    pub fn from_args(input: &In) -> Result<Options, ToolError> {
        let (a, d) = (&input.args, Options::default());
        let background = match a.opt_str("background")?.unwrap_or("checker") {
            "checker" => Background::Checker,
            "white" => Background::White,
            "black" => Background::Black,
            "none" => Background::None,
            other => return fail(format!("background is checker, white, black or none, not \"{other}\"")),
        };
        let format = match (a.opt_str("format")?.unwrap_or("auto"), background) {
            // Only PNG keeps transparency.
            ("auto", Background::None) | ("png", _) => Format::Png,
            ("auto", _) => Format::Auto,
            ("jpeg", Background::None) => return fail("JPEG can't be transparent: use background checker, white or black, or format png"),
            ("jpeg", _) => Format::Jpeg,
            (other, _) => return fail(format!("format is auto, jpeg or png, not \"{other}\"")),
        };
        let renderer = match a.opt_str("renderer")?.unwrap_or("ink") {
            "ink" => Renderer::Ink,
            "lantern" => Renderer::Lantern,
            other => return fail(format!("renderer is ink or lantern, not \"{other}\"")),
        };
        let region = match a.opt_map("region", "x, y, width and height")? {
            None => None,
            Some(r) => {
                let num = |k: &str| r.get(k).and_then(|v| v.as_f64()).filter(|v| v.is_finite()).ok_or_else(|| ToolError(format!("region needs a number \"{k}\"")));
                let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
                if !(w > 0.0 && h > 0.0) {
                    return fail("region's width and height must be more than nothing");
                }
                Some(Rect::from_xywh(x, y, w, h))
            }
        };
        if region.is_some() && renderer == Renderer::Lantern {
            return fail("the lantern renderer shows the whole icon at its sizes: leave region out");
        }
        Ok(Options {
            max_edge: a.opt_int("max_edge", 16, MAX_EDGE as i64)?.map_or(d.max_edge, |v| v as u32),
            background,
            format,
            quality: a.opt_int("quality", 1, 100)?.map_or(d.quality, |v| v as u8),
            max_bytes: a.opt_int("max_bytes", 4096, 8 << 20)?.map_or(d.max_bytes, |v| v as usize),
            region,
            renderer,
        })
    }
}

/// Straight-alpha pixels over a background, opaque (or as they are, for
/// `None`). The checker's squares are 8 px, light greys.
pub(crate) fn flatten(img: &Image, bg: Background) -> Image {
    if bg == Background::None {
        return img.clone();
    }
    let w = img.width as usize;
    let rgba = img
        .rgba
        .chunks_exact(4)
        .enumerate()
        .flat_map(|(i, p)| {
            let under = match bg {
                Background::White => 255,
                Background::Black => 0,
                _ => {
                    if ((i % w) / 8 + (i / w) / 8).is_multiple_of(2) {
                        255
                    } else {
                        204
                    }
                }
            } as u32;
            let a = p[3] as u32;
            let mix = |c: u8| ((c as u32 * a + under * (255 - a) + 127) / 255) as u8;
            [mix(p[0]), mix(p[1]), mix(p[2]), 255]
        })
        .collect();
    Image::new(img.width, img.height, rgba)
}

/// The view that shows `region` of the page's px (all of it, for `None`)
/// with its longer side `edge` px.
fn view(viewport: &Viewport, region: Option<Rect>, edge: u32) -> View {
    let Some(region) = region else {
        return View::page(viewport, edge as f64 / viewport.size.x.max(viewport.size.y));
    };
    // The region is in the drawing's coordinates; the page's px are
    // where it lands.
    let on_page = viewport.to_page.bounds(&region);
    let scale = edge as f64 / on_page.width().max(on_page.height());
    View {
        width: (on_page.width() * scale).ceil().max(1.0) as u32,
        height: (on_page.height() * scale).ceil().max(1.0) as u32,
        page_to_px: Affine::translate(-on_page.min.x, -on_page.min.y).then(&Affine::scale(scale, scale)),
        clip_to_page: true,
    }
}

fn background_note(bg: Background) -> &'static str {
    match bg {
        Background::Checker => "transparency as a checkerboard",
        Background::White => "on white",
        Background::Black => "on black",
        Background::None => "transparent",
    }
}

/// Write `image` to `path` as a PNG, and say where (or why not).
fn keep(image: &Image, path: &Path) -> String {
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(path, lntrn_image::encode_png_with(image, Compression::Fast)));
    match written {
        Ok(()) => format!("On disk: {}", path.display()),
        Err(e) => format!("(it couldn't be written to disk: {e})"),
    }
}

/// A picture of `doc` as `o` asks, and a line saying what it is.
pub(crate) fn preview(core: &Core, env: &Env, doc: DocId, o: &Options) -> Result<(Picture, String), ToolError> {
    if o.renderer == Renderer::Lantern {
        return lantern(core, env, doc, o);
    }
    let viewport = core.viewport(doc).map_err(refused)?;
    let edges: Vec<u32> = std::iter::once(o.max_edge).chain([768, 512, 256].into_iter().filter(|&e| e < o.max_edge)).collect();
    // Each edge's encodings, in order; `None` is PNG. Auto tries PNG at
    // the asked size only: a lossless picture isn't worth shrinking for.
    let qualities = || std::iter::once(o.quality).chain([70, 60].into_iter().filter(|&q| q < o.quality)).map(Some);
    let tries = |first: bool| -> Vec<Option<u8>> {
        match o.format {
            Format::Png => vec![None],
            Format::Jpeg => qualities().collect(),
            Format::Auto => (first.then_some(None)).into_iter().chain(qualities()).collect(),
        }
    };
    let mut best: Option<(Vec<u8>, u32, u32, Option<u8>)> = None;
    let mut on_disk = String::new();
    'search: for (i, &edge) in edges.iter().enumerate() {
        let drawn = core.render(doc, &view(&viewport, o.region, edge)).map_err(refused)?;
        if i == 0 {
            on_disk = keep(&drawn, &env.preview_path(doc));
        }
        let flat = flatten(&drawn, o.background);
        for q in tries(i == 0) {
            let bytes = match q {
                None => lntrn_image::encode_png(&flat),
                Some(q) => lntrn_image::encode_jpeg(&flat, q),
            };
            let fits = bytes.len() <= o.max_bytes;
            best = Some((bytes, flat.width, flat.height, q));
            if fits {
                break 'search;
            }
        }
    }
    let Some((bytes, pw, ph, q)) = best else { return fail("nothing to preview") };
    let format = q.map_or("PNG".to_owned(), |q| format!("JPEG q{q}"));
    let over = if bytes.len() > o.max_bytes { format!(", over the {} KB budget even at its smallest", o.max_bytes >> 10) } else { String::new() };
    let what = match o.region {
        Some(r) => format!("the part at {} of", rect(&r)),
        None => "all of".into(),
    };
    let note = format!("Preview: {what} {doc} (page {} × {}) → {pw}×{ph} {format} ({} KB{over}), {}. {on_disk}", n(viewport.size.x), n(viewport.size.y), bytes.len().div_ceil(1024), background_note(o.background));
    Ok((if q.is_none() { Picture::png(&bytes) } else { Picture::jpeg(&bytes) }, note))
}

/// The sizes the Lantern preview draws an icon at.
const ICON_SIZES: [u32; 5] = [16, 24, 32, 48, 64];
/// How big each is shown, about: enlarged by a whole number, pixel for
/// pixel, so every pixel of the small icon can be seen.
const SHOWN: u32 = 128;
const GAP: u32 = 12;

/// What's in `doc` that `lntrn-svg` doesn't draw, by name.
fn lantern_misses(doc: &Document) -> Vec<&'static str> {
    let mut misses: Vec<&'static str> = Vec::new();
    for id in doc.descendants(doc.root()) {
        let Some(node) = doc.get(id) else { continue };
        let miss = match node.kind {
            Kind::Text | Kind::TSpan => "<text>",
            Kind::Use => "<use>",
            Kind::Image => "<image>",
            Kind::Mask => "<mask>",
            Kind::Pattern => "<pattern>",
            Kind::Marker => "<marker>",
            Kind::Style => "<style> rules",
            Kind::FilterPrimitive if node.local() != "feDropShadow" => "filters other than feDropShadow",
            _ => continue,
        };
        if !misses.contains(&miss) {
            misses.push(miss);
        }
    }
    misses
}

/// The drawing as `lntrn-svg` draws it at each icon size, side by side,
/// each enlarged pixel for pixel.
fn lantern(core: &Core, env: &Env, doc: DocId, o: &Options) -> Result<(Picture, String), ToolError> {
    let document = core.doc(doc).map_err(refused)?;
    let svg = document.to_svg();
    let icons: Vec<Image> = ICON_SIZES.iter().map(|&size| lntrn_svg::render(&svg, size).map(|icon| icon.resize(size * (SHOWN / size), size * (SHOWN / size), Filter::Nearest))).collect::<Option<_>>().ok_or_else(|| ToolError("lntrn-svg couldn't read the drawing at all".into()))?;
    let width = icons.iter().map(|i| i.width).sum::<u32>() + GAP * (icons.len() as u32 + 1);
    let height = SHOWN + 2 * GAP;
    let mut strip = Image::new(width, height, vec![0; (width * height * 4) as usize]);
    let mut x = GAP;
    for icon in &icons {
        let y = GAP + (SHOWN - icon.height) / 2;
        for row in 0..icon.height {
            let (from, to) = ((row * icon.width * 4) as usize, (((y + row) * width + x) * 4) as usize);
            strip.rgba[to..to + (icon.width * 4) as usize].copy_from_slice(&icon.rgba[from..from + (icon.width * 4) as usize]);
        }
        x += icon.width + GAP;
    }
    let on_disk = keep(&strip, &env.preview_path(doc));
    let bytes = lntrn_image::encode_png(&flatten(&strip, o.background));
    let misses = lantern_misses(document);
    let missing = if misses.is_empty() { String::new() } else { format!(" lntrn-svg doesn't draw what this drawing has of: {}.", misses.join(", ")) };
    let sizes = ICON_SIZES.map(|s| s.to_string()).join(", ");
    let note = format!("Lantern preview of {doc}: as lntrn-svg (what Lantern's apps show icons with) draws it at {sizes} px, left to right, each enlarged pixel for pixel to about {SHOWN} px; {}.{missing} {on_disk}", background_note(o.background));
    Ok((Picture::png(&bytes), note))
}

#[cfg(test)]
mod tests {
    use ink_geom::Vec2;

    use super::*;

    #[test]
    fn transparency_shows_the_checker() {
        let clear = Image::new(16, 1, vec![0; 64]);
        let flat = flatten(&clear, Background::Checker);
        assert_eq!((&flat.rgba[..4], &flat.rgba[32..36]), (&[255, 255, 255, 255][..], &[204, 204, 204, 255][..]));
        let half_red = Image::new(1, 1, vec![255, 0, 0, 128]);
        assert_eq!(flatten(&half_red, Background::Black).rgba, [128, 0, 0, 255]);
        assert_eq!(flatten(&half_red, Background::None).rgba, [255, 0, 0, 128]);
    }

    #[test]
    fn a_view_shows_the_page_or_a_part_of_it() {
        let doc = Document::parse(DocId(1), r#"<svg viewBox="0 0 24 12" width="48" height="24"/>"#).unwrap();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        let whole = view(&viewport, None, 96);
        assert_eq!((whole.width, whole.height), (96, 48));
        // A part, in the drawing's own coordinates: the right half.
        let part = view(&viewport, Some(Rect::from_xywh(12.0, 0.0, 12.0, 12.0)), 96);
        assert_eq!((part.width, part.height), (96, 96));
        let corner = part.page_to_px.apply(viewport.to_page.apply(Vec2::new(12.0, 0.0)));
        assert!(corner.distance(Vec2::ZERO) < 1e-9, "the part's corner is the picture's");
    }
}
