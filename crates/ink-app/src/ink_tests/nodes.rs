//! The Node tool on the canvas: shapes picked by themselves, anchors
//! picked, moved, marqueed, nudged and taken out, handles and segments
//! dragged, each drag a gesture that lands as one step; and a shape
//! that isn't a path yet made one by the first change to its anchors.

use ink_core::NodeId;
use ink_doc::outline::AnchorId;

use super::*;

/// A hill (anchors 1 to 3: a hump from 4,20 to 20,20, then a line down
/// to 20,30), a card, a moved group with a box in it (anchors 4 to 7),
/// and a locked corner.
const SCENE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 48 48\">\n  <path id=\"hill\" d=\"M4 20 C4 8 20 8 20 20 L20 30\" fill=\"none\" stroke=\"#000\"/>\n  <rect id=\"card\" x=\"28\" y=\"4\" width=\"14\" height=\"10\" fill=\"#08f\"/>\n  <g id=\"g\" transform=\"translate(4 34)\">\n    <path id=\"in\" d=\"M0 0 H10 V8 H0 Z\" fill=\"#fc0\"/>\n  </g>\n  <path id=\"held\" d=\"M30 30 H44 V44\" fill=\"none\" stroke=\"#c00\" stroke-width=\"2\" ink:locked=\"true\"/>\n</svg>\n";
const HILL: NodeId = NodeId(2);
const CARD: NodeId = NodeId(3);
const G: NodeId = NodeId(4);
const IN: NodeId = NodeId(5);

fn scene(name: &str) -> Running {
    let path = scratch(name).join("scene.svg");
    std::fs::write(&path, SCENE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r.key(Key::Char('a'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Node);
    r
}

fn a(n: u64) -> AnchorId {
    AnchorId(n)
}

impl Running {
    /// The anchors the Node tool has picked.
    fn anchors(&self) -> Vec<(NodeId, AnchorId)> {
        self.ink.noding.picked().to_vec()
    }

    /// Press at `from`, go to `to`, and stay there with the button down.
    fn drag_to(&mut self, from: Vec2, to: Vec2) {
        self.h.advance(1.0);
        self.h.move_to(from);
        self.frames(1);
        self.h.press();
        self.frames(1);
        self.h.move_to(from + (to - from) * 0.5);
        self.frames(1);
        self.h.move_to(to);
        self.frames(2);
    }

    fn let_go(&mut self) {
        self.h.release();
        self.frames(2);
    }

    /// The hill picked, by a click on its line.
    fn hill(&mut self) {
        self.click(self.spot(20.0, 25.0));
        assert_eq!(self.selected(), [HILL]);
    }

    fn undo(&mut self, steps: usize) {
        for _ in 0..steps {
            self.key(Key::Char('z'), Modifiers::CTRL);
        }
    }
}

#[test]
fn the_node_tool_picks_shapes_and_their_anchors() {
    let mut r = scene("nodes-pick");
    // A click on a shape picks it; then its anchors can be.
    r.hill();
    assert!(r.anchors().is_empty());
    r.click(r.spot(20.0, 20.0));
    assert_eq!(r.anchors(), [(HILL, a(2))]);
    // Shift adds one, and lets one go.
    r.click_with(r.spot(20.0, 30.0), Modifiers::SHIFT);
    assert_eq!(r.anchors(), [(HILL, a(2)), (HILL, a(3))]);
    r.click_with(r.spot(20.0, 20.0), Modifiers::SHIFT);
    assert_eq!(r.anchors(), [(HILL, a(3))]);
    // A plain click on one of several makes it the only one.
    r.click_with(r.spot(4.0, 20.0), Modifiers::SHIFT);
    r.click(r.spot(20.0, 30.0));
    assert_eq!(r.anchors(), [(HILL, a(3))]);
    // A click on a segment picks its two ends.
    r.click(r.spot(20.0, 25.0));
    assert_eq!(r.anchors(), [(HILL, a(2)), (HILL, a(3))]);
    // A drag on nothing is a marquee over anchors (kept, with Shift).
    r.drag(r.spot(1.0, 17.0), r.spot(7.0, 23.0));
    assert_eq!(r.anchors(), [(HILL, a(1))]);
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.spot(23.0, 27.0), r.spot(17.0, 33.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!((r.anchors(), r.selected()), (vec![(HILL, a(1)), (HILL, a(3))], vec![HILL]));
    // A shape in a group is picked itself, and the Pointer is at its
    // level; the anchors of the last one are let go.
    r.click(r.spot(9.0, 38.0));
    assert_eq!((r.selected(), r.ink.tabs.active().unwrap().selection.within, r.anchors()), (vec![IN], Some(G), vec![]));
    r.click(r.spot(14.0, 42.0));
    assert_eq!(r.anchors(), [(IN, a(6))]);
    // Escape lets go of the anchors, then of the shape.
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!((r.anchors(), r.selected()), (vec![], vec![IN]));
    // What's locked isn't picked: a click there is a click on nothing,
    // which lets go of everything.
    r.click(r.spot(37.0, 30.0));
    assert!(r.selected().is_empty() && r.steps().is_empty() && r.svg() == SCENE);
}

#[test]
fn anchors_drag_onto_whole_units_and_land_as_one_step() {
    let mut r = scene("nodes-move");
    let doc = r.doc();
    r.hill();
    // Shown as it goes, with the drawing untouched; the anchor's handle
    // goes with it.
    r.drag_to(r.spot(20.0, 20.0), r.spot(23.4, 21.7));
    assert!(r.ink.core.gesturing(doc) && r.steps().is_empty() && r.svg() == SCENE);
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("d=\"M4 20 C4 8 23 10 23 22 L20 30\""));
    r.let_go();
    assert_eq!((r.says(HILL, "d").as_deref(), r.steps(), r.anchors()), (Some("M4 20 C4 8 23 10 23 22 L20 30"), vec!["Move Anchor".to_owned()], vec![(HILL, a(2))]));
    // With Ctrl, where the pointer is.
    r.h.set_mods(Modifiers::CTRL);
    r.drag(r.spot(20.0, 30.0), r.spot(20.5, 30.25));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.says(HILL, "d").as_deref(), Some("M4 20 C4 8 23 10 23 22 L20.5 30.25"));
    // Every anchor picked goes along with the one dragged.
    r.click(r.spot(4.0, 20.0));
    r.click_with(r.spot(23.0, 22.0), Modifiers::SHIFT);
    r.drag(r.spot(23.0, 22.0), r.spot(23.0, 19.8));
    assert_eq!((r.says(HILL, "d").as_deref(), r.steps().last().map(String::as_str)), (Some("M4 18 C4 6 23 8 23 20 L20.5 30.25"), Some("Move Anchors")));
    // Gone nowhere, nothing was done; nor by a drag given up.
    r.drag(r.spot(23.0, 20.0), r.spot(23.3, 20.4));
    assert_eq!(r.steps().len(), 3);
    r.drag_to(r.spot(23.0, 20.0), r.spot(30.0, 26.0));
    assert!(r.ink.core.gesturing(doc));
    r.key(Key::Escape, Modifiers::NONE);
    r.let_go();
    assert_eq!((r.steps().len(), r.ink.core.gesturing(doc), r.anchors().len()), (3, false, 2));
    // The arrows move the anchors picked, not the shape; Delete takes
    // them out, and the path is joined across.
    r.click(r.spot(23.0, 20.0));
    r.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!((r.says(HILL, "d").as_deref(), r.steps().last().map(String::as_str)), (Some("M4 18 C4 6 24 8 24 20 L20.5 30.25"), Some("Nudge")));
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!((r.says(HILL, "d").as_deref(), r.steps().last().map(String::as_str), r.anchors()), (Some("M4 18 C4 6 20.5 30.25 20.5 30.25"), Some("Delete Anchor"), vec![]));
    r.undo(5);
    assert_eq!(r.svg(), SCENE);
}

#[test]
fn a_handle_and_a_segment_are_dragged() {
    let mut r = scene("nodes-bend");
    r.hill();
    // A picked anchor shows its handles: one dragged is that control
    // point moved (the line on its other side has none to follow).
    r.click(r.spot(20.0, 20.0));
    r.drag(r.spot(20.0, 8.0), r.spot(22.0, 6.0));
    assert_eq!((r.says(HILL, "d").as_deref(), r.steps()), (Some("M4 20 C4 8 22 6 20 20 V30"), vec!["Handle".to_owned()]));
    // A handle isn't there to take until its anchor is picked: a drag
    // from where the other anchor's is draws a marquee.
    r.drag(r.spot(4.0, 8.0), r.spot(6.0, 6.0));
    assert_eq!((r.steps().len(), r.anchors()), (1, vec![]));
    // A segment dragged bends: the point taken follows the pointer.
    r.drag(r.spot(20.0, 25.0), r.spot(23.0, 25.0));
    assert_eq!((r.says(HILL, "d").as_deref(), r.steps().last().map(String::as_str)), (Some("M4 20 C4 8 22 6 20 20 C24 20 24 30 20 30"), Some("Bend")));
    // Now the anchor between is smooth enough to try: its handles are
    // up and across, a corner, so each goes alone.
    r.click(r.spot(20.0, 20.0));
    r.drag(r.spot(24.0, 20.0), r.spot(24.0, 22.0));
    assert_eq!(r.says(HILL, "d").as_deref(), Some("M4 20 C4 8 22 6 20 20 C24 22 24 30 20 30"));
    r.undo(3);
    assert_eq!(r.svg(), SCENE);
}

#[test]
fn a_shape_becomes_a_path_when_its_anchors_are_first_changed() {
    let mut r = scene("nodes-shape");
    let name = |r: &Running| r.ink.core.doc(r.doc()).unwrap().node(CARD).map(|n| n.name.clone()).ok();
    // A rectangle shows the corners it would have as a path, and
    // picking one changes nothing.
    r.click(r.spot(35.0, 9.0));
    r.click(r.spot(28.0, 4.0));
    assert_eq!((r.selected(), r.anchors().len(), name(&r).as_deref(), r.steps().len()), (vec![CARD], 1, Some("rect"), 0));
    assert_eq!(r.svg(), SCENE);
    // Moved, it's a path: the same element, with everything else it
    // had; one step, and undone it's a rectangle again.
    r.drag(r.spot(28.0, 4.0), r.spot(26.0, 2.0));
    assert_eq!((name(&r).as_deref(), r.says(CARD, "d").as_deref(), r.says(CARD, "fill").as_deref(), r.says(CARD, "id").as_deref()), (Some("path"), Some("M26 2 L42 4 V14 H28 Z"), Some("#08f"), Some("card")));
    assert_eq!((r.steps(), r.anchors().len()), (vec!["Move Anchor".to_owned()], 1));
    r.undo(1);
    assert_eq!((name(&r).as_deref(), r.svg().as_str()), (Some("rect"), SCENE));
    // So does a side bent.
    r.drag(r.spot(35.0, 4.0), r.spot(35.0, 1.0));
    assert_eq!((name(&r).as_deref(), r.says(CARD, "d").as_deref(), r.steps()), (Some("path"), Some("M28 4 C28 0 42 0 42 4 V14 H28 Z"), vec!["Bend".to_owned()]));
    r.undo(1);
    // And two corners taken out: what's left is joined across.
    r.click(r.spot(28.0, 4.0));
    r.click_with(r.spot(42.0, 4.0), Modifiers::SHIFT);
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!((r.says(CARD, "d").as_deref(), r.steps(), r.selected()), (Some("M42 14 H28 Z"), vec!["Delete Anchors".to_owned()], vec![CARD]));
    // A shape left with no anchors goes altogether.
    r.undo(1);
    r.drag(r.spot(26.0, 2.0), r.spot(44.0, 16.0));
    assert_eq!(r.anchors().len(), 4);
    r.key(Key::Delete, Modifiers::NONE);
    assert!(name(&r).is_none() && r.selected().is_empty() && r.steps() == ["Delete Anchors"]);
    r.undo(1);
    assert_eq!(r.svg(), SCENE);
}
