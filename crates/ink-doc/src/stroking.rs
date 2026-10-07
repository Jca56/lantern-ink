//! A stroke made a shape of its own (ARCHITECTURE §3.4): what "outline
//! stroke" does to a line. The shape's stroke becomes a `<path>` that
//! covers what the stroke covered, filled with what the stroke was
//! painted with; the work is `ink-geom`'s ([`outline_stroke`]), with
//! the stroke's curves kept wherever a stroke's edge has a shape a path
//! can write.
//!
//! A shape with no fill becomes that path itself. One with a fill
//! keeps it: the shape stays, without its stroke, and the outline is a
//! new path over it (under it, where `paint-order` puts strokes under).

use ink_geom::{number, outline_stroke};

use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::node::Node;
use crate::style::{Paint, Style, prop};
use crate::value::Precision;

/// What a shape has as one thing, fill and stroke together: where it
/// is, how far it fades, what filters, clips and masks it.
const AS_ONE: [&str; 6] = ["opacity", "filter", "clip-path", "mask", "transform", "transform-origin"];

/// What says how a stroke is drawn, and means nothing without one.
const OF_A_STROKE: [&str; 7] = ["stroke-width", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-dasharray", "stroke-dashoffset", "stroke-opacity"];

impl Document {
    /// How `id` is drawn: what comes down the tree to it, and what it
    /// says itself.
    pub(crate) fn style_of(&self, id: NodeId) -> Result<Style, DocError> {
        let node = self.node(id)?;
        let mut above: Vec<&Node> = self.ancestors(id).collect();
        above.reverse();
        Ok(above.into_iter().chain([node]).fold(Style::default(), |style, node| style.cascade(node)))
    }

    /// The property `name` as `id` says it, or the nearest group above
    /// that does.
    fn handed(&self, id: NodeId, name: &str) -> Option<String> {
        self.get(id).into_iter().chain(self.ancestors(id)).find_map(|node| prop(node, name).map(str::to_owned))
    }

    /// Make the stroke of the shape `id` a shape of its own. The nodes
    /// made on the way: none when the shape itself became the outline;
    /// the outline, when the shape had a fill to keep; and the group
    /// the two are in, when the shape faded or was filtered, clipped or
    /// masked as one thing (the group does that now, to both).
    ///
    /// `tolerance` is how near the stroke's true edge the outline's
    /// fitted curves keep (lines and circles' arcs are exact whatever
    /// it is): a two-hundredth of the stroke's width if not said, and
    /// never finer than the file can write.
    pub(crate) fn stroke_to_shape(&mut self, id: NodeId, tolerance: Option<f64>) -> Result<Vec<NodeId>, DocError> {
        if tolerance.is_some_and(|t| !(t > 0.0 && t.is_finite())) {
            return invalid("a tolerance says how near the stroke's edge the outline keeps: it has to be more than nothing");
        }
        let style = self.style_of(id)?;
        let (path, _) = self.filled(id)?;
        let paint = self.handed(id, "stroke").filter(|_| style.stroke != Paint::None && style.line.width > 0.0);
        let Some(paint) = paint else { return invalid(format!("{id} has no stroke to outline (its stroke is none, or has no width): node_style gives it one")) };
        let precision = Precision::of(self);
        let near = tolerance.unwrap_or(style.line.width / 200.0).max(precision.within());
        let Ok(made) = outline_stroke(&path, &style.line, near, precision.within()) else {
            return invalid(format!("{id}'s stroke lies on itself in a way Ink can't work out, so nothing was changed (a slightly different width, or node_set-ing a simpler d, gets round it)"));
        };
        if made.is_empty() {
            return invalid(format!("{id}'s stroke covers nothing (its line has no length, or its dashes are all gaps): nothing was changed"));
        }
        // What the outline would be filled with if it said nothing: what
        // its groups hand down.
        let handed_opacity = self.ancestors(id).collect::<Vec<_>>().into_iter().rev().fold(Style::default(), |style, node| style.cascade(node)).fill_opacity;
        let keeps_fill = style.fill != Paint::None;
        let mut made_nodes = Vec::new();
        let outline = if keeps_fill {
            let copy = self.duplicate(id)?;
            if style.stroke_first {
                self.relocate(copy, Place::Before(id))?;
            }
            made_nodes.push(copy);
            // What it had as one thing can't be had twice over: a
            // shape half see-through would show its fill through its
            // stroke's outline.
            let node = self.node(id)?;
            let as_one: Vec<(&str, String)> = AS_ONE.into_iter().filter_map(|name| prop(node, name).map(|value| (name, value.to_owned()))).collect();
            if as_one.iter().any(|(name, value)| !(name.starts_with("transform") || *name == "opacity" && value.trim() == "1")) {
                let group = self.group(&[id, copy])?;
                for (name, value) in &as_one {
                    self.set_attr(group, name, Some(value))?;
                    self.set_prop(id, name, None)?;
                    self.set_prop(copy, name, None)?;
                }
                made_nodes.push(group);
            }
            // The shape itself keeps its fill and has no stroke now.
            self.set_prop(id, "stroke", Some("none"))?;
            for name in OF_A_STROKE {
                self.set_prop(id, name, None)?;
            }
            copy
        } else {
            id
        };
        self.make_path(outline)?;
        self.set_attr(outline, "d", Some(&precision.path(&made)))?;
        self.set_prop(outline, "fill", Some(&paint))?;
        self.set_prop(outline, "stroke", Some("none"))?;
        for name in OF_A_STROKE {
            self.set_prop(outline, name, None)?;
        }
        // As see-through as the stroke was.
        let opacity = (style.stroke_opacity - handed_opacity).abs() > 1e-9;
        self.set_prop(outline, "fill-opacity", opacity.then(|| number::format(style.stroke_opacity, 3)).as_deref())?;
        Ok(made_nodes)
    }
}
