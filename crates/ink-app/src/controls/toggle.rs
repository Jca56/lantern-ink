//! The toggle (LS3's): a switch that slides, gold when it's on, with
//! its name beside it. The whole row is the target.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, FILL, Sense, Ui};

use super::{EDGE, GOLD_EDGE, px};
use crate::theme::{ACCENT, INPUT_BG, ON_ACCENT, SLIDER_FILL, TEXT, TEXT_DIM};

/// The switch, logical px.
const WIDE: f64 = 54.0;
const HIGH: f64 = 28.0;

/// A switch with `label`. Whether it was flipped.
pub fn toggle(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
    let id = ui.id(label);
    let style = ui.text_style();
    let (wide, high) = (px(ui, WIDE), px(ui, HIGH));
    let gap = ui.m.gap * 2.0;
    let w = if ui.in_row() { wide + gap + ui.measure(label, &style) } else { FILL };
    let rect = ui.alloc(Vec2::new(w, ui.m.widget_h));
    let mut r = ui.interact(id, rect, Sense::CLICK);
    ui.focusable(id, rect);
    ui.key_click(id, &mut r);
    if r.clicked {
        *value = !*value;
    }
    if r.hovered {
        ui.state.cursor_icon = CursorIcon::Pointer;
    }
    let pill = Rect::from_min_size(Vec2::new(rect.min.x, (rect.center().y - high / 2.0).round()), Vec2::new(wide, high));
    // Where the knob is on its way to.
    let along = ui.animate(id.with("slide"), if *value { 1.0 } else { 0.0 }, 0.12);
    let radius = high / 2.0;
    if along > 0.5 {
        ui.draw.rounded_rect_gradient(pill, radius, SLIDER_FILL.top, SLIDER_FILL.bottom);
    } else {
        ui.draw.rounded_rect(pill, radius, INPUT_BG);
    }
    let line = px(ui, 2.0);
    ui.draw.stroke_rect(pill, line, radius, if along > 0.5 { GOLD_EDGE } else if r.hovered { ACCENT } else { EDGE });
    let travel = wide - high;
    let knob = Vec2::new(pill.min.x + radius + travel * along, pill.center().y);
    ui.draw.circle(knob, radius - px(ui, 5.0), if along > 0.5 { ON_ACCENT } else { TEXT_DIM });
    ui.text_in_rect(label, &style, Rect::new(Vec2::new(pill.max.x + gap, rect.min.y), rect.max), TEXT);
    ui.focus_ring(id, pill);
    r.clicked
}
