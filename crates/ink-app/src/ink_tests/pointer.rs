//! The Pointer on the canvas: picking, the marquee, going into groups,
//! and the selection's box dragged to move, scale and turn, each a
//! gesture that shows as it goes and lands as one step.

use ink_core::NodeId;

use super::*;

/// Back to front: a blue rect with a line round it, a group of (a dot
/// and a bar), and a locked red square.
const SCENE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"12\" height=\"8\" fill=\"#08f\" stroke=\"#000\" stroke-width=\"1\"/>\n  <g id=\"g\">\n    <circle id=\"dot\" cx=\"30\" cy=\"10\" r=\"4\" fill=\"#fc0\"/>\n    <rect id=\"bar\" x=\"24\" y=\"20\" width=\"16\" height=\"4\" fill=\"#333\"/>\n  </g>\n  <rect id=\"held\" x=\"4\" y=\"30\" width=\"10\" height=\"10\" fill=\"#c00\" ink:locked=\"true\"/>\n</svg>\n";
const A: NodeId = NodeId(2);
const G: NodeId = NodeId(3);
const DOT: NodeId = NodeId(4);
const BAR: NodeId = NodeId(5);

fn scene(name: &str) -> Running {
    let path = scratch(name).join("scene.svg");
    std::fs::write(&path, SCENE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    assert_eq!(r.ink.tools.active(), Tool::Pointer);
    r
}

impl Running {
    /// Where the drawing's point (`x`, `y`) shows in the window.
    fn at(&self, x: f64, y: f64) -> Vec2 {
        let page = self.ink.viewport().unwrap().to_page.apply(Vec2::new(x, y));
        self.camera().window_at(self.ink.layout.canvas, page)
    }

    fn attr(&self, node: NodeId, name: &str) -> Option<String> {
        self.ink.core.doc(self.doc()).unwrap().node(node).unwrap().attr(name).map(str::to_owned)
    }

    fn within(&self) -> Option<NodeId> {
        self.ink.tabs.active().unwrap().selection.within
    }

    /// A press and a release again at once: the second press of a
    /// double-click.
    fn press_again(&mut self) {
        self.h.press();
        self.frames(1);
        self.h.release();
        self.frames(2);
    }
}

#[test]
fn a_click_picks_what_is_there_and_shift_adds_or_takes_away() {
    let mut r = scene("pointer-picks");
    r.click(r.at(10.0, 8.0));
    assert_eq!(r.selected(), [A]);
    // The dot is in a group: at the top level, the group is picked.
    r.click(r.at(30.0, 10.0));
    assert_eq!(r.selected(), [G]);
    r.click_with(r.at(10.0, 8.0), Modifiers::SHIFT);
    assert_eq!(r.selected(), [G, A]);
    r.click_with(r.at(30.0, 10.0), Modifiers::SHIFT);
    assert_eq!(r.selected(), [A]);
    // What's locked isn't picked: the click falls through to nothing,
    // which lets go of everything.
    r.click(r.at(9.0, 35.0));
    assert!(r.selected().is_empty());
    // None of it is an edit.
    assert!(r.steps().is_empty() && !r.ink.is_modified(r.doc()));
}

#[test]
fn a_drag_on_nothing_is_a_marquee() {
    let mut r = scene("pointer-marquee");
    r.drag(r.at(44.0, 44.0), r.at(20.0, 2.0));
    assert_eq!(r.selected(), [G]);
    // Over everything: the locked square stays out. Back to front.
    r.drag(r.at(46.0, 46.0), r.at(1.0, 1.0));
    assert_eq!(r.selected(), [A, G]);
    // With Shift it adds to what was picked.
    r.click(r.at(10.0, 8.0));
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.at(44.0, 44.0), r.at(20.0, 16.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.selected(), [A, G]);
    assert!(r.steps().is_empty());
}

#[test]
fn a_double_click_goes_into_a_group_and_escape_comes_back_out() {
    let mut r = scene("pointer-groups");
    r.click(r.at(30.0, 10.0));
    assert_eq!((r.selected(), r.within()), (vec![G], None));
    r.press_again();
    assert_eq!((r.selected(), r.within()), (vec![DOT], Some(G)));
    // In the group, a click picks what's in it.
    r.click(r.at(32.0, 22.0));
    assert_eq!(r.selected(), [BAR]);
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!((r.selected(), r.within()), (vec![G], None));
    r.key(Key::Escape, Modifiers::NONE);
    assert!(r.selected().is_empty());
    // A click outside the group comes out of it, too.
    r.click(r.at(30.0, 10.0));
    r.press_again();
    r.click(r.at(10.0, 8.0));
    assert_eq!((r.selected(), r.within()), (vec![A], None));
}

#[test]
fn the_box_dragged_moves_what_it_holds_as_one_step() {
    let mut r = scene("pointer-move");
    let doc = r.doc();
    let was = r.svg();
    // A press on something not picked picks it, and the drag moves it.
    let (from, to) = (r.at(10.0, 8.0), r.at(20.0, 13.0));
    r.h.advance(1.0);
    r.h.move_to(from);
    r.frames(1);
    r.h.press();
    r.frames(1);
    assert_eq!(r.selected(), [A]);
    r.h.move_to(from + (to - from) * 0.5);
    r.frames(2);
    // Going: shown where it is, with the drawing itself untouched.
    assert!(r.ink.core.gesturing(doc) && r.ink.pointing.busy());
    assert_eq!((r.svg(), r.steps().len()), (was, 0));
    assert_eq!(r.ink.core.shown(doc).unwrap().0.node(A).unwrap().attr("x"), Some("9"));
    r.h.move_to(to);
    r.frames(1);
    r.h.release();
    r.frames(2);
    assert!(!r.ink.core.gesturing(doc) && !r.ink.pointing.busy());
    assert_eq!((r.attr(A, "x"), r.attr(A, "y"), r.steps()), (Some("14".into()), Some("9".into()), vec!["Move".to_owned()]));
    // Shift keeps it to one axis.
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.at(20.0, 13.0), r.at(30.0, 16.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!((r.attr(A, "x"), r.attr(A, "y")), (Some("24".into()), Some("9".into())));
    // Escape in the middle of a drag: as if it never began. (From a
    // part of it the dot isn't over.)
    let (from, to) = (r.at(26.0, 15.0), r.at(40.0, 30.0));
    r.h.advance(1.0);
    r.h.move_to(from);
    r.frames(1);
    r.h.press();
    r.frames(1);
    r.h.move_to(to);
    r.frames(2);
    assert!(r.ink.core.gesturing(doc));
    r.key(Key::Escape, Modifiers::NONE);
    assert!(!r.ink.core.gesturing(doc));
    r.h.release();
    r.frames(2);
    assert_eq!((r.attr(A, "x"), r.steps().len(), r.selected()), (Some("24".into()), 2, vec![A]));
    r.key(Key::Char('z'), Modifiers::CTRL);
    r.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(r.svg(), SCENE);
}

#[test]
fn a_handle_scales_and_the_line_keeps_its_width() {
    let mut r = scene("pointer-scale");
    r.click(r.at(10.0, 8.0));
    // Its bottom right corner, out to half as big again each way.
    r.drag(r.at(16.0, 12.0), r.at(22.0, 16.0));
    assert_eq!((r.attr(A, "x"), r.attr(A, "y"), r.attr(A, "width"), r.attr(A, "height")), (Some("4".into()), Some("4".into()), Some("18".into()), Some("12".into())));
    assert_eq!((r.attr(A, "stroke-width"), r.attr(A, "transform"), r.steps()), (Some("1".into()), None, vec!["Scale".to_owned()]));
    // Its right side: across only.
    r.drag(r.at(22.0, 10.0), r.at(13.0, 14.0));
    assert_eq!((r.attr(A, "width"), r.attr(A, "height"), r.attr(A, "stroke-width")), (Some("9".into()), Some("12".into()), Some("1".into())));
    // With "Scale strokes" on, the line grows with an even scale.
    r.ink.settings.scale_strokes = true;
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.at(13.0, 16.0), r.at(22.0, 28.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!((r.attr(A, "width"), r.attr(A, "height"), r.attr(A, "stroke-width")), (Some("18".into()), Some("24".into()), Some("2".into())));
    assert_eq!(r.steps().len(), 3);
}

#[test]
fn outside_a_corner_turns() {
    let mut r = scene("pointer-turn");
    r.click(r.at(10.0, 8.0));
    // From just outside its top right corner, a quarter of the way
    // round its middle (10, 8).
    r.drag(r.at(16.8, 3.2), r.at(14.8, 14.8));
    assert_eq!((r.attr(A, "x"), r.attr(A, "y"), r.attr(A, "width"), r.attr(A, "height"), r.attr(A, "transform")), (Some("6".into()), Some("2".into()), Some("8".into()), Some("12".into()), None));
    assert_eq!(r.steps(), ["Rotate"]);
}

#[test]
fn the_arrow_keys_nudge() {
    let mut r = scene("pointer-nudge");
    // Nothing picked, nothing moves.
    r.key(Key::ArrowRight, Modifiers::NONE);
    assert!(r.steps().is_empty());
    r.click(r.at(10.0, 8.0));
    r.key(Key::ArrowRight, Modifiers::NONE);
    r.key(Key::ArrowDown, Modifiers::SHIFT);
    r.key(Key::ArrowUp, Modifiers::NONE);
    r.key(Key::ArrowLeft, Modifiers::SHIFT);
    assert_eq!((r.attr(A, "x"), r.attr(A, "y"), r.steps()), (Some("-5".into()), Some("13".into()), vec!["Nudge".to_owned(); 4]));
}
