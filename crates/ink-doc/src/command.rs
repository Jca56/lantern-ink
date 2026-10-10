//! Commands: every edit there is (ARCHITECTURE §3.4). The window, the MCP
//! tools and the live bridge all change a document by applying one. A
//! Command names its nodes, carries all it needs, and applies whole or
//! not at all.

use ink_geom::{Affine, Combine};

use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::lettering::Span;
use crate::node::{Content, Element};
use crate::outline::AnchorId;
use crate::pathedit::PathEdit;
use crate::paths::NewRun;
use crate::shape::Geometry;
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
    /// Put the outlines of `nodes` through `by`, and leave their lines
    /// as they are: what a handle on a shape does (M4b). A stroke keeps
    /// its width and its dashes, and a rect its corners' rounding, so a
    /// shape scaled more one way than the other still takes it into its
    /// own numbers (a circle becomes an ellipse to do it), where
    /// [`Command::Transform`] would keep a `transform` for its stroke
    /// to grow by. Where a shape's numbers can't say it (a rect scaled
    /// across the way it's turned; anything under a shadow) it's
    /// Transform's way after all, stroke and all.
    Resize { nodes: Vec<NodeId>, by: Affine },
    /// Make the shape `node` what `geometry` says ([`crate::shape`]):
    /// its own numbers, a rect's corner, size and rounding, a circle's
    /// middle and radius, a polygon's corners. Each is written only
    /// where what it means changes; the rest stays as the file says it.
    /// The shape stays the kind it is: a rect is given a rect's
    /// numbers. What a handle of a shape's own does (M4c).
    SetGeometry { node: NodeId, geometry: Geometry },
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
    /// are set `leading` ems apart (`None`: as far as its lines were;
    /// 1.2 where it had one).
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
    /// The drawing's guides, all of them: the lines a person lines
    /// things up by, kept on the root as `ink:guides`
    /// ([`crate::guides`]). None takes the mark off.
    SetGuides { guides: Vec<crate::guides::Guide> },
    /// Put what the drawing `svg` holds (text off the clipboard) into
    /// this one ([`crate::Document::clipping`] makes such text): what it
    /// draws at `place`, its definitions into `<defs>`. A definition
    /// this drawing has already is used as it is; any other `id` that's
    /// taken gets another, and what's pasted goes by that.
    Paste { svg: String, place: Place },
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
    /// What went for good because the Command was told to let it: a
    /// group's opacity or clip path, at an ungroup told to drop them.
    pub lost: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::MAX_BATCH_DEPTH;
    use crate::document::Document;
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
