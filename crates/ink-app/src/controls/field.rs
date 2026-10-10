//! Places to type (LS3's `controls/field.rs`): the text field, and the
//! number field: drag along it to change its number, press and let go
//! to type one. The editing itself (caret, selection, clipboard, input
//! methods) is LUI2's one text core; the frame round it is Ink's.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, FILL, Sense, TextOpts, TextResponse, Ui, WidgetId};

use super::{EDGE, frame, px, snapped, well, written};
use crate::theme::{ACCENT, CLOSE, TEXT, TEXT_DIM};

/// A line of text to edit, checked by `validate`, which says what's
/// wrong with the text (`None`: nothing). While it fails the frame is
/// red, the field says why, and Enter enters nothing. Enter is
/// `committed`, Escape `cancelled`.
pub fn field_validated(ui: &mut Ui, label: &str, value: &mut String, validate: &dyn Fn(&str) -> Option<String>) -> TextResponse {
    let id = ui.id(label);
    let rect = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    let out = ui.text_edit_core_with(id, rect, value, TextOpts { validate: Some(validate), ..TextOpts::default() });
    frame(ui, rect, if out.invalid { CLOSE } else if out.focused { ACCENT } else { EDGE });
    out
}

/// How long a half-typed value keeps while its field isn't drawn: past
/// that it's forgotten, not entered.
const KEPT: f64 = 0.5;

/// Whether a value is being typed, and whether that changed it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Typing {
    /// The editor is open in the box: there's nothing else to draw there.
    pub open: bool,
    pub changed: bool,
}

/// The typing of a number into `rect`, for the widget `id`. While `id`
/// has the keyboard the editor is open, starting from the value with all
/// of it selected; Enter takes what's typed (within `bounds`), Escape
/// drops it, and so does nothing: taking the keyboard elsewhere enters
/// it. `shown` is what the box says at rest, whose unit a typed number
/// may carry.
pub(crate) fn typing(ui: &mut Ui, id: WidgetId, rect: Rect, value: &mut f64, bounds: (f64, f64), step: f64, shown: &str) -> Typing {
    let now = ui.now();
    let seen = id.with("seen");
    let last = ui.state.floats(seen, [now; 4])[0];
    ui.state.floats(seen, [now; 4])[0] = now;
    let unit = shown.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | ',')).trim().to_owned();
    let read = |text: &str| -> Option<f64> {
        let t = text.trim().trim_end_matches(unit.as_str()).trim().trim_start_matches('+').replace(',', ".");
        t.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| snapped(v, bounds.0, bounds.1, step))
    };
    let mut out = Typing::default();
    if !ui.state.has_focus(id) {
        // Typed, and then the keyboard went elsewhere: it's entered,
        // unless this box hasn't been on screen since.
        if let Some(text) = ui.state.text_edit(id).buffer.take()
            && now - last <= KEPT
            && let Some(v) = read(&text)
            && v != *value
        {
            *value = v;
            out.changed = true;
        }
        return out;
    }
    out.open = true;
    let mut text = match ui.state.text_edit(id).buffer.take() {
        Some(text) => text,
        None => {
            // Just taken up: the number alone, all of it selected, so
            // typing replaces it.
            let text = written(*value, 6);
            let te = ui.state.text_edit(id);
            (te.anchor, te.cursor, te.scroll) = (0, text.len(), 0.0);
            text
        }
    };
    let edit = ui.text_edit_core(id, rect, &mut text);
    frame(ui, rect, ACCENT);
    if edit.committed {
        if let Some(v) = read(&text)
            && v != *value
        {
            *value = v;
            out.changed = true;
        }
        ui.state.focus = None;
    } else if edit.cancelled {
        ui.state.focus = None;
    } else {
        ui.state.text_edit(id).buffer = Some(text);
    }
    if edit.committed || edit.cancelled {
        ui.state.request_rebuild = true;
    }
    out
}

/// A number in a rect of the caller's, with `label` beside it: drag
/// along it to change it by `step` a pixel (a tenth with Shift), press
/// and let go to type one. `range` holds it in. Whether it changed.
#[allow(clippy::too_many_arguments)]
pub fn number_in(ui: &mut Ui, id: WidgetId, rect: Rect, label: &str, value: &mut f64, step: f64, range: Option<(f64, f64)>, decimals: usize) -> bool {
    let (min, max) = range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
    let whole = if decimals == 0 { 1.0 } else { 0.0 };
    let shown = written(*value, decimals);
    let typed = typing(ui, id.with("typed"), rect, value, (min, max), whole, &shown);
    if typed.open {
        return typed.changed;
    }
    let mut changed = typed.changed;
    let r = ui.interact(id, rect, Sense::DRAG);
    if r.hovered || r.held {
        ui.state.cursor_icon = CursorIcon::EwResize;
    }
    // Where the drag began, and whether it has gone anywhere.
    let memory = id.with("drag");
    if r.pressed {
        *ui.state.floats(memory, [0.0; 4]) = [*value, 0.0, 0.0, 0.0];
    }
    if r.held {
        let travelled = ui.state.pointer.x - ui.state.press_pos.x;
        if travelled.abs() >= 3.0 {
            let [from, ..] = *ui.state.floats(memory, [0.0; 4]);
            ui.state.floats(memory, [0.0; 4])[1] = 1.0;
            let fine = if ui.state.mods.shift() { 0.1 } else { 1.0 };
            let v = snapped(from + travelled / ui.m.scale * step * fine, min, max, whole);
            changed |= v != *value;
            *value = v;
        }
    }
    if r.released && r.hovered && ui.state.floats(memory, [0.0; 4])[1] == 0.0 {
        ui.state.focus = Some(id.with("typed"));
        ui.state.request_rebuild = true;
    }
    let style = ui.text_style();
    well(ui, rect, if r.hovered || r.held { ACCENT } else { EDGE });
    let inner = Rect::new(Vec2::new(rect.min.x + px(ui, 8.0), rect.min.y), Vec2::new(rect.max.x - px(ui, 8.0), rect.max.y));
    ui.text_in_rect(label, &style, inner, TEXT_DIM);
    ui.text_right(&written(*value, decimals), &style, inner, TEXT);
    changed
}
