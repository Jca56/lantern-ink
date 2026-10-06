//! Documents and nodes in words (and data) for the model: the ids it
//! needs, front to back as a layers panel shows them.

use std::collections::HashMap;

use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::style::prop;
use ink_core::ink_doc::{Document, Kind, Node, Viewport};
use ink_core::{Actor, Core, DocId, NodeId};
use ink_geom::{Rect, number};
use lntrn_data::{Doc, Map};
use lntrn_mcp::ToolError;

use crate::input::refused;

/// The most nodes one listing spells out.
const MAX_LISTED: usize = 300;

pub(crate) fn actor(a: Actor) -> &'static str {
    match a {
        Actor::Alva => "Alva",
        Actor::Claude => "Claude",
    }
}

/// A number as a person reads it: two decimals at most.
pub(crate) fn n(v: f64) -> String {
    number::format(v, 2)
}

/// `4,4 16×16`: a box's corner and size.
pub(crate) fn rect(r: &Rect) -> String {
    format!("{},{} {}×{}", n(r.min.x), n(r.min.y), n(r.width()), n(r.height()))
}

/// `<rect>`, with its `id` if it has one: `<rect id="door">`.
pub(crate) fn tag(node: &Node) -> String {
    match node.attr("id") {
        Some(id) => format!("<{} id=\"{id}\">", node.name),
        None => format!("<{}>", node.name),
    }
}

/// Why a node doesn't show, if it's of a kind that doesn't.
pub(crate) fn undrawn(node: &Node) -> Option<&'static str> {
    match node.kind {
        Kind::Text | Kind::TSpan | Kind::Image | Kind::Use => Some("not drawn yet"),
        Kind::Other => Some("kept as it is, not drawn"),
        _ if prop(node, "display") == Some("none") => Some("display: none"),
        _ => None,
    }
}

/// One line about a node: its id, what it is, the paint it gives itself,
/// and where it shows.
fn node_line(node: &Node, bounds: Option<&Rect>) -> String {
    let mut line = format!("{} {}", node.id, node.name);
    if let Some(id) = node.attr("id") {
        line += &format!(" #{id}");
    }
    if let Some(fill) = prop(node, "fill") {
        line += &format!("  fill {fill}");
    }
    if let Some(stroke) = prop(node, "stroke") {
        line += &format!("  stroke {stroke}");
        if let Some(width) = prop(node, "stroke-width") {
            line += &format!(" {width}");
        }
    }
    if let Some(opacity) = prop(node, "opacity") {
        line += &format!("  opacity {opacity}");
    }
    if node.attr("transform").is_some() {
        line += "  transformed";
    }
    if let Some(b) = bounds {
        line += &format!("  at {}", rect(b));
    }
    if let Some(why) = undrawn(node) {
        line += &format!("  ({why})");
    }
    line
}

fn node_data(node: &Node, depth: usize, bounds: Option<&Rect>) -> Doc {
    let mut m = Map::new();
    m.insert("id", node.id.to_string().into());
    m.insert("element", node.name.as_str().into());
    m.insert("depth", Doc::Int(depth as i64));
    if let Some(id) = node.attr("id") {
        m.insert("svg_id", id.into());
    }
    if let Some(b) = bounds {
        m.insert("box", Doc::List([b.min.x, b.min.y, b.width(), b.height()].into_iter().map(Doc::from).collect()));
    }
    Doc::Map(m)
}

/// A listing of nodes being put together.
struct Listing<'a> {
    doc: &'a Document,
    bounds: HashMap<NodeId, Rect>,
    lines: Vec<String>,
    data: Vec<Doc>,
    /// How many more nodes it will spell out, and how many it has had
    /// to leave out.
    left: usize,
    skipped: usize,
}

impl Listing<'_> {
    /// The tree under `id`, front to back (what's later in the file, and
    /// so on top, first), each level indented under its parent.
    fn tree(&mut self, id: NodeId, depth: usize) {
        let Some(node) = self.doc.get(id) else { return };
        if self.left == 0 {
            self.skipped += self.doc.descendants(id).len();
            return;
        }
        self.left -= 1;
        self.lines.push(format!("{}{}", "  ".repeat(depth), node_line(node, self.bounds.get(&id))));
        self.data.push(node_data(node, depth, self.bounds.get(&id)));
        for child in node.elements().rev() {
            self.tree(child, depth + 1);
        }
    }
}

/// A document's file as a name to call it by.
pub(crate) fn name(core: &Core, id: DocId) -> String {
    match core.path(id).ok().flatten().and_then(|p| p.file_name()) {
        Some(file) => format!("\"{}\"", file.to_string_lossy()),
        None => "(no file yet)".to_owned(),
    }
}

/// How a document stands against its file.
pub(crate) fn saved(core: &Core, id: DocId) -> &'static str {
    match (core.path(id).ok().flatten().is_some(), core.is_modified(id).unwrap_or(true)) {
        (false, _) => "never saved",
        (true, true) => "unsaved changes",
        (true, false) => "saved",
    }
}

/// `24 × 24, viewBox 0 0 24 24`: the page, and the coordinates on it.
pub(crate) fn page(doc: &Document) -> String {
    let Some(root) = doc.get(doc.root()) else { return String::new() };
    let v = Viewport::of(root);
    match root.attr("viewBox") {
        Some(view_box) => format!("{} × {}, viewBox {}", n(v.size.x), n(v.size.y), view_box.split_whitespace().collect::<Vec<_>>().join(" ")),
        None => format!("{} × {}", n(v.size.x), n(v.size.y)),
    }
}

/// One line for `doc_list`.
pub(crate) fn summary(core: &Core, id: DocId) -> Result<String, ToolError> {
    let doc = core.doc(id).map_err(refused)?;
    Ok(format!("{id} {}: page {}, {} nodes, {}", name(core, id), page(doc), doc.len(), saved(core, id)))
}

/// Everything `doc_info` says: text for the model, and the same as data.
pub(crate) fn info(core: &Core, id: DocId) -> Result<(String, Doc), ToolError> {
    let doc = core.doc(id).map_err(refused)?;
    let history = core.history(id).map_err(refused)?;
    let mut listing = Listing { doc, bounds: page_bounds(doc), lines: Vec::new(), data: Vec::new(), left: MAX_LISTED, skipped: 0 };
    listing.tree(doc.root(), 0);
    let Listing { lines, data: nodes, skipped, .. } = listing;
    let (undo, redo) = (history.undoable().count(), history.redoable().count());
    let mut text = format!("{id} {}", name(core, id));
    if let Ok(Some(path)) = core.path(id) {
        text += &format!(" ({})", path.display());
    }
    text += &format!(", {}.\nPage {}. {} nodes. Undo: {undo}", saved(core, id), page(doc), doc.len());
    if let Some(latest) = history.undoable().next_back() {
        text += &format!(" (latest: \"{}\" by {})", latest.label, actor(latest.actor));
    }
    text += &format!(". Redo: {redo}.\nNodes, front to back (the first listed is on top; boxes are where each shows in the drawing's coordinates, strokes aside):\n{}", lines.join("\n"));
    if skipped > 0 {
        text += &format!("\n… and {skipped} more (doc_source shows the whole file, or one node's markup)");
    }
    let mut m = Map::new();
    m.insert("doc_id", id.to_string().into());
    m.insert("path", core.path(id).ok().flatten().map_or(Doc::Null, |p| p.display().to_string().into()));
    m.insert("modified", core.is_modified(id).unwrap_or(true).into());
    m.insert("root", doc.root().to_string().into());
    m.insert("node_count", Doc::Int(doc.len() as i64));
    m.insert("undo", Doc::Int(undo as i64));
    m.insert("redo", Doc::Int(redo as i64));
    m.insert("nodes", Doc::List(nodes));
    Ok((text, Doc::Map(m)))
}
