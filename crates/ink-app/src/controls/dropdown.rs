//! The dropdown (LS3's `controls/dropdown.rs`, copied as D12 says): a
//! face that says what's chosen, and a list of Ink's own that opens
//! under it (over it, with no room below), inside the window, scrolling
//! by the wheel when it's longer than the window.
//!
//! Ink's own beside LS3's: `faces`, for a list of fonts, each row
//! lettered in the family it names.

use std::hash::{DefaultHasher, Hash, Hasher};

use lntrn_math::{Rect, Vec2};
use lntrn_text::{Family, TextStyle};
use lntrn_ui::{CursorIcon, FILL, Key, KeyStep, Sense, Ui, WidgetId};

use super::{EDGE, face, frame, px, round, window};
use crate::theme::{self, ACCENT, BUTTON, BUTTON_HOVER, DROPDOWN_HOVER, PANEL, TEXT};

/// What an open list came to this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub picked: Option<usize>,
    /// Escape, or a press outside it and its anchor.
    pub closed: bool,
    /// The row the pointer is on.
    pub hovered: Option<usize>,
}

/// The widest of `items` in the body text, kept under `id` until they or
/// the text size change: hundreds of fonts aren't measured every frame.
fn widest(ui: &mut Ui, id: WidgetId, items: &[&str]) -> f64 {
    let style = ui.text_style();
    let mut h = DefaultHasher::new();
    items.hash(&mut h);
    style.size.to_bits().hash(&mut h);
    let key = (h.finish() >> 11) as f64;
    let slot = id.with("widest");
    let [seen, width, ..] = *ui.state.floats(slot, [f64::NAN; 4]);
    if seen == key {
        return width;
    }
    let width = items.iter().map(|o| ui.measure(o, &style)).fold(0.0, f64::max);
    *ui.state.floats(slot, [f64::NAN; 4]) = [key, width, 0.0, 0.0];
    width
}

/// The mark that says a face opens: gold, pointing down.
fn chevron(ui: &mut Ui, rect: Rect) {
    let s = px(ui, 6.0);
    let c = Vec2::new(rect.max.x - ui.m.pad - s, rect.center().y - s * 0.25);
    let w = px(ui, 2.0);
    ui.draw.line(Vec2::new(c.x - s, c.y - s * 0.5), Vec2::new(c.x, c.y + s * 0.5), w, ACCENT);
    ui.draw.line(Vec2::new(c.x, c.y + s * 0.5), Vec2::new(c.x + s, c.y - s * 0.5), w, ACCENT);
}

/// The face of a list that's closed (or open, its line gold), with the
/// mark that says it opens: for a caller that hangs a [`list`] of its
/// own.
pub fn closed_list(ui: &mut Ui, rect: Rect, hovered: bool, open: bool) {
    face(ui, rect, if hovered || open { BUTTON_HOVER } else { BUTTON }, if open || hovered { ACCENT } else { EDGE });
    chevron(ui, rect);
}

/// The style a row saying `item` is lettered in: the body text's, or
/// (`faces`) that in the family the row names.
fn lettered(style: &TextStyle, item: &str, faces: bool) -> TextStyle {
    if faces { TextStyle { family: Family::Named(item.to_owned()), ..style.clone() } } else { style.clone() }
}

/// Pick one of `options`. `faces`: they're fonts, each shown in its
/// own. Whether the choice changed.
pub fn dropdown(ui: &mut Ui, label: &str, selected: &mut usize, options: &[&str], faces: bool) -> bool {
    let id = ui.id(label);
    let w = if ui.in_row() { widest(ui, id, options) + ui.m.pad * 3.0 + px(ui, 15.0) } else { FILL };
    let rect = ui.alloc(Vec2::new(w, ui.m.widget_h));
    let mut r = ui.interact(id, rect, Sense::CLICK);
    let focused = ui.focusable(id, rect);
    ui.key_click(id, &mut r);
    if r.hovered {
        ui.state.cursor_icon = CursorIcon::Pointer;
    }
    let mut stepped = false;
    if focused && !options.is_empty() {
        // Down the list is the next row (a "step up" is a row up).
        let next = match ui.key_step(id) {
            KeyStep::By(n) => (*selected as i64 - i64::from(n)).clamp(0, options.len() as i64 - 1) as usize,
            KeyStep::Min => 0,
            KeyStep::Max => options.len() - 1,
            KeyStep::None => *selected,
        };
        stepped = next != *selected;
        *selected = next;
    }
    let open = *ui.state.open(id);
    closed_list(ui, rect, r.hovered, open);
    let style = ui.text_style();
    let inner = Rect::new(Vec2::new(rect.min.x + ui.m.pad, rect.min.y), Vec2::new(rect.max.x - ui.m.pad - px(ui, 18.0), rect.max.y));
    let chosen = options.get(*selected).copied().unwrap_or("");
    ui.text_in_rect(chosen, &lettered(&style, chosen, faces), inner, TEXT);
    ui.focus_ring(id, rect);
    if r.clicked {
        *ui.state.open(id) = !open;
        ui.state.request_rebuild = true;
    }
    if *ui.state.open(id) {
        let res = list(ui, id, rect, options, Some(*selected).filter(|i| *i < options.len()), faces);
        if let Some(i) = res.picked {
            let changed = *selected != i;
            *selected = i;
            *ui.state.open(id) = false;
            return changed;
        }
        if res.closed {
            *ui.state.open(id) = false;
        }
    }
    stepped
}

/// An open list of `items` hung off `anchor`, one layer up, the row
/// `selected` in gold. Its rows are `id`'s `item`s by index.
pub fn list(ui: &mut Ui, id: WidgetId, anchor: Rect, items: &[&str], selected: Option<usize>, faces: bool) -> Listed {
    let style = ui.text_style();
    let (item_h, pad, gap) = (ui.m.widget_h, ui.m.pad, ui.m.gap);
    let line = px(ui, 2.0);
    let w = (widest(ui, id, items) + pad * 3.0).max(anchor.width());
    let window = window(ui);
    let h_full = item_h * items.len() as f64 + line * 2.0;
    // Longer than the window: it scrolls, and never runs off it.
    let h = h_full.min((window.height() - gap * 2.0).max(item_h));
    let below = anchor.max.y + gap;
    let y = if below + h <= window.max.y { below } else { (anchor.min.y - gap - h).max(window.min.y) };
    let x = anchor.min.x.min(window.max.x - w).max(window.min.x);
    let rect = Rect::from_min_size(Vec2::new(x, y), Vec2::new(w, h));

    let mut out = Listed::default();
    if ui.state.take_key(|k| k.key == Key::Escape).is_some() {
        out.closed = true;
    }
    if ui.state.pressed && !rect.contains(ui.state.press_pos) && !anchor.contains(ui.state.press_pos) {
        out.closed = true;
        ui.state.request_rebuild = true;
    }
    let max_scroll = (h_full - h).max(0.0);
    let mut scroll = ui.state.scroll(id).offset.y.clamp(0.0, max_scroll);
    if max_scroll > 0.0 && ui.state.pointer_in_window && rect.contains(ui.state.pointer) && ui.state.wheel.y != 0.0 {
        scroll = (scroll - ui.state.wheel.y).clamp(0.0, max_scroll);
        ui.state.wheel = Vec2::ZERO;
    }
    ui.state.scroll(id).offset.y = scroll;

    let layer = ui.layer() + 1;
    let radius = round(ui);
    ui.child(rect, layer, |ui| {
        let g = theme::sheen(PANEL);
        ui.draw.rounded_rect_gradient(rect, radius, g.top, g.bottom);
        let mut y = rect.min.y + line - scroll;
        for (i, item) in items.iter().enumerate() {
            // Only the rows in view are there at all.
            if y + item_h < rect.min.y || y > rect.max.y {
                y += item_h;
                continue;
            }
            let row = Rect::from_min_size(Vec2::new(rect.min.x + line, y), Vec2::new(w - line * 2.0, item_h));
            let r = ui.interact(id.with("item").with_index(i), row, Sense::CLICK);
            if r.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
                out.hovered = Some(i);
            }
            if r.hovered || r.held {
                ui.draw.rect(row, DROPDOWN_HOVER);
            }
            let inner = Rect::new(Vec2::new(row.min.x + pad, row.min.y), Vec2::new(row.max.x - pad, row.max.y));
            ui.text_in_rect(item, &lettered(&style, item, faces), inner, if selected == Some(i) { ACCENT } else { TEXT });
            if r.clicked {
                out.picked = Some(i);
                ui.state.request_rebuild = true;
            }
            y += item_h;
        }
        frame(ui, rect, if selected.is_some() { ACCENT } else { EDGE });
    });
    out
}
