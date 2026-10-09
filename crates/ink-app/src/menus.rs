//! The title bar's six menus and the keys (ARCHITECTURE §8; the
//! machinery is LS3's `menus.rs`). Rows whose work comes in a later
//! slice of M4 are there, greyed, so the menus have their final shape
//! from the start (`docs/M4.md` says which slice lights each).

use lntrn_props::Value;
use lntrn_ui::keymap::CTX_WINDOW;
use lntrn_ui::{Action, Key, KeyConfig, KeyItem, KeyPress, Menu, MenuItem, Modifiers, Trigger};

use ink_core::{Actor, Step};

use crate::ops::Picked;
use crate::settings::Recent;
use crate::tools;

pub const TITLE_MENUS: [(&str, &str); 6] = [("File", "file"), ("Edit", "edit"), ("Object", "object"), ("Path", "path"), ("Text", "text"), ("View", "view")];

pub const NEW: &str = "file.new";
pub const OPEN: &str = "file.open";
pub const SAVE: &str = "file.save";
pub const SAVE_AS: &str = "file.save_as";
pub const RECENT: &str = "file.recent";
pub const CLEAR_RECENT: &str = "file.clear_recent";
/// Close the tab that shows, asking first if it has changes.
pub const CLOSE_TAB: &str = "file.close_tab";
pub const UNDO: &str = "edit.undo";
pub const REDO: &str = "edit.redo";
pub const CUT: &str = "edit.cut";
pub const COPY: &str = "edit.copy";
pub const PASTE: &str = "edit.paste";
pub const DUPLICATE: &str = "edit.duplicate";
pub const DELETE: &str = "edit.delete";
pub const SELECT_ALL: &str = "edit.select_all";
pub const DESELECT: &str = "edit.deselect";
pub const GROUP: &str = "object.group";
pub const UNGROUP: &str = "object.ungroup";
/// "Ungroup anyway?" answered yes: what only the group held goes.
pub const UNGROUP_ANYWAY: &str = "object.ungroup_anyway";
pub const TO_FRONT: &str = "object.to_front";
pub const FORWARD: &str = "object.forward";
pub const BACKWARD: &str = "object.backward";
pub const TO_BACK: &str = "object.to_back";
/// Line the selection up: `x` or `y`, which part of each (0, 0.5, 1).
pub const ALIGN: &str = "object.align";
/// Whether Align is against the page: a setting, flipped.
pub const ALIGN_TO_PAGE: &str = "object.align_to_page";
/// Share the space out: `across`, or down.
pub const DISTRIBUTE: &str = "object.distribute";
pub const FLIP_H: &str = "object.flip_h";
pub const FLIP_V: &str = "object.flip_v";
pub const ROTATE_CW: &str = "object.rotate_cw";
pub const ROTATE_CCW: &str = "object.rotate_ccw";
pub const LOCK: &str = "object.lock";
/// A row of a palette swatch's menu: `op`, on swatch `index`.
pub const PALETTE_OP: &str = "palette.op";
/// A row of the Node tool's menu: `op` says which.
pub const NODE_OP: &str = "node.op";
pub const ZOOM_IN: &str = "view.zoom_in";
pub const ZOOM_OUT: &str = "view.zoom_out";
pub const FIT: &str = "view.fit";
/// The page at its own size: a px of it a px of the screen.
pub const ACTUAL: &str = "view.actual";
/// A tool's letter was pressed: `key`.
pub const TOOL_KEY: &str = "tool.key";
/// Escape on the canvas, and the arrow keys.
pub const ESCAPE: &str = "pointer.escape";
pub const NUDGE: &str = "pointer.nudge";
/// How far an arrow key moves the selection, in the drawing's units,
/// and how many times that with Shift.
pub const NUDGE_BY: f64 = 1.0;
pub const NUDGE_MORE: f64 = 10.0;
/// A row whose work isn't built yet: greyed, and it does nothing.
pub const LATER: &str = "later";
/// "Save first?" answered: save, then close.
pub const SAVE_AND_CLOSE: &str = "tab.save_and_close";
/// "Save first?" answered: close without saving.
pub const DISCARD_AND_CLOSE: &str = "tab.discard_and_close";
/// Quit, asking first if work is unsaved. (LUI2's own `actions::QUIT`
/// quits at once, without `close_requested`.)
pub const QUIT: &str = "app.quit";
/// Quit, letting go of unsaved work.
pub const QUIT_ANYWAY: &str = "app.quit_anyway";

/// What the menus need to know of the window's state.
pub struct MenuState<'a> {
    pub has_doc: bool,
    /// The step Undo would take back and the one Redo would do again.
    pub undo: Option<&'a Step>,
    pub redo: Option<&'a Step>,
    pub recent: &'a Recent,
    /// What's selected.
    pub picked: Picked,
    /// Align is against the page.
    pub align_to_page: bool,
}

fn row(label: &str, id: &str) -> MenuItem {
    MenuItem::new(label, Action::new(id))
}

/// A row that waits for its slice.
fn later(label: &str) -> MenuItem {
    row(label, LATER).enabled(false)
}

/// Undo's row, or Redo's: the step it's for by name, and that it's
/// Claude's when it is ("Undo Claude's Move"). Dim with none.
fn step_row(verb: &str, id: &str, step: Option<&Step>) -> MenuItem {
    let label = match step {
        None => verb.to_owned(),
        Some(Step { label, actor: Actor::Alva }) => format!("{verb} {label}"),
        Some(Step { label, actor: Actor::Claude }) => format!("{verb} Claude\u{2019}s {label}"),
    };
    row(&label, id).enabled(step.is_some())
}

/// A row of the Align menu: which part of each thing goes in line.
fn align_row(label: &str, way: &str, at: f64) -> MenuItem {
    MenuItem::new(label, Action::new(ALIGN).with(way, Value::F64(at)))
}

pub fn menu(name: &str, st: &MenuState) -> Option<Menu> {
    let doc = st.has_doc;
    let sep = MenuItem::separator;
    // Something selected; something of it that's drawn, so can be moved.
    let (any, drawn) = (st.picked.count > 0, st.picked.drawn > 0);
    Some(match name {
        "file" => {
            let mut items = vec![row("New", NEW), row("Open\u{2026}", OPEN)];
            if !st.recent.paths.is_empty() {
                items.push(MenuItem::sub("Open Recent", recent_rows(st.recent)));
            }
            items.extend([row("Save", SAVE).enabled(doc), row("Save As\u{2026}", SAVE_AS).enabled(doc), sep(), later("Page\u{2026}"), later("Export\u{2026}"), sep(), row("Close Tab", CLOSE_TAB).enabled(doc), row("Quit", QUIT)]);
            Menu::new("File", items)
        }
        "edit" => Menu::new(
            "Edit",
            vec![
                step_row("Undo", UNDO, st.undo),
                step_row("Redo", REDO, st.redo),
                sep(),
                row("Cut", CUT).enabled(any),
                row("Copy", COPY).enabled(any),
                row("Paste", PASTE).enabled(doc),
                row("Duplicate", DUPLICATE).enabled(any),
                row("Delete", DELETE).enabled(any),
                sep(),
                row("Select All", SELECT_ALL).enabled(doc),
                row("Deselect", DESELECT).enabled(any),
                sep(),
                later("Tidy"),
                sep(),
                later("Let Claude In"),
                later("Preferences\u{2026}"),
            ],
        ),
        "object" => Menu::new(
            "Object",
            vec![
                row("Group", GROUP).enabled(drawn),
                row("Ungroup", UNGROUP).enabled(st.picked.group),
                sep(),
                row("Bring to Front", TO_FRONT).enabled(drawn),
                row("Bring Forward", FORWARD).enabled(drawn),
                row("Send Backward", BACKWARD).enabled(drawn),
                row("Send to Back", TO_BACK).enabled(drawn),
                sep(),
                MenuItem::sub(
                    "Align",
                    vec![
                        align_row("Left", "x", 0.0),
                        align_row("Centre", "x", 0.5),
                        align_row("Right", "x", 1.0),
                        sep(),
                        align_row("Top", "y", 0.0),
                        align_row("Middle", "y", 0.5),
                        align_row("Bottom", "y", 1.0),
                        sep(),
                        row("To the Page", ALIGN_TO_PAGE).checked(st.align_to_page),
                    ],
                )
                .enabled(drawn),
                MenuItem::sub("Distribute", vec![MenuItem::new("Across", Action::new(DISTRIBUTE).with("across", Value::Bool(true))), MenuItem::new("Down", Action::new(DISTRIBUTE).with("across", Value::Bool(false)))]).enabled(st.picked.drawn >= 3),
                sep(),
                row("Flip Horizontal", FLIP_H).enabled(drawn),
                row("Flip Vertical", FLIP_V).enabled(drawn),
                row("Rotate 90\u{b0} CW", ROTATE_CW).enabled(drawn),
                row("Rotate 90\u{b0} CCW", ROTATE_CCW).enabled(drawn),
                sep(),
                later("Clip"),
                later("Release Clip"),
                later("Drop Shadow\u{2026}"),
                later("Blur\u{2026}"),
                sep(),
                row(if st.picked.locked { "Unlock" } else { "Lock" }, LOCK).enabled(any),
            ],
        ),
        "path" => Menu::new("Path", vec![later("Object to Path"), sep(), later("Union"), later("Subtract"), later("Intersect"), later("Exclude"), sep(), later("Outline Stroke"), later("Simplify"), later("Reverse")]),
        "text" => Menu::new("Text", vec![later("Text to Path")]),
        "view" => Menu::new(
            "View",
            vec![
                row("Zoom In", ZOOM_IN).enabled(doc),
                row("Zoom Out", ZOOM_OUT).enabled(doc),
                row("Fit to Window", FIT).enabled(doc),
                row("Actual Size", ACTUAL).enabled(doc),
                sep(),
                later("Pixel Grid"),
                later("Snapping"),
                later("Guides"),
            ],
        ),
        _ => return None,
    })
}

fn recent_rows(recent: &Recent) -> Vec<MenuItem> {
    let mut rows: Vec<MenuItem> = recent
        .paths
        .iter()
        .zip(&recent.missing)
        .map(|(p, &missing)| {
            let name = p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
            let label = if missing { format!("{name} (missing)") } else { name };
            MenuItem::new(&label, Action::new(RECENT).with("path", Value::Str(p.display().to_string())))
        })
        .collect();
    rows.extend([MenuItem::separator(), row("Clear Recent", CLEAR_RECENT)]);
    rows
}

/// The Ctrl keys.
pub fn keys() -> KeyConfig {
    let ctrl = Modifiers::CTRL;
    let shift = Modifiers::CTRL | Modifiers::SHIFT;
    let mut k = KeyConfig::default();
    let bound = [
        ('n', ctrl, NEW),
        ('o', ctrl, OPEN),
        ('s', ctrl, SAVE),
        ('s', shift, SAVE_AS),
        ('w', ctrl, CLOSE_TAB),
        ('q', ctrl, QUIT),
        ('z', ctrl, UNDO),
        ('z', shift, REDO),
        ('x', ctrl, CUT),
        ('c', ctrl, COPY),
        ('v', ctrl, PASTE),
        ('d', ctrl, DUPLICATE),
        ('a', ctrl, SELECT_ALL),
        ('a', shift, DESELECT),
        ('g', ctrl, GROUP),
        ('g', shift, UNGROUP),
        (']', ctrl, FORWARD),
        ('[', ctrl, BACKWARD),
        ('}', shift, TO_FRONT),
        ('{', shift, TO_BACK),
        // The key comes as the sign or, shifted, what's above it.
        ('=', ctrl, ZOOM_IN),
        ('+', shift, ZOOM_IN),
        ('-', ctrl, ZOOM_OUT),
        ('0', ctrl, FIT),
        ('1', ctrl, ACTUAL),
    ];
    for (key, mods, id) in bound {
        k.bind(CTX_WINDOW, KeyItem::new(Trigger::key(Key::Char(key), mods), id));
    }
    k
}

/// A plain letter that's some tool's key, as the action that steps its
/// cycle. A key held down doesn't count (it would spin through a
/// group), and Ctrl, Alt and Super are other bindings'.
pub fn tool_key(press: KeyPress) -> Option<Action> {
    let Key::Char(c) = press.key else { return None };
    let m = press.mods;
    if press.repeat || m.ctrl() || m.alt() || m.super_key() {
        return None;
    }
    let c = c.to_ascii_lowercase();
    tools::key_cycle(c)?;
    Some(Action::new(TOOL_KEY).with("key", Value::Str(c.to_string())))
}

/// Escape and the arrow keys, as what they do on the canvas. Ctrl, Alt
/// and Super are other bindings'.
pub fn canvas_key(press: KeyPress) -> Option<Action> {
    let m = press.mods;
    if m.ctrl() || m.alt() || m.super_key() {
        return None;
    }
    let step = if m.shift() { NUDGE_BY * NUDGE_MORE } else { NUDGE_BY };
    let (dx, dy) = match press.key {
        Key::Escape => return Some(Action::new(ESCAPE)),
        Key::Delete | Key::Backspace => return Some(Action::new(DELETE)),
        Key::ArrowLeft => (-step, 0.0),
        Key::ArrowRight => (step, 0.0),
        Key::ArrowUp => (0.0, -step),
        Key::ArrowDown => (0.0, step),
        _ => return None,
    };
    Some(Action::new(NUDGE).with("dx", Value::F64(dx)).with("dy", Value::F64(dy)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_title_menu_is_there() {
        let recent = Recent::default();
        let st = MenuState { has_doc: true, undo: None, redo: None, recent: &recent, picked: Picked::default(), align_to_page: false };
        for (_, name) in TITLE_MENUS {
            assert!(menu(name, &st).is_some(), "{name}");
        }
        assert!(menu("nope", &st).is_none());
    }

    #[test]
    fn undo_names_its_step_and_whose_it_is() {
        let named = |verb: &str, step: Option<&Step>| step_row(verb, UNDO, step).label;
        assert_eq!(named("Undo", None), "Undo");
        assert_eq!(named("Undo", Some(&Step { label: "Move".into(), actor: Actor::Alva })), "Undo Move");
        assert_eq!(named("Redo", Some(&Step { label: "Move".into(), actor: Actor::Claude })), "Redo Claude\u{2019}s Move");
    }
}
