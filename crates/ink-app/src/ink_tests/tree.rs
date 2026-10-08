//! The object tree in the right panel: the drawing's rows, picked,
//! hidden, locked, renamed and dragged about, each edit a step that
//! undoes.

use ink_core::NodeId;

use super::*;

/// Back to front: a, the group "Lamp" holding (b1, b2), c. With the
/// definitions under them all.
const LAMP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 48 48\">\n  <defs>\n    <linearGradient id=\"sky\">\n      <stop stop-color=\"#08f\"/>\n      <stop offset=\"1\" stop-color=\"#fc0\"/>\n    </linearGradient>\n  </defs>\n  <rect id=\"a\" x=\"2\" y=\"2\" width=\"20\" height=\"20\" fill=\"url(#sky)\"/>\n  <g ink:label=\"Lamp\">\n    <circle id=\"b1\" cx=\"30\" cy=\"30\" r=\"6\" fill=\"#fc0\"/>\n    <path id=\"b2\" d=\"M24 40 H36\" stroke=\"#000\"/>\n  </g>\n  <ellipse id=\"c\" cx=\"40\" cy=\"10\" rx=\"5\" ry=\"3\"/>\n</svg>\n";
const DEFS: NodeId = NodeId(2);
const A: NodeId = NodeId(6);
const G: NodeId = NodeId(7);
const B1: NodeId = NodeId(8);
const B2: NodeId = NodeId(9);
const C: NodeId = NodeId(10);

fn lamp(name: &str) -> Running {
    let path = scratch(name).join("lamp.svg");
    std::fs::write(&path, LAMP).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    fn rows(&self) -> Vec<NodeId> {
        self.ink.tree.laid.iter().map(|(row, _)| row.id).collect()
    }

    fn row(&self, id: NodeId) -> Rect {
        self.ink.tree.laid.iter().find(|(row, _)| row.id == id).map(|(_, r)| *r).unwrap_or_else(|| panic!("no row for {id}"))
    }

    /// The middle of `id`'s name.
    fn name(&self, id: NodeId) -> Vec2 {
        self.row(id).center()
    }

    /// Its eye: the button at the row's right end.
    fn eye(&self, id: NodeId) -> Vec2 {
        let r = self.row(id);
        Vec2::new(r.max.x - 26.0, r.center().y)
    }

    /// Its padlock: the button left of the eye (the last one, on a row
    /// with no eye).
    fn padlock(&self, id: NodeId) -> Vec2 {
        let r = self.row(id);
        let eye = if crate::select::is_drawn(self.ink.core.doc(self.doc()).unwrap(), id) { 42.0 } else { 0.0 };
        Vec2::new(r.max.x - 26.0 - eye, r.center().y)
    }

    /// What opens and closes it, at its left end (`depth` rows in).
    fn disclosure(&self, id: NodeId, depth: usize) -> Vec2 {
        let r = self.row(id);
        Vec2::new(r.min.x + 16.0 * depth as f64 + 20.0, r.center().y)
    }

    fn click_with(&mut self, at: Vec2, mods: Modifiers) {
        self.h.set_mods(mods);
        self.click(at);
        self.h.set_mods(Modifiers::NONE);
        self.frames(1);
    }

    fn selected(&self) -> Vec<NodeId> {
        self.ink.tabs.active().unwrap().selection.nodes.clone()
    }

    fn steps(&self) -> Vec<String> {
        self.ink.core.history(self.doc()).unwrap().undoable().map(|s| s.label.clone()).collect()
    }

    fn svg(&self) -> String {
        self.ink.core.doc(self.doc()).unwrap().to_svg()
    }
}

#[test]
fn the_tree_lists_the_drawing_front_to_back() {
    let mut r = lamp("tree-lists");
    // The topmost first; the group open, the definitions shut.
    assert_eq!(r.rows(), [C, G, B2, B1, A, DEFS]);
    let (top, inside) = (r.row(C), r.row(B2));
    assert!(r.ink.layout.panel.contains(top.center()) && top.height() >= 44.0, "{top:?}");
    assert_eq!(inside.width(), top.width(), "a row is as wide as the list; its card stands in");
    // Shut the group: what it holds goes out of sight. Open the
    // definitions: the gradient shows, shut over its stops.
    r.click(r.disclosure(G, 0));
    assert_eq!(r.rows(), [C, G, A, DEFS]);
    r.click(r.disclosure(DEFS, 0));
    assert_eq!(r.rows(), [C, G, A, DEFS, NodeId(3)]);
    r.click(r.disclosure(G, 0));
    assert_eq!(r.rows(), [C, G, B2, B1, A, DEFS, NodeId(3)]);
    // None of that is an edit, or a selection.
    assert!(r.steps().is_empty() && r.selected().is_empty() && !r.ink.is_modified(r.doc()));
}

#[test]
fn rows_are_picked_alone_added_to_and_taken_in_runs() {
    let mut r = lamp("tree-picks");
    r.click(r.name(B1));
    assert_eq!(r.selected(), [B1]);
    r.click_with(r.name(C), Modifiers::CTRL);
    assert_eq!(r.selected(), [B1, C]);
    r.click_with(r.name(B1), Modifiers::CTRL);
    assert_eq!(r.selected(), [C]);
    // Shift: every row from the one in hand to here.
    r.click_with(r.name(B1), Modifiers::SHIFT);
    assert_eq!(r.selected(), [C, G, B2, B1]);
    // A plain click on one of several makes it the only one.
    r.click(r.name(B2));
    assert_eq!(r.selected(), [B2]);
    // The list's empty space lets go of everything.
    let below = Vec2::new(r.row(DEFS).center().x, r.row(DEFS).max.y + 60.0);
    r.click(below);
    assert!(r.selected().is_empty() && r.steps().is_empty());
}

#[test]
fn the_eye_and_the_padlock_are_steps_that_undo() {
    let mut r = lamp("tree-eye");
    let was = r.svg();
    r.click(r.eye(B1));
    assert_eq!((r.ink.core.doc(r.doc()).unwrap().node(B1).unwrap().attr("display"), r.steps()), (Some("none"), vec!["Hide".to_owned()]));
    r.click(r.eye(B1));
    assert_eq!((r.svg(), r.steps()), (was.clone(), vec!["Hide".to_owned(), "Show".to_owned()]));
    // A definition has no eye: only its padlock.
    r.click(r.padlock(DEFS));
    assert!(r.ink.core.doc(r.doc()).unwrap().is_locked(DEFS));
    assert_eq!(r.steps().last().map(String::as_str), Some("Lock"));
    r.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(r.svg(), was);
    // Locked, a row refuses every edit, and says so by its name.
    r.click(r.padlock(G));
    let steps = r.steps().len();
    r.click(r.eye(B1));
    assert_eq!(r.steps().len(), steps);
    assert_eq!(r.ink.toast_text(), Some("\u{201c}b1\u{201d} is in \u{201c}Lamp\u{201d}, which is locked: nothing in it changes until \u{201c}Lamp\u{201d} is unlocked"));
    // Its padlock opens it again.
    r.click(r.padlock(G));
    r.click(r.eye(B1));
    assert_eq!(r.steps().last().map(String::as_str), Some("Hide"));
}

#[test]
fn a_row_is_renamed_where_it_stands() {
    let mut r = lamp("tree-rename");
    // A second press on the row: its name is a field, all of it picked.
    let at = r.name(C);
    r.click(at);
    r.h.press();
    r.frames(1);
    r.h.release();
    r.frames(2);
    assert_eq!(r.ink.tabs.active().unwrap().selection.renaming, Some((C, "c".to_owned())));
    r.h.type_text("Moon");
    r.frames(1);
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!((r.ink.core.doc(r.doc()).unwrap().label(C), r.steps()), (Some("Moon"), vec!["Rename".to_owned()]));
    assert_eq!(r.ink.tabs.active().unwrap().selection.renaming, None);
    // Escape leaves it as it was.
    r.click(at);
    r.h.press();
    r.frames(1);
    r.h.release();
    r.frames(2);
    r.h.type_text("Sun");
    r.frames(1);
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!((r.ink.core.doc(r.doc()).unwrap().label(C), r.steps().len()), (Some("Moon"), 1));
}

#[test]
fn rows_drag_to_restack_and_into_and_out_of_groups() {
    let mut r = lamp("tree-drag");
    // a, from the bottom to the very top.
    let (from, top) = (r.name(A), r.row(C));
    r.drag(from, Vec2::new(from.x, top.min.y + 4.0));
    assert_eq!((r.rows(), r.steps()), (vec![A, C, G, B2, B1, DEFS], vec!["Restack".to_owned()]));
    // c into the group, between its two.
    let (from, between) = (r.name(C), (r.row(B2).max.y + r.row(B1).min.y) / 2.0);
    r.drag(from, Vec2::new(from.x, between));
    assert_eq!(r.rows(), [A, G, B2, C, B1, DEFS]);
    assert_eq!(r.ink.core.doc(r.doc()).unwrap().node(C).unwrap().parent, Some(G));
    // And out again, under the group: let go far to the left.
    let from = r.name(C);
    let under = Vec2::new(r.row(G).min.x + 4.0, r.row(B1).max.y + 2.0);
    r.drag(from, under);
    assert_eq!(r.rows(), [A, G, B2, B1, C, DEFS]);
    assert_eq!(r.steps().len(), 3);
    // Two picked rows go together, in their order.
    r.click(r.name(A));
    r.click_with(r.name(C), Modifiers::CTRL);
    let from = r.name(C);
    let into = Vec2::new(from.x, (r.row(B2).max.y + r.row(B1).min.y) / 2.0);
    r.drag(from, into);
    assert_eq!((r.rows(), r.selected()), (vec![G, B2, A, C, B1, DEFS], vec![A, C]));
    // Undone, they're back.
    r.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(r.rows(), [A, G, B2, B1, C, DEFS]);
}
