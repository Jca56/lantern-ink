//! Dragging rows to restack them (LS3's `layers/drag.rs`): a press on a
//! row becomes a drag once the pointer has gone 10 px up or down; a
//! gold line shows where the selection would land; letting go moves it
//! there, as one undo step. Rows run front to back, so a row higher in
//! the list is further up the picture. Between the last thing in an
//! open group and what comes after the group, how far in the pointer
//! is says which: in line with what the group holds, it joins them;
//! further left, it lands under the group.

use ink_doc::{Command, Document, NodeId, Place};
use lntrn_math::{Rect, Vec2};

use crate::select::{Row, Selection};

/// How far the pointer goes before a press on a row is a drag, logical px.
pub const THRESHOLD: f64 = 10.0;

/// A row pressed, and maybe being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub id: NodeId,
    /// It has gone far enough to be a drag.
    pub live: bool,
}

/// Where dragged rows would land.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub place: Place,
    /// The insertion line: its left end and its right, window px.
    pub line: (Vec2, Vec2),
}

/// `id`'s ancestor that sits `depth` rows in (itself, at its own).
fn ancestor_at(doc: &Document, id: NodeId, own: usize, depth: usize) -> Option<NodeId> {
    let mut at = id;
    for _ in depth..own {
        at = doc.get(at)?.parent?;
    }
    Some(at)
}

/// Where `dragged` (back to front) would land with the pointer at
/// `pointer`, among `rows` as laid out (the topmost first, each with
/// its rect). `indent`: how far in each level stands, px. None where
/// they can't go (into themselves, somewhere the drawing refuses) or
/// wouldn't move.
pub fn target(doc: &Document, sel: &Selection, rows: &[(Row, Rect)], dragged: &[NodeId], pointer: Vec2, indent: f64) -> Option<Target> {
    let first = rows.first()?;
    // The gap the pointer is nearest: above the first row whose middle
    // is below it, or after the last.
    let gap = rows.iter().position(|(_, r)| pointer.y < r.center().y).unwrap_or(rows.len());
    let (left, right) = (first.1.min.x, first.1.max.x);
    let (place, depth, y) = if gap == 0 {
        (Place::After(first.0.id), first.0.depth, first.1.min.y)
    } else {
        let (above, above_rect) = rows[gap - 1];
        let below = rows.get(gap);
        let y = below.map_or(above_rect.max.y, |(_, r)| (above_rect.max.y + r.min.y) / 2.0);
        // An open group above: the deepest place is on top of what it
        // holds. Else under the row above; then, further left, under
        // each thing that ends here.
        let opens = doc.get(above.id).is_some_and(|n| n.kind.is_group() || above.holds) && sel.is_open(doc, above.id) && !dragged.contains(&above.id);
        let deepest = above.depth + usize::from(opens);
        let shallowest = below.map_or(0, |(row, _)| row.depth).min(deepest);
        let wanted = ((pointer.x - left) / indent.max(1.0)).floor().max(0.0) as usize;
        let depth = wanted.clamp(shallowest, deepest);
        let place = if opens && depth == deepest { Place::LastIn(above.id) } else { Place::Before(ancestor_at(doc, above.id, above.depth, depth)?) };
        (place, depth, y)
    };
    // Not into itself.
    let anchor = match place {
        Place::FirstIn(a) | Place::LastIn(a) | Place::Before(a) | Place::After(a) => a,
    };
    if dragged.iter().any(|&d| doc.is_within(anchor, d)) {
        return None;
    }
    // And only where the drawing takes it, and it's a change.
    let moved = doc.clone().apply(&Command::Move { nodes: dragged.to_vec(), place }).ok()?;
    if moved.is_nothing() {
        return None;
    }
    Some(Target { place, line: (Vec2::new(left + indent * depth as f64, y), Vec2::new(right, y)) })
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    const A: NodeId = NodeId(2);
    const G: NodeId = NodeId(3);
    const B1: NodeId = NodeId(4);
    const B2: NodeId = NodeId(5);
    const C: NodeId = NodeId(6);

    /// Front to back: c, group g holding (b2, b1), a. Rows 46 tall, 6
    /// apart, from y = 0; what's in a group stands 16 in.
    fn scene() -> (Document, Selection, Vec<(Row, Rect)>) {
        let d = Document::parse(DocId(1), "<svg xmlns=\"http://www.w3.org/2000/svg\">\n  <rect id=\"a\"/>\n  <g>\n    <rect id=\"b1\"/>\n    <rect id=\"b2\"/>\n  </g>\n  <rect id=\"c\"/>\n</svg>\n").unwrap();
        let s = Selection::default();
        let rows = laid(&s, &d);
        (d, s, rows)
    }

    fn laid(s: &Selection, d: &Document) -> Vec<(Row, Rect)> {
        s.rows(d).into_iter().enumerate().map(|(i, row)| (row, Rect::from_xywh(0.0, i as f64 * 52.0, 300.0, 46.0))).collect()
    }

    #[test]
    fn rows_land_in_the_gap_the_pointer_is_nearest() {
        let (d, s, rows) = scene();
        assert_eq!(rows.iter().map(|(r, _)| r.id).collect::<Vec<_>>(), [C, G, B2, B1, A]);
        let at = |id: NodeId, x: f64, y: f64| target(&d, &s, &rows, &[id], Vec2::new(x, y), 16.0);
        // a to the very top: over c, the line along the list's top.
        let t = at(A, 100.0, 5.0).unwrap();
        assert_eq!((t.place, t.line.0), (Place::After(C), Vec2::new(0.0, 0.0)));
        // Just under the group's row: on top of what it holds, the line
        // standing in.
        let t = at(A, 100.0, 100.0).unwrap();
        assert_eq!((t.place, t.line.0), (Place::LastIn(G), Vec2::new(16.0, 101.0)));
        // Between the two it holds.
        assert_eq!(at(A, 100.0, 152.0).map(|t| t.place), Some(Place::Before(B2)));
        // After its last: in line with them, it joins the group;
        // further left, it lands under the group.
        assert_eq!(at(C, 100.0, 205.0).map(|t| t.place), Some(Place::Before(B1)));
        let t = at(C, 5.0, 205.0).unwrap();
        assert_eq!((t.place, t.line.0.x), (Place::Before(G), 0.0));
        // Past the last row: the bottom.
        assert_eq!(at(C, 100.0, 900.0).map(|t| t.place), Some(Place::Before(A)));
        // Where it already is, there's nowhere to go: either side of
        // itself.
        assert_eq!((at(C, 100.0, 5.0), at(C, 100.0, 50.0)), (None, None));
        assert_eq!(at(A, 5.0, 205.0), None, "a is already right under the group");
        // A group can't go into itself.
        assert_eq!((at(G, 100.0, 100.0), at(G, 100.0, 152.0)), (None, None));
        // A closed group takes nothing in: under its row is under it.
        let mut closed = s.clone();
        closed.toggle_open(G);
        let rows = laid(&closed, &d);
        assert_eq!(target(&d, &closed, &rows, &[C], Vec2::new(100.0, 100.0), 16.0).map(|t| t.place), Some(Place::Before(G)));
    }

    #[test]
    fn several_rows_go_together_and_stay_in_their_order() {
        let (d, s, rows) = scene();
        // a and c, into the group between its two.
        let t = target(&d, &s, &rows, &[A, C], Vec2::new(100.0, 152.0), 16.0).unwrap();
        let mut moved = d.clone();
        moved.apply(&Command::Move { nodes: vec![A, C], place: t.place }).unwrap();
        assert_eq!(s.rows(&moved).iter().map(|r| r.id).collect::<Vec<_>>(), [G, B2, C, A, B1]);
        // What's locked stays where it is: nowhere to land.
        let mut locked = d.clone();
        locked.apply(&Command::SetLocked { nodes: vec![A], locked: true }).unwrap();
        assert_eq!(target(&locked, &s, &rows, &[A], Vec2::new(100.0, 5.0), 16.0), None);
    }
}
