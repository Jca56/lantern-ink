//! Shapes made one (ARCHITECTURE §3.4): union, subtract, intersect and
//! exclude, on what each shape's fill covers. The first shape named
//! takes the result as its outline (a `<path>`, if it wasn't one) and
//! keeps everything else about itself: its place in the drawing, its
//! paint, its id. The others are taken out of the drawing.
//!
//! The work is `ink-geom`'s ([`combine`]); what's here is getting each
//! shape's outline into the first one's coordinates, and by which rule
//! it's filled.

use ink_geom::{Combine, FillRule, Path, combine};

use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::geometry::to_doc;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::shape::Geometry;
use crate::value::Precision;

impl Document {
    /// The outline `id` is filled within, in its own coordinates, and
    /// the rule it's filled by. Only a shape has one.
    pub(crate) fn filled(&self, id: NodeId) -> Result<(Path, FillRule), DocError> {
        let node = self.node(id)?;
        let path = match node.kind {
            Kind::Path => {
                let read = Path::parse(node.attr("d").unwrap_or(""));
                if read.stopped_at.is_some() {
                    return invalid(format!("{id}'s path data can't all be read, so there's no saying what it covers: set its d to path data that reads first"));
                }
                read.path
            }
            Kind::Rect | Kind::Circle | Kind::Ellipse | Kind::Line | Kind::Polyline | Kind::Polygon => match Geometry::of(node) {
                Some(geometry) => geometry.path(),
                None => return invalid(format!("{id}'s numbers can't all be read (one is a percentage, or isn't a number), so there's no saying what it covers")),
            },
            _ => return invalid(format!("{id} is a <{}>: only shapes (paths, rects, circles, ellipses, lines, polygons) have an outline to work on; for a group, name the shapes in it", node.name)),
        };
        // How it's filled comes down the tree to it.
        Ok((path, self.style_of(id)?.fill_rule))
    }

    /// Make the shapes `nodes` one, as `how` says: the first of them
    /// takes the result, the rest are removed. The removed ones, in
    /// order.
    pub(crate) fn combine(&mut self, nodes: &[NodeId], how: Combine) -> Result<Vec<NodeId>, DocError> {
        let Some((&first, rest)) = nodes.split_first() else { return invalid("there's nothing to combine: name the shapes") };
        if rest.is_empty() && how != Combine::Union {
            return invalid("that takes two shapes or more: the first is kept, and the others are taken from it, or met with it (a union of one shape makes its outline simple, where it crosses itself)");
        }
        if let Some(twice) = nodes.iter().enumerate().find_map(|(i, id)| nodes[..i].contains(id).then_some(id)) {
            return invalid(format!("{twice} is named twice: a shape is combined with others, not with itself"));
        }
        // Everything in the first one's own coordinates: where its
        // outline is written.
        let home = to_doc(self, first).and_then(|t| t.inverse());
        let Some(home) = home else { return invalid(format!("{first}'s transform squashes it flat, so nothing can be worked out in its coordinates: give it a transform that can be undone first")) };
        let mut shapes = Vec::with_capacity(nodes.len());
        for &id in nodes {
            let (path, rule) = self.filled(id)?;
            let Some(to_first) = to_doc(self, id).map(|t| t.then(&home)) else { return invalid(format!("{id} is under a transform that can't be read")) };
            shapes.push((if id == first { path } else { path.transformed(&to_first) }, rule));
        }
        let precision = Precision::of(self);
        let borrowed: Vec<(&Path, FillRule)> = shapes.iter().map(|(path, rule)| (path, *rule)).collect();
        let Ok(made) = combine(&borrowed, how, precision.within()) else {
            return invalid("these outlines lie on each other in a way Ink can't work out, so nothing was changed (moving one of them a hair, or node_set-ing a simpler d, gets round it)");
        };
        if made.is_empty() {
            return invalid(match how {
                Combine::Intersect => "the shapes don't overlap anywhere, so nothing would be left: nothing was changed",
                Combine::Subtract => "the others cover all of the first shape, so nothing would be left: nothing was changed (node_delete takes shapes out)",
                _ => "nothing would be left (the shapes cover nothing, or exactly each other): nothing was changed",
            });
        }
        self.make_path(first)?;
        self.set_attr(first, "d", Some(&precision.path(&made)))?;
        for &id in rest {
            self.remove(id)?;
        }
        Ok(rest.to_vec())
    }
}
