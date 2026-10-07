//! The outline a shape element draws, from its own attributes: a
//! `<rect>`'s x, y, width, height and radii, a `<path>`'s `d`.

use std::collections::HashMap;

use ink_geom::number::parse_list;
use ink_geom::{Affine, Path, Rect, Vec2};

use crate::document::Document;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::length::number;
use crate::node::Node;
use crate::style::prop;
use crate::transform;
use crate::viewport::Viewport;

/// The path `node` draws, in its own coordinates. Empty for what isn't a
/// shape, and for a shape with no size.
pub fn path_of(node: &Node) -> Path {
    let n = |name: &str| node.attr(name).and_then(number).unwrap_or(0.0);
    let some = |name: &str| node.attr(name).and_then(number);
    match node.kind {
        // What can be read of a path is drawn, up to where it stops
        // being path data.
        Kind::Path => node.attr("d").map(|d| Path::parse(d).path).unwrap_or_default(),
        Kind::Rect => {
            // One corner radius given is both.
            let (rx, ry) = (some("rx"), some("ry"));
            Path::rect(n("x"), n("y"), n("width"), n("height"), rx.or(ry).unwrap_or(0.0), ry.or(rx).unwrap_or(0.0))
        }
        Kind::Circle => Path::ellipse(Vec2::new(n("cx"), n("cy")), n("r"), n("r")),
        Kind::Ellipse => Path::ellipse(Vec2::new(n("cx"), n("cy")), n("rx"), n("ry")),
        Kind::Line => Path::polyline(&[Vec2::new(n("x1"), n("y1")), Vec2::new(n("x2"), n("y2"))], false),
        Kind::Polyline | Kind::Polygon => {
            // An odd number left over is dropped; a list that can't be
            // read draws nothing.
            let numbers = node.attr("points").and_then(parse_list).unwrap_or_default();
            let points: Vec<Vec2> = numbers.chunks_exact(2).map(|p| Vec2::new(p[0], p[1])).collect();
            Path::polyline(&points, node.kind == Kind::Polygon)
        }
        _ => Path::new(),
    }
}

/// From `id`'s own coordinates to the document's: its own transform,
/// then every transform it's under.
pub fn to_doc(doc: &Document, id: NodeId) -> Option<Affine> {
    crate::settle::node_to_doc(doc, id).ok()
}

/// Where every drawn node sits in the drawing: the box around its
/// outline (for a group, around everything in it), strokes aside, in the
/// document's coordinates (the root's user units), through whatever
/// transforms it's under. Nodes that draw nothing have no box.
pub fn page_bounds(doc: &Document) -> HashMap<NodeId, Rect> {
    fn walk(doc: &Document, node: &Node, parent: &Affine, view: Vec2, out: &mut HashMap<NodeId, Rect>) -> Option<Rect> {
        if !(node.kind.is_group() || node.kind.is_shape()) || prop(node, "display") == Some("none") {
            return None;
        }
        // The root's own transform isn't one: its coordinates are the
        // document's.
        let t = if node.parent.is_none() { *parent } else { transform::of(node, view).map_or(*parent, |t| t.then(parent)) };
        let mut all = path_of(node).bounds_through(&t);
        for child in node.elements().filter_map(|id| doc.get(id)) {
            if let Some(b) = walk(doc, child, &t, view, out) {
                all = Some(all.map_or(b, |a| a.union(&b)));
            }
        }
        if let Some(b) = all {
            out.insert(node.id, b);
        }
        all
    }
    let mut out = HashMap::new();
    if let Some(root) = doc.get(doc.root()) {
        walk(doc, root, &Affine::IDENTITY, Viewport::of(root).view, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    fn path(element: &str) -> Path {
        let d = Document::parse(DocId(1), &format!("<svg>{element}</svg>")).unwrap();
        path_of(d.node(NodeId(2)).unwrap())
    }

    #[test]
    fn shapes_draw_what_their_attributes_say() {
        assert_eq!(path(r#"<rect x="1" y="2" width="10" height="4"/>"#).to_data(3), "M1 2 H11 V6 H1 Z");
        assert_eq!(path(r#"<rect width="10" height="4" rx="2"/>"#), Path::rect(0.0, 0.0, 10.0, 4.0, 2.0, 2.0), "one radius given is both");
        assert_eq!(path(r#"<rect width="10" height="4" ry="1pt"/>"#), Path::rect(0.0, 0.0, 10.0, 4.0, 4.0 / 3.0, 4.0 / 3.0));
        assert!(path(r#"<rect width="10"/>"#).is_empty(), "no height, no shape");
        assert_eq!(path(r#"<circle cx="5" cy="6" r="2"/>"#), Path::ellipse(Vec2::new(5.0, 6.0), 2.0, 2.0));
        assert_eq!(path(r#"<ellipse rx="3" ry="2"/>"#), Path::ellipse(Vec2::ZERO, 3.0, 2.0));
        assert_eq!(path(r#"<line x1="1" y1="2" x2="3" y2="4"/>"#).to_data(3), "M1 2 L3 4");
        assert_eq!(path(r#"<polygon points="0,0 4,0 4,4 9"/>"#).to_data(3), "M0 0 H4 V4 Z");
        assert_eq!(path(r#"<polyline points="0 0 4 0"/>"#).to_data(3), "M0 0 H4");
        assert!(path(r#"<polyline points="0 0 four 0"/>"#).is_empty());
        assert_eq!(path(r#"<path d="M0 0 L5 5 nonsense"/>"#).to_data(3), "M0 0 L5 5", "a path is drawn as far as it reads");
        assert!(path("<g/>").is_empty() && path("<path/>").is_empty());
    }

    #[test]
    fn nodes_have_boxes_where_they_show() {
        let d = Document::parse(
            DocId(1),
            r#"<svg viewBox="0 0 100 100"><g transform="translate(10 20) scale(2)"><rect x="1" y="2" width="3" height="4"/><circle cx="10" cy="10" r="5" transform="rotate(45 10 10)"/></g><rect width="8" height="8" transform="rotate(45)"/><defs><rect width="50" height="50"/></defs><g/><rect width="5" height="5" display="none"/></svg>"#,
        )
        .unwrap();
        let boxes = page_bounds(&d);
        let near = |id: u64, x: f64, y: f64, w: f64, h: f64| {
            let b = boxes[&NodeId(id)];
            assert!((b.min.x - x).abs() < 1e-3 && (b.min.y - y).abs() < 1e-3 && (b.width() - w).abs() < 1e-3 && (b.height() - h).abs() < 1e-3, "N{id}: {b:?}");
        };
        near(3, 12.0, 24.0, 6.0, 8.0);
        near(4, 20.0, 30.0, 20.0, 20.0);
        near(2, 12.0, 24.0, 28.0, 26.0);
        // A square turned 45°: its box is its diagonal across, not its
        // old box's corners turned.
        let d8 = 8.0 * std::f64::consts::SQRT_2;
        near(5, -d8 / 2.0, 0.0, d8, d8);
        near(1, -d8 / 2.0, 0.0, 40.0 + d8 / 2.0, 50.0);
        for undrawn in [6, 7, 8, 9] {
            assert!(!boxes.contains_key(&NodeId(undrawn)), "N{undrawn}");
        }
    }
}
