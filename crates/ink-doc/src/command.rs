//! Commands: every edit there is (ARCHITECTURE §3.4). The window, the MCP
//! tools and the live bridge all change a document by applying one. A
//! Command names its nodes, carries all it needs, and applies whole or
//! not at all.

use ink_geom::{Affine, Combine};

use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::lettering::{LEADING, Span};
use crate::node::{Content, Element};
use crate::outline::AnchorId;
use crate::pathedit::PathEdit;
use crate::paths::NewRun;
use crate::settle;
use crate::tidy::Extra;
use crate::xml::parse::parse_fragment;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Set an attribute to `value`, as written, or take it off (`None`).
    /// Any attribute at all: what has no Command of its own yet goes
    /// through here.
    SetAttr { node: NodeId, name: String, value: Option<String> },
    /// Put `elements` in at `place`, in order.
    Insert { place: Place, elements: Vec<Element> },
    /// Take `nodes` out, with everything in them.
    Delete { nodes: Vec<NodeId> },
    /// Move `nodes` to `place`, in the order given, with everything in
    /// them. Each stays where it shows: one that lands under other
    /// transforms than it was under changes to make up for them (as
    /// [`Command::Transform`] writes it). What it inherits is its new
    /// parent's.
    Move { nodes: Vec<NodeId>, place: Place },
    /// Copy each of `nodes`, with everything in it, right on top of
    /// itself. A copy's `id`s are its own.
    Duplicate { nodes: Vec<NodeId> },
    /// Put `nodes` (which share a parent) into a new group where the
    /// topmost of them was.
    Group { nodes: Vec<NodeId> },
    /// Take each of the groups `nodes` away from around what's in it,
    /// which looks as it did. What only a group can hold for its
    /// children (a filter, a clip path, a mask, an opacity over several)
    /// refuses, unless `drop` says to lose it.
    Ungroup { nodes: Vec<NodeId>, drop: bool },
    /// Put `nodes` through `by`, a transform in the document's
    /// coordinates. Each looks exactly as SVG says it would with that
    /// transform on it; it goes into the node's own numbers where they
    /// can say so, and into its `transform` where they can't (D13).
    Transform { nodes: Vec<NodeId>, by: Affine },
    /// Put `elements` (gradients, clip paths, filters: each with an `id`
    /// nothing else has) into the drawing's `<defs>`, which is made if
    /// there isn't one.
    Define { elements: Vec<Element> },
    /// Cut `nodes` to the shapes `by`, which become a clip path called
    /// `id` in `<defs>` and stay where they show. With no `by`, take the
    /// clip paths off `nodes` instead: one that then cuts nothing has
    /// its shapes put back in the drawing, over what they cut.
    SetClip { nodes: Vec<NodeId>, by: Vec<NodeId>, id: String },
    /// Set how `nodes` are painted: each of `set` is a property and what
    /// it's set to (`None` takes it off). A property is one SVG has,
    /// and its value is checked where Ink draws with it. Each is written
    /// where its node has it (D14): in its `style`, as an attribute, or,
    /// for what a `<style>` rule gives it, in its `style` to outvote the
    /// rule.
    SetStyle { nodes: Vec<NodeId>, set: Vec<(String, Option<String>)> },
    /// Make each of the shapes `nodes` a `<path>` that draws the same
    /// outline. A path is left as it is.
    ToPath { nodes: Vec<NodeId> },
    /// Edit the path `node` by its anchors, each edit in turn. A shape
    /// that isn't a path yet is made one first.
    EditPath { node: NodeId, edits: Vec<PathEdit> },
    /// Set the path `node`'s whole outline. An anchor given the id of
    /// one the path has is that anchor still.
    SetPath { node: NodeId, runs: Vec<NewRun> },
    /// Make the shapes `nodes` one by what their fills cover: the first
    /// takes the result as its outline and keeps its place and paint,
    /// the others are removed ([`crate::boolean`]).
    Boolean { nodes: Vec<NodeId>, how: Combine },
    /// Make the stroke of each of the shapes `nodes` a shape of its
    /// own: a path that covers what the stroke did, filled with what it
    /// was painted with ([`crate::stroking`]). A shape with a fill
    /// keeps it, and the outline is a new path beside it. `tolerance`
    /// is how near the stroke's edge the outline's curves keep (`None`:
    /// a two-hundredth of the stroke's width).
    OutlineStroke { nodes: Vec<NodeId>, tolerance: Option<f64> },
    /// Say each of the paths `nodes` with as few segments as keep its
    /// outline within `tolerance` of where it was. Corners stay, and
    /// no anchor moves.
    Simplify { nodes: Vec<NodeId>, tolerance: f64 },
    /// Make the text `node` say `lines`, each a row of stretches with
    /// whatever lettering or paint each sets for itself
    /// ([`crate::lettering`]): everything that was in it goes. Lines
    /// are set `leading` ems apart (`None`: 1.2).
    SetText { node: NodeId, lines: Vec<Vec<Span>>, leading: Option<f64> },
    /// Make each of the texts `nodes` paths that draw what it draws
    /// ([`crate::outlined`]): one path, or a group of them where its
    /// spans paint for themselves. A text set in another font than it
    /// asks for is refused unless `as_drawn`.
    TextToPath { nodes: Vec<NodeId>, as_drawn: bool },
    /// Drop what nothing uses ([`crate::tidy`]): definitions nothing
    /// refers to, empty groups and `<defs>`, namespace declarations
    /// nothing uses; and of `also`, what's asked for by name.
    Tidy { also: Vec<Extra> },
    /// Give `node` a name for a person to know it by (`ink:label`), or
    /// with `None` take its name off ([`crate::marks`]).
    SetLabel { node: NodeId, label: Option<String> },
    /// Lock `nodes`, or unlock them (`ink:locked`). A locked node, and
    /// everything in it, refuses every other Command until it's
    /// unlocked.
    SetLocked { nodes: Vec<NodeId>, locked: bool },
    /// Several Commands as one step: all of them, or none.
    Batch(Vec<Command>),
}

/// What a Command did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Nodes whose own attributes changed.
    pub changed: Vec<NodeId>,
    /// Nodes put in (each with what's in it), in order.
    pub created: Vec<NodeId>,
    /// Nodes taken out (each with what was in it).
    pub removed: Vec<NodeId>,
    /// Nodes now somewhere else.
    pub moved: Vec<NodeId>,
    /// Anchors a path edit made (the node they're in is in `changed`).
    pub anchors: Vec<AnchorId>,
}

impl Applied {
    /// The document is as it was: there's nothing to undo.
    pub fn is_nothing(&self) -> bool {
        self.changed.is_empty() && self.created.is_empty() && self.removed.is_empty() && self.moved.is_empty()
    }

    /// These nodes' own attributes changed too.
    pub(crate) fn note(&mut self, changed: Vec<NodeId>) {
        for id in changed {
            if !self.changed.contains(&id) {
                self.changed.push(id);
            }
        }
    }

    /// The tree's shape changed, not just a node's attributes.
    pub fn structure_changed(&self) -> bool {
        !(self.created.is_empty() && self.removed.is_empty() && self.moved.is_empty())
    }
}

/// How deep Batches may nest.
const MAX_BATCH_DEPTH: usize = 16;

/// The elements in `markup`, to insert: any number of them, side by
/// side. Whitespace between them is left out (they get lines of their
/// own where they land); other text between them is refused.
pub fn elements(markup: &str) -> Result<Vec<Element>, DocError> {
    let mut out = Vec::new();
    for content in parse_fragment(markup)? {
        match content {
            Content::Element(el) => out.push(el),
            Content::Text(text) if text.trim().is_empty() => {}
            Content::Text(text) => return invalid(format!("text outside any element can't be inserted: \"{}\"", text.trim().chars().take(40).collect::<String>())),
        }
    }
    Ok(out)
}

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
                    let (inside, changed) = self.ungroup(id, *drop)?;
                    applied.removed.push(id);
                    applied.moved.extend(inside);
                    applied.note(changed);
                }
            }
            Command::Transform { nodes, by } => {
                let changed = self.make(&settle::plan(self, nodes, by)?)?;
                applied.note(changed);
            }
            Command::Define { elements } => {
                let made = self.define(elements)?;
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
                let (made, gone) = self.set_text(*node, lines, leading.unwrap_or(LEADING))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    const N: fn(u64) -> NodeId = NodeId;
    const TEXT: &str = "<svg>\n  <a/>\n  <g>\n    <b/>\n  </g>\n</svg>\n";

    fn doc() -> Document {
        Document::parse(DocId(1), TEXT).unwrap()
    }

    fn set(node: u64, name: &str, value: Option<&str>) -> Command {
        Command::SetAttr { node: N(node), name: name.to_owned(), value: value.map(str::to_owned) }
    }

    #[test]
    fn a_command_says_what_it_did() {
        let mut d = doc();
        let applied = d.apply(&set(2, "fill", Some("red"))).unwrap();
        assert_eq!(applied, Applied { changed: vec![N(2)], ..Applied::default() });
        assert!(!applied.structure_changed() && !applied.is_nothing());
        assert!(d.apply(&set(2, "fill", Some("red"))).unwrap().is_nothing(), "the same again does nothing");
        let made = d.apply(&Command::Insert { place: Place::After(N(2)), elements: elements("<x/> <y><z/></y>").unwrap() }).unwrap();
        assert_eq!(made.created, vec![N(5), N(6)]);
        assert!(made.structure_changed());
        assert_eq!(d.to_svg(), "<svg>\n  <a fill=\"red\"/>\n  <x/>\n  <y><z/></y>\n  <g>\n    <b/>\n  </g>\n</svg>\n");
        let moved = d.apply(&Command::Move { nodes: vec![N(6), N(5)], place: Place::FirstIn(N(3)) }).unwrap();
        assert_eq!(moved.moved, vec![N(6), N(5)]);
        assert_eq!(d.to_svg(), "<svg>\n  <a fill=\"red\"/>\n  <g>\n    <y><z/></y>\n    <x/>\n    <b/>\n  </g>\n</svg>\n");
        // A node and one inside it: the outer one's going covers both.
        let gone = d.apply(&Command::Delete { nodes: vec![N(3), N(4), N(2)] }).unwrap();
        assert_eq!(gone.removed, vec![N(3), N(2)]);
        assert_eq!(d.to_svg(), "<svg>\n</svg>\n");
    }

    #[test]
    fn a_refused_command_changes_nothing() {
        let mut d = doc();
        let batch = Command::Batch(vec![set(2, "fill", Some("red")), Command::Delete { nodes: vec![N(4)] }, set(99, "x", Some("1"))]);
        assert_eq!(d.apply(&batch), Err(DocError::NoSuchNode(N(99))));
        assert_eq!(d.to_svg(), TEXT, "the steps before the refused one are undone with it");
        assert!(d.get(N(4)).is_some());
        assert!(d.apply(&Command::Insert { place: Place::LastIn(N(1)), elements: vec![] }).is_err());
        assert!(d.apply(&Command::Delete { nodes: vec![N(1)] }).is_err());
        assert_eq!(d.to_svg(), TEXT);
        // An ID a refused command would have used is free again: none
        // was ever given out.
        d.apply(&Command::Batch(vec![Command::Insert { place: Place::LastIn(N(1)), elements: vec![Element::new("x")] }, set(99, "x", None)])).unwrap_err();
        let made = d.apply(&Command::Insert { place: Place::LastIn(N(1)), elements: vec![Element::new("x")] }).unwrap();
        assert_eq!(made.created, vec![N(5)]);
    }

    #[test]
    fn a_batch_is_one_step_and_nests_only_so_far() {
        let mut d = doc();
        let applied = d.apply(&Command::Batch(vec![set(2, "x", Some("1")), set(2, "y", Some("2")), Command::Batch(vec![set(4, "x", Some("3"))])])).unwrap();
        assert_eq!(applied.changed, vec![N(2), N(4)], "a node changed twice is listed once");
        let mut nested = set(2, "z", Some("1"));
        for _ in 0..=MAX_BATCH_DEPTH {
            nested = Command::Batch(vec![nested]);
        }
        assert!(d.apply(&nested).is_err());
    }

    #[test]
    fn markup_to_insert_is_elements_side_by_side() {
        let els = elements("\n  <rect x=\"1\"/>\n  <g><circle/></g>\n").unwrap();
        assert_eq!(els.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["rect", "g"]);
        assert!(elements("").unwrap().is_empty());
        assert!(elements("<rect/> loose words <g/>").is_err());
        assert!(elements("<rect>").is_err());
    }
}
