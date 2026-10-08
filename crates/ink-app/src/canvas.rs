//! The canvas area's input (LS3's `canvas.rs`): the wheel pans,
//! Ctrl+wheel and a touchpad's pinch zoom toward the pointer; the
//! middle button, Alt+drag, Space+drag and the Hand tool drag the view.
//! Chrome and floating UI draw before this, so their clicks are theirs;
//! the canvas only gets what's left, and hands a plain click on to the
//! tool in hand.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, Key, Sense, Ui};

use crate::camera::{Camera, pinch_zoom, wheel_zoom};

/// LS3's wheel: a notch pans 30 logical px.
const WHEEL_LINE: f64 = 30.0;
/// How far the pointer may stray between press and release, logical
/// px, and still have clicked.
const CLICK_SLOP: f64 = 4.0;

/// What the pointer did on the canvas this frame.
pub struct CanvasInput {
    /// Where on the page (its px) the pointer is, while it's over the
    /// canvas.
    pub pointer: Option<Vec2>,
    /// The pointer is over the canvas, with nothing floating in between.
    pub over: bool,
    /// The left button came up where it went down: a click, for the
    /// tool in hand (not one that dragged the view).
    pub clicked: bool,
}

/// How the view takes the pointer this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hold {
    /// A gesture is under way, or a menu is up: the view holds still.
    pub locked: bool,
    /// The tool in hand has its own use for Alt, so Alt+drag doesn't
    /// move the view.
    pub owns_alt: bool,
    /// The Hand tool is in hand: a plain drag moves the view.
    pub hand: bool,
}

pub fn input(ui: &mut Ui, area: Rect, cam: &mut Camera, hold: Hold) -> CanvasInput {
    let resp = ui.interact(ui.id("canvas"), area, Sense::DRAG);
    let st = &ui.state;
    let pointer = st.pointer;
    let over = st.pointer_in_window && area.contains(pointer) && !st.shielded(ui.layer(), pointer);
    if hold.locked {
        ui.state.wheel = Vec2::ZERO;
        return CanvasInput { pointer: over.then(|| cam.page_at(area, pointer)), over, clicked: false };
    }

    // The middle button pans from wherever it went down on the canvas;
    // so does the left with Space or Alt held, or the Hand in hand.
    let middle = st.middle_down && area.contains(st.middle_press_pos);
    let grabbing = hold.hand || st.keys_down.contains(&Key::Space) || (st.mods.alt() && !hold.owns_alt);
    let clicked = resp.released && over && !grabbing && pointer.distance(st.press_pos) <= CLICK_SLOP * ui.m.scale;
    if middle || (resp.held && grabbing) {
        // By how far the pointer has gone with the button down: on the
        // frame of the press, not the way it came there.
        let by = if resp.held && grabbing {
            resp.drag_delta
        } else if st.middle_pressed {
            st.pointer - st.middle_press_pos
        } else {
            st.delta
        };
        cam.pan(by);
        ui.state.cursor_icon = CursorIcon::Grabbing;
    }

    if over && ui.state.wheel != Vec2::ZERO {
        let (wheel, smooth, s) = (ui.state.wheel, ui.state.wheel_smooth, ui.m.scale);
        if ui.state.mods.ctrl() {
            // LUI2 reports a notch as a widget's height of pixels.
            let factor = if smooth { wheel_zoom(0.0, wheel.y / s) } else { wheel_zoom(wheel.y / ui.m.widget_h, 0.0) };
            cam.zoom_about(area, factor, pointer);
        } else {
            let d = if smooth { wheel } else { wheel / ui.m.widget_h * WHEEL_LINE * s };
            cam.pan(d);
        }
        ui.state.wheel = Vec2::ZERO;
    }

    // Two fingers spreading or closing on a touchpad.
    if over && ui.state.pinch != 1.0 {
        cam.zoom_about(area, pinch_zoom(ui.state.pinch), pointer);
    }

    // Where the pointer is now the view has moved.
    CanvasInput { pointer: over.then(|| cam.page_at(area, pointer)), over, clicked }
}
