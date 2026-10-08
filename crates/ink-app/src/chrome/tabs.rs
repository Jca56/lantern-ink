//! The document tabs over the canvas, browser-style (LS3's, copied:
//! D12): each tab the bar's height less a 4 px gap on top, rounded
//! above, the showing one amber with a gold label and border; a `•`
//! after a changed drawing's name, a `×` on each, and a `+` after the
//! last.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui};

use crate::chrome::{Look, surface, text_button};
use crate::theme::{self, ACCENT, BORDER, CLOSE, FONT_LG, FONT_MD, PANEL, TAB_ACTIVE, TAB_INACTIVE, TAB_INACTIVE_HOVER, TEXT, TEXT_DIM};

/// One tab as the bar shows it.
pub struct TabLabel {
    pub name: String,
    pub modified: bool,
}

pub enum TabClick {
    Select(usize),
    Close(usize),
    New,
}

pub fn draw(ui: &mut Ui, bar: Rect, tabs: &[TabLabel], active: usize) -> Option<TabClick> {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    surface(ui, bar, PANEL, 2.0);
    let (gap, top, radius, border) = (px(4.0), px(4.0), px(6.0), px(2.0).max(1.0));
    let style = theme::text(ui, FONT_MD);
    let close_w = ui.measure("×", &style) + px(12.0);
    let mut x = bar.min.x + gap;
    let mut click = None;
    ui.draw.push_clip(bar);
    for (i, tab) in tabs.iter().enumerate() {
        let label = if tab.modified { format!("{} \u{2022}", tab.name) } else { tab.name.clone() };
        let label_w = ui.measure(&label, &style);
        let w = px(12.0) + label_w + px(4.0) + close_w + px(12.0);
        let r = Rect::new(Vec2::new(x, bar.min.y + top), Vec2::new(x + w, bar.max.y));
        ui.push_index(i);
        // The close button first: its click is its own, not the tab's.
        let close = Rect::from_min_size(Vec2::new(r.max.x - px(12.0) - close_w, r.min.y + (r.height() - px(30.0)) / 2.0), Vec2::new(close_w, px(30.0)));
        let x_resp = ui.interact(ui.id("close"), close, Sense::CLICK);
        let resp = ui.interact(ui.id("tab"), r, Sense::CLICK);
        ui.pop_id();
        let is_active = i == active;
        let (fill, edge, ink) = match (is_active, resp.hovered) {
            (true, _) => (TAB_ACTIVE, ACCENT, ACCENT),
            (false, true) => (TAB_INACTIVE_HOVER, BORDER, TEXT),
            (false, false) => (TAB_INACTIVE, BORDER, TEXT),
        };
        // Rounded above only: the bottom corners hang below the bar's clip.
        let shape = Rect::new(r.min, Vec2::new(r.max.x, r.max.y + radius));
        ui.draw.rounded_rect(shape, radius, fill);
        ui.draw.stroke_rect(shape, border, radius, edge);
        let text_rect = Rect::new(Vec2::new(r.min.x + px(12.0), r.min.y), Vec2::new(r.min.x + px(12.0) + label_w, r.max.y));
        ui.text_in_rect(&label, &style, text_rect, ink);
        if x_resp.hovered {
            ui.draw.rounded_rect(close, close.height() / 2.0, if x_resp.held { Color::rgb(0.75, 0.05, 0.10) } else { CLOSE });
        }
        ui.text_centered("×", &style, close, if x_resp.hovered { Color::WHITE } else { TEXT_DIM });
        if x_resp.hovered || resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        if x_resp.clicked {
            click = Some(TabClick::Close(i));
        } else if resp.clicked {
            click = Some(TabClick::Select(i));
        }
        x += w + gap;
    }
    let plus_style = theme::text(ui, FONT_LG);
    let plus_w = ui.measure("+", &plus_style) + px(20.0);
    let plus = Rect::new(Vec2::new(x, bar.min.y + top), Vec2::new(x + plus_w, bar.max.y));
    let look = Look { size: FONT_LG, ink: TEXT_DIM, hover_ink: ACCENT, hover: TAB_INACTIVE_HOVER, radius: 6.0 };
    if text_button(ui, "new-tab", plus, "+", &look).clicked {
        click = Some(TabClick::New);
    }
    ui.draw.pop_clip();
    click
}
