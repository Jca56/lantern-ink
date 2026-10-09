//! The Edit and Object menus' work on the selection, by the keys and
//! by name; the clipboard; and the Box's numbers.

use ink_core::{Actor, Command, NodeId};
use lntrn_ui::{HostCx, ShellRequest};

use super::*;
use crate::ops::{Op, Order};

/// Back to front: a blue square, a tall gold bar, a low dark bar.
const THREE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"8\" height=\"8\" fill=\"#08f\" stroke=\"#000\" stroke-width=\"1\"/>\n  <rect id=\"b\" x=\"20\" y=\"10\" width=\"4\" height=\"12\" fill=\"#fc0\"/>\n  <rect id=\"c\" x=\"36\" y=\"30\" width=\"8\" height=\"4\" fill=\"#333\"/>\n</svg>\n";
const A: NodeId = NodeId(2);
const B: NodeId = NodeId(3);
const C: NodeId = NodeId(4);

fn three(name: &str) -> Running {
    let path = scratch(name).join("three.svg");
    std::fs::write(&path, THREE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    /// Do `op` as a menu row would; what it asked of the shell.
    fn op(&mut self, op: Op) -> Vec<ShellRequest> {
        let mut requests = Vec::new();
        self.ink.op(op, &mut HostCx { pointer: Vec2::ZERO, requests: &mut requests });
        self.frames(2);
        requests
    }

    /// The drawing's top level, back to front, by `id`.
    fn stack(&self) -> Vec<String> {
        let doc = self.ink.core.doc(self.doc()).unwrap();
        doc.node(doc.root()).unwrap().elements().filter_map(|id| doc.node(id).ok()?.attr("id").map(str::to_owned)).collect()
    }

    fn num(&self, node: NodeId, name: &str) -> f64 {
        self.ink.core.doc(self.doc()).unwrap().node(node).unwrap().attr(name).unwrap().parse().unwrap()
    }

    fn pick(&mut self, nodes: &[NodeId]) {
        let sel = &mut self.ink.tabs.active_mut().unwrap().selection;
        (sel.active, sel.nodes) = (nodes.last().copied(), nodes.to_vec());
        self.frames(1);
    }

    /// Where the Box drew what it calls `name`.
    fn boxed(&self, name: &str) -> Rect {
        self.ink.toolbox.laid.iter().find(|(n, _)| *n == name).map(|(_, r)| *r).unwrap_or_else(|| panic!("the Box has no {name}"))
    }
}

#[test]
fn the_keys_duplicate_delete_and_pick_everything() {
    let mut r = three("ops-keys");
    r.pick(&[A]);
    r.key(Key::Char('d'), Modifiers::CTRL);
    // The copy is what's picked now, over its original.
    let copy = r.selected();
    assert_eq!((copy.len(), r.stack(), r.steps()), (1, vec!["a".to_owned(), "a-2".into(), "b".into(), "c".into()], vec!["Duplicate".to_owned()]));
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!((r.selected().len(), r.stack().len(), r.steps().len()), (0, 3, 2));
    // With nothing picked, nothing more goes.
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!(r.steps().len(), 2);
    r.key(Key::Char('a'), Modifiers::CTRL);
    assert_eq!(r.selected(), [A, B, C]);
    r.key(Key::Char('a'), Modifiers::CTRL | Modifiers::SHIFT);
    assert!(r.selected().is_empty());
}

#[test]
fn things_group_and_ungroup_and_a_group_is_asked_about_what_it_would_lose() {
    let mut r = three("ops-group");
    r.pick(&[A, B]);
    r.key(Key::Char('g'), Modifiers::CTRL);
    let group = r.selected();
    assert_eq!((group.len(), r.steps()), (1, vec!["Group".to_owned()]));
    let doc = r.doc();
    assert_eq!(r.ink.core.doc(doc).unwrap().node(A).unwrap().parent, Some(group[0]));
    r.key(Key::Char('g'), Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!((r.selected(), r.svg(), r.steps().len()), (vec![A, B], THREE.to_owned(), 2));
    // A group that holds an opacity over what's in it: ungrouping
    // would lose that, so it's asked first, and nothing is done yet.
    r.key(Key::Char('g'), Modifiers::CTRL);
    let group = r.selected()[0];
    r.ink.core.apply(doc, &Command::SetStyle { nodes: vec![group], set: vec![("opacity".into(), Some("0.5".into()))] }, Actor::Alva, "Fade").unwrap();
    let steps = r.steps().len();
    let asked = r.op(Op::Ungroup(false));
    assert!(matches!(asked.as_slice(), [ShellRequest::Dialog(_)]), "{} requests", asked.len());
    assert_eq!(r.steps().len(), steps);
    // Told to, it does.
    assert!(r.op(Op::Ungroup(true)).is_empty());
    assert_eq!((r.selected(), r.steps().last().map(String::as_str)), (vec![A, B], Some("Ungroup")));
}

#[test]
fn things_go_up_and_down_the_stack() {
    let mut r = three("ops-order");
    r.pick(&[A]);
    r.key(Key::Char(']'), Modifiers::CTRL);
    assert_eq!((r.stack(), r.steps()), (vec!["b".to_owned(), "a".into(), "c".into()], vec!["Bring Forward".to_owned()]));
    r.key(Key::Char('}'), Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!(r.stack(), ["b", "c", "a"]);
    r.key(Key::Char('['), Modifiers::CTRL);
    assert_eq!(r.stack(), ["b", "a", "c"]);
    r.key(Key::Char('{'), Modifiers::CTRL | Modifiers::SHIFT);
    assert_eq!((r.stack(), r.steps().len(), r.selected()), (vec!["a".to_owned(), "b".into(), "c".into()], 4, vec![A]));
    // At the back already: nowhere to go, and no step.
    r.op(Op::Order(Order::Back));
    assert_eq!(r.steps().len(), 4);
}

#[test]
fn things_line_up_spread_out_flip_and_turn() {
    let mut r = three("ops-arrange");
    r.pick(&[A, B, C]);
    // Their left edges, against the box round all three.
    r.op(Op::Align(Some(0.0), None));
    assert_eq!((r.num(A, "x"), r.num(B, "x"), r.num(C, "x"), r.steps()), (4.0, 4.0, 4.0, vec!["Align".to_owned()]));
    r.key(Key::Char('z'), Modifiers::CTRL);
    // The gaps across made the same: 4 to 44 holds 20 of them, 10 each.
    r.op(Op::Distribute(true));
    assert_eq!((r.num(A, "x"), r.num(B, "x"), r.num(C, "x")), (4.0, 22.0, 36.0));
    // One alone lines up against the page: its middle to the page's.
    r.pick(&[B]);
    r.op(Op::Align(Some(0.5), Some(0.5)));
    assert_eq!((r.num(B, "x"), r.num(B, "y")), (22.0, 18.0));
    // A quarter turn about its own middle (24, 24).
    r.op(Op::Quarter(true));
    assert_eq!((r.num(B, "x"), r.num(B, "y"), r.num(B, "width"), r.num(B, "height")), (18.0, 22.0, 12.0, 4.0));
    // "To the page" on: several line up against the page too.
    r.ink.settings.align_to_page = true;
    r.pick(&[A, C]);
    r.op(Op::Align(Some(1.0), None));
    assert_eq!((r.num(A, "x"), r.num(C, "x")), (40.0, 40.0));
    // Mirrored about the box round both: they change sides.
    r.op(Op::Align(Some(0.0), None));
    r.ink.settings.align_to_page = false;
    r.pick(&[A, C]);
    r.op(Op::Flip(false));
    assert_eq!((r.num(A, "y"), r.num(C, "y"), r.steps().last().map(String::as_str)), (26.0, 4.0, Some("Flip Vertical")));
    // Locked, and unlocked again.
    r.op(Op::Lock);
    assert!(r.ink.core.doc(r.doc()).unwrap().is_locked(A) && r.ink.picked().locked);
    r.op(Op::Lock);
    assert!(!r.ink.core.doc(r.doc()).unwrap().is_locked(A));
}

#[test]
fn copy_cut_and_paste_go_by_the_clipboard_as_svg() {
    let mut r = three("ops-clipboard");
    r.pick(&[A]);
    r.key(Key::Char('c'), Modifiers::CTRL);
    assert!(r.steps().is_empty(), "a copy is no edit");
    // On the clipboard: a drawing of its own, that any SVG app reads.
    assert!(r.shell.state.clipboard.starts_with("<svg ") && r.shell.state.clipboard.contains("<rect id=\"a\" x=\"4\"") && !r.shell.state.clipboard.contains("id=\"b\""), "{}", r.shell.state.clipboard);
    r.key(Key::Char('v'), Modifiers::CTRL);
    r.frames(2);
    // Pasted where it was copied from, on top, picked, under a name of
    // its own.
    let pasted = r.selected();
    assert_eq!((pasted.len(), r.stack(), r.steps()), (1, vec!["a".to_owned(), "b".into(), "c".into(), "a-2".into()], vec!["Paste".to_owned()]));
    assert_eq!(r.num(pasted[0], "x"), 4.0);
    // Cut takes it out, and it pastes back.
    r.pick(&[B]);
    r.key(Key::Char('x'), Modifiers::CTRL);
    assert_eq!((r.stack(), r.steps().last().map(String::as_str)), (vec!["a".to_owned(), "c".into(), "a-2".into()], Some("Cut")));
    r.key(Key::Char('v'), Modifiers::CTRL);
    r.frames(2);
    assert_eq!(r.stack(), ["a", "c", "a-2", "b"]);
    // What isn't a drawing says so, and pastes nothing.
    r.shell.state.set_clipboard("just words");
    r.key(Key::Char('v'), Modifiers::CTRL);
    r.frames(2);
    assert!(r.ink.toast_text().is_some_and(|said| said.contains("isn't a drawing")), "{:?}", r.ink.toast_text());
    assert_eq!(r.stack().len(), 4);
}

#[test]
fn the_box_holds_the_selections_place_and_size() {
    let mut r = three("ops-box");
    // Shut until asked for: a gold square at the canvas's top right.
    let canvas = r.ink.layout.canvas;
    let square = r.ink.toolbox.rect().expect("the Box, with the Pointer in hand");
    assert!(square.width() == 64.0 && square.max.x == canvas.max.x - 16.0 && square.min.y == canvas.min.y + 16.0, "{square:?}");
    r.click(square.center());
    assert!(r.ink.toolbox.open);
    // Nothing picked: the Pointer's own setting, and no numbers.
    assert_eq!(r.ink.toolbox.laid.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["Scale strokes"]);
    r.pick(&[A]);
    r.frames(1);
    // (A rectangle: its corners are the Box's too.)
    assert_eq!(r.ink.toolbox.laid.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["X", "Y", "W", "H", "Corners", "Scale strokes"]);
    // A press on the Box is the Box's: the canvas under it lets go of
    // nothing, and starts no marquee.
    let open = r.ink.toolbox.rect().unwrap();
    r.click(Vec2::new(open.min.x + 8.0, open.max.y - 8.0));
    assert_eq!((r.selected(), r.ink.pointing.busy()), (vec![A], false));
    // Dragged along, a number is one step: 30 px at a tenth of a unit
    // each.
    let x = r.boxed("X").center();
    r.drag(x, x + Vec2::new(30.0, 0.0));
    assert_eq!((r.num(A, "x"), r.steps(), r.ink.core.gesturing(r.doc())), (7.0, vec!["Move".to_owned()], false));
    // Pressed and let go, it's typed into: its width, with the line
    // left as it is.
    r.click(r.boxed("W").center());
    r.h.type_text("16");
    r.frames(1);
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!((r.num(A, "width"), r.num(A, "height"), r.num(A, "x"), r.num(A, "stroke-width"), r.steps().last().map(String::as_str)), (16.0, 8.0, 7.0, 1.0, Some("Scale")));
    // "Scale strokes" on: its height halved halves nothing of the line
    // (it's uneven), but twice as big both ways doubles it.
    r.click(r.boxed("Scale strokes").center());
    assert!(r.ink.settings.scale_strokes);
    assert_eq!(r.steps().len(), 2, "a setting is no edit");
}

#[test]
fn a_right_click_picks_what_is_there_and_opens_its_menu() {
    let mut r = three("ops-menu");
    let at = {
        let page = r.ink.viewport().unwrap().to_page.apply(Vec2::new(22.0, 16.0));
        r.camera().window_at(r.ink.layout.canvas, page)
    };
    r.h.advance(1.0);
    r.h.move_to(at);
    r.frames(1);
    r.h.right_press();
    r.frames(2);
    assert_eq!(r.selected(), [B]);
    let menu = r.ink.selection_menu(at);
    assert_eq!(menu.title, "b");
    // With nothing picked it's the drawing's.
    r.pick(&[]);
    assert_eq!(r.ink.selection_menu(at).title, "Drawing");
    r.pick(&[A, C]);
    assert_eq!(r.ink.selection_menu(at).title, "2 Objects");
}
