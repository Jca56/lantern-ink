//! Paths by their anchors (ARCHITECTURE §3.4, D18): what each edit
//! leaves in the file, and which anchor is which after it.

use ink_doc::outline::AnchorId;
use ink_doc::pathedit::{Along, PathEdit};
use ink_doc::paths::{NewAnchor, NewRun};
use ink_doc::{Command, DocError, DocId, Document, NodeId};
use ink_geom::Vec2;

const N: NodeId = NodeId(2);

fn doc(shape: &str) -> Document {
    Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 24 24\">{shape}</svg>")).unwrap()
}

fn path(d: &str) -> Document {
    doc(&format!("<path d=\"{d}\"/>"))
}

fn a(n: u64) -> AnchorId {
    AnchorId(n)
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}

/// The path's data and its anchors' ids, run by run.
fn state(d: &Document) -> (String, Vec<Vec<u64>>) {
    let ids = d.outline(N).unwrap().runs.iter().map(|run| run.anchors.iter().map(|a| a.id.0).collect()).collect();
    (d.node(N).unwrap().attr("d").unwrap().to_owned(), ids)
}

/// The same after `edits` on the path `d`, and the anchors they made.
fn edited(d: &str, edits: Vec<PathEdit>) -> (String, Vec<Vec<u64>>, Vec<u64>) {
    let mut doc = path(d);
    let applied = doc.apply(&Command::EditPath { node: N, edits }).unwrap();
    let (data, ids) = state(&doc);
    (data, ids, applied.anchors.iter().map(|a| a.0).collect())
}

fn data(d: &str, edits: Vec<PathEdit>) -> String {
    edited(d, edits).0
}

fn refused(d: &str, edits: Vec<PathEdit>) -> String {
    let mut doc = path(d);
    let before = doc.to_svg();
    let why = match doc.apply(&Command::EditPath { node: N, edits }) {
        Err(DocError::Invalid(why)) => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(doc.to_svg(), before, "a refused edit changes nothing");
    why
}

const SQUARE: &str = "M0 0 H10 V10 H0 Z";

#[test]
fn anchors_move_and_their_handles_go_with_them() {
    assert_eq!(data(SQUARE, vec![PathEdit::Move { anchors: vec![a(2), a(3)], by: v(2.0, 0.0) }]), "M0 0 H12 V10 H0 Z");
    // A curve's control point belongs to the anchor it's a handle of; a
    // quadratic's one point, between two anchors, goes half as far.
    assert_eq!(data("M0 0 C0 5 5 10 10 10", vec![PathEdit::Move { anchors: vec![a(2)], by: v(1.0, 1.0) }]), "M0 0 C0 5 6 11 11 11");
    assert_eq!(data("M0 0 Q5 10 10 0", vec![PathEdit::Move { anchors: vec![a(2)], by: v(2.0, 0.0) }]), "M0 0 Q6 10 12 0");
    assert_eq!(data("M0 0 Q5 10 10 0", vec![PathEdit::Move { anchors: vec![a(1), a(2)], by: v(2.0, 0.0) }]), "M2 0 Q7 10 12 0");
    // A closed curve's start is its last segment's end.
    assert_eq!(data("M0 0 C0 5 5 10 10 10 C5 10 0 5 0 0 Z", vec![PathEdit::Move { anchors: vec![a(1)], by: v(0.0, -2.0) }]), "M0 -2 C0 3 5 10 10 10 C5 10 0 3 0 -2 Z");
    assert_eq!(refused(SQUARE, vec![PathEdit::Move { anchors: vec![a(9)], by: v(1.0, 0.0) }]), "this path has no anchor A9 (node_info lists the ones it has)");
}

#[test]
fn handles_are_given_and_taken_off() {
    let line = "M0 0 L10 0";
    let out = PathEdit::Handles { anchor: a(1), into: None, out: Some(Some(v(0.0, 5.0))) };
    let into = PathEdit::Handles { anchor: a(2), into: Some(Some(v(0.0, 5.0))), out: None };
    assert_eq!(data(line, vec![out.clone()]), "M0 0 C0 5 10 0 10 0", "a line becomes the curve that handle makes of it");
    assert_eq!(data(line, vec![out.clone(), into]), "M0 0 C0 5 10 5 10 0");
    // Both off again: a line again.
    let off = |anchor: u64| PathEdit::Handles { anchor: a(anchor), into: Some(None), out: Some(None) };
    assert_eq!(data("M0 0 C0 5 10 5 10 0 L20 0", vec![off(2)]), "M0 0 C0 5 10 0 10 0 H20");
    assert_eq!(data("M0 0 C0 5 10 5 10 0 L20 0", vec![PathEdit::Handles { anchor: a(1), into: None, out: Some(None) }, PathEdit::Handles { anchor: a(2), into: Some(None), out: None }]), "M0 0 H10 H20");
    // A quadratic given a handle of its own for one end is a cubic.
    assert_eq!(data("M0 0 Q6 9 12 0", vec![out]), "M0 0 C0 5 8 6 12 0");
    assert_eq!(refused(line, vec![PathEdit::Handles { anchor: a(2), into: None, out: Some(Some(v(1.0, 1.0))) }]), "A2 is the end of an open run: nothing goes out of it to have a handle");
    assert_eq!(refused("M0 0 A5 5 0 0 1 10 0", vec![PathEdit::Handles { anchor: a(1), into: None, out: Some(Some(v(1.0, 1.0))) }]), "what goes out of A1 is an arc, which has radii, not handles: bend it, or make it a line first");
}

#[test]
fn an_anchor_put_in_has_an_id_of_its_own_and_changes_no_other() {
    let (d, ids, made) = edited(SQUARE, vec![PathEdit::Add { after: a(2), at: Along::Share(0.5) }]);
    assert_eq!((d.as_str(), ids, made), ("M0 0 H10 V5 V10 H0 Z", vec![vec![1, 2, 5, 3, 4]], vec![5]));
    // On the side that closes it; and nearest a point.
    let (d, ids, _) = edited(SQUARE, vec![PathEdit::Add { after: a(4), at: Along::Share(0.25) }]);
    assert_eq!((d.as_str(), ids), ("M0 0 H10 V10 H0 V7.5 Z", vec![vec![1, 2, 3, 4, 5]]));
    assert_eq!(data(SQUARE, vec![PathEdit::Add { after: a(1), at: Along::Nearest(v(3.0, -4.0)) }]), "M0 0 H3 H10 V10 H0 Z");
    // A curve keeps its shape: each half is the curve it was there.
    assert_eq!(data("M0 0 C0 8 8 8 8 0", vec![PathEdit::Add { after: a(1), at: Along::Share(0.5) }]), "M0 0 C0 4 2 6 4 6 C6 6 8 4 8 0");
    assert_eq!(data("M5 0 A5 5 0 0 1 -5 0", vec![PathEdit::Add { after: a(1), at: Along::Share(0.5) }]), "M5 0 A5 5 0 0 1 0 5 A5 5 0 0 1 -5 0");
    // Two in one go: the second is told by the first's place.
    let (d, ids, made) = edited("M0 0 H8", vec![PathEdit::Add { after: a(1), at: Along::Share(0.5) }, PathEdit::Add { after: a(1), at: Along::Share(0.5) }]);
    assert_eq!((d.as_str(), ids, made), ("M0 0 H2 H4 H8", vec![vec![1, 4, 3, 2]], vec![3, 4]));
    assert_eq!(refused("M0 0 H8", vec![PathEdit::Add { after: a(2), at: Along::Share(0.5) }]), "A2 is the last anchor of an open run: no segment comes after it to put one on");
    assert!(refused("M0 0 H8", vec![PathEdit::Add { after: a(1), at: Along::Share(1.0) }]).starts_with("an anchor goes between a segment's ends"));
}

#[test]
fn an_anchor_taken_out_leaves_its_neighbours_joined() {
    let (d, ids, _) = edited(SQUARE, vec![PathEdit::Delete { anchors: vec![a(2)] }]);
    assert_eq!((d.as_str(), ids), ("M0 0 L10 10 H0 Z", vec![vec![1, 3, 4]]));
    assert_eq!(edited(SQUARE, vec![PathEdit::Delete { anchors: vec![a(1)] }]).1, [vec![2, 3, 4]], "the first of a closed run: the next is first now");
    assert_eq!(data(SQUARE, vec![PathEdit::Delete { anchors: vec![a(1)] }]), "M10 0 V10 H0 Z");
    // The ends of an open run just go.
    assert_eq!(data("M0 0 H10 V10 H0", vec![PathEdit::Delete { anchors: vec![a(1), a(4)] }]), "M10 0 V10");
    // Curves are joined by a curve with the handles that are left.
    assert_eq!(data("M0 0 C0 4 2 6 4 6 C6 6 8 4 8 0", vec![PathEdit::Delete { anchors: vec![a(2)] }]), "M0 0 C0 4 8 4 8 0");
    // The last anchor of a run takes the run with it.
    let (d, ids, _) = edited("M0 0 H4 M9 9", vec![PathEdit::Delete { anchors: vec![a(3)] }]);
    assert_eq!((d.as_str(), ids), ("M0 0 H4", vec![vec![1, 2]]));
    // One put in and taken out again was never made.
    let (d, _, made) = edited("M0 0 H8", vec![PathEdit::Add { after: a(1), at: Along::Share(0.5) }, PathEdit::Delete { anchors: vec![a(3)] }]);
    assert_eq!((d.as_str(), made), ("M0 0 H8", vec![]));
}

#[test]
fn a_segment_bends_through_a_point_and_straightens_again() {
    // A line becomes the simplest curve that goes there: one control
    // point.
    assert_eq!(data("M0 0 H10", vec![PathEdit::Bend { after: a(1), through: v(5.0, 5.0) }]), "M0 0 Q5 10 10 0");
    assert_eq!(data("M0 0 Q5 10 10 0", vec![PathEdit::Bend { after: a(1), through: v(5.0, -1.0) }]), "M0 0 Q5 -2 10 0");
    // A cubic keeps its handles' lean and moves them both.
    assert_eq!(data("M0 0 C0 8 8 8 8 0", vec![PathEdit::Bend { after: a(1), through: v(4.0, 9.0) }]), "M0 0 C0 12 8 12 8 0");
    // An arc becomes the arc of the circle through all three.
    assert_eq!(data("M0 0 A5 5 0 0 1 10 0", vec![PathEdit::Bend { after: a(1), through: v(5.0, -2.0) }]), "M0 0 A7.25 7.25 0 0 1 10 0");
    assert_eq!(data("M0 0 A5 5 0 0 1 10 0", vec![PathEdit::Bend { after: a(1), through: v(5.0, 8.0) }]), "M0 0 A5.5625 5.5625 0 1 0 10 0", "the long way round, the other way; its radius as finely as keeps it there");
    assert_eq!(refused("M0 0 A5 5 0 0 1 10 0", vec![PathEdit::Bend { after: a(1), through: v(5.0, 0.0) }]), "those three points are in a line, and no arc goes through them: straighten the segment instead");
    assert_eq!(data("M0 0 Q5 10 10 0 V8", vec![PathEdit::Straighten { after: a(1) }]), "M0 0 H10 V8");
    assert_eq!(refused("M0 0 H10", vec![PathEdit::Bend { after: a(2), through: v(1.0, 1.0) }]), "A2 is the last anchor of an open run: no segment comes after it to bend");
}

#[test]
fn a_segment_is_pulled_by_any_point_of_it() {
    // A line taken by its middle: the cubic whose middle is there, its
    // two control points pulled alike.
    assert_eq!(data("M0 0 H12", vec![PathEdit::Pull { after: a(1), share: 0.5, to: v(6.0, 3.0) }]), "M0 0 C0 4 12 4 12 0");
    // Taken nearer one end, the point taken is what goes there, and the
    // nearer control point goes further.
    for (d, share, to) in [("M0 0 H12", 0.25, v(3.0, 2.0)), ("M0 0 C0 8 8 8 8 0", 0.8, v(9.0, 1.0)), ("M0 0 Q5 10 10 0", 0.3, v(2.0, 6.0)), ("M0 0 A5 5 0 0 1 10 0", 0.5, v(4.0, -3.0))] {
        let mut doc = path(d);
        doc.apply(&Command::EditPath { node: N, edits: vec![PathEdit::Pull { after: a(1), share, to }] }).unwrap();
        let piece = doc.outline(N).unwrap().runs[0].piece(0).unwrap();
        let nearest = piece.at(piece.nearest(to));
        assert!(nearest.distance(to) < 0.002, "{d}: {nearest:?}");
        // (A curve's own count along it is the one it was taken by.)
        if d.contains('C') || d.contains('Q') {
            assert!(piece.at(share).distance(to) < 0.002, "{d}: the point taken");
        }
    }
    let (near, _) = state(&{
        let mut doc = path("M0 0 H12");
        doc.apply(&Command::EditPath { node: N, edits: vec![PathEdit::Pull { after: a(1), share: 0.25, to: v(3.0, 2.0) }] }).unwrap();
        doc
    });
    assert_eq!(near, "M0 0 C0 3.646 12 1.766 12 0");
    // Each kind stays what it can: a quadratic moves its one control
    // point, an arc is the arc of a circle through the point.
    assert_eq!(data("M0 0 Q5 10 10 0", vec![PathEdit::Pull { after: a(1), share: 0.5, to: v(5.0, -1.0) }]), "M0 0 Q5 -2 10 0");
    assert_eq!(data("M0 0 A5 5 0 0 1 10 0", vec![PathEdit::Pull { after: a(1), share: 0.2, to: v(5.0, -2.0) }]), "M0 0 A7.25 7.25 0 0 1 10 0");
    // Pulled nowhere, a line is a line still.
    assert_eq!(data("M0 0 H12 V4", vec![PathEdit::Pull { after: a(1), share: 0.3, to: v(3.6, 0.0) }, PathEdit::Pull { after: a(2), share: 0.5, to: v(13.0, 2.0) }]), "M0 0 H12 C13.333 0 13.333 4 12 4");
    assert_eq!(refused("M0 0 H10", vec![PathEdit::Pull { after: a(1), share: 1.0, to: v(1.0, 1.0) }]), "a segment is pulled by a point between its ends: 1 of the way along isn't (give more than 0 and less than 1)");
    assert_eq!(refused("M0 0 H10", vec![PathEdit::Pull { after: a(2), share: 0.5, to: v(1.0, 1.0) }]), "A2 is the last anchor of an open run: no segment comes after it to pull");
}

#[test]
fn a_path_goes_on_from_a_loose_end() {
    let on = |to: Vec2, out: Option<Vec2>, into: Option<Vec2>, from: u64| PathEdit::Extend { from: a(from), to, out, into };
    // With a line; and the new anchor has an id of its own, and is the
    // run's end now.
    let (d, ids, made) = edited("M0 0 H8", vec![on(v(8.0, 6.0), None, None, 2)]);
    assert_eq!((d.as_str(), ids, made), ("M0 0 H8 V6", vec![vec![1, 2, 3]], vec![3]));
    // With a curve, where either end has a handle on it.
    assert_eq!(data("M0 0 H8", vec![on(v(16.0, 8.0), Some(v(4.0, 0.0)), Some(v(0.0, -4.0)), 2)]), "M0 0 H8 C12 0 16 4 16 8");
    assert_eq!(data("M0 0 H8", vec![on(v(16.0, 8.0), None, Some(v(0.0, -4.0)), 2)]), "M0 0 H8 C8 0 16 4 16 8");
    // From its first anchor it goes on backwards: the new one is where
    // the run starts, and each handle is still its own anchor's.
    let (d, ids, _) = edited("M0 0 H8", vec![on(v(-6.0, 4.0), Some(v(-2.0, 0.0)), Some(v(0.0, -3.0)), 1)]);
    assert_eq!((d.as_str(), ids), ("M-6 4 C-6 1 -2 0 0 0 H8", vec![vec![3, 1, 2]]));
    // A point alone is both ends of its run; so one press after another
    // draws a path.
    let (d, ids, made) = edited("M4 4", vec![on(v(10.0, 4.0), None, None, 1), on(v(10.0, 10.0), None, None, 2)]);
    assert_eq!((d.as_str(), ids, made), ("M4 4 H10 V10", vec![vec![1, 2, 3]], vec![2, 3]));
    // Only from a loose end.
    assert_eq!(refused("M0 0 H8 V8", vec![on(v(1.0, 1.0), None, None, 2)]), "A2 isn't an end of an open run: a path goes on from a loose end");
    assert_eq!(refused(SQUARE, vec![on(v(1.0, 1.0), None, None, 1)]), "A1 isn't an end of an open run: a path goes on from a loose end");
}

#[test]
fn an_anchor_says_where_its_handles_are() {
    let handles = |d: &str, n: u64| path(d).outline(N).unwrap().handles(a(n));
    // A cubic's two control points are its ends' handles; a line has
    // none, nor has the end of an open run.
    let d = "M0 0 C0 4 12 4 12 0 L20 0";
    assert_eq!((handles(d, 1), handles(d, 2), handles(d, 3)), ((None, Some(v(0.0, 4.0))), (Some(v(12.0, 4.0)), None), (None, None)));
    // A quadratic's one control point stands for both ends.
    assert_eq!((handles("M0 0 Q6 9 12 0", 1), handles("M0 0 Q6 9 12 0", 2)), ((None, Some(v(4.0, 6.0))), (Some(v(8.0, 6.0)), None)));
    // Round a closed run, the first anchor has what comes home to it.
    let d = "M0 0 C3 -2 7 -2 10 0 L5 8 C3 8 0 3 0 0 Z";
    assert_eq!(handles(d, 1), (Some(v(0.0, 3.0)), Some(v(3.0, -2.0))));
    // An arc has radii, not handles; a handle on its anchor is none;
    // and no such anchor has none.
    assert_eq!((handles("M0 0 A5 5 0 0 1 10 0", 1), handles("M0 0 C0 0 8 4 10 0", 1), handles("M0 0 H4", 9)), ((None, None), (None, None), (None, None)));
}

#[test]
fn anchors_are_made_smooth_and_made_corners() {
    // Handles in line with the anchors either side, a third of the way
    // to each.
    let smooth = data("M0 0 L10 10 L20 0", vec![PathEdit::Smooth { anchors: vec![a(2)] }]);
    assert_eq!(smooth, "M0 0 C0 0 5.286 10 10 10 C14.714 10 20 0 20 0");
    assert_eq!(data(&smooth, vec![PathEdit::Corner { anchors: vec![a(2)] }]), "M0 0 L10 10 L20 0");
    // All of a closed run: a round thing through its corners.
    assert_eq!(data("M0 0 H10 V10 H0 Z", vec![PathEdit::Smooth { anchors: vec![a(1), a(2), a(3), a(4)] }]), "M0 0 C2.357 -2.357 7.643 -2.357 10 0 C12.357 2.357 12.357 7.643 10 10 C7.643 12.357 2.357 12.357 0 10 C-2.357 7.643 -2.357 2.357 0 0 Z");
    // An end leans the way its one neighbour is.
    assert_eq!(data("M0 0 L9 0 L9 9", vec![PathEdit::Smooth { anchors: vec![a(1)] }]), "M0 0 C3 0 9 0 9 0 V9");
}

#[test]
fn runs_are_closed_parted_joined_and_turned_round() {
    assert_eq!(data("M0 0 H10 V10", vec![PathEdit::Close { anchor: a(2) }]), "M0 0 H10 V10 Z");
    // Closed where it had been drawn back to its start: that last
    // anchor is the first.
    let (d, ids, _) = edited("M0 0 H10 V10 L0 0", vec![PathEdit::Close { anchor: a(1) }]);
    assert_eq!((d.as_str(), ids), ("M0 0 H10 V10 Z", vec![vec![1, 2, 3]]));
    // A closed run parted at an anchor opens there: it starts at that
    // anchor and ends at its twin.
    let (d, ids, made) = edited(SQUARE, vec![PathEdit::Break { at: a(3) }]);
    assert_eq!((d.as_str(), ids, made), ("M10 10 H0 V0 H10 V10", vec![vec![3, 4, 1, 2, 5]], vec![5]));
    // An open one becomes two.
    let (d, ids, made) = edited("M0 0 H10 V10 H0", vec![PathEdit::Break { at: a(2) }]);
    assert_eq!((d.as_str(), ids, made), ("M0 0 H10 M10 0 V10 H0", vec![vec![1, 2], vec![5, 3, 4]], vec![5]));
    assert_eq!(refused("M0 0 H10", vec![PathEdit::Break { at: a(1) }]), "A1 is an end of its run already: there's nothing to part there");
    // Two loose ends joined by a line, whichever ends they are; ends
    // lying on each other become one anchor.
    let two = "M0 0 H10 M20 0 H30";
    assert_eq!(edited(two, vec![PathEdit::Join { a: a(2), b: a(3) }]).0, "M0 0 H10 H20 H30");
    let (d, ids, _) = edited(two, vec![PathEdit::Join { a: a(1), b: a(4) }]);
    assert_eq!((d.as_str(), ids), ("M10 0 H0 H30 H20", vec![vec![2, 1, 4, 3]]));
    let (d, ids, _) = edited("M0 0 H10 M10 0 V10", vec![PathEdit::Join { a: a(2), b: a(3) }]);
    assert_eq!((d.as_str(), ids), ("M0 0 H10 V10", vec![vec![1, 2, 4]]));
    assert_eq!(data("M0 0 H10 V10", vec![PathEdit::Join { a: a(3), b: a(1) }]), "M0 0 H10 V10 Z", "a run's own two ends: it closes");
    assert_eq!(refused(SQUARE, vec![PathEdit::Join { a: a(1), b: a(3) }]), "A1 isn't an end of an open run: only two loose ends can be joined");
    // Turned round: the same line from its other end. A closed run
    // still starts where it did.
    let (d, ids, _) = edited("M0 0 C0 5 5 10 10 10", vec![PathEdit::Reverse { anchor: None }]);
    assert_eq!((d.as_str(), ids), ("M10 10 C5 10 0 5 0 0", vec![vec![2, 1]]));
    let (d, ids, _) = edited("M0 0 H10 V10 Z", vec![PathEdit::Reverse { anchor: None }]);
    assert_eq!((d.as_str(), ids), ("M0 0 L10 10 V0 Z", vec![vec![1, 3, 2]]));
    assert_eq!(data("M0 0 A5 5 0 0 1 10 0 M20 0 H30", vec![PathEdit::Reverse { anchor: Some(a(1)) }]), "M10 0 A5 5 0 0 0 0 0 M20 0 H30", "one run of two, its arc sweeping the other way");
}

#[test]
fn a_shape_is_made_a_path_that_draws_the_same() {
    let made = |shape: &str| {
        let mut d = doc(shape);
        let applied = d.apply(&Command::ToPath { nodes: vec![N] }).unwrap();
        assert_eq!(applied.changed, vec![N]);
        d.markup(N).unwrap()
    };
    // Its path data where its numbers were; all else as it was. A
    // rounded corner stays an arc.
    assert_eq!(made(r##"<rect id="pane" x="4" y="2" width="16" height="20" rx="3" fill="#ffc800"/>"##), r##"<path id="pane" d="M7 2 H17 A3 3 0 0 1 20 5 V19 A3 3 0 0 1 17 22 H7 A3 3 0 0 1 4 19 V5 A3 3 0 0 1 7 2 Z" fill="#ffc800"/>"##);
    assert_eq!(made(r#"<circle cx="12" cy="12" r="4" class="dot"/>"#), r#"<path d="M16 12 A4 4 0 0 1 12 16 A4 4 0 0 1 8 12 A4 4 0 0 1 12 8 A4 4 0 0 1 16 12 Z" class="dot"/>"#);
    assert_eq!(made(r#"<line stroke="red" x1="1" y1="2" x2="3" y2="4"/>"#), r#"<path stroke="red" d="M1 2 L3 4"/>"#);
    assert_eq!(made(r#"<polygon points="0,0 4,0 4,4"/>"#), r#"<path d="M0 0 H4 V4 Z"/>"#);
    assert_eq!(made(r#"<ellipse rx="3" ry="2"/>"#), r#"<path d="M3 0 A3 2 0 0 1 0 2 A3 2 0 0 1 -3 0 A3 2 0 0 1 0 -2 A3 2 0 0 1 3 0 Z"/>"#);
    // It has anchors now, and an edit that needs them makes it one
    // without being asked.
    let mut d = doc(r#"<rect width="8" height="4"/>"#);
    let applied = d.apply(&Command::EditPath { node: N, edits: vec![PathEdit::Move { anchors: vec![a(3)], by: v(2.0, 0.0) }] }).unwrap();
    assert_eq!((d.markup(N).unwrap().as_str(), applied.changed), (r#"<path d="M0 0 H8 L10 4 H0 Z"/>"#, vec![N]));
    // A path is one already; what has no outline can't be.
    let mut p = path("m0 0h4");
    assert!(p.apply(&Command::ToPath { nodes: vec![N] }).unwrap().is_nothing());
    for (shape, why) in [("<g/>", "N2 is a <g>: only a shape (a rect, a circle, an ellipse, a line, a polyline, a polygon) can be made into a path"), (r#"<rect width="50%" height="4"/>"#, "N2's numbers can't all be read (one is a percentage, or isn't a number), so its outline can't be written out as a path")] {
        assert_eq!(doc(shape).apply(&Command::ToPath { nodes: vec![N] }), Err(DocError::Invalid(why.into())));
    }
    assert_eq!(path("M0 0 L5 5 nonsense").apply(&Command::EditPath { node: N, edits: vec![PathEdit::Reverse { anchor: None }] }), Err(DocError::Invalid("N2's path data can't all be read, so it can't be taken point by point: set its d to path data that reads first".into())));
}

#[test]
fn a_whole_outline_is_set_and_the_anchors_named_keep_their_ids() {
    let at = |x: f64, y: f64| NewAnchor { at: v(x, y), into: None, out: None, id: None };
    let mut d = path("M0 0 H10 V10");
    // A triangle through new points, but for one that is A2 still; a
    // second run with a curve in it.
    let runs = vec![
        NewRun { anchors: vec![at(1.0, 1.0), NewAnchor { id: Some(a(2)), ..at(9.0, 1.0) }, at(5.0, 8.0)], closed: true },
        NewRun { anchors: vec![NewAnchor { out: Some(v(0.0, 4.0)), ..at(12.0, 0.0) }, NewAnchor { into: Some(v(0.0, 4.0)), ..at(20.0, 0.0) }], closed: false },
    ];
    let applied = d.apply(&Command::SetPath { node: N, runs }).unwrap();
    assert_eq!(state(&d), ("M1 1 H9 L5 8 Z M12 0 C12 4 20 4 20 0".to_owned(), vec![vec![4, 2, 5], vec![6, 7]]));
    assert_eq!(applied.anchors, [a(4), a(5), a(6), a(7)]);
    for (runs, why) in [
        (vec![NewRun { anchors: vec![], closed: false }], "a run with no anchors draws nothing: give it at least one"),
        (vec![NewRun { anchors: vec![NewAnchor { id: Some(a(1)), ..at(0.0, 0.0) }], closed: false }], "this path has no anchor A1 to keep (node_info lists the ones it has)"),
        (vec![NewRun { anchors: vec![NewAnchor { id: Some(a(2)), ..at(0.0, 0.0) }, NewAnchor { id: Some(a(2)), ..at(1.0, 0.0) }], closed: false }], "A2 is named twice: an anchor is in one place"),
    ] {
        assert_eq!(d.apply(&Command::SetPath { node: N, runs }), Err(DocError::Invalid(why.into())));
    }
    // Nothing at all: the path draws nothing.
    d.apply(&Command::SetPath { node: N, runs: vec![] }).unwrap();
    assert_eq!(d.node(N).unwrap().attr("d"), Some(""));
}

mod boolean {
    use ink_doc::{Command, DocError, DocId, Document, NodeId};
    use ink_geom::Combine;

    const N: fn(u64) -> NodeId = NodeId;

    fn doc(body: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 24 24\">\n{body}\n</svg>\n")).unwrap()
    }

    fn combined(body: &str, nodes: &[u64], how: Combine) -> Result<String, String> {
        let mut d = doc(body);
        let applied = d.apply(&Command::Boolean { nodes: nodes.iter().map(|n| N(*n)).collect(), how }).map_err(|e| e.to_string())?;
        assert_eq!((applied.changed, applied.removed), (vec![N(nodes[0])], nodes[1..].iter().map(|n| N(*n)).collect::<Vec<_>>()));
        Ok(d.to_svg().lines().skip(1).take_while(|l| *l != "</svg>").collect::<Vec<_>>().join("\n"))
    }

    #[test]
    fn the_first_shape_takes_the_result_and_the_others_go() {
        let body = "  <rect id=\"a\" x=\"2\" y=\"2\" width=\"10\" height=\"10\" fill=\"#ffc800\"/>\n  <circle id=\"b\" cx=\"12\" cy=\"12\" r=\"5\" stroke=\"red\"/>";
        // A rect is made a path to take it: where its numbers were, with
        // its paint and its id; the circle's arc is an arc still.
        assert_eq!(combined(body, &[2, 3], Combine::Union).unwrap(), "  <path id=\"a\" d=\"M2 2 H12 V7 A5 5 0 0 1 17 12 A5 5 0 0 1 12 17 A5 5 0 0 1 7 12 H2 Z\" fill=\"#ffc800\"/>");
        assert_eq!(combined(body, &[2, 3], Combine::Subtract).unwrap(), "  <path id=\"a\" d=\"M2 2 H12 V7 A5 5 0 0 0 7 12 H2 Z\" fill=\"#ffc800\"/>");
        assert_eq!(combined(body, &[2, 3], Combine::Intersect).unwrap(), "  <path id=\"a\" d=\"M12 7 V12 H7 A5 5 0 0 1 12 7 Z\" fill=\"#ffc800\"/>");
        // The other way about, the circle is the one kept.
        assert_eq!(combined(body, &[3, 2], Combine::Subtract).unwrap(), "  <path id=\"b\" d=\"M17 12 A5 5 0 0 1 12 17 A5 5 0 0 1 7 12 H12 V7 A5 5 0 0 1 17 12 Z\" stroke=\"red\"/>");
    }

    #[test]
    fn each_shape_is_where_it_shows_however_it_got_there() {
        // The second is under a group's move and has a scale of its
        // own: on the page it's the square from 6,6 to 14,14. The
        // result is written in the first one's coordinates, which are
        // moved too.
        let body = "  <rect id=\"a\" transform=\"translate(2 2)\" width=\"8\" height=\"8\"/>\n  <g transform=\"translate(6 6)\">\n    <rect id=\"b\" transform=\"scale(2)\" width=\"4\" height=\"4\"/>\n  </g>";
        assert_eq!(combined(body, &[2, 4], Combine::Intersect).unwrap(), "  <path id=\"a\" transform=\"translate(2 2)\" d=\"M8 4 V8 H4 V4 Z\"/>\n  <g transform=\"translate(6 6)\">\n  </g>");
        assert_eq!(combined(body, &[4, 2], Combine::Intersect).unwrap(), "  <g transform=\"translate(6 6)\">\n    <path id=\"b\" transform=\"scale(2)\" d=\"M0 0 H2 V2 H0 Z\"/>\n  </g>");
    }

    #[test]
    fn a_shape_is_what_its_rule_fills_and_one_alone_is_made_simple() {
        // A ring by the rule its group gives it: the band crosses the
        // ring's two sides, not its hole.
        let ring = "  <g fill-rule=\"evenodd\">\n    <path id=\"ring\" d=\"M2 2 H12 V12 H2 Z M5 5 H9 V9 H5 Z\"/>\n  </g>\n  <rect id=\"band\" x=\"0\" y=\"6\" width=\"24\" height=\"2\"/>";
        assert_eq!(combined(ring, &[3, 4], Combine::Intersect).unwrap(), "  <g fill-rule=\"evenodd\">\n    <path id=\"ring\" d=\"M12 6 V8 H9 V6 Z M2 8 V6 H5 V8 Z\"/>\n  </g>");
        // A bow tie, united with nothing: two triangles.
        assert_eq!(combined("  <polygon points=\"2,2 12,12 12,2 2,12\"/>", &[2], Combine::Union).unwrap(), "  <path d=\"M2 2 L7 7 L2 12 Z M12 12 L7 7 L12 2 Z\"/>");
    }

    #[test]
    fn what_cannot_be_combined_says_why() {
        let body = "  <rect id=\"a\" width=\"4\" height=\"4\"/>\n  <rect id=\"far\" x=\"10\" width=\"4\" height=\"4\"/>\n  <g id=\"g\"/>\n  <path id=\"bad\" d=\"M0 0 L5 5 X\"/>\n  <rect id=\"flat\" transform=\"scale(0)\" width=\"4\" height=\"4\"/>\n  <rect id=\"over\" x=\"-1\" y=\"-1\" width=\"9\" height=\"9\"/>";
        for (nodes, how, says) in [
            (&[2u64, 3][..], Combine::Intersect, "the shapes don't overlap anywhere, so nothing would be left: nothing was changed"),
            (&[2, 7], Combine::Subtract, "the others cover all of the first shape, so nothing would be left: nothing was changed (node_delete takes shapes out)"),
            (&[2, 4], Combine::Union, "N4 is a <g>: only shapes (paths, rects, circles, ellipses, lines, polygons) have an outline to work on; for a group, name the shapes in it"),
            (&[2, 5], Combine::Union, "N5's path data can't all be read, so there's no saying what it covers: set its d to path data that reads first"),
            (&[6, 2], Combine::Union, "N6's transform squashes it flat, so nothing can be worked out in its coordinates: give it a transform that can be undone first"),
            (&[2, 2], Combine::Union, "N2 is named twice: a shape is combined with others, not with itself"),
            (&[2], Combine::Subtract, "that takes two shapes or more: the first is kept, and the others are taken from it, or met with it (a union of one shape makes its outline simple, where it crosses itself)"),
            (&[], Combine::Union, "there's nothing to combine: name the shapes"),
        ] {
            assert_eq!(combined(body, nodes, how), Err(says.to_owned()), "{nodes:?} {how:?}");
        }
        let mut d = doc(body);
        assert_eq!(d.apply(&Command::Boolean { nodes: vec![N(2), N(99)], how: Combine::Union }), Err(DocError::NoSuchNode(N(99))));
        assert!(d.get(N(2)).is_some_and(|n| n.name == "rect"), "nothing of a refused one is left");
    }
}

mod stroking {
    use ink_doc::{Command, DocId, Document, NodeId};

    const N: fn(u64) -> NodeId = NodeId;

    fn outlined(body: &str, nodes: &[u64]) -> Result<(String, Vec<NodeId>), String> {
        let mut d = Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 24 24\">\n{body}\n</svg>\n")).unwrap();
        let applied = d.apply(&Command::OutlineStroke { nodes: nodes.iter().map(|n| N(*n)).collect(), tolerance: None }).map_err(|e| e.to_string())?;
        assert_eq!(applied.changed, nodes.iter().map(|n| N(*n)).collect::<Vec<_>>());
        Ok((d.to_svg().lines().skip(1).take_while(|l| *l != "</svg>").collect::<Vec<_>>().join("\n"), applied.created))
    }

    #[test]
    fn a_line_becomes_the_shape_of_its_stroke() {
        // No fill: the node itself is the outline now, filled with what
        // the stroke was painted with, where it said so.
        let (made, new) = outlined("  <path id=\"rule\" d=\"M4 12 H20\" fill=\"none\" stroke=\"#ffc800\" stroke-width=\"2\" stroke-linecap=\"round\"/>", &[2]).unwrap();
        assert_eq!(made, "  <path id=\"rule\" d=\"M20 13 H4 A1 1 0 0 1 3 12 A1 1 0 0 1 4 11 H20 A1 1 0 0 1 21 12 A1 1 0 0 1 20 13 Z\" fill=\"#ffc800\" stroke=\"none\"/>");
        assert!(new.is_empty());
        // A <line> is made a path; its stroke came from its group, and
        // its style says how it was drawn.
        let grouped = "  <g stroke=\"red\" stroke-opacity=\"0.5\">\n    <line x1=\"4\" y1=\"4\" x2=\"4\" y2=\"10\" style=\"stroke-width: 2; fill: none\"/>\n  </g>";
        let (made, _) = outlined(grouped, &[3]).unwrap();
        assert_eq!(made, "  <g stroke=\"red\" stroke-opacity=\"0.5\">\n    <path d=\"M3 10 V4 H5 V10 Z\" style=\"fill: red\" stroke=\"none\" fill-opacity=\"0.5\"/>\n  </g>");
        // A line that never said its fill is none has none to keep all
        // the same: there's no inside to it. It's the outline, itself.
        for bare in ["<line id=\"bar\" x1=\"4\" y1=\"12\" x2=\"20\" y2=\"12\" stroke=\"#ffc800\" stroke-width=\"2\"/>", "<path id=\"bar\" d=\"M4 12 L12 12 L20 12 L8 12\" stroke=\"#ffc800\" stroke-width=\"2\"/>", "<polyline id=\"bar\" points=\"4,4 12,12 20,20\" stroke=\"#ffc800\" stroke-width=\"2\"/>"] {
            let (made, new) = outlined(&format!("  {bare}"), &[2]).unwrap();
            assert!(new.is_empty() && made.starts_with("  <path id=\"bar\" d=\"M") && made.ends_with("stroke=\"none\" fill=\"#ffc800\"/>") && made.lines().count() == 1, "{made}");
        }
        // One that bends has an inside, and its fill (black, unsaid) stays.
        let (made, new) = outlined("  <polyline id=\"bend\" points=\"4,4 12,12 20,4\" stroke=\"#ffc800\" stroke-width=\"2\"/>", &[2]).unwrap();
        assert!(new.len() == 1 && made.lines().count() == 2 && made.starts_with("  <polyline id=\"bend\" points=\"4,4 12,12 20,4\" stroke=\"none\"/>"), "{made}");
    }

    #[test]
    fn a_shape_with_a_fill_keeps_it() {
        // The ring is a new path over the disc, which has no stroke now.
        let coin = "  <circle id=\"coin\" cx=\"12\" cy=\"12\" r=\"6\" fill=\"#ffc800\" stroke=\"#12100e\" stroke-width=\"2\"/>";
        let (made, new) = outlined(coin, &[2]).unwrap();
        assert_eq!(made, "  <circle id=\"coin\" cx=\"12\" cy=\"12\" r=\"6\" fill=\"#ffc800\" stroke=\"none\"/>\n  <path id=\"coin-2\" d=\"M12 17 A5 5 0 0 0 17 12 A5 5 0 0 0 12 7 A5 5 0 0 0 7 12 A5 5 0 0 0 12 17 Z M19 12 A7 7 0 0 1 12 19 A7 7 0 0 1 5 12 A7 7 0 0 1 12 5 A7 7 0 0 1 19 12 Z\" fill=\"#12100e\" stroke=\"none\"/>");
        assert_eq!(new, [N(3)]);
        // Under it, where strokes are painted first.
        let (made, _) = outlined(&coin.replace("stroke-width=\"2\"", "stroke-width=\"2\" paint-order=\"stroke\""), &[2]).unwrap();
        assert!(made.starts_with("  <path id=\"coin-2\" d=\"M12 17 ") && made.ends_with("\n  <circle id=\"coin\" cx=\"12\" cy=\"12\" r=\"6\" fill=\"#ffc800\" stroke=\"none\" paint-order=\"stroke\"/>"), "{made}");
    }

    #[test]
    fn what_has_no_stroke_to_outline_says_so() {
        let body = "  <rect id=\"plain\" width=\"4\" height=\"4\"/>\n  <rect id=\"thin\" width=\"4\" height=\"4\" stroke=\"red\" stroke-width=\"0\"/>\n  <g id=\"g\" stroke=\"red\"/>\n  <path id=\"dot\" d=\"M5 5 L5 5\" stroke=\"red\"/>";
        for (node, says) in [
            (2u64, "N2 has no stroke to outline (its stroke is none, or has no width): node_style gives it one"),
            (3, "N3 has no stroke to outline (its stroke is none, or has no width): node_style gives it one"),
            (4, "N4 is a <g>: only shapes (paths, rects, circles, ellipses, lines, polygons) have an outline to work on; for a group, name the shapes in it"),
            (5, "N5's stroke covers nothing (its line has no length, or its dashes are all gaps): nothing was changed"),
            (99, "no node N99 in this document"),
        ] {
            assert_eq!(outlined(body, &[node]).unwrap_err(), says);
        }
        assert_eq!(outlined(body, &[]).unwrap_err(), "there's no stroke to outline: name at least one shape");
    }
}
