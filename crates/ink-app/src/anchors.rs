//! Which anchors, of which shapes (ARCHITECTURE §8): the outlines the
//! Node tool shows, and the Commands that change the anchors picked. No
//! window here (`noding.rs` has the tool's frame, `nodes.rs` what a
//! press takes).
//!
//! **A shape that isn't a path yet shows the anchors it would have as
//! one, and becomes one when they're first changed**, not when they're
//! looked at or picked. So its anchors need names before they have any:
//! they're called what they'd be called were the shape made a path now,
//! alone ([`WouldBe`]). The document hands out anchor ids from one
//! counter, so those names are right for a change that makes one shape
//! a path; a change that makes several goes by [`firsts`], which says
//! what each anchor is called once they've all been made, in order.

use std::collections::HashMap;

use ink_core::{Command, DocId, Document, NodeId};
use ink_doc::outline::{AnchorId, Outline};
use ink_doc::pathedit::PathEdit;
use ink_doc::{Kind, geometry};
use lntrn_math::Vec2;

/// An anchor, by the shape it's in.
pub type Picked = (NodeId, AnchorId);

/// `node` made a path by itself, on a copy: the outline it would have.
fn made_path(drawing: &Document, node: NodeId) -> Option<Outline> {
    let mut copy = drawing.clone();
    copy.apply(&Command::ToPath { nodes: vec![node] }).ok()?;
    copy.outline(node)
}

/// Where `id` is among `outline`'s anchors, and the one there in
/// `other` (the same shape, its anchors called otherwise).
fn there(outline: &Outline, id: AnchorId, other: &Outline) -> Option<AnchorId> {
    let nth = outline.anchors().position(|a| a.id == id)?;
    other.anchors().nth(nth).map(|a| a.id)
}

/// The shapes that aren't paths yet, as the paths they'd be made: each
/// by itself, from the drawing in one state.
#[derive(Default)]
pub struct WouldBe {
    of: Option<(DocId, u64)>,
    outlines: HashMap<NodeId, Outline>,
}

impl WouldBe {
    /// See that each of `shapes` (of `drawing`: `doc`, in the state
    /// `stamp`) is here. An anchor of `picked` on a shape that was here
    /// in another state is called what it's called in this one.
    pub fn refresh(&mut self, doc: DocId, stamp: u64, drawing: &Document, shapes: &[NodeId], picked: &mut [Picked]) {
        let stale = (self.of != Some((doc, stamp))).then(|| std::mem::take(&mut self.outlines));
        self.of = Some((doc, stamp));
        self.outlines.retain(|node, _| shapes.contains(node));
        for &node in shapes {
            if !self.outlines.contains_key(&node)
                && let Some(outline) = made_path(drawing, node)
            {
                self.outlines.insert(node, outline);
            }
        }
        // (Another drawing's shapes aren't this one's, whatever they're
        // called.)
        for (node, id) in picked.iter_mut() {
            if let Some((was, now)) = stale.as_ref().and_then(|old| old.get(node)).zip(self.outlines.get(node))
                && let Some(named) = there(was, *id, now)
            {
                *id = named;
            }
        }
    }

    pub fn get(&self, node: NodeId) -> Option<&Outline> {
        self.outlines.get(&node)
    }
}

/// `node`'s outline in `drawing`: a path's own, or the one a shape
/// would have.
pub fn outline_of(drawing: &Document, would_be: &WouldBe, node: NodeId) -> Option<Outline> {
    match drawing.get(node)?.kind {
        Kind::Path => drawing.outline(node),
        _ => would_be.get(node).cloned(),
    }
}

/// What a change to anchors of `nodes` starts with: the shapes among
/// them that aren't paths yet, which are made paths first, in this
/// order; and what each of their anchors is called once they are, where
/// that's not what it's called now.
pub fn firsts(drawing: &Document, would_be: &WouldBe, nodes: &[NodeId]) -> (Vec<NodeId>, HashMap<Picked, AnchorId>) {
    let mut first: Vec<NodeId> = Vec::new();
    for &node in nodes {
        if drawing.get(node).is_some_and(|n| n.kind != Kind::Path) && !first.contains(&node) {
            first.push(node);
        }
    }
    let mut names = HashMap::new();
    // One made a path by itself is called what it's called here.
    if first.len() > 1 {
        let mut copy = drawing.clone();
        if copy.apply(&Command::ToPath { nodes: first.clone() }).is_ok() {
            for &node in &first {
                if let Some((was, now)) = would_be.get(node).zip(copy.outline(node)) {
                    names.extend(was.anchors().zip(now.anchors()).filter(|(a, b)| a.id != b.id).map(|(a, b)| ((node, a.id), b.id)));
                }
            }
        }
    }
    (first, names)
}

/// `picked`, each called what `names` says it will be.
pub fn renamed(picked: &mut [Picked], names: &HashMap<Picked, AnchorId>) {
    for one in picked.iter_mut() {
        if let Some(named) = names.get(one) {
            one.1 = *named;
        }
    }
}

/// `picked` shape by shape, in the order each shape first comes.
pub fn grouped(picked: &[Picked]) -> Vec<(NodeId, Vec<AnchorId>)> {
    let mut out: Vec<(NodeId, Vec<AnchorId>)> = Vec::new();
    for &(node, id) in picked {
        match out.iter_mut().find(|(n, _)| *n == node) {
            Some((_, ids)) => ids.push(id),
            None => out.push((node, vec![id])),
        }
    }
    out
}

/// The Command that makes the shapes `first` paths, then `edits` on
/// each path named. With neither, one that does nothing.
pub fn command(first: &[NodeId], edits: Vec<(NodeId, Vec<PathEdit>)>) -> Command {
    let mut steps: Vec<Command> = Vec::new();
    if !first.is_empty() && !edits.is_empty() {
        steps.push(Command::ToPath { nodes: first.to_vec() });
    }
    steps.extend(edits.into_iter().map(|(node, edits)| Command::EditPath { node, edits }));
    if steps.len() == 1 { steps.remove(0) } else { Command::Batch(steps) }
}

/// What moves the anchors `picked` by `by` (the drawing's coordinates):
/// each shape's by as far in its own.
pub fn moved(drawing: &Document, would_be: &WouldBe, picked: &[Picked], by: Vec2) -> Option<Command> {
    let nodes: Vec<NodeId> = picked.iter().map(|(node, _)| *node).collect();
    let (first, names) = firsts(drawing, would_be, &nodes);
    let mut picked = picked.to_vec();
    renamed(&mut picked, &names);
    let edits: Vec<(NodeId, Vec<PathEdit>)> = grouped(&picked)
        .into_iter()
        .filter_map(|(node, anchors)| {
            let to_own = geometry::to_doc(drawing, node)?.inverse()?;
            Some((node, vec![PathEdit::Move { anchors, by: to_own.linear(by) }]))
        })
        .collect();
    (!edits.is_empty()).then(|| command(&first, edits))
}

/// What takes the anchors `picked` out. A shape left with none goes
/// altogether.
pub fn deleted(drawing: &Document, would_be: &WouldBe, picked: &[Picked]) -> Option<Command> {
    let whole = |(node, anchors): &(NodeId, Vec<AnchorId>)| outline_of(drawing, would_be, *node).is_some_and(|o| o.anchors().all(|a| anchors.contains(&a.id)));
    let (gone, kept): (Vec<_>, Vec<_>) = grouped(picked).into_iter().partition(whole);
    let nodes: Vec<NodeId> = kept.iter().map(|(node, _)| *node).collect();
    let (first, names) = firsts(drawing, would_be, &nodes);
    let mut steps: Vec<Command> = Vec::new();
    if !kept.is_empty() {
        let mut picked: Vec<Picked> = kept.iter().flat_map(|(node, ids)| ids.iter().map(|id| (*node, *id))).collect();
        renamed(&mut picked, &names);
        steps.push(command(&first, grouped(&picked).into_iter().map(|(node, anchors)| (node, vec![PathEdit::Delete { anchors }])).collect()));
    }
    if !gone.is_empty() {
        steps.push(Command::Delete { nodes: gone.into_iter().map(|(node, _)| node).collect() });
    }
    match steps.len() {
        0 => None,
        1 => steps.pop(),
        _ => Some(Command::Batch(steps)),
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId as Id;

    use super::*;

    /// A path of three anchors, two squares (the second moved by its
    /// group), and a circle.
    fn doc() -> Document {
        Document::parse(Id(1), r#"<svg viewBox="0 0 48 48"><path d="M0 0 H8 V8"/><rect x="10" y="10" width="4" height="4"/><g transform="translate(20 0) scale(2)"><rect width="4" height="4"/></g><circle cx="30" cy="30" r="4"/></svg>"#).unwrap()
    }

    const PATH: NodeId = NodeId(2);
    const ONE: NodeId = NodeId(3);
    const TWO: NodeId = NodeId(5);
    const DOC: DocId = Id(1);

    fn ids(outline: &Outline) -> Vec<u64> {
        outline.anchors().map(|a| a.id.0).collect()
    }

    fn data(doc: &Document, node: NodeId) -> String {
        doc.node(node).unwrap().attr("d").unwrap_or("-").to_owned()
    }

    #[test]
    fn a_shape_shows_the_anchors_it_would_have_as_a_path() {
        let d = doc();
        let mut would = WouldBe::default();
        would.refresh(DOC, 1, &d, &[ONE, TWO], &mut []);
        // The path's own are 1 to 3; each square alone would have the
        // next four. Nothing was changed by looking.
        let (path, one, two) = (outline_of(&d, &would, PATH).unwrap(), outline_of(&d, &would, ONE).unwrap(), outline_of(&d, &would, TWO).unwrap());
        assert_eq!((ids(&path), ids(&one), ids(&two)), (vec![1, 2, 3], vec![4, 5, 6, 7], vec![4, 5, 6, 7]));
        assert_eq!((one.runs[0].anchors[2].at, one.runs[0].closed, d.node(ONE).unwrap().name.as_str()), (Vec2::new(14.0, 14.0), true, "rect"));
        // A shape nobody asked after isn't here.
        assert!(outline_of(&d, &would, NodeId(6)).is_none() && outline_of(&d, &would, NodeId(99)).is_none());
    }

    #[test]
    fn anchors_keep_their_names_through_whatever_makes_their_shapes_paths() {
        let mut d = doc();
        let mut would = WouldBe::default();
        would.refresh(DOC, 1, &d, &[ONE, TWO], &mut []);
        // One shape made a path: called what it was called alone.
        let (first, names) = firsts(&d, &would, &[PATH, TWO, TWO]);
        assert_eq!((first, names.len()), (vec![TWO], 0));
        // Two: the second's come after the first's.
        let (first, names) = firsts(&d, &would, &[TWO, PATH, ONE]);
        assert_eq!(first, [TWO, ONE]);
        let mut picked = vec![(ONE, AnchorId(4)), (TWO, AnchorId(6)), (PATH, AnchorId(2)), (ONE, AnchorId(7))];
        renamed(&mut picked, &names);
        assert_eq!(picked, [(ONE, AnchorId(8)), (TWO, AnchorId(6)), (PATH, AnchorId(2)), (ONE, AnchorId(11))]);
        // Moved by two units of the drawing: each in its own coordinates
        // (the second square's are twice the size), and the names held.
        let picked = [(ONE, AnchorId(4)), (TWO, AnchorId(6)), (PATH, AnchorId(2))];
        let command = moved(&d, &would, &[picked[1], picked[2], picked[0]], Vec2::new(2.0, 0.0)).unwrap();
        d.apply(&command).unwrap();
        assert_eq!((data(&d, PATH).as_str(), data(&d, ONE).as_str(), data(&d, TWO).as_str()), ("M0 0 H10 L8 8", "M12 10 H14 V14 H10 Z", "M0 0 H4 L5 4 H0 Z"));
        // A shape of the picked that's still no path is called anew in
        // the drawing's next state: the anchor picked is the same one.
        let mut d = doc();
        d.apply(&Command::EditPath { node: PATH, edits: vec![PathEdit::Add { after: AnchorId(1), at: ink_doc::pathedit::Along::Share(0.5) }] }).unwrap();
        let mut picked = [(ONE, AnchorId(6)), (PATH, AnchorId(2))];
        would.refresh(DOC, 2, &d, &[ONE], &mut picked);
        assert_eq!((picked, ids(would.get(ONE).unwrap())), ([(ONE, AnchorId(7)), (PATH, AnchorId(2))], vec![5, 6, 7, 8]));
    }

    #[test]
    fn anchors_are_taken_out_and_a_shape_left_with_none_goes() {
        let mut d = doc();
        let mut would = WouldBe::default();
        would.refresh(DOC, 1, &d, &[ONE, TWO], &mut []);
        // One corner of a square, and the whole of the path.
        let picked = [(PATH, AnchorId(1)), (ONE, AnchorId(5)), (PATH, AnchorId(3)), (PATH, AnchorId(2))];
        let applied = d.apply(&deleted(&d, &would, &picked).unwrap()).unwrap();
        assert_eq!((data(&d, ONE).as_str(), applied.removed, d.get(PATH).is_none()), ("M10 10 L14 14 H10 Z", vec![PATH], true));
        assert_eq!((deleted(&d, &would, &[]), moved(&d, &would, &[], Vec2::ZERO)), (None, None));
        // Nothing to make, nothing made a path for it.
        assert_eq!((command(&[ONE], Vec::new()), grouped(&picked).len()), (Command::Batch(Vec::new()), 2));
    }
}
