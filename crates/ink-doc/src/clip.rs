//! Cutting nodes to a shape (ARCHITECTURE §3.4): `SetClip`. The shapes
//! that cut go into a `<clipPath>` in `<defs>`, written in the
//! coordinates of what they cut and showing where they showed; taking
//! the clip off again puts them back in the drawing.

use ink_geom::Affine;

use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::Element;
use crate::refs::{self, Ids};
use crate::settle;
use crate::style::{prop, url_id};
use crate::transform;
use crate::value::Precision;
use crate::viewport::Viewport;

/// What a clip did to the tree: for `Applied`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Clipped {
    pub created: Vec<NodeId>,
    pub moved: Vec<NodeId>,
    pub removed: Vec<NodeId>,
    pub changed: Vec<NodeId>,
}

impl Clipped {
    fn note(&mut self, changed: Vec<NodeId>) {
        for id in changed {
            if !self.changed.contains(&id) {
                self.changed.push(id);
            }
        }
    }
}

impl Document {
    /// Cut `nodes` to the shapes `by`: those go into a new `<clipPath
    /// id="…">` in `<defs>`, staying where they show, and each of
    /// `nodes` is clipped by it. The nodes must be under the same
    /// transforms (a clip path is in the coordinates of what it cuts,
    /// and has only one set of numbers).
    pub(crate) fn clip(&mut self, nodes: &[NodeId], by: &[NodeId], id: &str) -> Result<Clipped, DocError> {
        if nodes.is_empty() {
            return invalid("there's nothing to clip: name at least one node");
        }
        for &node in nodes {
            let n = self.node(node)?;
            if n.parent.is_none() {
                return invalid("the root <svg> can't be clipped: clip a group of what's in it");
            }
            if n.kind.is_never_drawn() {
                return invalid(format!("{node} is a <{}>, which shows nowhere itself: there's nothing of it to clip", n.name));
            }
        }
        for &shape in by {
            let n = self.node(shape)?;
            if !n.kind.is_shape() {
                return invalid(format!("{shape} is a <{}>: a clip path is cut from shapes (a rect, a circle, a path, …), one or several", n.name));
            }
            if let Some(&node) = nodes.iter().find(|&&node| self.is_within(shape, node)) {
                return invalid(format!("{shape} is {}: a node can't be cut by itself or by what's in it", if shape == node { "one of the nodes to clip".to_owned() } else { format!("inside {node}") }));
            }
        }
        // The coordinates the clip is written in: those of what it cuts.
        let space = settle::node_to_doc(self, nodes[0])?;
        let p = Precision::of(self);
        for &node in &nodes[1..] {
            if !p.same(&settle::node_to_doc(self, node)?, &space) {
                return invalid(format!("{} and {node} are under different transforms, and one clip path can only be in the coordinates of one of them: clip them one at a time, or group them and clip the group", nodes[0]));
            }
        }
        let mut out = Clipped::default();
        let name = self.node(self.root)?.name.rsplit_once(':').map_or("clipPath".to_owned(), |(prefix, _)| format!("{prefix}:clipPath"));
        let clip = self.define(&[Element::new(name).with("id", id)])?[0];
        out.created.push(clip);
        // In the file's order, so they sit in the clip as they sat.
        let shapes: Vec<NodeId> = self.descendants(self.root).into_iter().filter(|id| by.contains(id)).collect();
        for shape in shapes {
            let was = settle::parent_to_doc(self, shape)?;
            self.relocate(shape, Place::LastIn(clip))?;
            let kept = self.make(&settle::keep_place_in(self, shape, &was, Some(&space))?)?;
            out.note(kept);
            out.moved.push(shape);
        }
        let said = format!("url(#{id})");
        for &node in nodes {
            if self.set_prop(node, "clip-path", Some(&said))? {
                out.note(vec![node]);
            }
        }
        Ok(out)
    }

    /// Take the clip paths off `nodes`. A clip path that then cuts
    /// nothing is taken apart: its shapes go back into the drawing, just
    /// over what they cut and showing where they did, and it goes.
    pub(crate) fn unclip(&mut self, nodes: &[NodeId]) -> Result<Clipped, DocError> {
        if nodes.is_empty() {
            return invalid("there's nothing to release: name at least one node");
        }
        let mut out = Clipped::default();
        let view = self.get(self.root).map_or(Default::default(), |root| Viewport::of(root).view);
        // Each clip path met, with the first node it cut.
        let mut cut: Vec<(NodeId, NodeId)> = Vec::new();
        for &node in nodes {
            let n = self.node(node)?;
            let clip = prop(n, "clip-path").and_then(url_id).and_then(|(id, _)| Ids::of(self).get(id)).filter(|c| self.get(*c).is_some_and(|c| c.kind == Kind::ClipPath));
            if let Some(clip) = clip.filter(|clip| !cut.iter().any(|(seen, _)| seen == clip)) {
                cut.push((clip, node));
            }
            if self.set_prop(node, "clip-path", None)? {
                out.note(vec![node]);
            }
        }
        let users = refs::users(self);
        for (clip, node) in cut {
            let still_used = self.node(clip)?.attr("id").and_then(|id| users.get(id)).is_some_and(|users| !users.is_empty());
            if still_used {
                continue;
            }
            // Its shapes were in the coordinates of what they cut,
            // through the clip path's own transform.
            let was = transform::of(self.node(clip)?, view).unwrap_or(Affine::IDENTITY).then(&settle::node_to_doc(self, node)?);
            let mut over = node;
            for shape in self.node(clip)?.elements().collect::<Vec<_>>() {
                self.relocate(shape, Place::After(over))?;
                let kept = self.make(&settle::keep_place(self, shape, &was)?)?;
                out.note(kept);
                out.moved.push(shape);
                over = shape;
            }
            self.remove(clip)?;
            out.removed.push(clip);
        }
        Ok(out)
    }
}
