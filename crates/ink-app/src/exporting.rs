//! File > Export… (`docs/M4.md`, slice f; LS3's `exporting.rs`): the
//! drawing written out for use elsewhere. As an SVG it's a clean copy
//! to ship (`ink_doc::tidy::shipped`: tidied, without its comments or
//! Ink's own marks, drawing exactly what the drawing draws). As a PNG
//! it's a picture at the page's own size, at any of the icon sizes, or
//! at several at once: then each is written beside the name chosen,
//! with its size on the end (`lamp-16.png`, `lamp-24.png`, …).
//!
//! The dialog says what; Export… then asks where. Nothing holds a
//! frame up: the pictures are drawn, packed and written on the job
//! pool, from the drawing as it was when the place was chosen. The
//! drawing itself, and its own file, aren't touched.

use std::path::{Path, PathBuf};

use ink_core::{DocId, Document, View, write_atomic};
use ink_doc::Viewport;
use lntrn_image::Compression;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{Action, CursorIcon, Dialog, FILL, HostCx, Sense, ShellRequest, Ui};

use crate::controls;
use crate::ink::Ink;
use crate::lifecycle::file_name;
use crate::menus::{DIALOG_EXPORT, EXPORT_GO};
use crate::theme::{self, ACCENT, BORDER, BUTTON, BUTTON_HOVER, FONT_BASE, ON_ACCENT, TEXT};

/// The formats the dialog offers, in its list's order.
pub const FORMATS: [&str; 2] = ["SVG: a clean copy to ship", "PNG: a picture"];
/// The sizes a picture is offered at: its longer side, px.
pub const SIZES: [u32; 8] = [16, 24, 32, 48, 64, 128, 256, 512];

/// What the Export dialog holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exporting {
    /// The drawing it's of, and its page's size.
    doc: Option<DocId>,
    page: (f64, f64),
    /// Which of `FORMATS`.
    pub format: usize,
    /// A picture at the page's own size; and at which of `SIZES`.
    pub own: bool,
    pub sizes: [bool; 8],
}

/// A picture to write: how many px each px of the page is, and where.
pub type Picture = (f64, PathBuf);

/// What an export writes, away from the window's thread.
pub enum ExportJob {
    /// The clean copy's text, and its file.
    Svg(String, PathBuf),
    /// The drawing, and each picture of it.
    Pngs(Box<Document>, Vec<Picture>),
}

/// `path` ending `.ext`, whatever it ended in that wasn't.
fn with_ext(mut path: PathBuf, ext: &str) -> PathBuf {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)) {
        let mut name = path.file_name().unwrap_or_default().to_owned();
        name.push(format!(".{ext}"));
        path.set_file_name(name);
    }
    path
}

/// The pictures to write for a page `page` px, at the place chosen:
/// one at its own size (`None`) or one of `sizes` is the file chosen
/// itself; several are each beside it, its size on the end of its name.
pub fn pictures(chosen: &Path, page: (f64, f64), sizes: &[Option<u32>]) -> Vec<Picture> {
    let chosen = with_ext(chosen.to_owned(), "png");
    let longer = page.0.max(page.1).max(1e-9);
    let scale = |size: &Option<u32>| size.map_or(1.0, |px| f64::from(px) / longer);
    match sizes {
        [one] => vec![(scale(one), chosen)],
        many => {
            let stem = chosen.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            many.iter().map(|size| (scale(size), chosen.with_file_name(format!("{stem}-{}.png", size.map_or(longer.round() as u32, |px| px))))).collect()
        }
    }
}

impl ExportJob {
    /// Write it. What was written, in words for the status bar.
    pub fn run(self) -> Result<String, String> {
        match self {
            ExportJob::Svg(text, path) => {
                write_atomic(&path, text.as_bytes()).map_err(|e| format!("Couldn't export {}: {e}", file_name(&path)))?;
                Ok(format!("Exported {}: a clean copy", file_name(&path)))
            }
            ExportJob::Pngs(doc, pictures) => {
                let viewport = Viewport::of(doc.node(doc.root()).map_err(|e| e.to_string())?);
                let mut written = Vec::new();
                for (scale, path) in &pictures {
                    let name = file_name(path);
                    let image = ink_core::ink_render::render(&doc, &View::page(&viewport, *scale)).map_err(|e| format!("Couldn't export {name}: {e}"))?;
                    write_atomic(path, &lntrn_image::png::encode_with(&image, Compression::Best)).map_err(|e| format!("Couldn't export {name}: {e}"))?;
                    written.push((name, image.width, image.height));
                }
                Ok(match written.as_slice() {
                    [] => "Nothing to export".to_owned(),
                    [(name, w, h)] => format!("Exported {name} ({w} \u{00d7} {h})"),
                    [(first, ..), .., (last, ..)] => format!("Exported {} pictures: {first} \u{2026} {last}", written.len()),
                })
            }
        }
    }
}

impl Exporting {
    /// For `doc`, whose page is `page` px: as the dialog was last left,
    /// or at first a clean SVG, and for a picture the page's own size.
    fn of(&self, doc: DocId, page: (f64, f64)) -> Exporting {
        let fresh = self.doc.is_none();
        Exporting { doc: Some(doc), page, own: self.own || fresh, ..self.clone() }
    }

    /// The sizes ticked: the page's own (`None`) first.
    pub fn chosen(&self) -> Vec<Option<u32>> {
        self.own.then_some(None).into_iter().chain(SIZES.iter().zip(self.sizes).filter(|(_, on)| *on).map(|(size, _)| Some(*size))).collect()
    }

    /// There's something to export as it's set.
    pub fn ready(&self) -> bool {
        self.format == 0 || !self.chosen().is_empty()
    }

    /// The dialog's rows. Whether anything changed.
    pub fn rows(&mut self, ui: &mut Ui) -> bool {
        let before = self.clone();
        ui.labelled("Format", |ui| {
            controls::dropdown(ui, "Format", &mut self.format, &FORMATS, false);
        });
        if self.format == 1 {
            let own = format!("Its own size ({} \u{00d7} {} px)", self.page.0.round(), self.page.1.round());
            controls::toggle(ui, &own, &mut self.own);
            ui.labelled("Sizes", |ui| self.size_grid(ui));
        }
        *self != before
    }

    /// The icon sizes, four a row: each a button that stays down.
    fn size_grid(&mut self, ui: &mut Ui) {
        let s = ui.m.scale;
        let (cell, gap, across) = (Vec2::new((64.0 * s).round(), (40.0 * s).round()), (6.0 * s).round(), 4);
        let rows = SIZES.len().div_ceil(across);
        let r = ui.alloc(Vec2::new(FILL, cell.y * rows as f64 + gap * (rows - 1) as f64));
        let style = theme::text(ui, FONT_BASE);
        for (i, size) in SIZES.iter().enumerate() {
            let at = Rect::from_min_size(Vec2::new(r.min.x + (i % across) as f64 * (cell.x + gap), r.min.y + (i / across) as f64 * (cell.y + gap)), cell);
            let resp = ui.interact(ui.id(&format!("size {size}")), at, Sense::CLICK);
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            if resp.clicked {
                self.sizes[i] = !self.sizes[i];
            }
            let on = self.sizes[i];
            ui.draw.rounded_rect(at, 6.0 * s, if on { ACCENT } else if resp.hovered { BUTTON_HOVER } else { BUTTON });
            ui.draw.stroke_rect(at, (2.0 * s).round().max(1.0), 6.0 * s, if on { ACCENT } else { BORDER });
            ui.text_centered(&size.to_string(), &style, at, if on { ON_ACCENT } else { TEXT });
        }
    }
}

impl Ink {
    /// File > Export…: what to write, asked in a dialog.
    pub(crate) fn ask_export(&mut self, cx: &mut HostCx) {
        let Some(doc) = self.tabs.active_doc() else { return };
        let Ok(viewport) = self.core.viewport(doc) else { return };
        self.exporting = self.exporting.of(doc, (viewport.size.x, viewport.size.y));
        cx.request(ShellRequest::Dialog(Dialog::new("Export", "").content(DIALOG_EXPORT).button("Cancel", None).button("Export\u{2026}", Some(Action::new(EXPORT_GO))).default_button(1)));
    }

    /// The dialog's Export…: where it goes is asked next.
    pub(crate) fn export_go(&mut self) {
        let Some(doc) = self.exporting.doc.filter(|_| self.exporting.ready()) else { return };
        let png = self.exporting.format == 1;
        self.files.pick_export(format!("{}.{}", self.label(doc), if png { "png" } else { "svg" }), png);
    }

    /// The place was chosen: the drawing as it is now is what's written.
    pub(crate) fn export_to(&mut self, chosen: PathBuf) {
        let Some(doc) = self.exporting.doc else { return };
        let Ok(drawing) = self.core.doc(doc) else { return };
        let job = if self.exporting.format == 0 {
            let path = with_ext(chosen, "svg");
            // An open drawing's file isn't a place for a copy: what's
            // open would no longer be what's on disk.
            if let Some(open) = self.core.doc_at(&path) {
                let whose = if open == doc { "this drawing\u{2019}s own file: Save saves the drawing itself".to_owned() } else { format!("open in the tab \u{201c}{}\u{201d}", self.label(open)) };
                return self.toast(format!("{} is {whose}. Give the clean copy another name", file_name(&path)));
            }
            ExportJob::Svg(ink_doc::tidy::shipped(drawing).svg, path)
        } else {
            ExportJob::Pngs(Box::new(drawing.clone()), pictures(&chosen, self.exporting.page, &self.exporting.chosen()))
        };
        self.files.export(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_picture_is_the_file_chosen_and_several_stand_beside_it() {
        let at = |name: &str| PathBuf::from("/tmp/out").join(name);
        // Its own size: a px a px, under the name chosen (a .png).
        assert_eq!(pictures(&at("lamp"), (24.0, 24.0), &[None]), [(1.0, at("lamp.png"))]);
        assert_eq!(pictures(&at("lamp.png"), (24.0, 24.0), &[Some(64)]), [(64.0 / 24.0, at("lamp.png"))]);
        // Several: each with its size on its name. A page that isn't
        // square is measured by its longer side.
        assert_eq!(pictures(&at("lamp.png"), (48.0, 24.0), &[None, Some(16), Some(512)]), [(1.0, at("lamp-48.png")), (16.0 / 48.0, at("lamp-16.png")), (512.0 / 48.0, at("lamp-512.png"))]);
        assert_eq!((with_ext(at("a.SVG"), "svg"), with_ext(at("a.png"), "svg"), with_ext(at("a"), "svg")), (at("a.SVG"), at("a.png.svg"), at("a.svg")));
    }

    #[test]
    fn the_dialog_knows_when_there_is_something_to_export() {
        let first = Exporting::default().of(DocId(1), (24.0, 24.0));
        assert_eq!((first.format, first.own, first.chosen(), first.ready()), (0, true, vec![None], true));
        let mut png = Exporting { format: 1, own: false, ..first };
        assert!(!png.ready(), "a picture at no size");
        png.sizes[1] = true;
        png.sizes[7] = true;
        assert_eq!((png.chosen(), png.ready()), (vec![Some(24), Some(512)], true));
        // Opened again, it's as it was left (its own size still off).
        let again = png.of(DocId(2), (32.0, 32.0));
        assert_eq!((again.own, again.chosen(), again.page), (false, vec![Some(24), Some(512)], (32.0, 32.0)));
    }

    #[test]
    fn a_job_writes_what_it_says() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/ink-app/exporting");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = Document::parse(DocId(1), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 24 12\" ink:guides=\"x4\">\n  <!-- a note -->\n  <rect width=\"24\" height=\"12\" fill=\"#08f\"/>\n</svg>\n").unwrap();
        let clean = ink_doc::tidy::shipped(&doc).svg;
        assert_eq!(ExportJob::Svg(clean, dir.join("clean.svg")).run().unwrap(), "Exported clean.svg: a clean copy");
        assert_eq!(std::fs::read_to_string(dir.join("clean.svg")).unwrap(), "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 12\">\n  <rect width=\"24\" height=\"12\" fill=\"#08f\"/>\n</svg>\n");
        let one = ExportJob::Pngs(Box::new(doc.clone()), pictures(&dir.join("one"), (24.0, 12.0), &[None]));
        assert_eq!(one.run().unwrap(), "Exported one.png (24 \u{00d7} 12)");
        let many = ExportJob::Pngs(Box::new(doc.clone()), pictures(&dir.join("lamp.png"), (24.0, 12.0), &[Some(16), Some(48), Some(64)]));
        assert_eq!(many.run().unwrap(), "Exported 3 pictures: lamp-16.png \u{2026} lamp-64.png");
        let read = |name: &str| lntrn_image::decode(&std::fs::read(dir.join(name)).unwrap()).map(|i| (i.width, i.height)).ok();
        assert_eq!((read("one.png"), read("lamp-16.png"), read("lamp-48.png"), read("lamp-64.png")), (Some((24, 12)), Some((16, 8)), Some((48, 24)), Some((64, 32))));
        // Somewhere that can't be written says so.
        let nowhere = ExportJob::Pngs(Box::new(doc), vec![(1.0, dir.join("no/such/folder/x.png"))]);
        assert!(nowhere.run().unwrap_err().starts_with("Couldn't export x.png: "));
    }
}
