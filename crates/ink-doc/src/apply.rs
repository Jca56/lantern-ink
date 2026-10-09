//! Applying a Command (ARCHITECTURE §3.4): all of it or none, and what
//! it did. The Commands themselves are `command.rs`; what each one
//! does to the tree is in the file its doc comment names.

use crate::command::{Applied, Command};
use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::lettering::LEADING;
use crate::settle;

/// How deep Batches may nest.
pub(crate) const MAX_BATCH_DEPTH: usize = 16;

impl Document {
    /// Apply `command`: all of it, or (when any part is refused) none.
    pub fn apply(&mut self, command: &Command) -> Result<Applied, DocError> {
        // On a copy that shares every node: a refusal part-way leaves
        // nothing half done.
        let mut work = self.clone();
        let mut applied = Applied::default();
        work.run(command, &mut applied, 0)?;
        if !applied.is_nothing() {
            // What its `<style>` rules say may be different now.
            work.restyle();
            *self = work;
        }
        Ok(applied)
    }

    fn run(&mut self, command: &Command, applied: &mut Applied, depth: usize) -> Result<(), DocError> {
        // What's locked changes only by being unlocked.
        self.guard(command)?;
        match command {
            Command::SetAttr { node, name, value } => {
                if self.set_attr(*node, name, value.as_deref())? && !applied.changed.contains(node) {
                    applied.changed.push(*node);
                }
            }
            Command::Insert { place, elements } => {
                if elements.is_empty() {
                    return invalid("there's nothing to insert");
                }
                let mut place = *place;
                for element in elements {
                    let id = self.insert(place, element.clone())?;
                    // New markup takes the file's indentation all the
                    // way down.
                    self.lay_out(id)?;
                    applied.created.push(id);
                    place = Place::After(id);
                }
            }
            Command::Delete { nodes } => {
                for &id in nodes {
                    self.node(id)?;
                }
                for &id in nodes {
                    // One inside another named before it is gone already.
                    if self.get(id).is_some() {
                        self.remove(id)?;
                        applied.removed.push(id);
                    }
                }
            }
            Command::Move { nodes, place } => {
                let mut place = *place;
                for &id in nodes {
                    let was = settle::parent_to_doc(self, id)?;
                    if self.relocate(id, place)? {
                        if !applied.moved.contains(&id) {
                            applied.moved.push(id);
                        }
                        let kept = self.make(&settle::keep_place(self, id, &was)?)?;
                        applied.note(kept);
                    }
                    place = Place::After(id);
                }
            }
            Command::Duplicate { nodes } => {
                if nodes.is_empty() {
                    return invalid("there's nothing to copy");
                }
                for &id in nodes {
                    let copy = self.duplicate(id)?;
                    applied.created.push(copy);
                }
            }
            Command::Group { nodes } => {
                let group = self.group(nodes)?;
                applied.created.push(group);
                applied.moved.extend(self.node(group)?.elements());
            }
            Command::Ungroup { nodes, drop } => {
                if nodes.is_empty() {
                    return invalid("there's nothing to ungroup");
                }
                for &id in nodes {
                    let did = self.ungroup(id, *drop)?;
                    applied.lost.extend(did.lost);
                    applied.removed.push(id);
                    applied.moved.extend(did.inside);
                    applied.note(did.changed);
                }
            }
            Command::Transform { nodes, by } => {
                let changed = self.make(&settle::plan(self, nodes, by, false)?)?;
                applied.note(changed);
            }
            Command::Resize { nodes, by } => {
                let changed = self.make(&settle::plan(self, nodes, by, true)?)?;
                applied.note(changed);
            }
            Command::SetGeometry { node, geometry } => {
                if self.set_geometry(*node, geometry)? {
                    applied.note(vec![*node]);
                }
            }
            Command::Define { elements } => {
                let made = self.define(elements)?;
                applied.created.extend(made);
            }
            Command::Paste { svg, place } => {
                let made = self.paste(svg, *place)?;
                applied.created.extend(made);
            }
            Command::SetClip { nodes, by, id } => {
                let did = if by.is_empty() { self.unclip(nodes)? } else { self.clip(nodes, by, id)? };
                applied.created.extend(did.created);
                applied.moved.extend(did.moved);
                applied.removed.extend(did.removed);
                applied.note(did.changed);
            }
            Command::SetStyle { nodes, set } => {
                let changed = self.set_style(nodes, set)?;
                applied.note(changed);
            }
            Command::ToPath { nodes } => {
                if nodes.is_empty() {
                    return invalid("there's nothing to make a path of: name at least one shape");
                }
                for &id in nodes {
                    if self.make_path(id)? {
                        applied.note(vec![id]);
                    }
                }
            }
            Command::EditPath { node, edits } => {
                let (changed, made) = self.edit_path(*node, edits)?;
                applied.note(if changed { vec![*node] } else { Vec::new() });
                applied.anchors.extend(made);
            }
            Command::SetPath { node, runs } => {
                let (changed, made) = self.set_path(*node, runs)?;
                applied.note(if changed { vec![*node] } else { Vec::new() });
                applied.anchors.extend(made);
            }
            Command::Boolean { nodes, how } => {
                let removed = self.combine(nodes, *how)?;
                applied.note(nodes[..1].to_vec());
                applied.removed.extend(removed);
            }
            Command::OutlineStroke { nodes, tolerance } => {
                if nodes.is_empty() {
                    return invalid("there's no stroke to outline: name at least one shape");
                }
                for &id in nodes {
                    let made = self.stroke_to_shape(id, *tolerance)?;
                    applied.note(vec![id]);
                    applied.created.extend(made);
                }
            }
            Command::Simplify { nodes, tolerance } => {
                if nodes.is_empty() {
                    return invalid("there's nothing to simplify: name at least one path");
                }
                for &id in nodes {
                    if self.simplify(id, *tolerance)? {
                        applied.note(vec![id]);
                    }
                }
            }
            Command::SetText { node, lines, leading } => {
                let was = self.markup(*node)?;
                let (made, gone) = self.set_text(*node, lines, leading.or_else(|| self.leading(*node)).unwrap_or(LEADING))?;
                // The same words again, written the same, are no change.
                if self.markup(*node)? != was {
                    applied.note(vec![*node]);
                    applied.created.extend(made);
                    applied.removed.extend(gone);
                }
            }
            Command::TextToPath { nodes, as_drawn } => {
                if nodes.is_empty() {
                    return invalid("there's nothing to make paths of: name at least one text");
                }
                for &id in nodes {
                    let (made, gone) = self.text_to_path(id, *as_drawn)?;
                    applied.note(vec![id]);
                    applied.created.extend(made);
                    applied.removed.extend(gone);
                }
            }
            Command::Tidy { also } => {
                let dropped = self.tidy(also);
                applied.removed.extend(dropped.words.iter().chain(&dropped.unused).chain(&dropped.empty).copied());
                // One written differently and then dropped is just gone.
                let still: Vec<NodeId> = dropped.changed.into_iter().filter(|id| self.get(*id).is_some()).collect();
                applied.note(still);
            }
            Command::SetLabel { node, label } => {
                let root = self.root;
                let declared = self.node(root)?.attrs.len();
                if self.set_label(*node, label.as_deref())? {
                    applied.note(vec![*node]);
                }
                // The first of Ink's marks brings its namespace's
                // declaration with it.
                if self.node(root)?.attrs.len() != declared {
                    applied.note(vec![root]);
                }
            }
            Command::SetLocked { nodes, locked } => {
                if nodes.is_empty() {
                    return invalid("there's nothing to lock: name at least one node");
                }
                let root = self.root;
                let declared = self.node(root)?.attrs.len();
                for &id in nodes {
                    if self.set_locked(id, *locked)? {
                        applied.note(vec![id]);
                    }
                }
                if self.node(root)?.attrs.len() != declared {
                    applied.note(vec![root]);
                }
            }
            Command::Batch(commands) => {
                if depth >= MAX_BATCH_DEPTH {
                    return invalid(format!("batches nested more than {MAX_BATCH_DEPTH} deep"));
                }
                for command in commands {
                    self.run(command, applied, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
