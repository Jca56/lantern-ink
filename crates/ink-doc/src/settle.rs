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
//!   gradient across its box doesn't turn with baked numbers, so a turn
//!   or a mirror stays in `transform` (so does one of a dashed rect,
//!   circle or ellipse, whose dashes start where its kind says). A clip
//!   path, a mask, markers and
//!   a gradient or filter laid out in its own coordinates would be left
//!   behind, so then everything stays in `transform`; a shadow would
//!   not turn or grow, so a filtered node takes only a move.
//! - **A group passes it down** to what's in it when everything there
//!   can take it without being left a transform it didn't have, and
//!   keeps it as its own `transform` otherwise.

use ink_geom::{Affine, Vec2};

use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::gradient::{Gradient, Units};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::Node;
use crate::refs::Ids;
use crate::shape::Geometry;
use crate::style::{Paint, Style, prop, url_id};
use crate::transform;
use crate::value::Precision;
use crate::viewport::Viewport;

/// One change to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Attr { node: NodeId, name: &'static str, value: Option<String> },
    /// A property, written where the node has it (D14).
    Prop { node: NodeId, name: &'static str, value: Option<String> },
}

/// A stroke as it's drawn: what grows with its shape.
struct Outline {
    width: f64,
    dashes: Vec<f64>,
    offset: f64,
}

/// What may go into a shape's numbers without changing how it looks.
struct Limits {
    /// Nothing: something it's drawn with would be left behind.
    held: bool,
    /// A move only: its shadow wouldn't turn or grow with it.
    move_only: bool,
    /// No turn and no mirror: a gradient across its box wouldn't
    /// follow, nor dashes along an outline that starts where its kind
    /// says.
    level: bool,
    /// An even scale only, its stroke growing by as much.
    stroke: Option<Outline>,
}

impl Limits {
    /// Whether `b` may go into the numbers; if so, how much the stroke
    /// grows.
    fn allows(&self, b: &Affine, p: &Precision) -> Option<f64> {
        if p.same(b, &Affine::IDENTITY) {
            return Some(1.0);
        }
        let linear = b.without_move();
        if self.held || (self.move_only && !p.same(&linear, &Affine::IDENTITY)) {
            return None;
        }
        if self.level && !(b.a > 0.0 && b.d > 0.0 && p.same(&Affine::scale(b.a, b.d), &linear)) {
            return None;
        }
        let Some(stroke) = &self.stroke else { return Some(1.0) };
        let axes = b.axes()?;
        (p.same(&axes.linear(), &linear) && axes.is_uniform(stroke.width, p.within())).then_some(axes.sx)
    }
}

/// How a filter sits on what it filters.
enum Filtered {
    No,
    /// Its region and its shadows go where the node's box goes.
    Follows,
    /// Laid out in the node's coordinates: it stays where they are.
    Fixed,
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
    pub edits: Vec<Edit>,
}

impl<'a> Settle<'a> {
    pub fn new(doc: &'a Document) -> Settle<'a> {
        let view = doc.get(doc.root()).map_or(Vec2::ZERO, |root| Viewport::of(root).view);
        Settle { doc, ids: Ids::of(doc), view, p: Precision::of(doc), edits: Vec::new() }
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

    fn is_clipped(&self, node: &Node) -> bool {
        ["clip-path", "mask"].iter().any(|name| prop(node, name).is_some_and(|v| v != "none"))
    }

    fn has_markers(&self, node: &Node) -> bool {
        let said = |n: &Node| ["marker", "marker-start", "marker-mid", "marker-end"].iter().any(|name| prop(n, name).is_some_and(|v| v != "none"));
        said(node) || self.doc.ancestors(node.id).any(said)
    }

    fn filtered(&self, node: &Node) -> Filtered {
        let Some(said) = prop(node, "filter").filter(|v| *v != "none") else { return Filtered::No };
        let Some(filter) = url_id(said).and_then(|(id, _)| self.ids.get(id)).and_then(|id| self.doc.get(id)).filter(|f| f.kind == Kind::Filter) else {
            // Nothing Ink can look into: taken as the kind that follows.
            return Filtered::Follows;
        };
        let user = |name: &str, default: Units| filter.attr(name).and_then(Units::parse).unwrap_or(default) == Units::UserSpace;
        // A step given a place of its own is in the node's coordinates
        // too, unless the filter says its steps go by the box.
        let placed = filter.elements().filter_map(|id| self.doc.get(id)).any(|step| ["x", "y", "width", "height"].iter().any(|a| step.attr(a).is_some()));
        if user("filterUnits", Units::BBox) || (placed && user("primitiveUnits", Units::UserSpace)) { Filtered::Fixed } else { Filtered::Follows }
    }

    /// What may go into the numbers of `node` (which is `geometry`),
    /// drawn as `style` says.
    fn limits(&self, node: &Node, geometry: &Geometry, style: &Style) -> Limits {
        let stroked = style.stroke != Paint::None && style.line.width > 0.0;
        let mut limits = Limits { held: self.is_clipped(node) || self.has_markers(node), move_only: false, level: false, stroke: None };
        for paint in [Some(&style.fill), stroked.then_some(&style.stroke)].into_iter().flatten() {
            let Paint::Server { id, .. } = paint else { continue };
            match self.ids.get(id).and_then(|id| self.doc.get(id)) {
                Some(server) if matches!(server.kind, Kind::LinearGradient | Kind::RadialGradient) => match Gradient::of(self.doc, &self.ids, server).map(|g| g.units) {
                    Some(Units::UserSpace) => limits.held = true,
                    _ => limits.level = true,
                },
                Some(server) if server.kind == Kind::Pattern => limits.held = true,
                // Nothing there: it's painted a plain colour, or not.
                _ => {}
            }
        }
        match self.filtered(node) {
            Filtered::No => {}
            Filtered::Follows => limits.move_only = true,
            Filtered::Fixed => limits.held = true,
        }
        if stroked {
            // A rect, a circle and an ellipse start their outline where
            // their kind says, so dashes along one would start
            // somewhere else once it's turned or mirrored.
            limits.level |= !style.line.dashes.is_empty() && matches!(geometry, Geometry::Rect { .. } | Geometry::Circle { .. } | Geometry::Ellipse { .. });
            limits.stroke = Some(Outline { width: style.line.width, dashes: style.line.dashes.clone(), offset: style.line.dash_offset });
        }
        limits
    }

    /// Put `b` into `node`'s numbers, if it may go there and a shape of
    /// its kind can be what `b` makes of it. Returns the shape it now
    /// is, as the file will hold it.
    fn bake(&mut self, node: &Node, geometry: &Geometry, b: &Affine, limits: &Limits, p: &Precision) -> Option<Geometry> {
        let grow = limits.allows(b, p)?;
        let baked = geometry.through(b, p)?.rounded(p);
        for (name, value) in baked.write(node, p) {
            self.edits.push(Edit::Attr { node: node.id, name, value });
        }
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
        let passes = hold.is_some()
            && !self.is_clipped(node)
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
        None
    }

    /// Give `node` the transform `to` (from its own coordinates to its
    /// parent's). Returns what of it is left for its `transform`.
    fn give(&mut self, node: &Node, to: Affine, inherited: &Style) -> Option<Affine> {
        match node.kind {
            kind if kind.is_shape() => self.shape(node, to, inherited),
            Kind::G | Kind::A | Kind::Switch => self.group(node, to, inherited),
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

/// The edits that put `nodes` through `by`. One named along with a
/// group it's in goes with the group, once.
pub(crate) fn plan(doc: &Document, nodes: &[NodeId], by: &Affine) -> Result<Vec<Edit>, DocError> {
    if !by.is_finite() || by.inverse().is_none() {
        return invalid("that transform squashes everything flat (or isn't numbers): give one that leaves things some size");
    }
    for &id in nodes {
        doc.node(id)?;
    }
    let mut settle = Settle::new(doc);
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

/// The edits that leave `id` showing where it did, now that what it's
/// in has changed: `was` is [`parent_to_doc`] as it was before. Nothing
/// for what shows nowhere itself.
pub(crate) fn keep_place(doc: &Document, id: NodeId, was: &Affine) -> Result<Vec<Edit>, DocError> {
    let mut settle = Settle::new(doc);
    let node = doc.node(id)?;
    let now = settle.parent_to_doc(node);
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
