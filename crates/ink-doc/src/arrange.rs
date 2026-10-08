//! Lining things up and sharing out the space between them: how far
//! each of some boxes has to move. One reckoning for the window's
//! Object menu and for Claude's `node_align`, so they can't disagree.
//! Boxes are where nodes show in the drawing's coordinates
//! ([`crate::geometry::page_bounds`]), strokes aside; nothing is scaled.

use ink_geom::{Affine, Rect, Vec2};

use crate::command::Command;
use crate::document::Document;
use crate::id::NodeId;
use crate::viewport::Viewport;

/// The page, in the drawing's coordinates: what "align to the page"
/// lines up against.
pub fn page_box(doc: &Document) -> Rect {
    let Some(root) = doc.get(doc.root()) else { return Rect::new(Vec2::ZERO, Vec2::ZERO) };
    let v = Viewport::of(root);
    let (a, b) = v.to_page.inverse().map_or((Vec2::ZERO, v.size), |back| (back.apply(Vec2::ZERO), back.apply(v.size)));
    Rect::new(a.min(b), a.max(b))
}

/// The box round all of `boxes`.
pub fn joint(boxes: &[(NodeId, Rect)]) -> Option<Rect> {
    boxes.iter().map(|(_, b)| *b).reduce(|a, b| a.union(&b))
}

/// How far each of `boxes` moves to stand in line `against` a box:
/// `x` and `y` say which part of each goes to the same part of that
/// box (0 its left or top, 0.5 its middle, 1 its right or bottom;
/// `None` leaves that way alone). `still` stays where it is.
pub fn line_up(boxes: &[(NodeId, Rect)], x: Option<f64>, y: Option<f64>, against: Rect, still: Option<NodeId>) -> Vec<(NodeId, Vec2)> {
    let along = |at: f64, from: f64, to: f64| from + (to - from) * at;
    boxes
        .iter()
        .map(|(id, b)| {
            let mut shift = Vec2::ZERO;
            if still != Some(*id) {
                if let Some(at) = x {
                    shift.x = along(at, against.min.x, against.max.x) - along(at, b.min.x, b.max.x);
                }
                if let Some(at) = y {
                    shift.y = along(at, against.min.y, against.max.y) - along(at, b.min.y, b.max.y);
                }
            }
            (*id, shift)
        })
        .collect()
}

/// How far each of `boxes` moves for the gaps between them to be the
/// same, `across` the page or down it: in the order they stand, the
/// outer two staying put. (Fewer than three have no gaps to share.)
pub fn spread(boxes: &[(NodeId, Rect)], across: bool) -> Vec<(NodeId, Vec2)> {
    let mut moves: Vec<(NodeId, Vec2)> = boxes.iter().map(|(id, _)| (*id, Vec2::ZERO)).collect();
    if boxes.len() < 3 {
        return moves;
    }
    let side = |b: &Rect| if across { (b.min.x, b.max.x) } else { (b.min.y, b.max.y) };
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| side(&boxes[a].1).0.total_cmp(&side(&boxes[b].1).0));
    let (start, end) = (side(&boxes[order[0]].1).0, order.iter().map(|&i| side(&boxes[i].1).1).fold(f64::MIN, f64::max));
    let taken: f64 = order.iter().map(|&i| side(&boxes[i].1).1 - side(&boxes[i].1).0).sum();
    let gap = (end - start - taken) / (boxes.len() - 1) as f64;
    let mut at = start;
    for &i in &order {
        let (lo, hi) = side(&boxes[i].1);
        if across { moves[i].1.x = at - lo } else { moves[i].1.y = at - lo }
        at += hi - lo + gap;
    }
    moves
}

/// `moves` as one Command: each node that goes anywhere, moved.
pub fn moved(moves: Vec<(NodeId, Vec2)>) -> Command {
    Command::Batch(moves.into_iter().filter(|(_, d)| *d != Vec2::ZERO).map(|(id, d)| Command::Transform { nodes: vec![id], by: Affine::translate(d.x, d.y) }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    fn boxes() -> Vec<(NodeId, Rect)> {
        vec![(NodeId(2), Rect::from_xywh(0.0, 0.0, 4.0, 4.0)), (NodeId(3), Rect::from_xywh(10.0, 2.0, 2.0, 6.0)), (NodeId(4), Rect::from_xywh(30.0, 10.0, 10.0, 2.0))]
    }

    #[test]
    fn boxes_line_up_against_a_box_by_the_same_part_of_each() {
        let b = boxes();
        let all = joint(&b).unwrap();
        assert_eq!(all, Rect::from_xywh(0.0, 0.0, 40.0, 12.0));
        let by = |x, y| line_up(&b, x, y, all, None).into_iter().map(|(_, d)| d).collect::<Vec<_>>();
        assert_eq!(by(Some(0.0), None), [Vec2::ZERO, Vec2::new(-10.0, 0.0), Vec2::new(-30.0, 0.0)]);
        assert_eq!(by(Some(1.0), Some(0.5)), [Vec2::new(36.0, 4.0), Vec2::new(28.0, 1.0), Vec2::new(0.0, -5.0)]);
        // Against one of them, which stays put.
        let to_third = line_up(&b, Some(0.5), None, b[2].1, Some(NodeId(4)));
        assert_eq!(to_third.iter().map(|(_, d)| d.x).collect::<Vec<_>>(), [33.0, 24.0, 0.0]);
        // What doesn't move isn't in the Command.
        let Command::Batch(steps) = moved(to_third) else { panic!() };
        assert_eq!(steps.len(), 2);
    }

    #[test]
    fn the_space_between_is_shared_out_with_the_outer_two_staying() {
        // Across: 4 wide, then 2, then 10, from 0 to 40: 24 of gaps, 12 each.
        let across = spread(&boxes(), true);
        assert_eq!(across.iter().map(|(_, d)| *d).collect::<Vec<_>>(), [Vec2::ZERO, Vec2::new(6.0, 0.0), Vec2::ZERO]);
        // Down: 4 tall from 0, 6 from 2, 2 from 10 to 12: no room
        // between, so they overlap evenly.
        let down = spread(&boxes(), false);
        assert_eq!(down.iter().map(|(_, d)| d.y).collect::<Vec<_>>(), [0.0, 2.0, 0.0]);
        assert!(spread(&boxes()[..2], true).iter().all(|(_, d)| *d == Vec2::ZERO));
    }

    #[test]
    fn the_page_is_its_view_box() {
        let doc = Document::parse(DocId(1), r#"<svg width="48" height="48" viewBox="-12 -12 24 24"/>"#).unwrap();
        assert_eq!(page_box(&doc), Rect::from_xywh(-12.0, -12.0, 24.0, 24.0));
    }
}
