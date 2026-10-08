//! The Box (ARCHITECTURE §8; LS3's `toolbox.rs`): a gold square at the
//! top right of the canvas that opens a panel holding every setting of
//! the tool in hand, and of what's selected. It floats over the canvas
//! (`Ui::child`), so nothing under it takes a press on it. It starts
//! closed, and stays as it was left: what it holds is worked on the
//! canvas with it open.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui};

use crate::theme::{self, ACCENT, BUTTON_HOVER, FONT_2XL, PANEL};

/// The layer it floats on, over the canvas and the chrome.
const LAYER: usize = 1;

/// What the Box keeps between frames.
#[derive(Default)]
pub struct ToolBox {
    pub open: bool,
    /// Where it was drawn (the toggle, and the panel when open).
    rect: Option<Rect>,
    /// How tall its rows came out, window px.
    rows: f64,
    /// Where what it holds was last drawn, by name.
    #[cfg(test)]
    pub(crate) laid: Vec<(&'static str, Rect)>,
}

impl ToolBox {
    /// Where it is: the square, and the panel when it's open.
    #[cfg(test)]
    pub(crate) fn rect(&self) -> Option<Rect> {
        self.rect
    }

    /// There's no Box this frame (no drawing shows).
    pub fn gone(&mut self) {
        self.rect = None;
    }
}

/// The Box, titled `title`, holding whatever `body` lays out.
pub fn draw_with(ui: &mut Ui, canvas: Rect, st: &mut ToolBox, title_text: &str, body: impl FnOnce(&mut Ui)) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let (margin, side, gap, pad) = (px(16.0), px(64.0), px(10.0), px(22.0));
    let toggle = Rect::from_xywh(canvas.max.x - margin - side, canvas.min.y + margin, side, side);
    let title = theme::text(ui, FONT_2XL);
    let head = (title.line_height() as f64).ceil() + px(14.0);
    let room = (canvas.height() - margin * 2.0).max(side);
    let panel = st.open.then(|| {
        let w = px(460.0).min(toggle.min.x - gap - canvas.min.x - margin).max(px(260.0));
        let h = (pad * 2.0 + head + st.rows).min(room);
        Rect::from_xywh(toggle.min.x - gap - w, toggle.min.y, w, h)
    });
    let bounds = panel.map_or(toggle, |p| p.union(&toggle));
    st.rect = Some(bounds);
    let mut flip = false;
    let mut rows = st.rows;
    ui.push_id("toolbox");
    ui.child(bounds, LAYER, |ui| {
        let resp = ui.interact(ui.id("toggle"), toggle, Sense::CLICK);
        let ground = theme::sheen(if resp.hovered { BUTTON_HOVER } else { PANEL });
        ui.draw.rounded_rect_gradient(toggle, px(8.0), ground.top, ground.bottom);
        ui.draw.stroke_rect(toggle, px(2.5).max(1.0), px(8.0), ACCENT);
        // The arrow points the way the panel goes: out to the left to
        // open, back to the right to close.
        let (c, a) = (toggle.center(), px(11.0));
        let tip = if st.open { a } else { -a };
        ui.draw.triangle(Vec2::new(c.x - tip * 0.6, c.y - a), Vec2::new(c.x - tip * 0.6, c.y + a), Vec2::new(c.x + tip * 0.9, c.y), ACCENT);
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        flip = resp.clicked;
        let Some(panel) = panel else { return };
        ui.draw.shadow(panel.translate(Vec2::new(-px(3.0), px(4.0))), px(10.0), px(10.0), Color::rgba(0.0, 0.0, 0.0, 0.45));
        let ground = theme::sheen(PANEL);
        ui.draw.rounded_rect_gradient(panel, px(10.0), ground.top, ground.bottom);
        ui.draw.stroke_rect(panel, px(2.0).max(1.0), px(10.0), ACCENT);
        let inner = panel.shrink(pad);
        ui.draw.push_clip(inner);
        ui.text_in_rect(title_text, &title, Rect::new(inner.min, Vec2::new(inner.max.x, inner.min.y + head - px(14.0))), ACCENT);
        ui.draw.pop_clip();
        let area = Rect::new(Vec2::new(inner.min.x, inner.min.y + head), inner.max);
        ui.child(area, LAYER, |ui| {
            let lay = |ui: &mut Ui| {
                let top = ui.cursor().y;
                body(ui);
                rows = ui.cursor().y - top;
            };
            // In a window too short for them all, the rows scroll.
            if pad * 2.0 + head + st.rows > room {
                ui.scroll_area("rows", Some(area.height()), lay);
            } else {
                lay(ui);
            }
        });
    });
    ui.pop_id();
    if flip {
        st.open = !st.open;
    }
    if flip || (rows - st.rows).abs() > 0.5 {
        st.rows = rows;
        ui.state.request_rebuild = true;
    }
}
