//! The right panel's frame (LS3's): its surface, the 2 px rule beside
//! it, and the resize grip over its left edge. The grip shows the
//! resize pointer at once and turns gold after a second of hovering;
//! drag to resize (260–520 px), double-click for 400. Its sections
//! come with the object tree (M4b) and the paint (M4c).

use lntrn_math::Rect;
use lntrn_ui::{CursorIcon, Sense, Ui};

use crate::chrome::surface;
use crate::layout::{Layout, clamp_panel};
use crate::theme::{ACCENT, BORDER, PANEL, PANEL_W};

/// How long the pointer rests on the grip before it lights up.
const GRIP_GLOW_AFTER: f64 = 1.0;

/// The panel's frame state between frames.
#[derive(Default)]
pub struct Grip {
    /// When the pointer came to rest on the grip.
    hover_since: Option<f64>,
    /// The width when a drag began, logical px.
    drag_from: Option<f64>,
}

pub enum Resize {
    /// Mid-drag: show it at this width.
    To(f64),
    /// Done (let go, or reset): keep this width.
    Settled(f64),
}

pub fn draw(ui: &mut Ui, l: &Layout, width: f64, grip: &mut Grip) -> Option<Resize> {
    surface(ui, l.panel, PANEL, 0.0);
    ui.draw.rect(l.rule, BORDER);
    let resp = ui.interact(ui.id("panel-grip"), l.grip, Sense::DRAG);
    let now = ui.now();
    let mut out = None;
    if resp.hovered || resp.held {
        ui.state.cursor_icon = CursorIcon::EwResize;
        let since = *grip.hover_since.get_or_insert(now);
        if now - since >= GRIP_GLOW_AFTER || resp.held {
            ui.draw.rect(Rect::new(l.grip.min, l.grip.max), ACCENT.with_alpha(0.85));
        } else {
            ui.state.request_redraw_after(GRIP_GLOW_AFTER - (now - since));
        }
    } else {
        grip.hover_since = None;
    }
    if resp.double_clicked {
        grip.drag_from = None;
        return Some(Resize::Settled(PANEL_W));
    }
    if resp.pressed {
        grip.drag_from = Some(width);
    }
    if let Some(from) = grip.drag_from {
        // The panel is on the right: dragging left widens it.
        let dragged = (ui.state.press_pos.x - ui.state.pointer.x) / ui.m.scale;
        let w = clamp_panel(from + dragged);
        out = Some(if resp.held { Resize::To(w) } else { Resize::Settled(w) });
        if !resp.held {
            grip.drag_from = None;
        }
    }
    out
}
