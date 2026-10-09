//! What the Pointer finds on the canvas (ARCHITECTURE §8): the thing
//! under a click, at the level the Pointer is in, and what a marquee
//! catches there. Apart from the window, so it's tested by itself.

use std::collections::HashMap;

use ink_core::{Document, NodeId};
use ink_doc::hit;
use lntrn_math::{Rect, Vec2};

/// What a click at `point` (the drawing's coordinates) picks, with the
/// Pointer inside `context`: the thing on top there, as the child of
/// `context` it is or is in. And whether it's outside `context`
/// altogether: then it's picked at the drawing's top level, and the
/// Pointer comes out. Nothing locked is picked: a click goes through
/// it to what's behind. `reach`: how far from the point still counts.
pub fn pick(doc: &Document, context: NodeId, point: Vec2, reach: f64) -> Option<(NodeId, bool)> {
    // The point itself; then, for a thin line just missed, round it.
    let ring = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (-0.7, -0.7), (0.7, -0.7)];
    let root = doc.root();
    let child_of = |holder: NodeId, node: NodeId| std::iter::once(node).chain(doc.ancestors(node).map(|n| n.id)).find(|&n| doc.get(n).and_then(|n| n.parent) == Some(holder));
    for (dx, dy) in ring {
        let found = hit::at(doc, point + Vec2::new(dx, dy) * reach).into_iter().find(|h| doc.lock_over(h.node).is_none());
        let Some(found) = found else { continue };
        return match child_of(context, found.node) {
            Some(inside) => Some((inside, false)),
            None => child_of(root, found.node).map(|top| (top, true)),
        };
    }
    None
}

/// The children of `context` that `marquee` touches, back to front: a
/// shape by its box, a group by anything it holds (not by the empty
/// room between them). Nothing locked, and nothing by way of
/// something locked.
pub(crate) fn caught(doc: &Document, context: NodeId, boxes: &HashMap<NodeId, Rect>, marquee: Rect) -> Vec<NodeId> {
    fn touched(doc: &Document, id: NodeId, boxes: &HashMap<NodeId, Rect>, marquee: &Rect) -> bool {
        let Some((node, b)) = doc.get(id).zip(boxes.get(&id)) else { return false };
        let reaches = b.min.x <= marquee.max.x && b.max.x >= marquee.min.x && b.min.y <= marquee.max.y && b.max.y >= marquee.min.y;
        if !reaches || doc.is_locked(id) {
            return false;
        }
        !node.kind.is_group() || node.elements().any(|child| touched(doc, child, boxes, marquee))
    }
    let Some(holder) = doc.get(context) else { return Vec::new() };
    holder.elements().filter(|&id| doc.lock_over(id).is_none() && touched(doc, id, boxes, &marquee)).collect()
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    /// Back to front: a square, a group of (a dot over a locked bar),
    /// a ring with no fill.
    fn doc() -> Document {
        Document::parse(DocId(1), r##"<svg xmlns:ink="urn:lantern:ink" viewBox="0 0 48 48"><rect id="a" x="2" y="2" width="20" height="20"/><g id="g"><rect id="bar" x="10" y="26" width="30" height="6" ink:locked="true"/><circle id="dot" cx="12" cy="12" r="4"/></g><circle id="ring" cx="36" cy="12" r="6" fill="none" stroke="#000" stroke-width="0.5"/></svg>"##).unwrap()
    }

    const A: NodeId = NodeId(2);
    const G: NodeId = NodeId(3);
    const DOT: NodeId = NodeId(5);
    const RING: NodeId = NodeId(6);

    #[test]
    fn a_click_picks_the_top_thing_at_the_level_the_pointer_is_in() {
        let d = doc();
        let at = |x: f64, y: f64, context: NodeId| pick(&d, context, Vec2::new(x, y), 0.3);
        let root = d.root();
        // The dot is over the square: at the top level, its group.
        assert_eq!((at(12.0, 12.0, root), at(4.0, 4.0, root)), (Some((G, false)), Some((A, false))));
        // Inside the group, the dot itself; the square is outside it,
        // and picking it comes back out.
        assert_eq!((at(12.0, 12.0, G), at(4.0, 4.0, G)), (Some((DOT, false)), Some((A, true))));
        // What's locked isn't picked: the click goes through.
        assert_eq!(at(30.0, 29.0, root), None);
        // A thin ring: on its line, or within reach of it; not in its
        // empty middle.
        assert_eq!((at(42.0, 12.0, root), at(42.5, 12.0, root), at(36.0, 12.0, root)), (Some((RING, false)), Some((RING, false)), None));
        assert_eq!(at(46.0, 46.0, root), None);
    }

    #[test]
    fn a_marquee_catches_what_it_touches_at_that_level() {
        let d = doc();
        let boxes = ink_doc::geometry::page_bounds(&d);
        let over = |x0: f64, y0: f64, x1: f64, y1: f64, context: NodeId| caught(&d, context, &boxes, Rect::new(Vec2::new(x0, y0), Vec2::new(x1, y1)));
        // Across the whole page: everything at the top level, back to
        // front. (The group isn't locked; the bar in it is.)
        assert_eq!(over(0.0, 0.0, 48.0, 48.0, d.root()), [A, G, RING]);
        // A shape by its box (not its line); a group by what it holds
        // (not the room between them).
        assert_eq!(over(30.0, 5.0, 31.0, 8.0, d.root()), [RING]);
        assert_eq!(over(24.0, 19.0, 28.0, 24.0, d.root()), Vec::<NodeId>::new());
        // Only its locked bar touched: the group isn't caught by that.
        assert_eq!(over(30.0, 27.0, 34.0, 30.0, d.root()), Vec::<NodeId>::new());
        assert_eq!(over(44.0, 40.0, 48.0, 48.0, d.root()), Vec::<NodeId>::new());
        // Inside the group: the dot, and never the locked bar.
        assert_eq!(over(0.0, 0.0, 48.0, 48.0, G), [DOT]);
    }
}
