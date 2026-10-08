//! What a node is drawn with, and what that lets its numbers take
//! (see [`super`]): its stroke, its paint, its filter and its clip
//! path. And the two things that go along with a node when they are
//! its alone: a gradient in its own coordinates, and its clip path.

use ink_geom::{Affine, Vec2};

use super::{Edit, Settle};
use crate::gradient::{Gradient, Units};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::length::number;
use crate::node::Node;
use crate::refs;
use crate::shape::Geometry;
use crate::style::{Paint, Style, prop, url_id};
use crate::transform;
use crate::value::Precision;

/// A stroke as it's drawn: what grows with its shape.
pub(super) struct Outline {
    pub(super) width: f64,
    pub(super) dashes: Vec<f64>,
    pub(super) offset: f64,
}

/// What may go into a shape's numbers without changing how it looks.
pub(super) struct Limits {
    /// Nothing: something it's drawn with would be left behind.
    pub(super) held: bool,
    /// A move only: its shadow wouldn't turn or grow with it.
    pub(super) move_only: bool,
    /// No turn and no mirror: a gradient across its box wouldn't
    /// follow, nor dashes along an outline that starts where its kind
    /// says.
    pub(super) level: bool,
    /// An even scale only, its stroke growing by as much.
    pub(super) stroke: Option<Outline>,
    /// Gradients laid out in its own coordinates that nothing else
    /// uses: they go wherever its numbers go.
    pub(super) own: Vec<NodeId>,
    /// Its clip path, when that is its alone: it goes along too.
    pub(super) clip: Option<NodeId>,
}

impl Limits {
    /// Whether `b` may go into the numbers; if so, how much the stroke
    /// grows.
    pub(super) fn allows(&self, b: &Affine, p: &Precision) -> Option<f64> {
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

/// What cuts a node to shape.
pub(super) enum Cut<'a> {
    Nothing,
    /// A clip path that is this node's alone, in its own coordinates:
    /// it goes where the node's numbers go.
    Own(&'a Node),
    /// Something that would be left behind: a clip path others use (or
    /// that isn't there to look at, or is cut by another), a mask.
    Held,
}

/// How a filter sits on what it filters.
pub(super) enum Filtered {
    No,
    /// Its region and its shadows go where the node's box goes.
    Follows,
    /// Laid out in the node's coordinates: it stays where they are.
    Fixed,
}

impl<'a> Settle<'a> {
    /// Whether the gradient `server` is `node`'s alone, to take along:
    /// nothing else names it, and it takes nothing from another.
    pub(super) fn is_own(&self, server: &Node, node: &Node) -> bool {
        let users = server.attr("id").and_then(|id| self.users.get(id));
        users.is_some_and(|users| users.as_slice() == [node.id]) && refs::href(server).is_none()
    }

    /// The edits that put the gradient `server` (in its user's own
    /// coordinates) through `b` along with its user. Into its own
    /// numbers when they're plain ones and all it's been through is a
    /// move, an even scale, a turn or a mirror (only then do its colours
    /// stay square to its line); as its `gradientTransform` otherwise.
    pub(super) fn carry(&mut self, server: &Node, b: &Affine) {
        let radial = server.kind == Kind::RadialGradient;
        let said = server.attr("gradientTransform").map(transform::parse);
        let through = said.unwrap_or(Affine::IDENTITY).then(b);
        let num = |name: &str| server.attr(name).and_then(number);
        // Its line or circle, when every number of it is said, and
        // plainly: `None` otherwise.
        let points: Option<Vec<(&'static str, &'static str, Vec2)>> = if radial {
            let centre = num("cx").zip(num("cy")).map(|(x, y)| ("cx", "cy", Vec2::new(x, y)));
            let focus = match (server.attr("fx"), server.attr("fy")) {
                (None, None) => Some(None),
                _ => num("fx").zip(num("fy")).map(|(x, y)| Some(("fx", "fy", Vec2::new(x, y)))),
            };
            centre.zip(focus).filter(|_| num("r").is_some()).map(|(c, f)| std::iter::once(c).chain(f).collect())
        } else {
            let from = num("x1").zip(num("y1")).map(|(x, y)| ("x1", "y1", Vec2::new(x, y)));
            let to = num("x2").zip(num("y2")).map(|(x, y)| ("x2", "y2", Vec2::new(x, y)));
            from.zip(to).map(|(a, b)| vec![a, b])
        };
        let reach = points.iter().flatten().map(|(_, _, at)| at.abs().max_element()).fold(0.0, f64::max);
        let p = self.p.reaching(reach);
        let even = through.axes().filter(|axes| axes.is_uniform(reach.max(1.0), p.within()) && p.same(&axes.linear(), &through.without_move()));
        let mut set = |name: &'static str, value: Option<String>| self.edits.push(Edit::Attr { node: server.id, name, value });
        let (Some(points), Some(axes)) = (points, even) else {
            if said.is_none_or(|said| !p.same(&said, &through)) {
                set("gradientTransform", p.transform(&through));
            }
            return;
        };
        // A number that comes out as it was written stays as written.
        let mut number_to = |name: &'static str, v: f64| {
            if server.attr(name).and_then(number).is_none_or(|was| p.number(was) != p.number(v)) {
                set(name, Some(p.number(v)));
            }
        };
        for (x, y, at) in points {
            let at = through.apply(at);
            number_to(x, at.x);
            number_to(y, at.y);
        }
        if let Some(r) = num("r").filter(|_| radial) {
            number_to("r", r * axes.sx);
        }
        if said.is_some() {
            set("gradientTransform", None);
        }
    }

    pub(super) fn cut(&self, node: &Node) -> Cut<'a> {
        let said = |name: &str| prop(node, name).filter(|v| *v != "none");
        let Some(clip) = said("clip-path").filter(|_| said("mask").is_none()) else {
            return if said("mask").is_some() { Cut::Held } else { Cut::Nothing };
        };
        let Some((id, clip)) = url_id(clip).and_then(|(id, _)| Some((id, self.doc.get(self.ids.get(id)?)?))).filter(|(_, n)| n.kind == Kind::ClipPath) else { return Cut::Held };
        let alone = self.users.get(id).is_some_and(|users| users.as_slice() == [node.id]);
        let boxed = clip.attr("clipPathUnits").and_then(Units::parse) == Some(Units::BBox);
        let cut_itself = ["clip-path", "mask"].iter().any(|name| prop(clip, name).is_some_and(|v| v != "none"));
        if alone && !boxed && !cut_itself { Cut::Own(clip) } else { Cut::Held }
    }

    /// Put the clip path `clip` through `b` along with what it clips:
    /// into its shapes' own numbers when they can all take it, as a
    /// group's would, and on the clip path's own `transform` otherwise.
    pub(super) fn carry_clip(&mut self, clip: &Node, b: &Affine) {
        let own = transform::of(clip, self.view);
        let left = self.group(clip, own.unwrap_or(Affine::IDENTITY).then(b), &Style::default());
        self.write(clip, own.as_ref(), left);
    }

    pub(super) fn has_markers(&self, node: &Node) -> bool {
        let said = |n: &Node| ["marker", "marker-start", "marker-mid", "marker-end"].iter().any(|name| prop(n, name).is_some_and(|v| v != "none"));
        said(node) || self.doc.ancestors(node.id).any(said)
    }

    pub(super) fn filtered(&self, node: &Node) -> Filtered {
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
    pub(super) fn limits(&self, node: &Node, geometry: &Geometry, style: &Style) -> Limits {
        let stroked = style.stroke != Paint::None && style.line.width > 0.0;
        let cut = self.cut(node);
        let mut limits = Limits { held: matches!(cut, Cut::Held) || self.has_markers(node), move_only: false, level: false, stroke: None, own: Vec::new(), clip: None };
        if let Cut::Own(clip) = cut {
            limits.clip = Some(clip.id);
        }
        for paint in [Some(&style.fill), stroked.then_some(&style.stroke)].into_iter().flatten() {
            let Paint::Server { id, .. } = paint else { continue };
            match self.ids.get(id).and_then(|id| self.doc.get(id)) {
                Some(server) if matches!(server.kind, Kind::LinearGradient | Kind::RadialGradient) => match Gradient::of(self.doc, &self.ids, server).map(|g| g.units) {
                    // In the node's own coordinates: it goes along if
                    // it's this node's alone. One that other shapes use
                    // is never touched, and holds the node where it is.
                    Some(Units::UserSpace) if self.is_own(server, node) => {
                        if !limits.own.contains(&server.id) {
                            limits.own.push(server.id);
                        }
                    }
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
            // Resized, the line stays as it is: it asks nothing of the
            // numbers.
            if !self.keep {
                limits.stroke = Some(Outline { width: style.line.width, dashes: style.line.dashes.clone(), offset: style.line.dash_offset });
            }
        }
        limits
    }
}
