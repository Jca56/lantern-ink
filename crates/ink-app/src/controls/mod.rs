//! Ink's own controls (LS3's `controls/`, copied as D12 says: the look
//! is LS3's): the number field you drag along or type into, the
//! toggle, the slider and the button. More of LS3's come with the
//! slices that need them (the dropdown). Only the text editing
//! itself is LUI2's (`Ui::text_edit_core`): carets, selections and the
//! clipboard are one thing everywhere.
//!
//! The look: surfaces with the sheen, lines 2 px wide, LS3's golds,
//! text at LUI2's size (never under 18 px).

mod button;
mod field;
mod slider;
mod toggle;

pub use button::button_if;
pub use field::number_in;
pub use slider::Slider;
pub use toggle::toggle;

use lntrn_math::{Color, Rect};
use lntrn_ui::Ui;

use crate::theme::{INPUT_BG, LAYER_ROW_BORDER};

/// The line round a control at rest.
pub(crate) const EDGE: Color = LAYER_ROW_BORDER;
/// The line round what's gold (a toggle that's on).
pub(crate) const GOLD_EDGE: Color = Color::rgb(0.4, 0.31, 0.0);
/// A line's width and a control's corners, logical px.
const LINE: f64 = 2.0;
const ROUND: f64 = 5.0;

/// `v` logical px in this frame's pixels, one at least.
pub(crate) fn px(ui: &Ui, v: f64) -> f64 {
    (v * ui.m.scale).round().max(1.0)
}

pub(crate) fn round(ui: &Ui) -> f64 {
    px(ui, ROUND)
}

/// The line round a control.
pub(crate) fn frame(ui: &mut Ui, r: Rect, color: Color) {
    let (w, radius) = (px(ui, LINE), round(ui));
    ui.draw.stroke_rect(r, w, radius, color);
}

/// A raised face (a button): `base` with the sheen, in a line.
pub(crate) fn face(ui: &mut Ui, r: Rect, base: Color, edge: Color) {
    let (g, radius) = (crate::theme::sheen(base), round(ui));
    ui.draw.rounded_rect_gradient(r, radius, g.top, g.bottom);
    frame(ui, r, edge);
}

/// A well (a value's box): the dark of a place to type, in a line.
pub(crate) fn well(ui: &mut Ui, r: Rect, edge: Color) {
    let radius = round(ui);
    ui.draw.rounded_rect(r, radius, INPUT_BG);
    frame(ui, r, edge);
}

/// A right press on `r` this frame that nothing floating over it took:
/// what puts a slider back to rest.
pub(crate) fn right_pressed(ui: &Ui, r: Rect) -> bool {
    let st = &ui.state;
    st.right_pressed && r.intersection(&ui.clip()).contains(st.right_press_pos) && !st.shielded(ui.layer(), st.right_press_pos)
}

/// A number as a person writes it: up to `decimals` places, no trailing
/// zeros.
pub(crate) fn written(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if !s.contains('.') {
        return s;
    }
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t == "-0" { "0".to_owned() } else { t.to_owned() }
}

/// `v` within `min..=max`, on a multiple of `step` from `min` (0: any).
pub(crate) fn snapped(v: f64, min: f64, max: f64, step: f64) -> f64 {
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    if !v.is_finite() {
        return if min.is_finite() { min } else { 0.0 };
    }
    let base = if min.is_finite() { min } else { 0.0 };
    let v = if step > 0.0 && step.is_finite() { base + ((v - base) / step).round() * step } else { v };
    v.max(min).min(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_written_as_a_person_would() {
        assert_eq!((written(12.0, 3), written(0.5, 3), written(1.23456, 3), written(-0.0004, 3), written(7.10, 2)), ("12".to_owned(), "0.5".to_owned(), "1.235".to_owned(), "0".to_owned(), "7.1".to_owned()));
        assert_eq!((snapped(7.4, 0.0, 10.0, 1.0), snapped(12.0, 0.0, 10.0, 0.0), snapped(f64::NAN, 2.0, 10.0, 0.0), snapped(-3.2, f64::NEG_INFINITY, f64::INFINITY, 0.0)), (7.0, 10.0, 2.0, -3.2));
    }
}
