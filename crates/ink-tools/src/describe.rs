//! Documents and nodes in words (and data) for the model: the ids it
//! needs, front to back as a layers panel shows them.

use std::collections::HashMap;

use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::outline::Link;
use ink_core::ink_doc::style::prop;
use ink_core::ink_doc::{Document, Kind, Node, Precision, Viewport, text};
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

/// Why a node doesn't show, if it's of a kind that doesn't (or a text
/// that can't be set).
pub(crate) fn undrawn(doc: &Document, node: &Node) -> Option<String> {
    match node.kind {
        Kind::Image | Kind::Use => Some("not drawn yet".to_owned()),
        Kind::Other => Some("kept as it is, not drawn".to_owned()),
        _ if prop(node, "display") == Some("none") => Some("display: none".to_owned()),
        Kind::Text => text::unset(doc, node).map(|why| format!("not drawn: {why}")),
        Kind::TSpan => Some("drawn as part of its <text>".to_owned()),
        _ => None,
    }
}

/// The marks Ink keeps on a node, as a listing shows them: its label
/// in brackets, and whether it's locked.
pub(crate) fn marks(doc: &Document, node: &Node) -> String {
    let label = doc.label(node.id).map_or(String::new(), |label| format!(" [{}]", label.trim()));
    let lock = match doc.lock_over(node.id) {
        Some(lock) if lock == node.id => " (locked)".to_owned(),
        Some(lock) => format!(" (in {lock}, locked)"),
        None => String::new(),
    };
    format!("{label}{lock}")
}

/// The most of a text's words one line quotes.
const MAX_WORDS: usize = 40;

/// What a text says, in quotes, cut short if it's long: its lines
/// with a stroke between them.
pub(crate) fn words(doc: &Document, node: &Node) -> String {
    let said = text::lines(doc, node).join(" / ");
    match said.char_indices().nth(MAX_WORDS) {
        Some((cut, _)) => format!("\"{}…\"", &said[..cut]),
        None => format!("\"{said}\""),
    }
}

/// A definition: something others refer to, which is what its
/// attributes say and shows nowhere itself.
pub(crate) fn is_definition(kind: Kind) -> bool {
    matches!(kind, Kind::LinearGradient | Kind::RadialGradient | Kind::Stop | Kind::Filter | Kind::FilterPrimitive | Kind::ClipPath | Kind::Mask | Kind::Pattern | Kind::Marker | Kind::Symbol)
}

/// Whether what a node of this kind holds is listed in the file's
/// order: definitions and steps, where later isn't "on top".
fn in_file_order(kind: Kind) -> bool {
    matches!(kind, Kind::Defs | Kind::LinearGradient | Kind::RadialGradient | Kind::Filter | Kind::FilterPrimitive)
}

/// The longest attribute value, and all of a node's attributes, that
/// one line spells out.
const MAX_VALUE: usize = 40;
const MAX_ATTRS: usize = 160;

/// A node's attributes as it writes them, its `id` aside (that's said
/// already): ` offset="0" stop-color="#ffc800"`.
pub(crate) fn attributes(node: &Node) -> String {
    let mut out = String::new();
    for attr in node.attrs.iter().filter(|a| a.name != "id") {
        if out.len() > MAX_ATTRS {
            out += " …";
            break;
        }
        let value = match attr.value.char_indices().nth(MAX_VALUE) {
            Some((cut, _)) => format!("{}…", &attr.value[..cut]),
            None => attr.value.clone(),
        };
        out += &format!(" {}=\"{value}\"", attr.name);
    }
    out
}

/// The paint `node` gives itself, each part led by two spaces: its
/// fill, its stroke and how wide, its opacity. Nothing for what it
/// leaves to the groups above.
fn paint(node: &Node) -> String {
    let mut said = String::new();
    if let Some(fill) = prop(node, "fill") {
        said += &format!("  fill {fill}");
    }
    if let Some(stroke) = prop(node, "stroke") {
        said += &format!("  stroke {stroke}");
        if let Some(width) = prop(node, "stroke-width") {
            said += &format!(" {width}");
        }
    } else if let Some(width) = prop(node, "stroke-width") {
        said += &format!("  stroke-width {width}");
    }
    if let Some(opacity) = prop(node, "opacity") {
        said += &format!("  opacity {opacity}");
    }
    said
}

/// The same, as a phrase: `fill #ffc800, stroke #000 2`.
pub(crate) fn painted(node: &Node) -> String {
    let said = paint(node);
    if said.is_empty() { "no fill, stroke or opacity of its own".to_owned() } else { said.trim_start().replace("  ", ", ") }
}

/// One line about a node: its id, what it is (a text: what it says),
/// the paint it gives itself (or, for a definition, its attributes),
/// and where it shows.
fn node_line(doc: &Document, node: &Node, bounds: Option<&Rect>) -> String {
    let mut line = format!("{} {}", node.id, node.name);
    if let Some(id) = node.attr("id") {
        line += &format!(" #{id}");
    }
    if node.kind == Kind::Text {
        line += &format!(" {}", words(doc, node));
    }
    if let Some(label) = doc.label(node.id) {
        line += &format!(" [{}]", label.trim());
    }
    if is_definition(node.kind) {
        let attrs = attributes(node);
        if !attrs.is_empty() {
            line += &format!(" {attrs}");
        }
    } else {
        line += &paint(node);
        if node.attr("transform").is_some() {
            line += "  transformed";
        }
    }
    if let Some(b) = bounds {
        line += &format!("  at {}", rect(b));
    }
    if let Some(why) = undrawn(doc, node) {
        line += &format!("  ({why})");
    }
    if doc.is_locked(node.id) {
        line += "  locked";
    }
    line
}

/// A listing of nodes being put together.
struct Listing<'a> {
    doc: &'a Document,
    bounds: HashMap<NodeId, Rect>,
    lines: Vec<String>,
    /// How many more nodes it will spell out, and how many it has had
    /// to leave out.
    left: usize,
    skipped: usize,
}

impl Listing<'_> {
    /// The tree under `id`, front to back (what's later in the file, and
    /// so on top, first), each level indented under its parent. What a
    /// definition holds (a gradient's stops, a filter's steps) is in the
    /// file's order: there, later isn't on top.
    fn tree(&mut self, id: NodeId, depth: usize) {
        let Some(node) = self.doc.get(id) else { return };
        if self.left == 0 {
            self.skipped += self.doc.descendants(id).len();
            return;
        }
        self.left -= 1;
        self.lines.push(format!("{}{}", "  ".repeat(depth), node_line(self.doc, node, self.bounds.get(&id))));
        let mut children: Vec<NodeId> = node.elements().collect();
        if !in_file_order(node.kind) {
            children.reverse();
        }
        for child in children {
            self.tree(child, depth + 1);
        }
    }
}

/// The most anchors of a path spelled out.
const MAX_ANCHORS: usize = 48;

/// A path's anchors, run by run, in its own coordinates: each with its
/// id, where it is, its handles (as offsets from it), and what joins it
/// to the next. As lines for the model, and as data. `None` for what
/// isn't a path whose data all reads.
pub(crate) fn anchors(doc: &Document, node: &Node) -> Option<(String, Map)> {
    let outline = doc.outline(node.id)?;
    let decimals = Precision::of(doc).decimals;
    let pt = |p: ink_geom::Vec2| format!("{},{}", number::format(p.x, decimals), number::format(p.y, decimals));
    let total: usize = outline.runs.iter().map(|run| run.anchors.len()).sum();
    let (mut lines, mut listed, mut data) = (Vec::new(), 0usize, Vec::new());
    for (r, run) in outline.runs.iter().enumerate() {
        lines.push(format!("  Run {} of {}, {}, {} anchor{}:", r + 1, outline.runs.len(), if run.closed { "closed" } else { "open" }, run.anchors.len(), if run.anchors.len() == 1 { "" } else { "s" }));
        for (i, anchor) in run.anchors.iter().enumerate() {
            // Its handles: the control points of what comes in and
            // what goes out, from where it is.
            let out = run.links.get(i).filter(|_| run.next(i).is_some()).and_then(|link| match link {
                Link::Cubic { c1, .. } => Some(*c1 - anchor.at),
                Link::Quad { c } => Some(*c - anchor.at),
                _ => None,
            });
            let into = run.prev(i).and_then(|p| match run.links[p] {
                Link::Cubic { c2, .. } => Some(c2 - anchor.at),
                Link::Quad { c } => Some(c - anchor.at),
                _ => None,
            });
            let mut m = Map::new();
            m.insert("id", anchor.id.to_string().into());
            m.insert("run", Doc::Int(r as i64 + 1));
            m.insert("at", Doc::List(vec![anchor.at.x.into(), anchor.at.y.into()]));
            for (key, handle) in [("in", into), ("out", out)] {
                if let Some(h) = handle.filter(|h| h.length() > 0.0) {
                    m.insert(key, Doc::List(vec![h.x.into(), h.y.into()]));
                }
            }
            data.push(Doc::Map(m));
            listed += 1;
            if listed > MAX_ANCHORS {
                continue;
            }
            let mut line = format!("    {} at {}", anchor.id, pt(anchor.at));
            for (name, handle) in [("in", into), ("out", out)] {
                if let Some(h) = handle.filter(|h| h.length() > 0.0) {
                    line += &format!("  {name} {}", pt(h));
                }
            }
            line += &match (run.links.get(i).filter(|_| run.next(i).is_some()), run.next(i)) {
                (Some(link), Some(next)) => {
                    let kind = match link {
                        Link::Line => "a line".to_owned(),
                        Link::Quad { .. } => "a curve (one control point)".to_owned(),
                        Link::Cubic { .. } => "a curve".to_owned(),
                        Link::Arc { arc } => format!("an arc of radii {} {}", number::format(arc.rx, decimals), number::format(arc.ry, decimals)),
                    };
                    format!("  then {kind} to {}", run.anchors[next].id)
                }
                _ => "  (the end)".to_owned(),
            };
            lines.push(line);
        }
    }
    if listed > MAX_ANCHORS {
        lines.push(format!("    … and {} more (doc_source shows its d)", listed - MAX_ANCHORS));
    }
    if total == 0 {
        lines.push("  (no anchors: it draws nothing)".to_owned());
    }
    let mut m = Map::new();
    m.insert("anchors", Doc::List(data));
    Some((lines.join("\n"), m))
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

/// Everything `doc_info` says: the drawing, and its nodes (or, with
/// `under`, that node and what's in it). The listing is said once, in
/// the text: as data it was every node over again, at twice the cost
/// to read. The data is what the drawing is.
pub(crate) fn info(core: &Core, id: DocId, under: Option<NodeId>) -> Result<(String, Doc), ToolError> {
    let doc = core.doc(id).map_err(refused)?;
    let history = core.history(id).map_err(refused)?;
    let top = match under {
        Some(node) => doc.node(node).map_err(crate::input::refused_edit)?.id,
        None => doc.root(),
    };
    let mut listing = Listing { doc, bounds: page_bounds(doc), lines: Vec::new(), left: MAX_LISTED, skipped: 0 };
    listing.tree(top, 0);
    let Listing { lines, skipped, .. } = listing;
    let (undo, redo) = (history.undoable().count(), history.redoable().count());
    let mut text = format!("{id} {}", name(core, id));
    if let Ok(Some(path)) = core.path(id) {
        text += &format!(" ({})", path.display());
    }
    text += &format!(", {}.\nPage {}. {} nodes. Undo: {undo}", saved(core, id), page(doc), doc.len());
    if let Some(latest) = history.undoable().next_back() {
        text += &format!(" (latest: \"{}\" by {})", latest.label, actor(latest.actor));
    }
    let which = if under.is_some() { format!("{top} and what's in it") } else { "Nodes".to_owned() };
    text += &format!(". Redo: {redo}.\n{which}, front to back (the first listed is on top; what a <defs>, a gradient or a filter holds is in the file's order; boxes are where each shows in the drawing's coordinates, strokes aside):\n{}", lines.join("\n"));
    if skipped > 0 {
        text += &format!("\n… and {skipped} more (node_id lists one node and what's in it; doc_source shows the markup)");
    }
    let mut m = Map::new();
    m.insert("doc_id", id.to_string().into());
    m.insert("path", core.path(id).ok().flatten().map_or(Doc::Null, |p| p.display().to_string().into()));
    m.insert("modified", core.is_modified(id).unwrap_or(true).into());
    m.insert("root", doc.root().to_string().into());
    m.insert("node_count", Doc::Int(doc.len() as i64));
    m.insert("undo", Doc::Int(undo as i64));
    m.insert("redo", Doc::Int(redo as i64));
    Ok((text, Doc::Map(m)))
}
