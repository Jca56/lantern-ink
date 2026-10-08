//! Ink's colour picker (LS3's `colour/picker.rs`, copied as D12 says):
//! a saturation and value square, a hue bar down its side, an alpha
//! bar under it, then the colour as hex and as red, green and blue.
//! One for the whole window: a fill, a stroke and (in M4e) a
//! gradient's stops all open the same one, each for its own colour,
//! alpha and all.
//!
//! It floats over everything under it. Escape, its ✕, or a press
//! anywhere else closes it.

use crate::controls;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Key, Sense, Ui, WidgetId};

use crate::icons::Icons;
use crate::theme::{self, ACCENT, FONT_MD, INPUT_BG, LAYER_ROW_BORDER, PANEL, TEXT};

/// The square's side, the hue bar's width and the alpha bar's height,
/// logical px (LS3's).
const SQUARE: f64 = 300.0;
const HUE_W: f64 = 28.0;
const ALPHA_H: f64 = 28.0;
const PAD: f64 = 14.0;
const GAP: f64 = 12.0;
const TITLE_H: f64 = 34.0;
/// How many strips the square is drawn in each way: each is exact at
/// its corners, so the colour under the marker is the colour picked.
const STRIPS: usize = 16;

/// The one picker's state.
#[derive(Default)]
pub struct Picker {
    /// The swatch it's open for.
    owner: Option<WidgetId>,
    /// The hue, saturation and value shown: they outlive a colour that
    /// can't say them (a grey has no hue, black no saturation).
    hsv: [f64; 3],
    /// Where it was drawn.
    rect: Option<Rect>,
    /// The window it may be anywhere in, this frame.
    window: Rect,
    /// Its swatch was drawn this frame.
    seen: bool,
}

impl Picker {
    /// A frame begins: it may go anywhere in `window`. A picker whose
    /// swatch wasn't drawn last frame (its drawer closed, its layer
    /// went) is closed.
    pub fn begin(&mut self, window: Rect) {
        if self.owner.is_some() && !std::mem::take(&mut self.seen) {
            self.close();
        }
        self.window = window;
    }

    pub fn is_open_for(&self, id: WidgetId) -> bool {
        self.owner == Some(id)
    }

    /// Where it's drawn, while it's open.
    #[cfg(test)]
    pub(crate) fn rect(&self) -> Option<Rect> {
        self.rect.filter(|_| self.owner.is_some())
    }

    pub fn close(&mut self) {
        self.owner = None;
        self.rect = None;
    }

    /// A press on `id`'s swatch: open for it, or closed if it was.
    pub fn toggle(&mut self, id: WidgetId) {
        if self.is_open_for(id) {
            self.close();
        } else {
            self.owner = Some(id);
            self.hsv = [-1.0; 3];
        }
    }

    /// The picker for `color`, if it's open for `id`, by its swatch at
    /// `anchor`. Returns whether the colour changed this frame.
    pub fn popup(&mut self, ui: &mut Ui, id: WidgetId, anchor: Rect, title: &str, color: &mut Color, icons: &Icons) -> bool {
        if !self.is_open_for(id) {
            return false;
        }
        self.seen = true;
        let s = ui.m.scale;
        let px = |v: f64| (v * s).round();
        let (pad, gap) = (px(PAD), px(GAP));
        let w = pad * 2.0 + px(SQUARE) + gap + px(HUE_W);
        let h = pad * 2.0 + px(TITLE_H) + px(SQUARE) + gap + px(ALPHA_H) + gap + ui.m.widget_h;
        let window = if self.window.is_empty() { ui.clip() } else { self.window };
        let below = anchor.max.y + px(6.0);
        let y = if below + h <= window.max.y { below } else { (anchor.min.y - px(6.0) - h).max(window.min.y) };
        let x = anchor.min.x.min(window.max.x - w).max(window.min.x);
        let rect = Rect::from_xywh(x, y, w, h);
        // Its ways out, before anything under it can take the press.
        let escape = ui.state.take_key(|k| k.key == Key::Escape).is_some();
        let outside = ui.state.pressed && !rect.contains(ui.state.press_pos) && !anchor.contains(ui.state.press_pos);
        if escape || outside {
            self.close();
            ui.state.request_rebuild = true;
            return false;
        }
        self.rect = Some(rect);

        // The hue and saturation kept, while they still make this colour.
        let now = color.to_hsv();
        let kept = self.hsv[0] >= 0.0 && Color::from_hsv(self.hsv[0], self.hsv[1], self.hsv[2]).approx_eq(Color::rgb(color.r, color.g, color.b), 1e-6);
        let [mut hue, mut sat, mut val] = if kept { self.hsv } else { now };
        let mut alpha = color.a;
        let mut changed = false;
        let mut close = false;
        let layer = ui.layer() + 1;
        ui.push_id("picker");
        ui.child(rect, layer, |ui| {
            let ground = theme::sheen(PANEL);
            ui.draw.shadow(rect.translate(Vec2::new(0.0, px(4.0))), px(8.0), px(12.0), Color::rgba(0.0, 0.0, 0.0, 0.5));
            ui.draw.rounded_rect_gradient(rect, px(8.0), ground.top, ground.bottom);
            ui.draw.stroke_rect(rect, px(2.0).max(1.0), px(8.0), ACCENT);
            let inner = rect.shrink(pad);

            // The title, and the way out.
            let head = Rect::from_min_size(inner.min, Vec2::new(inner.width(), px(TITLE_H) - px(6.0)));
            ui.text_in_rect(title, &theme::text(ui, FONT_MD).bold(), head, ACCENT);
            let x_rect = Rect::from_xywh(head.max.x - head.height(), head.min.y, head.height(), head.height());
            let out = ui.interact(ui.id("close"), x_rect, Sense::CLICK);
            if out.hovered {
                ui.draw.rounded_rect(x_rect, px(4.0), Color::rgba(1.0, 1.0, 1.0, 0.12));
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            let (c, a, w) = (x_rect.center(), px(7.0), px(2.5).max(1.0));
            ui.draw.line(c - Vec2::splat(a), c + Vec2::splat(a), w, TEXT);
            ui.draw.line(c + Vec2::new(-a, a), c + Vec2::new(a, -a), w, TEXT);
            close = out.clicked;

            // Saturation across, value up.
            let square = Rect::from_xywh(inner.min.x, inner.min.y + px(TITLE_H), px(SQUARE), px(SQUARE));
            let r = ui.interact(ui.id("sv"), square, Sense::DRAG);
            if r.hovered || r.held {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            if r.pressed || r.dragging {
                let p = square.clamp_point(ui.state.pointer);
                sat = (p.x - square.min.x) / square.width();
                val = 1.0 - (p.y - square.min.y) / square.height();
                changed = true;
            }
            let at = |i: usize, j: usize| Color::from_hsv(hue, i as f64 / STRIPS as f64, 1.0 - j as f64 / STRIPS as f64);
            for j in 0..STRIPS {
                for i in 0..STRIPS {
                    let cell = Rect::new(
                        Vec2::new(square.min.x + square.width() * i as f64 / STRIPS as f64, square.min.y + square.height() * j as f64 / STRIPS as f64),
                        Vec2::new(square.min.x + square.width() * (i + 1) as f64 / STRIPS as f64, square.min.y + square.height() * (j + 1) as f64 / STRIPS as f64),
                    );
                    ui.draw.rect_gradient4(cell, at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1));
                }
            }
            ui.draw.stroke_rect(square, px(2.0).max(1.0), 0.0, LAYER_ROW_BORDER);
            let marker = Vec2::new(square.min.x + sat * square.width(), square.min.y + (1.0 - val) * square.height());
            ui.draw.push_clip(square.expand(px(10.0)));
            ui.draw.ring(marker, px(9.0), px(2.5), Color::BLACK);
            ui.draw.ring(marker, px(6.5), px(2.5), Color::WHITE);
            ui.draw.pop_clip();

            // Hue, down the side.
            let bar = Rect::from_xywh(square.max.x + gap, square.min.y, px(HUE_W), square.height());
            let r = ui.interact(ui.id("hue"), bar, Sense::DRAG);
            if r.hovered || r.held {
                ui.state.cursor_icon = CursorIcon::NsResize;
            }
            if r.pressed || r.dragging {
                hue = ((ui.state.pointer.y - bar.min.y) / bar.height()).clamp(0.0, 0.9999);
                changed = true;
            }
            for i in 0..6 {
                let seg = Rect::new(Vec2::new(bar.min.x, bar.min.y + bar.height() * i as f64 / 6.0), Vec2::new(bar.max.x, bar.min.y + bar.height() * (i + 1) as f64 / 6.0));
                ui.draw.rect_gradient(seg, Color::from_hsv(i as f64 / 6.0, 1.0, 1.0), Color::from_hsv((i + 1) as f64 / 6.0, 1.0, 1.0));
            }
            ui.draw.stroke_rect(bar, px(2.0).max(1.0), 0.0, LAYER_ROW_BORDER);
            let y = bar.min.y + bar.height() * hue;
            ui.draw.rect(Rect::from_xywh(bar.min.x - px(3.0), y - px(3.0), bar.width() + px(6.0), px(6.0)), Color::BLACK);
            ui.draw.rect(Rect::from_xywh(bar.min.x - px(2.0), y - px(2.0), bar.width() + px(4.0), px(4.0)), Color::WHITE);

            // Alpha, under them: clear at the left, solid at the right.
            let solid = Color::from_hsv(hue, sat, val);
            let strip = Rect::from_xywh(inner.min.x, square.max.y + gap, inner.width(), px(ALPHA_H));
            let r = ui.interact(ui.id("alpha"), strip, Sense::DRAG);
            if r.hovered || r.held {
                ui.state.cursor_icon = CursorIcon::EwResize;
            }
            if r.pressed || r.dragging {
                alpha = ((ui.state.pointer.x - strip.min.x) / strip.width()).clamp(0.0, 1.0);
                changed = true;
            }
            if let Some(checks) = icons.checker() {
                let uv = Rect::from_xywh(0.0, 0.0, (strip.width() / checks.width as f64).min(1.0), (strip.height() / checks.height as f64).min(1.0));
                ui.draw.image_uv(strip, checks, uv, 0.0, Color::WHITE);
            } else {
                ui.draw.rect(strip, Color::rgb(0.7, 0.7, 0.7));
            }
            ui.draw.rect_gradient_h(strip, solid.with_alpha(0.0), solid);
            ui.draw.stroke_rect(strip, px(2.0).max(1.0), 0.0, LAYER_ROW_BORDER);
            let x = strip.min.x + strip.width() * alpha;
            ui.draw.rect(Rect::from_xywh(x - px(3.0), strip.min.y - px(3.0), px(6.0), strip.height() + px(6.0)), Color::BLACK);
            ui.draw.rect(Rect::from_xywh(x - px(2.0), strip.min.y - px(2.0), px(4.0), strip.height() + px(4.0)), Color::WHITE);

            // The colour in letters and numbers: hex, then R, G and B.
            let row = Rect::from_xywh(inner.min.x, strip.max.y + gap, inner.width(), ui.m.widget_h);
            let small = px(6.0);
            let number_w = ((row.width() * 0.6 - small * 3.0) / 3.0).floor();
            let hex_r = Rect::from_min_size(row.min, Vec2::new(row.width() - (number_w + small) * 3.0, row.height()));
            let hex_id = ui.id("hex");
            let shown = solid.with_alpha(1.0).to_hex_string();
            let editing = ui.state.has_focus(hex_id);
            let mut text = if editing { ui.state.text_edit(hex_id).buffer.take().unwrap_or_else(|| shown.clone()) } else { shown };
            ui.draw.rect(hex_r, INPUT_BG);
            let tr = ui.text_edit_core(hex_id, hex_r, &mut text);
            if tr.focused {
                if tr.changed
                    && let Some(c) = Color::parse_hex(&text)
                {
                    [hue, sat, val] = c.to_hsv();
                    changed = true;
                }
                ui.state.text_edit(hex_id).buffer = Some(text);
            }
            if tr.committed || tr.cancelled {
                ui.state.focus = None;
                ui.state.request_rebuild = true;
            }
            let mut rgb = [solid.r, solid.g, solid.b].map(|v| (v * 255.0).round());
            let mut typed = false;
            for (i, name) in ["R", "G", "B"].into_iter().enumerate() {
                let field = Rect::from_xywh(hex_r.max.x + small + (number_w + small) * i as f64, row.min.y, number_w, row.height());
                typed |= controls::number_in(ui, ui.id(name), field, name, &mut rgb[i], 1.0, Some((0.0, 255.0)), 0);
            }
            if typed {
                [hue, sat, val] = Color::rgb(rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0).to_hsv();
                changed = true;
            }
        });
        ui.pop_id();
        self.hsv = [hue, sat, val];
        if close {
            self.close();
            ui.state.request_rebuild = true;
        }
        let picked = Color::from_hsv(hue, sat, val).with_alpha(alpha);
        if changed && !picked.approx_eq(*color, 1e-12) {
            *color = picked;
            return true;
        }
        false
    }
}

/// A colour in a well: over checks, so a see-through one shows as one.
pub fn swatch_face(ui: &mut Ui, r: Rect, color: Color, lit: bool, icons: &Icons) {
    let w = (2.0 * ui.m.scale).round().max(1.0);
    if color.a < 1.0
        && let Some(checks) = icons.checker()
    {
        let uv = Rect::from_xywh(0.0, 0.0, (r.width() / checks.width as f64).min(1.0), (r.height() / checks.height as f64).min(1.0));
        ui.draw.image_uv(r, checks, uv, 0.0, Color::WHITE);
    }
    ui.draw.rect(r, color);
    ui.draw.stroke_rect(r, w, 0.0, if lit { ACCENT } else { LAYER_ROW_BORDER });
}
