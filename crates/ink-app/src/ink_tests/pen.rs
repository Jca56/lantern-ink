//! The Pen: a path drawn point by point (a click a corner, a press
//! dragged on a point with its handles pulled out), closed on its first
//! point, gone on with from a loose end, backed up, and let go of; with
//! the Node tool's drags under it all the while.

use ink_core::NodeId;
use ink_doc::outline::AnchorId;

use super::*;

/// A page with one old line on it (anchors 1 and 2).
const PAGE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <path id=\"old\" d=\"M30 40 H40\" fill=\"none\" stroke=\"#000\"/>\n</svg>\n";
const OLD: NodeId = NodeId(2);
const NEW: NodeId = NodeId(3);

fn page(name: &str) -> Running {
    let path = scratch(name).join("page.svg");
    std::fs::write(&path, PAGE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r.key(Key::Char('p'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Pen);
    r
}

fn a(n: u64) -> AnchorId {
    AnchorId(n)
}

#[test]
fn the_pen_draws_a_path_point_by_point() {
    let mut r = page("pen-draws");
    let doc = r.doc();
    // The first point is the Pen's own: nothing is in the drawing yet.
    r.click(r.spot(4.0, 4.0));
    assert_eq!((r.ink.penning.start(), r.steps().len(), r.svg().as_str()), (Some(Vec2::new(4.0, 4.0)), 0, PAGE));
    // The second makes the path: on the grid, painted as the last
    // shape was, on top, selected, its end the anchor to go on from.
    r.click(r.spot(20.2, 4.2));
    assert_eq!(r.ink.core.doc(doc).unwrap().markup(NEW).unwrap(), "<path d=\"M4 4 H20\" fill=\"#f3b700\"/>");
    assert_eq!((r.steps(), r.selected(), r.anchors(), r.ink.penning.start()), (vec!["Pen".to_owned()], vec![NEW], vec![(NEW, a(4))], None));
    // A click is a corner.
    r.click(r.spot(20.0, 16.0));
    assert_eq!((r.says(NEW, "d").as_deref(), r.anchors()), (Some("M4 4 H20 V16"), vec![(NEW, a(5))]));
    // A press dragged on pulls the new point's handles out, one each
    // way: shown as it goes, the drawing untouched till it's let go.
    r.drag_to(r.spot(10.0, 20.0), r.spot(14.0, 20.0));
    assert!(r.ink.core.gesturing(doc) && r.steps().len() == 2 && r.says(NEW, "d").as_deref() == Some("M4 4 H20 V16"));
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("d=\"M4 4 H20 V16 C20 16 6 20 10 20\""));
    r.let_go();
    assert_eq!((r.says(NEW, "d").as_deref(), r.steps().len()), (Some("M4 4 H20 V16 C20 16 6 20 10 20"), 3));
    // The handle pulled out the other way waits for the next segment.
    r.click(r.spot(4.0, 12.0));
    assert_eq!(r.says(NEW, "d").as_deref(), Some("M4 4 H20 V16 C20 16 6 20 10 20 C14 20 4 12 4 12"));
    // A second press on the same spot is no second point.
    r.click(r.spot(4.0, 12.0));
    assert_eq!(r.steps().len(), 4);
    // A press on the first point closes the path, and lets go of it.
    r.click(r.spot(4.0, 4.0));
    assert_eq!((r.says(NEW, "d").as_deref(), r.steps().last().map(String::as_str), r.anchors()), (Some("M4 4 H20 V16 C20 16 6 20 10 20 C14 20 4 12 4 12 Z"), Some("Close Path"), vec![]));
    // So the next press begins another; Escape forgets a first point.
    r.click(r.spot(30.0, 10.0));
    assert_eq!((r.ink.penning.start(), r.steps().len()), (Some(Vec2::new(30.0, 10.0)), 5));
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!((r.ink.penning.start(), r.selected()), (None, vec![NEW]));
    r.undo(5);
    assert_eq!(r.svg(), PAGE);
}

#[test]
fn a_first_point_dragged_starts_the_path_with_a_curve() {
    let mut r = page("pen-curve");
    // The first point's handle is kept till there's a segment for it;
    // with Shift it's held to 45°, and with Ctrl a point goes anywhere.
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.spot(4.0, 10.0), r.spot(8.0, 10.5));
    r.h.set_mods(Modifiers::NONE);
    assert!(r.ink.penning.start().is_some() && r.steps().is_empty());
    r.h.set_mods(Modifiers::CTRL);
    r.click(r.spot(16.5, 10.25));
    r.h.set_mods(Modifiers::NONE);
    let d = r.says(NEW, "d").unwrap();
    let numbers: Vec<f64> = d.split(|c: char| c == ' ' || c.is_ascii_alphabetic()).filter(|t| !t.is_empty()).map(|t| t.parse().unwrap()).collect();
    assert!(d.starts_with("M4 10 C") && (numbers[2] - 8.031).abs() < 0.01 && numbers[3] == 10.0 && numbers[4..] == [16.5, 10.25, 16.5, 10.25], "{d}");
    // Closing with a curve: the first point's handle, turned round, is
    // what the last segment comes home on.
    r.click(r.spot(16.0, 20.0));
    r.click(r.spot(4.0, 10.0));
    let d = r.says(NEW, "d").unwrap();
    assert!(d.ends_with(" C16 20 -0.031 10 4 10 Z"), "{d}");
    assert_eq!(r.steps(), ["Pen", "Pen", "Close Path"]);
}

#[test]
fn the_pen_goes_on_from_a_loose_end_and_backs_up() {
    let mut r = page("pen-ends");
    let doc = r.doc();
    // A path that's selected shows its anchors under the Pen; a click
    // on a loose end picks it, and the next press goes on from there.
    let sel = &mut r.ink.tabs.active_mut().unwrap().selection;
    (sel.active, sel.nodes) = (Some(OLD), vec![OLD]);
    r.frames(2);
    r.click(r.spot(40.0, 40.0));
    assert_eq!((r.anchors(), r.steps().len()), (vec![(OLD, a(2))], 0));
    r.click(r.spot(40.0, 30.0));
    assert_eq!((r.says(OLD, "d").as_deref(), r.steps(), r.anchors()), (Some("M30 40 H40 V30"), vec!["Pen".to_owned()], vec![(OLD, a(3))]));
    // From its first anchor it goes on backwards. (Let go of first: a
    // press on the other end of the path in hand would close it.)
    r.key(Key::Enter, Modifiers::NONE);
    r.click(r.spot(30.0, 40.0));
    assert_eq!(r.anchors(), [(OLD, a(1))]);
    r.click(r.spot(30.0, 30.0));
    assert_eq!((r.says(OLD, "d").as_deref(), r.anchors()), (Some("M30 30 V40 H40 V30"), vec![(OLD, a(4))]));
    // Delete takes the end back off, and the one before is the end.
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!((r.says(OLD, "d").as_deref(), r.anchors(), r.steps().last().map(String::as_str)), (Some("M30 40 H40 V30"), vec![(OLD, a(1))], Some("Delete Anchor")));
    r.click(r.spot(26.0, 36.0));
    assert_eq!(r.says(OLD, "d").as_deref(), Some("M26 36 L30 40 H40 V30"));
    // The Node tool's drags work under the Pen: a segment bent, an
    // anchor moved.
    r.drag(r.spot(35.0, 40.0), r.spot(35.0, 43.0));
    assert_eq!((r.says(OLD, "d").as_deref(), r.steps().last().map(String::as_str)), (Some("M26 36 L30 40 C30 44 40 44 40 40 V30"), Some("Bend")));
    r.drag(r.spot(40.0, 40.0), r.spot(42.0, 42.0));
    assert_eq!((r.says(OLD, "d").as_deref(), r.steps().last().map(String::as_str), r.anchors()), (Some("M26 36 L30 40 C30 44 42 46 42 42 L40 30"), Some("Move Anchor"), vec![(OLD, a(2))]));
    // A click on an end picks it to go on from; a point given up
    // mid-press was never placed.
    r.click(r.spot(40.0, 30.0));
    assert_eq!(r.anchors(), [(OLD, a(3))]);
    r.drag_to(r.spot(44.0, 20.0), r.spot(46.0, 18.0));
    assert!(r.ink.core.gesturing(doc));
    r.key(Key::Escape, Modifiers::NONE);
    r.let_go();
    assert_eq!((r.steps().len(), r.ink.core.gesturing(doc), r.anchors()), (6, false, vec![(OLD, a(3))]));
    // Enter lets go of the path: the next press is a new one's first.
    r.key(Key::Enter, Modifiers::NONE);
    assert!(r.anchors().is_empty());
    r.click(r.spot(10.0, 10.0));
    assert_eq!((r.ink.penning.start(), r.steps().len()), (Some(Vec2::new(10.0, 10.0)), 6));
    // And Delete forgets that.
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!((r.ink.penning.start(), r.steps().len()), (None, 6));
    r.undo(6);
    assert_eq!(r.svg(), PAGE);
}
