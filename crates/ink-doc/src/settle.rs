//! Where a transform settles (ARCHITECTURE §3.4, D13). A node put
//! through a transform looks exactly as SVG says it would with that
//! transform on it; what's worked out here is how to write that.
//!
//! - **A shape takes it into its own numbers** whenever that says the
//!   same picture: a path, a line and a polygon anything; a circle
//!   while it stays round; a rect and an ellipse a move, a scale along
//!   their sides and a quarter turn. Any other turn of those two stays
//!   as one `rotate` about their middle. What's left (a skew) stays in
//!   `transform` whole.
//! - **How it looks goes with it.** Its stroke grows with it, so only
//!   an even scale can go into the numbers of a stroked shape. A
//!   gradient laid out in its own coordinates goes with it, when it's
//!   the shape's alone (into the gradient's own numbers, or its
//!   `gradientTransform`); one that other shapes use is never touched,
//!   so its shape keeps everything in `transform`. A
//!   gradient across its box doesn't turn with baked numbers, so a turn
//!   or a mirror stays in `transform` (so does one of a dashed rect,
//!   circle or ellipse, whose dashes start where its kind says). A
//!   clip path that is the node's alone goes with it too, its shapes
//!   taking what the node took. A clip path others use, a mask,
//!   markers and
//!   a filter laid out in its own coordinates would be left
//!   behind, so then everything stays in `transform`; a shadow would
//!   not turn or grow, so a filtered node takes only a move.
//! - **A group passes it down** to what's in it when everything there
//!   can take it without being left a transform it didn't have, and
//!   keeps it as its own `transform` otherwise.
//! - **A text takes a move** into its `x` and `y` and its lines'
//!   (`lettered.rs`); a turn or a scale stays its `transform`.
//!
//! **Resizing** ([`crate::Command::Resize`], `keep`) is the same with
//! one thing taken out: lines stay as they are. A stroke doesn't grow,
//! so it holds nothing back from a shape's numbers (a rect stretched
//! one way stays a plain rect, its line the width it was); a rect's
//! corners stay as round as they were; and a circle stretched one way
//! becomes the ellipse it then is. What still can't go into the
//! numbers goes into `transform` as ever, and there a stroke grows
//! with it: there is no other way to write that.

use std::collections::HashMap;

use ink_geom::{Affine, Vec2};

use self::drawn::{Cut, Filtered, Limits};
use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::Node;
use crate::refs::{self, Ids};
use crate::shape::Geometry;
use crate::style::{Style, prop};
use crate::transform;
use crate::value::Precision;
use crate::viewport::Viewport;

mod drawn;
mod lettered;

/// One change to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Attr { node: NodeId, name: &'static str, value: Option<String> },
    /// A property, written where the node has it (D14).
    Prop { node: NodeId, name: &'static str, value: Option<String> },
    /// A `<circle>` made an `<ellipse>` with these radii, said where
    /// its `r` was.
    Ellipse { node: NodeId, rx: String, ry: String },
}

/// What can be put through a transform: what shows where it stands.
fn placeable(node: &Node) -> bool {
    node.kind.is_shape() || matches!(node.kind, Kind::G | Kind::A | Kind::Switch | Kind::Svg | Kind::Text | Kind::Image | Kind::Use)
}

/// What draws nothing where it stands: definitions, and words about
/// the picture. A group's transform means nothing to these.
fn stands_nowhere(kind: Kind) -> bool {
    kind.is_never_drawn() && kind != Kind::Other
}

pub(crate) struct Settle<'a> {
    doc: &'a Document,
    ids: Ids<'a>,
    /// The size percentages are of.
    view: Vec2,
    p: Precision,
    /// Which nodes name each id.
    users: HashMap<String, Vec<NodeId>>,
    /// Lines stay as they are (a resize): see the top of this file.
    keep: bool,
    pub edits: Vec<Edit>,
}

impl<'a> Settle<'a> {
    pub fn new(doc: &'a Document) -> Settle<'a> {
        let view = doc.get(doc.root()).map_or(Vec2::ZERO, |root| Viewport::of(root).view);
        Settle { doc, ids: Ids::of(doc), view, p: Precision::of(doc), users: refs::users(doc), keep: false, edits: Vec::new() }
    }

    /// What `node` inherits: what its ancestors say, from the root down.
    fn inherited(&self, node: &Node) -> Style {
        let above: Vec<&Node> = self.doc.ancestors(node.id).collect();
        above.iter().rev().fold(Style::default(), |style, n| style.cascade(n))
    }

    /// From the coordinates `node` is written in (its parent's) to the
    /// document's. The root's own transform isn't one.
    fn parent_to_doc(&self, node: &Node) -> Affine {
        self.doc.ancestors(node.id).filter(|n| n.parent.is_some()).filter_map(|n| transform::of(n, self.view)).fold(Affine::IDENTITY, |t, up| t.then(&up))
    }

    /// Put `b` into `node`'s numbers, if it may go there and a shape of
    /// its kind can be what `b` makes of it. Returns the shape it now
    /// is, as the file will hold it.
    fn bake(&mut self, node: &Node, geometry: &Geometry, b: &Affine, limits: &Limits, p: &Precision) -> Option<Geometry> {
        let grow = limits.allows(b, p)?;
        let baked = match (geometry.through(b, p), geometry) {
            // Resized, a rect's corners are as round as they were (the
            // other way about, after a quarter turn).
            (Some(Geometry::Rect { x, y, width, height, .. }), Geometry::Rect { rx, ry, .. }) if self.keep => {
                let across = b.linear(Vec2::X);
                let (rx, ry) = if across.x.abs() < across.y.abs() { (*ry, *rx) } else { (*rx, *ry) };
                Geometry::Rect { x, y, width, height, rx, ry }
            }
            (Some(through), _) => through,
            // Resized, a circle stretched one way is an ellipse.
            (None, Geometry::Circle { c, r }) if self.keep => {
                let Geometry::Ellipse { c, rx, ry } = Geometry::Ellipse { c: *c, rx: *r, ry: *r }.through(b, p)?.rounded(p) else { return None };
                for (name, value) in (Geometry::Circle { c, r: *r }).write(node, p) {
                    self.edits.push(Edit::Attr { node: node.id, name, value });
                }
                self.edits.push(Edit::Ellipse { node: node.id, rx: p.number(rx), ry: p.number(ry) });
                self.carried(limits, b, p);
                return Some(Geometry::Ellipse { c, rx, ry });
            }
            (None, _) => return None,
        }
        .rounded(p);
        for (name, value) in baked.write(node, p) {
            self.edits.push(Edit::Attr { node: node.id, name, value });
        }
        self.carried(limits, b, p);
        if let Some(stroke) = limits.stroke.as_ref().filter(|_| (grow - 1.0).abs() > 1e-9) {
            let mut set = |name, value: String| self.edits.push(Edit::Prop { node: node.id, name, value: Some(value) });
            set("stroke-width", p.number(stroke.width * grow));
            if !stroke.dashes.is_empty() {
                set("stroke-dasharray", stroke.dashes.iter().map(|d| p.number(d * grow)).collect::<Vec<_>>().join(" "));
            }
            if stroke.offset != 0.0 {
                set("stroke-dashoffset", p.number(stroke.offset * grow));
            }
        }
        Some(baked)
    }

    /// What goes along with a shape whose numbers took `b`: a gradient
    /// and a clip path that are its alone.
    fn carried(&mut self, limits: &Limits, b: &Affine, p: &Precision) {
        if p.same(b, &Affine::IDENTITY) {
            return;
        }
        for server in limits.own.iter().filter_map(|id| self.doc.get(*id)) {
            self.carry(server, b);
        }
        if let Some(clip) = limits.clip.and_then(|id| self.doc.get(id)) {
            self.carry_clip(clip, b);
        }
    }

    /// Give a shape the transform `to`. Returns what of it is left for
    /// its `transform` to say.
    fn shape(&mut self, node: &Node, to: Affine, inherited: &Style) -> Option<Affine> {
        let hold = Some(to).filter(|t| !self.p.same(t, &Affine::IDENTITY));
        let Some(geometry) = Geometry::of(node) else { return hold };
        let p = self.p.reaching(geometry.reach());
        if p.same(&to, &Affine::IDENTITY) {
            return None;
        }
        let limits = self.limits(node, &geometry, &inherited.cascade(node));
        // All of it.
        if self.bake(node, &geometry, &to, &limits, &p).is_some() {
            return None;
        }
        // All but a turn: scaled along its own sides about its middle,
        // which goes where `to` puts it, and turned about it there.
        if let Some(axes) = to.axes().filter(|axes| p.same(&axes.linear(), &to.without_move())) {
            let (from, at) = (geometry.center(), to.apply(geometry.center()));
            let scaled = Affine::translate(-from.x, -from.y).then(&Affine::scale(axes.sx, axes.sy)).then(&Affine::translate(at.x, at.y));
            if let Some(baked) = self.bake(node, &geometry, &scaled, &limits, &p) {
                return Some(Affine::rotate(axes.angle).about(baked.center()));
            }
        }
        hold
    }

    /// Give a group the transform `to`: passed down to what's in it
    /// when all of that can take it, else left for its own `transform`.
    fn group(&mut self, node: &Node, to: Affine, inherited: &Style) -> Option<Affine> {
        let hold = Some(to).filter(|t| !self.p.same(t, &Affine::IDENTITY));
        let cut = self.cut(node);
        let passes = hold.is_some()
            && !matches!(cut, Cut::Held)
            && match self.filtered(node) {
                Filtered::No => true,
                Filtered::Follows => self.p.same(&to.without_move(), &Affine::IDENTITY),
                Filtered::Fixed => false,
            };
        if !passes {
            return hold;
        }
        let style = inherited.cascade(node);
        let mark = self.edits.len();
        for child in node.elements().filter_map(|id| self.doc.get(id)).filter(|c| !stands_nowhere(c.kind)) {
            let own = transform::of(child, self.view);
            let child_to = own.unwrap_or(Affine::IDENTITY).then(&to);
            let left = if placeable(child) { self.give(child, child_to, &style) } else { Some(child_to) };
            // Passing down never leaves a child a transform it didn't
            // have: only one moved along with what it holds.
            let fine = match (&left, &own) {
                (None, _) => true,
                (Some(left), Some(own)) => placeable(child) && self.p.same(&left.without_move(), &own.without_move()),
                (Some(_), None) => false,
            };
            if !fine {
                self.edits.truncate(mark);
                return hold;
            }
            self.write(child, own.as_ref(), left);
        }
        // Its clip path, cut in the coordinates its children were in,
        // goes where they went.
        if let Cut::Own(clip) = cut {
            self.carry_clip(clip, &to);
        }
        None
    }

    /// Give `node` the transform `to` (from its own coordinates to its
    /// parent's). Returns what of it is left for its `transform`.
    fn give(&mut self, node: &Node, to: Affine, inherited: &Style) -> Option<Affine> {
        match node.kind {
            kind if kind.is_shape() => self.shape(node, to, inherited),
            Kind::G | Kind::A | Kind::Switch => self.group(node, to, inherited),
            Kind::Text => self.text(node, to, inherited),
            _ => Some(to).filter(|t| !self.p.same(t, &Affine::IDENTITY)),
        }
    }

    /// Write `left` as `node`'s `transform`, which is `own` now. One
    /// that says the same already is left as the file has it.
    fn write(&mut self, node: &Node, own: Option<&Affine>, left: Option<Affine>) {
        if self.p.same(own.unwrap_or(&Affine::IDENTITY), left.as_ref().unwrap_or(&Affine::IDENTITY)) {
            return;
        }
        self.edits.push(Edit::Attr { node: node.id, name: "transform", value: left.and_then(|t| self.p.transform(&t)) });
        // Written about the origin: an origin of its own would move it.
        if prop(node, "transform-origin").is_some() {
            self.edits.push(Edit::Prop { node: node.id, name: "transform-origin", value: None });
        }
    }

    /// Put `id` through `by`, a transform in the document's coordinates.
    /// The root has no transform of its own to take it: everything in
    /// it goes through instead, each thing for itself.
    pub fn node(&mut self, id: NodeId, by: &Affine) -> Result<(), DocError> {
        let node = self.doc.node(id)?;
        if node.parent.is_none() {
            let inside: Vec<NodeId> = node.elements().filter(|&c| self.doc.get(c).is_some_and(placeable)).collect();
            return inside.into_iter().try_for_each(|child| self.node(child, by));
        }
        if !placeable(node) {
            return invalid(format!("{id} is a <{}>, which shows nowhere itself: only shapes, groups, text, images and uses go through a transform", node.name));
        }
        let up = self.parent_to_doc(node);
        let Some(down) = up.inverse() else { return invalid(format!("{id} is under a transform that squashes it flat: nothing moves it from there")) };
        let own = transform::of(node, self.view);
        let to = own.unwrap_or(Affine::IDENTITY).then(&up).then(by).then(&down);
        let left = self.give(node, to, &self.inherited(node));
        self.write(node, own.as_ref(), left);
        Ok(())
    }
}

/// The edits that put `nodes` through `by`: lines and all, or with
/// `keep`, leaving their lines as they are (a resize). One named along
/// with a group it's in goes with the group, once.
pub(crate) fn plan(doc: &Document, nodes: &[NodeId], by: &Affine, keep: bool) -> Result<Vec<Edit>, DocError> {
    if !by.is_finite() || by.inverse().is_none() {
        return invalid("that transform squashes everything flat (or isn't numbers): give one that leaves things some size");
    }
    for &id in nodes {
        doc.node(id)?;
    }
    let mut settle = Settle { keep, ..Settle::new(doc) };
    for (i, &id) in nodes.iter().enumerate() {
        let under_another = nodes.iter().any(|&other| other != id && doc.is_within(id, other));
        if !under_another && !nodes[..i].contains(&id) {
            settle.node(id, by)?;
        }
    }
    Ok(settle.edits)
}

/// From the coordinates `id` is written in (its parent's) to the
/// document's, as things stand.
pub(crate) fn parent_to_doc(doc: &Document, id: NodeId) -> Result<Affine, DocError> {
    Ok(Settle::new(doc).parent_to_doc(doc.node(id)?))
}

/// From `id`'s own coordinates to the document's: its own transform,
/// then everything it's under.
pub(crate) fn node_to_doc(doc: &Document, id: NodeId) -> Result<Affine, DocError> {
    let settle = Settle::new(doc);
    let node = doc.node(id)?;
    let own = if node.parent.is_some() { transform::of(node, settle.view) } else { None };
    Ok(own.unwrap_or(Affine::IDENTITY).then(&settle.parent_to_doc(node)))
}

/// The edits that leave `id` showing where it did, now that what it's
/// in has changed: `was` is [`parent_to_doc`] as it was before. Nothing
/// for what shows nowhere itself.
pub(crate) fn keep_place(doc: &Document, id: NodeId, was: &Affine) -> Result<Vec<Edit>, DocError> {
    keep_place_in(doc, id, was, None)
}

/// The same, where the coordinates `id` is now written in aren't its
/// parent's by the tree: `now` takes them to the document's (a clip
/// path's shapes are in the coordinates of what they clip).
pub(crate) fn keep_place_in(doc: &Document, id: NodeId, was: &Affine, now: Option<&Affine>) -> Result<Vec<Edit>, DocError> {
    let mut settle = Settle::new(doc);
    let node = doc.node(id)?;
    let now = now.copied().unwrap_or_else(|| settle.parent_to_doc(node));
    if !placeable(node) || settle.p.same(was, &now) {
        return Ok(Vec::new());
    }
    let Some(down) = now.inverse() else { return invalid(format!("{id} can't stay where it shows there: it would be under a transform that squashes everything flat")) };
    let own = transform::of(node, settle.view);
    let to = own.unwrap_or(Affine::IDENTITY).then(was).then(&down);
    let left = settle.give(node, to, &settle.inherited(node));
    settle.write(node, own.as_ref(), left);
    Ok(settle.edits)
}

/// The edits that give everything in the group `id` the group's own
/// transform, each thing for itself: what it takes for the group to go
/// and leave them showing where they did.
pub(crate) fn dissolve(doc: &Document, id: NodeId) -> Vec<Edit> {
    let mut settle = Settle::new(doc);
    let Some((group, over)) = doc.get(id).and_then(|g| Some((g, transform::of(g, settle.view)?))) else { return Vec::new() };
    for child in group.elements().filter_map(|c| doc.get(c)).filter(|c| placeable(c)) {
        let own = transform::of(child, settle.view);
        let left = settle.give(child, own.unwrap_or(Affine::IDENTITY).then(&over), &settle.inherited(child));
        settle.write(child, own.as_ref(), left);
    }
    settle.edits
}
