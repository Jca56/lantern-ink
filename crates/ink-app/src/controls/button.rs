//! Buttons (LS3's `controls/button.rs`): a face with the sheen in a
//! line, its word in the middle. Ink's own beside it: a button that can
//! be greyed, for what there's nothing to do with just now.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, FILL, Response, Sense, Ui};

use super::{EDGE, face};
use crate::theme::{ACCENT, ACTIVE, BUTTON, BUTTON_HOVER, TEXT, TEXT_DIM};

fn pressed(ui: &mut Ui, label: &str) -> Response {
    let id = ui.id(label);
    let rect = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    let mut r = ui.interact(id, rect, Sense::CLICK);
    ui.focusable(id, rect);
    ui.key_click(id, &mut r);
    if r.hovered {
        ui.state.cursor_icon = CursorIcon::Pointer;
    }
    r
}

/// A button as wide as there's room.
pub fn button(ui: &mut Ui, label: &str) -> Response {
    let r = pressed(ui, label);
    let base = if r.held { BUTTON } else if r.hovered { ACTIVE } else { BUTTON_HOVER };
    face(ui, r.rect, base, if r.hovered { ACCENT } else { EDGE });
    let style = ui.text_style();
    ui.text_centered(label, &style, r.rect, TEXT);
    ui.focus_ring(r.id, r.rect);
    r
}

/// A button, or (`on` false) its place greyed: there to be seen, with
/// nothing to press. Whether it was clicked, and where it is.
pub fn button_if(ui: &mut Ui, label: &str, on: bool) -> (bool, Rect) {
    if on {
        let r = button(ui, label);
        return (r.clicked, r.rect);
    }
    let rect = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    face(ui, rect, BUTTON, EDGE.with_alpha(0.45));
    let style = ui.text_style();
    ui.text_centered(label, &style, rect, TEXT_DIM.with_alpha(0.7));
    (false, rect)
}
