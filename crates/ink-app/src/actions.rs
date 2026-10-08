//! What the menus, keys and tabs ask for (LS3's `actions.rs`): new,
//! open, save, close, quit, undo, the view, the tool in hand. Unsaved
//! work is always asked about.

use std::path::PathBuf;

use ink_core::DocId;
use lntrn_props::Value;
use lntrn_ui::{Action, HostCx, ShellRequest};

use crate::camera::Camera;
use crate::files::Then;
use crate::ink::Ink;
use crate::menus::*;
use crate::ops::{Op, Order};

fn doc_arg(action: &Action) -> Option<DocId> {
    match action.arg("doc") {
        Some(Value::Str(name)) => name.parse().ok(),
        _ => None,
    }
}

/// `id`, for `doc`.
pub(crate) fn doc_action(id: &str, doc: DocId) -> Action {
    Action::new(id).with("doc", Value::Str(doc.to_string()))
}

impl Ink {
    pub fn act(&mut self, action: &Action, cx: &mut HostCx) {
        let active = self.tabs.active_doc();
        match action.id.as_str() {
            NEW => self.new_document(),
            OPEN => self.files.pick_open(),
            SAVE => {
                if let Some(doc) = active {
                    self.save(doc, Then::Nothing);
                }
            }
            SAVE_AS => {
                if let Some(doc) = active {
                    self.save_as(doc, Then::Nothing);
                }
            }
            RECENT => {
                if let Some(Value::Str(p)) = action.arg("path") {
                    let path = PathBuf::from(p);
                    if path.exists() {
                        self.open(path);
                    } else {
                        self.recent.remove(&path);
                        self.toast(format!("{} isn't there any more", crate::lifecycle::file_name(&path)));
                    }
                }
            }
            CLEAR_RECENT => self.recent.clear(),
            CLOSE_TAB => {
                if let Some(doc) = active {
                    self.ask_close(doc, cx);
                }
            }
            TOOL_KEY => {
                if let Some(Value::Str(key)) = action.arg("key")
                    && let Some(c) = key.chars().next()
                {
                    self.tools.press(c);
                }
            }
            UNDO | REDO => {
                if let Some(doc) = active {
                    let done = if action.id == UNDO { self.core.undo(doc) } else { self.core.redo(doc) };
                    if let Err(e) = done {
                        lntrn_core::log_error!("{}: {e}", action.id);
                    }
                }
            }
            ZOOM_IN | ZOOM_OUT | FIT | ACTUAL => {
                let (area, page) = (self.layout.canvas, self.viewport().map(|v| v.size));
                if let (Some(page), Some(cam)) = (page, self.tabs.active_mut().and_then(|t| t.camera.as_mut())) {
                    match action.id.as_str() {
                        ZOOM_IN => cam.step(area, 1),
                        ZOOM_OUT => cam.step(area, -1),
                        FIT => *cam = Camera::fit(area, page),
                        _ => *cam = Camera::centred(area, page, 1.0),
                    }
                }
            }
            SAVE_AND_CLOSE => {
                if let Some(doc) = doc_arg(action) {
                    self.save(doc, Then::Close);
                }
            }
            DISCARD_AND_CLOSE => {
                if let Some(doc) = doc_arg(action) {
                    self.close(doc);
                }
            }
            QUIT => {
                if self.may_quit(cx) {
                    cx.request(ShellRequest::Quit);
                }
            }
            QUIT_ANYWAY => {
                self.quitting = true;
                cx.request(ShellRequest::Quit);
            }
            CUT => self.op(Op::Cut, cx),
            COPY => self.op(Op::Copy, cx),
            PASTE => self.op(Op::Paste, cx),
            DUPLICATE => self.op(Op::Duplicate, cx),
            DELETE => self.op(Op::Delete, cx),
            SELECT_ALL => self.op(Op::SelectAll, cx),
            DESELECT => self.op(Op::Deselect, cx),
            GROUP => self.op(Op::Group, cx),
            UNGROUP => self.op(Op::Ungroup(false), cx),
            UNGROUP_ANYWAY => {
                // The answer to a question about this tab's drawing.
                if doc_arg(action) == active {
                    self.op(Op::Ungroup(true), cx);
                }
            }
            TO_FRONT => self.op(Op::Order(Order::Front), cx),
            FORWARD => self.op(Op::Order(Order::Forward), cx),
            BACKWARD => self.op(Op::Order(Order::Backward), cx),
            TO_BACK => self.op(Op::Order(Order::Back), cx),
            ALIGN => {
                let part = |way: &str| match action.arg(way) {
                    Some(Value::F64(at)) => Some(*at),
                    _ => None,
                };
                self.op(Op::Align(part("x"), part("y")), cx);
            }
            ALIGN_TO_PAGE => {
                self.settings.align_to_page = !self.settings.align_to_page;
                self.settings.save();
            }
            DISTRIBUTE => self.op(Op::Distribute(matches!(action.arg("across"), Some(Value::Bool(true)))), cx),
            FLIP_H => self.op(Op::Flip(true), cx),
            FLIP_V => self.op(Op::Flip(false), cx),
            ROTATE_CW => self.op(Op::Quarter(true), cx),
            ROTATE_CCW => self.op(Op::Quarter(false), cx),
            LOCK => self.op(Op::Lock, cx),
            ESCAPE => self.escape(),
            NUDGE => {
                if let (Some(Value::F64(dx)), Some(Value::F64(dy))) = (action.arg("dx"), action.arg("dy")) {
                    self.nudge(lntrn_math::Vec2::new(*dx, *dy));
                }
            }
            // A greyed row: its slice hasn't come.
            LATER => {}
            other => lntrn_core::log_error!("no such action: {other}"),
        }
    }
}
