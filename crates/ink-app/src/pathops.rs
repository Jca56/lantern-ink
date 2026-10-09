//! What the Path menu does to the selection (ARCHITECTURE §8): each a
//! Command the core has had since M3c, one step of Alva's.
//!
//! **Which shapes each is for:**
//! - Union, Subtract, Intersect and Exclude make one shape of the
//!   shapes selected, themselves (not what a selected group holds). The
//!   one furthest back takes the result, and keeps its place and its
//!   paint; Subtract is that one less the ones in front of it.
//! - Object to Path, Outline Stroke, Simplify and Reverse are for every
//!   shape the selection is or holds, as a paint set on it is: each
//!   takes the ones it has something to do to (a shape that isn't a
//!   path yet; one with a stroke; a path).

use ink_core::{Command, Document, NodeId};
use ink_doc::pathedit::PathEdit;
use ink_doc::{Kind, paths};
use ink_geom::Combine;

use crate::ink::Ink;
use crate::paint::{self, Paint};

/// Something the Path menu does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathOp {
    ToPath,
    Combine(Combine),
    Outline,
    Simplify,
    Reverse,
}

impl PathOp {
    /// What its row, and its step, is called.
    pub fn label(self) -> &'static str {
        match self {
            PathOp::ToPath => "Object to Path",
            PathOp::Combine(Combine::Union) => "Union",
            PathOp::Combine(Combine::Subtract) => "Subtract",
            PathOp::Combine(Combine::Intersect) => "Intersect",
            PathOp::Combine(Combine::Exclude) => "Exclude",
            PathOp::Outline => "Outline Stroke",
            PathOp::Simplify => "Simplify",
            PathOp::Reverse => "Reverse",
        }
    }

    /// The menu's rows, in order (a gap between the groups).
    pub const ROWS: [&'static [PathOp]; 3] =
        [&[PathOp::ToPath], &[PathOp::Combine(Combine::Union), PathOp::Combine(Combine::Subtract), PathOp::Combine(Combine::Intersect), PathOp::Combine(Combine::Exclude)], &[PathOp::Outline, PathOp::Simplify, PathOp::Reverse]];
}

/// What there is to do with the selection: which rows are lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Can {
    /// A shape that isn't a path yet.
    pub to_path: bool,
    /// How many shapes are selected themselves: what's made one.
    pub shapes: usize,
    /// A shape with a stroke to outline.
    pub outline: bool,
    /// A path, to simplify or turn round.
    pub paths: bool,
}

impl Can {
    pub fn does(&self, op: PathOp) -> bool {
        match op {
            PathOp::ToPath => self.to_path,
            // One shape alone is made simple where it crosses itself.
            PathOp::Combine(Combine::Union) => self.shapes >= 1,
            PathOp::Combine(_) => self.shapes >= 2,
            PathOp::Outline => self.outline,
            PathOp::Simplify | PathOp::Reverse => self.paths,
        }
    }

    pub fn any(&self) -> bool {
        PathOp::ROWS.iter().flat_map(|group| group.iter()).any(|op| self.does(*op))
    }
}

/// What `op` is for, of the selection `tops` (back to front): see the
/// top of this file. Nothing that's locked.
fn takes(doc: &Document, tops: &[NodeId], op: PathOp) -> Vec<NodeId> {
    let is = |id: NodeId, kind: fn(Kind) -> bool| doc.get(id).is_some_and(|n| kind(n.kind)) && doc.lock_over(id).is_none();
    let within = || paint::painted(doc, tops).into_iter();
    match op {
        PathOp::Combine(_) => tops.iter().copied().filter(|&id| is(id, Kind::is_shape) && crate::select::is_drawn(doc, id)).collect(),
        PathOp::ToPath => within().filter(|&id| is(id, |k| k.is_shape() && k != Kind::Path)).collect(),
        PathOp::Outline => within()
            .filter(|&id| {
                let paints = paint::read(doc, id);
                is(id, Kind::is_shape) && paints.stroke != Paint::None && paints.line.width > 0.0
            })
            .collect(),
        PathOp::Simplify | PathOp::Reverse => within().filter(|&id| is(id, |k| k == Kind::Path)).collect(),
    }
}

/// What there is to do with the selection `tops` of `doc`.
pub fn can(doc: &Document, tops: &[NodeId]) -> Can {
    let any = |op: PathOp| !takes(doc, tops, op).is_empty();
    Can { to_path: any(PathOp::ToPath), shapes: takes(doc, tops, PathOp::Combine(Combine::Union)).len(), outline: any(PathOp::Outline), paths: any(PathOp::Simplify) }
}

/// The Command that does `op` to the selection `tops` of `doc`. None
/// where there's nothing for it to do.
pub fn command(doc: &Document, tops: &[NodeId], op: PathOp) -> Option<Command> {
    let nodes = takes(doc, tops, op);
    if !can(doc, tops).does(op) {
        return None;
    }
    let each = |one: &dyn Fn(NodeId) -> Command| {
        let mut steps: Vec<Command> = nodes.iter().map(|&id| one(id)).collect();
        if steps.len() == 1 { steps.remove(0) } else { Command::Batch(steps) }
    };
    Some(match op {
        PathOp::ToPath => Command::ToPath { nodes },
        PathOp::Combine(how) => Command::Boolean { nodes, how },
        PathOp::Outline => Command::OutlineStroke { nodes, tolerance: None },
        // Each by its own size.
        PathOp::Simplify => each(&|id| Command::Simplify { nodes: vec![id], tolerance: paths::simplify_tolerance(doc, id) }),
        PathOp::Reverse => each(&|id| Command::EditPath { node: id, edits: vec![PathEdit::Reverse { anchor: None }] }),
    })
}

impl Ink {
    /// What the Path menu can do with the selection of the tab that
    /// shows.
    pub(crate) fn path_can(&self) -> Can {
        self.tabs.active().and_then(|tab| Some(can(self.core.doc(tab.doc).ok()?, &tab.selection.tops(self.core.doc(tab.doc).ok()?)))).unwrap_or_default()
    }

    /// Do `op` to the selection of the tab that shows: one step. What's
    /// selected afterwards is what it left, with what it made.
    pub(crate) fn path_op(&mut self, op: PathOp) {
        let Some((doc, drawing, tops)) = self.tabs.active().and_then(|tab| {
            let drawing = self.core.doc(tab.doc).ok()?;
            Some((tab.doc, drawing, tab.selection.tops(drawing)))
        }) else {
            return;
        };
        let Some(command) = command(drawing, &tops, op) else { return };
        let Some(applied) = self.edit(doc, &command, op.label()) else { return };
        let Ok(drawing) = self.core.doc(doc) else { return };
        // What's left of the selection that isn't inside something this
        // made (a shape and its outline put in a group together), then
        // what it made.
        let inside = |id: NodeId| drawing.ancestors(id).any(|n| applied.created.contains(&n.id));
        let mut now: Vec<NodeId> = tops.into_iter().filter(|&id| drawing.get(id).is_some() && !inside(id)).collect();
        for made in &applied.created {
            if !now.contains(made) {
                now.push(*made);
            }
        }
        self.select(now);
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    /// Back to front: a square, a circle over its corner, a group of (a
    /// stroked line and a path), a locked square, and a text.
    fn doc() -> Document {
        Document::parse(DocId(1), r##"<svg xmlns:ink="urn:lantern:ink" viewBox="0 0 48 48"><rect id="a" width="10" height="10"/><circle id="b" cx="10" cy="10" r="4"/><g><line x2="8" y2="0" stroke="#000" stroke-width="2"/><path d="M0 20 L1 21 L2 22 L3 23 L8 28"/></g><rect width="4" height="4" ink:locked="true"/><text>hi</text></svg>"##).unwrap()
    }

    const A: NodeId = NodeId(2);
    const B: NodeId = NodeId(3);
    const G: NodeId = NodeId(4);
    const LINE: NodeId = NodeId(5);
    const PATH: NodeId = NodeId(6);
    const LOCKED: NodeId = NodeId(7);
    const TEXT: NodeId = NodeId(8);

    fn d(doc: &Document, node: NodeId) -> String {
        doc.node(node).unwrap().attr("d").unwrap_or("-").to_owned()
    }

    #[test]
    fn the_rows_light_for_what_they_have_something_to_do_to() {
        let doc = doc();
        let lit = |tops: &[NodeId]| PathOp::ROWS.iter().flat_map(|group| group.iter()).filter(|op| can(&doc, tops).does(**op)).map(|op| op.label()).collect::<Vec<_>>();
        assert_eq!(lit(&[]), Vec::<&str>::new());
        // One shape: made a path, or made simple.
        assert_eq!(lit(&[A]), ["Object to Path", "Union"]);
        assert_eq!(lit(&[A, B]), ["Object to Path", "Union", "Subtract", "Intersect", "Exclude"]);
        // A group: what it holds, but for making shapes one.
        assert_eq!(lit(&[G]), ["Object to Path", "Outline Stroke", "Simplify", "Reverse"]);
        assert_eq!(lit(&[PATH]), ["Union", "Simplify", "Reverse"]);
        // Nothing for what's locked, nor for a text.
        assert_eq!((lit(&[LOCKED, TEXT]), can(&doc, &[LOCKED]).any(), can(&doc, &[A]).any()), (vec![], false, true));
        assert_eq!(command(&doc, &[A], PathOp::Combine(Combine::Subtract)), None);
    }

    #[test]
    fn each_row_is_one_command_on_the_shapes_its_for() {
        let run = |tops: &[NodeId], op: PathOp| {
            let mut doc = doc();
            let applied = doc.apply(&command(&doc, tops, op).expect("something to do")).unwrap();
            (doc, applied)
        };
        // The one furthest back takes the result: the square less the
        // circle's quarter; the circle is gone.
        let (doc, applied) = run(&[A, B], PathOp::Combine(Combine::Subtract));
        assert_eq!((d(&doc, A).as_str(), applied.removed, doc.node(A).unwrap().attr("id")), ("M0 0 H10 V6 A4 4 0 0 0 6 10 H0 Z", vec![B], Some("a")));
        // What a group holds: its line made a path (the path it has is
        // one already), its stroke outlined, its path said with fewer
        // segments and turned round.
        let (doc, _) = run(&[G], PathOp::ToPath);
        assert_eq!((d(&doc, LINE).as_str(), d(&doc, PATH).as_str()), ("M0 0 H8", "M0 20 L1 21 L2 22 L3 23 L8 28"));
        let (doc, _) = run(&[G], PathOp::Outline);
        assert_eq!((doc.node(LINE).unwrap().name.as_str(), d(&doc, LINE).as_str()), ("path", "M8 1 H0 V-1 H8 Z"));
        let (doc, _) = run(&[G, A], PathOp::Simplify);
        assert_eq!(d(&doc, PATH), "M0 20 L8 28");
        let (doc, _) = run(&[PATH], PathOp::Reverse);
        assert_eq!(d(&doc, PATH), "M8 28 L3 23 L2 22 L1 21 L0 20");
    }
}
