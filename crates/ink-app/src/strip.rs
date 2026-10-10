//! The Lantern preview strip (`docs/M4.md`, slice f): the drawing as
//! Lantern's apps will show it as an icon. Drawn by `lntrn-svg`, their
//! renderer and not Ink's, at 16, 24, 32, 48 and 64 px (times the
//! screen's scale, as an app draws an icon), each shown pixel for
//! pixel on the dark an app shows icons on. It stands at the bottom of
//! the right panel, under the object tree; its heading folds it away.
//! What the drawing has that `lntrn-svg` doesn't draw is said under
//! it.
//!
//! Drawn on the job pool from the drawing's text, one strip at a time,
//! and again whenever the drawing looks different (a drag shows as it
//! goes). The pictures are kept behind LUI2's handles, the old ones let
//! go as the new ones come.

use std::sync::mpsc::{Receiver, Sender, channel};

use ink_core::{DocId, Document, Look};
use lntrn_app::Waker;
use lntrn_core::jobs::Pool;
use lntrn_image::Image;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, ImageHandle, Sense, Ui};

use crate::ink::Ink;
use crate::theme::{self, ACCENT, BORDER, FONT_PANEL, FONT_SM, INPUT_BG, TEXT, TEXT_DIM};
use crate::tiles::Store;

/// The sizes an icon is shown at, logical px.
pub const SIZES: [u32; 5] = [16, 24, 32, 48, 64];
/// The heading, the room round the icons and between them, and the
/// line their sizes are said on, logical px.
const HEAD: f64 = 44.0;
const PAD: f64 = 10.0;
const GAP: f64 = 12.0;
const LABEL: f64 = 24.0;

/// What a strip is of: a drawing as it looked, at a screen's scale (in
/// thousandths).
type Of = (DocId, Look, u32);

/// A size's picture as `lntrn-svg` drew it, or none where it couldn't.
type Drawn = Vec<Option<Image>>;

pub struct Strip {
    tx: Sender<(Of, Drawn)>,
    rx: Receiver<(Of, Drawn)>,
    waker: Option<Waker>,
    /// What the pictures that show are of, and the one on its way.
    shown: Option<Of>,
    asked: Option<Of>,
    /// The pictures, by size.
    pics: Vec<Option<ImageHandle>>,
    /// What `lntrn-svg` doesn't draw of the drawing asked about last.
    misses: Vec<&'static str>,
    /// How tall the section was drawn, window px.
    tall: f64,
    /// Where each size's picture was drawn, and the heading.
    #[cfg(test)]
    pub(crate) laid: Vec<(u32, Rect)>,
    #[cfg(test)]
    pub(crate) head: Rect,
}

impl Default for Strip {
    fn default() -> Strip {
        let (tx, rx) = channel();
        Strip { tx, rx, waker: None, shown: None, asked: None, pics: vec![None; SIZES.len()], misses: Vec::new(), tall: 0.0, #[cfg(test)] laid: Vec::new(), #[cfg(test)] head: Rect::default() }
    }
}

/// How many px an icon of `size` logical px is drawn with at `scale`.
pub fn px(size: u32, scale: f64) -> u32 {
    (f64::from(size) * scale).round().max(1.0) as u32
}

/// `svg` at each of `SIZES`, as Lantern's apps draw an icon at `scale`.
pub fn drawn(svg: &str, scale: f64) -> Drawn {
    SIZES.iter().map(|&size| lntrn_svg::render(svg, px(size, scale))).collect()
}

impl Strip {
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// What the pictures that show are of; and what's said the
    /// drawing has that an app won't draw.
    #[cfg(test)]
    pub(crate) fn shown(&self) -> Option<Of> {
        self.shown
    }

    #[cfg(test)]
    pub(crate) fn misses(&self) -> &[&'static str] {
        &self.misses
    }

    /// The strip of `drawing` as it looks (`of`): asked for unless it
    /// shows already or one is on its way.
    fn want(&mut self, of: Of, drawing: &Document, scale: f64) {
        if self.shown == Some(of) || self.asked.is_some() {
            return;
        }
        self.asked = Some(of);
        self.misses = ink_doc::lantern::misses(drawing);
        let (svg, tx, waker) = (drawing.to_svg(), self.tx.clone(), self.waker.clone());
        Pool::global().spawn(move || {
            // Only fails once the window is gone.
            let _ = tx.send((of, drawn(&svg, scale)));
            if let Some(w) = &waker {
                w.wake();
            }
        });
    }

    /// Take in the strips the pool has drawn: their pictures into
    /// `store`, the ones they replace out of it. Whether any came (so
    /// the frame wants building again).
    pub fn finished(&mut self, store: &mut impl Store) -> bool {
        let mut came = false;
        for (of, images) in self.rx.try_iter().collect::<Vec<_>>() {
            if self.asked == Some(of) {
                self.asked = None;
            }
            for (slot, image) in self.pics.iter_mut().zip(images) {
                if let Some(old) = slot.take() {
                    store.remove(old.id);
                }
                *slot = image.map(|image| store.add(&image));
            }
            self.shown = Some(of);
            came = true;
        }
        came
    }

    /// How tall the section stands, window px: what it last drew, or
    /// before it has, what it will without a note.
    pub fn height(&self, scale: f64, folded: bool) -> f64 {
        let px = |v: f64| (v * scale).round();
        if folded {
            px(HEAD) + px(2.0).max(1.0)
        } else if self.tall > 0.0 {
            self.tall
        } else {
            px(HEAD) + px(2.0).max(1.0) + px(PAD) * 2.0 + px(64.0) + px(LABEL) + px(PAD)
        }
    }
}

impl Ink {
    /// The preview strip, in `r` at the bottom of the right panel.
    pub(crate) fn preview_strip(&mut self, ui: &mut Ui, r: Rect) {
        let s = ui.m.scale;
        let px = |v: f64| (v * s).round();
        let rule = px(2.0).max(1.0);
        let pad = px(PAD);
        let inner = Rect::new(Vec2::new(r.min.x + pad, r.min.y + rule), Vec2::new(r.max.x - pad, r.max.y));
        if inner.width() <= 0.0 || r.height() <= 0.0 {
            return;
        }
        ui.push_id("strip");
        ui.draw.push_clip(r);
        // A rule over it, then the heading: a press folds it away.
        ui.draw.rect(Rect::from_min_size(r.min, Vec2::new(r.width(), rule)), BORDER);
        let head = Rect::from_min_size(inner.min, Vec2::new(inner.width(), px(HEAD)));
        let on_head = ui.interact(ui.id("head"), head, Sense::CLICK);
        if on_head.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        let folded = self.settings.strip_folded;
        let (c, a) = (Vec2::new(head.min.x + px(8.0), head.center().y), px(6.0));
        let ink = if on_head.hovered { ACCENT } else { TEXT_DIM };
        if folded {
            ui.draw.triangle(Vec2::new(c.x - a * 0.6, c.y - a), Vec2::new(c.x - a * 0.6, c.y + a), Vec2::new(c.x + a * 0.8, c.y), ink);
        } else {
            ui.draw.triangle(Vec2::new(c.x - a, c.y - a * 0.6), Vec2::new(c.x + a, c.y - a * 0.6), Vec2::new(c.x, c.y + a * 0.8), ink);
        }
        ui.text_in_rect("Preview", &theme::text(ui, FONT_PANEL), Rect::new(Vec2::new(head.min.x + px(22.0), head.min.y), head.max), ink);
        if on_head.clicked {
            self.settings.strip_folded = !folded;
            self.settings.save();
            ui.state.request_rebuild = true;
        }
        #[cfg(test)]
        {
            self.strip.laid.clear();
            self.strip.head = head;
        }
        let shown = self.tabs.active_doc().and_then(|doc| Some((doc, self.core.shown(doc).ok()?)));
        if let (false, Some((doc, (drawing, look)))) = (folded, shown) {
            self.strip.want((doc, look, (s * 1000.0).round() as u32), drawing, s);
            // The icons, their feet on one line, on the dark an app
            // shows them on; each one's size said under it.
            let style = theme::text(ui, FONT_SM);
            let ground = Rect::from_min_size(Vec2::new(inner.min.x, head.max.y), Vec2::new(inner.width(), pad * 2.0 + px(64.0) + px(LABEL)));
            ui.draw.rounded_rect(ground, px(6.0), INPUT_BG);
            let foot = ground.min.y + pad + px(64.0);
            let widths: Vec<f64> = SIZES.iter().map(|&size| f64::from(self::px(size, s)).max(ui.measure(&size.to_string(), &style))).collect();
            let all = widths.iter().sum::<f64>() + px(GAP) * (SIZES.len() - 1) as f64;
            let mut x = (ground.center().x - all / 2.0).round().max(ground.min.x);
            for ((&size, &width), pic) in SIZES.iter().zip(&widths).zip(&self.strip.pics) {
                let side = f64::from(self::px(size, s));
                let at = Rect::from_xywh((x + (width - side) / 2.0).round(), foot - side, side, side);
                if let Some(pic) = pic {
                    ui.draw.image(at, *pic, 0.0, Color::WHITE);
                }
                #[cfg(test)]
                self.strip.laid.push((size, at));
                ui.text_centered(&size.to_string(), &style, Rect::from_xywh(x, foot + px(2.0), width, px(LABEL)), TEXT_DIM);
                x += width + px(GAP);
            }
            let mut bottom = ground.max.y + pad;
            // What an app won't draw of it.
            if !self.strip.misses.is_empty() {
                let note = format!("Lantern's apps won't draw: {}", self.strip.misses.join(", "));
                let said = ui.text_at(&note, &style, Vec2::new(inner.min.x, bottom), inner.width(), TEXT);
                bottom += f64::from(said.height) + pad;
            }
            self.strip.tall = bottom - r.min.y;
        }
        ui.draw.pop_clip();
        ui.pop_id();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_drawn_with_the_pixels_an_app_gives_it() {
        assert_eq!(SIZES.map(|size| px(size, 1.0)), [16, 24, 32, 48, 64]);
        assert_eq!(SIZES.map(|size| px(size, 1.25)), [20, 30, 40, 60, 80]);
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><rect width=\"24\" height=\"12\" fill=\"#ff0000\"/></svg>";
        let all = drawn(svg, 1.25);
        assert_eq!(all.iter().map(|i| i.as_ref().map(|i| (i.width, i.height))).collect::<Vec<_>>(), [20, 30, 40, 60, 80].map(|side| Some((side, side))));
        // The top half red, the bottom clear: `lntrn-svg`'s own picture.
        let small = all[0].as_ref().unwrap();
        assert_eq!((&small.rgba[..4], small.rgba[(19 * 20 * 4) + 3]), (&[255, 0, 0, 255][..], 0));
        // What's no SVG draws nothing, at any size.
        assert!(drawn("not a drawing", 1.0).iter().all(Option::is_none));
    }
}
