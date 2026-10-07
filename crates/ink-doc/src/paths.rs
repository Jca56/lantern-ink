//! The path Commands (ARCHITECTURE §3.4): a shape made into a path, a
//! path edited by its anchors, a path's whole outline set. Each reads
//! the path as an [`Outline`], changes that, and writes it back, so the
//! anchors that are still there keep their ids.

use std::collections::HashSet;

use ink_geom::Vec2;

use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::{Attr, prefix};
use crate::outline::{Anchor, AnchorId, Outline, Run};
use crate::pathedit::{PathEdit, cubic};
use crate::shape::Geometry;
use crate::value::Precision;

/// An anchor of an outline to set: where it is, and its handles as
/// offsets from it (`None`: none, the path turns a corner or runs
/// straight). With `id`, it is that anchor of the path as it was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NewAnchor {
    pub at: Vec2,
    pub into: Option<Vec2>,
    pub out: Option<Vec2>,
    pub id: Option<AnchorId>,
}

/// A run of an outline to set.
#[derive(Clone, Debug, PartialEq)]
pub struct NewRun {
    pub anchors: Vec<NewAnchor>,
    pub closed: bool,
}

impl Document {
    /// Make the shape `id` a `<path>` that draws the same outline (a
    /// rect's corners stay arcs). Everything else about it stays: its
    /// id, its paint, its transform. Whether it changed (a path is one
    /// already).
    pub(crate) fn make_path(&mut self, id: NodeId) -> Result<bool, DocError> {
        let node = self.node(id)?;
        let own: &[&str] = match node.kind {
            Kind::Path => return Ok(false),
            Kind::Rect => &["x", "y", "width", "height", "rx", "ry"],
            Kind::Circle => &["cx", "cy", "r"],
            Kind::Ellipse => &["cx", "cy", "rx", "ry"],
            Kind::Line => &["x1", "y1", "x2", "y2"],
            Kind::Polyline | Kind::Polygon => &["points"],
            _ => return invalid(format!("{id} is a <{}>: only a shape (a rect, a circle, an ellipse, a line, a polyline, a polygon) can be made into a path", node.name)),
        };
        let Some(geometry) = Geometry::of(node) else { return invalid(format!("{id}'s numbers can't all be read (one is a percentage, or isn't a number), so its outline can't be written out as a path")) };
        let d = Precision::of(self).path(&geometry.path());
        let name = prefix(&node.name).map_or("path".to_owned(), |p| format!("{p}:path"));
        let node = self.edit(id)?;
        // Its path data goes where the first of its numbers was,
        // written as that was.
        let first = node.attrs.iter().position(|a| own.contains(&a.name.as_str()));
        let mut data = Attr::new("d", d);
        if let Some(was) = first.map(|i| &node.attrs[i]) {
            (data.lead, data.eq, data.quote) = (was.lead.clone(), was.eq.clone(), was.quote);
        }
        node.attrs.retain(|a| !own.contains(&a.name.as_str()));
        node.attrs.insert(first.unwrap_or(0).min(node.attrs.len()), data);
        (node.name, node.kind) = (name, Kind::Path);
        self.reanchor(id);
        Ok(true)
    }

    /// The path `id` as an outline to edit. A shape that isn't a path
    /// yet is made one first.
    fn editable(&mut self, id: NodeId) -> Result<(Outline, bool), DocError> {
        let made = self.make_path(id)?;
        match self.outline(id) {
            Some(outline) => Ok((outline, made)),
            None => invalid(format!("{id}'s path data can't all be read, so it can't be taken point by point: set its d to path data that reads first")),
        }
    }

    /// The ids of the path `id`'s anchors (none, for what has none).
    fn anchor_ids(&self, id: NodeId) -> Vec<AnchorId> {
        self.outline(id).map(|o| o.anchors().map(|a| a.id).collect()).unwrap_or_default()
    }

    /// The anchors the path `id` has that aren't among `had`: the ones
    /// made since, in the order they were made.
    fn made_since(&self, id: NodeId, had: &HashSet<AnchorId>) -> Vec<AnchorId> {
        let mut made: Vec<AnchorId> = self.anchor_ids(id).into_iter().filter(|a| !had.contains(a)).collect();
        made.sort();
        made
    }

    /// Make `edits` on the path `id`, in order. Whether it changed, and
    /// the anchors the edits made: the ones it has now that it didn't
    /// have (as a path) before.
    pub(crate) fn edit_path(&mut self, id: NodeId, edits: &[PathEdit]) -> Result<(bool, Vec<AnchorId>), DocError> {
        let made_path = self.make_path(id)?;
        let had: HashSet<AnchorId> = self.anchor_ids(id).into_iter().collect();
        let (changed, _) = self.edit_each(id, edits)?;
        Ok((changed || made_path, self.made_since(id, &had)))
    }

    /// The anchors each of `edits` would make, were they made on the
    /// path `id` in order. A Command that makes the same edits of the
    /// same document gives its anchors these ids, so an edit can name
    /// the anchor an earlier one of its call makes.
    pub fn anchors_made(&self, id: NodeId, edits: &[PathEdit]) -> Result<Vec<Vec<AnchorId>>, DocError> {
        self.clone().edit_each(id, edits).map(|(_, each)| each)
    }

    /// [`Self::edit_path`], saying what each edit made. A refusal says
    /// which edit it was, when there are several.
    fn edit_each(&mut self, id: NodeId, edits: &[PathEdit]) -> Result<(bool, Vec<Vec<AnchorId>>), DocError> {
        if edits.is_empty() {
            return invalid("there's no edit to make: name at least one");
        }
        let (mut outline, made_path) = self.editable(id)?;
        let mut each = Vec::with_capacity(edits.len());
        for (i, edit) in edits.iter().enumerate() {
            each.push(outline.edit(edit, &mut || self.next.anchor()).map_err(|e| match e {
                DocError::Invalid(why) if edits.len() > 1 => DocError::Invalid(format!("edit {}: {why}", i + 1)),
                e => e,
            })?);
        }
        Ok((self.set_outline(id, &outline)? || made_path, each))
    }

    /// Make the path `id`'s whole outline `runs`. An anchor given the id
    /// of one the path has is that anchor still; the rest are new.
    /// Whether it changed, and the new ones.
    pub(crate) fn set_path(&mut self, id: NodeId, runs: &[NewRun]) -> Result<(bool, Vec<AnchorId>), DocError> {
        // An anchor yet to be given an id: no anchor is ever called this.
        const NEW: AnchorId = AnchorId(0);
        let (was, made_path) = self.editable(id)?;
        let had: HashSet<AnchorId> = was.anchors().map(|a| a.id).collect();
        let mut kept = HashSet::new();
        let mut outline = Outline::default();
        for run in runs {
            if run.anchors.is_empty() {
                return invalid("a run with no anchors draws nothing: give it at least one");
            }
            let mut anchors = Vec::with_capacity(run.anchors.len());
            for new in &run.anchors {
                if !new.at.is_finite() || [new.into, new.out].into_iter().flatten().any(|h| !h.is_finite()) {
                    return invalid("an anchor has to be somewhere: its place or a handle isn't a number");
                }
                let id = match new.id {
                    Some(old) if !had.contains(&old) => return invalid(format!("this path has no anchor {old} to keep (node_info lists the ones it has)")),
                    Some(old) if !kept.insert(old) => return invalid(format!("{old} is named twice: an anchor is in one place")),
                    Some(old) => old,
                    None => NEW,
                };
                anchors.push(Anchor { id, at: new.at });
            }
            let count = if run.closed { anchors.len() } else { anchors.len() - 1 };
            let links = (0..count)
                .map(|i| {
                    let (a, b) = (&run.anchors[i], &run.anchors[(i + 1) % run.anchors.len()]);
                    cubic(a.at, a.at + a.out.unwrap_or(Vec2::ZERO), b.at + b.into.unwrap_or(Vec2::ZERO), b.at)
                })
                .collect();
            outline.runs.push(Run { anchors, links, closed: run.closed });
        }
        // Ids for the new ones that are there once it's written (a
        // closed run given its first anchor again at its end has it
        // once).
        outline.settle(&Precision::of(self));
        for anchor in outline.runs.iter_mut().flat_map(|run| &mut run.anchors) {
            if anchor.id == NEW {
                anchor.id = self.next.anchor();
            }
        }
        let changed = self.set_outline(id, &outline)? || made_path;
        Ok((changed, self.made_since(id, &had)))
    }
}
