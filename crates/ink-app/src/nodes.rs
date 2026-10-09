//! A path's anchors on the canvas (ARCHITECTURE §8; LS3's `pen.rs`):
//! what a press takes (a handle of a picked anchor, an anchor, the
//! segment between two), and what a drag of a handle makes of the path.
//! Apart from the window, so the geometry is tested by itself
//! (`noding.rs` has the Node tool's frame).
//!
//! An SVG path has no word for "smooth": an anchor's two handles are
//! smooth while they lie in line through it. One dragged takes the
//! other round with it then (each keeping its own length), unless it
//! goes alone (Alt), which makes the anchor a corner from there on.

use ink_doc::outline::{AnchorId, Outline};
use ink_doc::pathedit::PathEdit;
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};

/// How big things are for the pointer, logical px across (LS3's): an
/// anchor or a handle, and a segment's line. And half an anchor as
/// drawn, which is a handle's dot's radius too.
pub const ANCHOR: f64 = 28.0;
pub const SEGMENT: f64 = 12.0;
pub const DRAWN: f64 = 7.0;
/// How far out of line two handles may be and still be smooth: the sine
/// of about two degrees.
const IN_LINE: f64 = 0.035;
/// A handle's steps round its anchor with Shift.
const STEP: f64 = std::f64::consts::FRAC_PI_4;

/// What of a path a press took.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// A handle of a picked anchor: the one on what goes out of it
    /// (`out`), or on what comes in.
    Handle { anchor: AnchorId, out: bool },
    Anchor(AnchorId),
    /// The segment after this anchor, and how far along it (0..1).
    Segment { after: AnchorId, share: f64 },
}

impl Hit {
    /// Which comes first where two are under the pointer: a handle,
    /// then an anchor, then a segment.
    pub fn rank(&self) -> u8 {
        match self {
            Hit::Handle { .. } => 0,
            Hit::Anchor(_) => 1,
            Hit::Segment { .. } => 2,
        }
    }
}

/// What of `outline` is under `p` (window px), and how far off: a
/// handle of one of the `picked` anchors, the nearest anchor, a
/// segment. `to_window`: from the outline's own coordinates; `scale`:
/// window px a logical one.
pub fn hit(outline: &Outline, to_window: &Affine, picked: &[AnchorId], p: Vec2, scale: f64) -> Option<(Hit, f64)> {
    let reach = ANCHOR * scale / 2.0;
    let off = |at: Vec2| to_window.apply(at).distance(p);
    let nearest = |found: Vec<(Hit, f64)>| found.into_iter().filter(|(_, d)| *d <= reach).min_by(|a, b| a.1.total_cmp(&b.1));
    let handles = picked.iter().flat_map(|&anchor| {
        let (into, out) = outline.handles(anchor);
        [(into, false), (out, true)].into_iter().filter_map(move |(h, out)| Some((Hit::Handle { anchor, out }, h?)))
    });
    if let Some(found) = nearest(handles.map(|(hit, at)| (hit, off(at))).collect()) {
        return Some(found);
    }
    if let Some(found) = nearest(outline.anchors().map(|a| (Hit::Anchor(a.id), off(a.at))).collect()) {
        return Some(found);
    }
    let own = to_window.inverse()?.apply(p);
    let segments = outline.runs.iter().flat_map(|run| (0..run.links.len()).filter_map(move |i| Some((run.anchors[i].id, run.piece(i)?))));
    let on = segments.map(|(after, piece)| {
        // Not at its very ends: those are its anchors'.
        let share = piece.nearest(own).clamp(0.02, 0.98);
        (Hit::Segment { after, share }, off(piece.at(share)))
    });
    on.filter(|(_, d)| *d <= SEGMENT * scale / 2.0).min_by(|a, b| a.1.total_cmp(&b.1))
}

/// The anchors of `outline` that show inside `marquee` (window px).
pub fn caught(outline: &Outline, to_window: &Affine, marquee: &Rect) -> Vec<AnchorId> {
    outline.anchors().filter(|a| marquee.contains(to_window.apply(a.at))).map(|a| a.id).collect()
}

/// The outline's line, as window px to draw through: each run, and
/// whether it's closed. `fine`: how far off the curves it may be, px.
pub fn lines(outline: &Outline, to_window: &Affine, fine: f64) -> Vec<(Vec<Vec2>, bool)> {
    outline.path().transformed(to_window).flatten(fine).into_iter().map(|line| (line.points, line.closed)).collect()
}

/// A handle taken hold of: whose, which, and where it and the one
/// opposite were, from the anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Held {
    pub anchor: AnchorId,
    pub out: bool,
    was: Vec2,
    /// The handle opposite, while the two are in line: it's taken round
    /// with this one.
    other: Option<Vec2>,
}

impl Held {
    /// The handle of `anchor` on what goes out of it (`out`) or comes
    /// in, as `outline` has it. None where it has none there.
    pub fn of(outline: &Outline, anchor: AnchorId, out: bool) -> Option<Held> {
        let at = outline.anchors().find(|a| a.id == anchor)?.at;
        let (into, away) = outline.handles(anchor);
        let (this, other) = if out { (away, into) } else { (into, away) };
        let (was, other) = (this? - at, other.map(|o| o - at));
        // In line: pointing opposite ways, along one line.
        let smooth = |o: &Vec2| was.dot(*o) < 0.0 && was.perp_dot(*o).abs() <= IN_LINE * was.length() * o.length();
        Some(Held { anchor, out, was, other: other.filter(smooth) })
    }

    /// Whether the two handles turn together.
    #[cfg(test)]
    fn smooth(&self) -> bool {
        self.other.is_some()
    }

    /// The edit that puts the handle where a drag `by` (the path's own
    /// coordinates) has it. `shift`: at a multiple of 45° from its
    /// anchor; `alone`: the one opposite stays where it is.
    pub fn dragged(&self, by: Vec2, shift: bool, alone: bool) -> PathEdit {
        let mut at = self.was + by;
        if shift && at.length() > 0.0 {
            let angle = (at.y.atan2(at.x) / STEP).round() * STEP;
            at = Vec2::new(angle.cos(), angle.sin()) * at.length();
        }
        // The other stays in line, as long as it was.
        let other = self.other.filter(|_| !alone && at.length() > 0.0).map(|o| at * (-o.length() / at.length()));
        let (this, other) = (Some(Some(at)), other.map(Some));
        if self.out { PathEdit::Handles { anchor: self.anchor, into: other, out: this } } else { PathEdit::Handles { anchor: self.anchor, into: this, out: other } }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::{DocId, Document, NodeId};

    use super::*;

    fn outline(d: &str) -> Outline {
        Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 48 48\"><path d=\"{d}\"/></svg>")).unwrap().outline(NodeId(2)).unwrap()
    }

    fn a(n: u64) -> AnchorId {
        AnchorId(n)
    }

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    /// A hump from (0,0) to (12,0), then a line down to (12,10).
    const HUMP: &str = "M0 0 C0 -6 12 -6 12 0 V10";

    #[test]
    fn a_press_takes_a_handle_then_an_anchor_then_a_segment() {
        let (o, zoom) = (outline(HUMP), Affine::scale(10.0, 10.0));
        let at = |x: f64, y: f64, picked: &[AnchorId]| hit(&o, &zoom, picked, v(x, y), 1.0).map(|(hit, _)| hit);
        // An anchor, within 14 px of its middle; the nearer of two.
        assert_eq!((at(3.0, -4.0, &[]), at(112.0, 8.0, &[]), at(16.0, 0.0, &[])), (Some(Hit::Anchor(a(1))), Some(Hit::Anchor(a(2))), None));
        // A handle shows, and is taken, only while its anchor is picked;
        // and comes before an anchor it's near.
        assert_eq!((at(0.0, -60.0, &[]), at(2.0, -58.0, &[a(1)])), (None, Some(Hit::Handle { anchor: a(1), out: true })));
        assert_eq!(at(118.0, -55.0, &[a(1), a(2)]), Some(Hit::Handle { anchor: a(2), out: false }));
        // The curve, within 6 px of it: where along it, by its own count.
        let Some(Hit::Segment { after, share }) = at(60.0, -43.0, &[]) else { panic!("the hump's top") };
        assert!(after == a(1) && (share - 0.5).abs() < 0.01, "{share}");
        assert_eq!((at(60.0, -36.0, &[]), at(123.0, 50.0, &[])), (None, Some(Hit::Segment { after: a(2), share: 0.5 })));
        assert_eq!(at(127.0, 50.0, &[]), None);
        // Never a segment's very end: that's its anchor's.
        let Some((Hit::Segment { share, .. }, _)) = hit(&o, &Affine::scale(100.0, 100.0), &[], v(1200.0, 15.0), 1.0) else { panic!("the line, near its top") };
        assert_eq!(share, 0.02);
        // Sizes go with the screen's scale.
        assert_eq!(hit(&o, &zoom, &[], v(17.0, 0.0), 1.25).map(|(hit, _)| hit), Some(Hit::Anchor(a(1))));
        assert_eq!((Hit::Anchor(a(1)).rank(), Hit::Handle { anchor: a(1), out: true }.rank() < Hit::Segment { after: a(1), share: 0.5 }.rank()), (1, true));
    }

    #[test]
    fn a_marquee_catches_the_anchors_inside_it_and_the_line_is_drawn_through_them() {
        let (o, zoom) = (outline(HUMP), Affine::scale(10.0, 10.0).then(&Affine::translate(100.0, 100.0)));
        let over = |x0: f64, y0: f64, x1: f64, y1: f64| caught(&o, &zoom, &Rect::new(v(x0, y0), v(x1, y1)));
        assert_eq!((over(90.0, 90.0, 110.0, 110.0), over(200.0, 90.0, 240.0, 210.0), over(0.0, 0.0, 50.0, 50.0)), (vec![a(1)], vec![a(2), a(3)], vec![]));
        let drawn = lines(&o, &zoom, 0.25);
        assert_eq!((drawn.len(), drawn[0].1, drawn[0].0[0], drawn[0].0.last().copied()), (1, false, v(100.0, 100.0), Some(v(220.0, 200.0))));
        // The hump is many short lines, never further off it than asked.
        assert!(drawn[0].0.len() > 8 && drawn[0].0.iter().all(|p| p.y >= 100.0 - 45.01));
        let ring = lines(&outline("M0 0 H4 V4 Z"), &Affine::IDENTITY, 0.25);
        assert_eq!((ring.len(), ring[0].1), (1, true));
    }

    #[test]
    fn a_handle_dragged_takes_the_one_opposite_round_while_theyre_in_line() {
        // A smooth anchor at (12,0): in from (8,-3), out to (18,4.5).
        let o = outline("M0 0 C4 -6 8 -3 12 0 C18 4.5 20 8 24 0");
        let held = Held::of(&o, a(2), true).unwrap();
        assert!(held.smooth());
        // Turned a quarter: the other is opposite still, as long as it was.
        let PathEdit::Handles { anchor, into: Some(Some(into)), out: Some(Some(out)) } = held.dragged(v(-10.5, -10.5), false, false) else { panic!("both handles") };
        assert!(anchor == a(2) && out.distance(v(-4.5, -6.0)) < 1e-9 && into.distance(v(3.0, 4.0)) < 1e-9, "{into:?} {out:?}");
        // Alone, the other is left as it is.
        assert_eq!(held.dragged(v(1.0, 0.0), false, true), PathEdit::Handles { anchor: a(2), into: None, out: Some(Some(v(7.0, 4.5))) });
        // The one coming in, taken: it's the one going out that follows.
        let PathEdit::Handles { into: Some(Some(into)), out: Some(Some(out)), .. } = Held::of(&o, a(2), false).unwrap().dragged(v(0.0, 3.0), false, false) else { panic!("both handles") };
        assert!(into == v(-4.0, 0.0) && out.distance(v(7.5, 0.0)) < 1e-9, "{into:?} {out:?}");
        // With Shift, at a multiple of 45° from the anchor.
        let PathEdit::Handles { out: Some(Some(out)), .. } = held.dragged(v(0.5, -4.0), true, true) else { panic!("a handle") };
        assert!(out.distance(v(6.5, 0.0)) < 0.05 && out.y.abs() < 1e-9, "{out:?}");
        // A corner's handles go their own ways: the other isn't touched.
        let corner = outline("M0 0 C4 -6 8 -3 12 0 C12 6 20 8 24 0");
        let held = Held::of(&corner, a(2), true).unwrap();
        assert!(!held.smooth());
        assert_eq!(held.dragged(v(1.0, 1.0), false, false), PathEdit::Handles { anchor: a(2), into: None, out: Some(Some(v(1.0, 7.0))) });
        // No handle there, none to hold: a line's end, an open run's.
        assert_eq!((Held::of(&outline(HUMP), a(2), true), Held::of(&o, a(1), false), Held::of(&o, a(9), true)), (None, None, None));
    }
}
