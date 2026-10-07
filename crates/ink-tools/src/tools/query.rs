//! Asking about a drawing: everything one node says and is, and what's
//! drawn at a point. Reads: nothing changes.

use ink_core::NodeId;
use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::hit::{self, Part};
use ink_core::ink_doc::refs::{Ids, named as names};
use ink_core::ink_doc::style::{INHERITED, prop};
use ink_core::ink_doc::{Document, Node};
use ink_geom::Vec2;
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, Tool, ToolError, fail, schema};

use crate::describe::{n, rect, tag, undrawn};
use crate::input::{In, common, refused, refused_edit};
use crate::tools::{Ctx, Direct, Entry, Handler};

/// The longest attribute value spelled out whole.
const MAX_VALUE: usize = 200;
/// The most nodes named as using one.
const MAX_USERS: usize = 12;

fn direct(name: &'static str, title: &'static str, description: &'static str, schema: fn() -> Doc, f: Direct) -> Entry {
    Entry { spec: Tool { name, title, description, schema, kind: Kind::Read }, handler: Handler::Direct(f) }
}

pub(super) fn tools() -> Vec<Entry> {
    vec![
        direct(
            "node_info",
            "Describe node",
            "Everything about one node: its element and where it is in the tree, every attribute as the file writes it, the paint it has from the groups above, the box where it shows in the drawing's coordinates, the transforms its own numbers are under, what it uses (a gradient, a clip path) and what uses it.",
            info_schema,
            info,
        ),
        direct(
            "doc_query",
            "What's at a point",
            "What's drawn at a point, in the drawing's coordinates: the shapes there, the one on top first, each with the part of it that's there (its fill or its stroke) and the groups it's in. It looks through every transform, and not where a clip path cuts a shape away or a shape is hidden. Text, images and uses aren't drawn yet, so aren't found.",
            query_schema,
            query,
        ),
    ]
}

fn info_schema() -> Doc {
    schema::object(&["doc_id", "node_id"], vec![("doc_id", common::doc_id()), ("node_id", common::node_id("The node"))])
}

/// An attribute's value, cut short when it's long.
fn shown(value: &str) -> String {
    match value.char_indices().nth(MAX_VALUE) {
        Some((cut, _)) => format!("{}… ({} characters: doc_source shows all)", &value[..cut], value.chars().count()),
        None => value.to_owned(),
    }
}

fn info(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let doc = ctx.core.doc(input.doc()?).map_err(refused)?;
    let id = input.node("node_id")?;
    let node = doc.node(id).map_err(refused_edit)?;
    let above: Vec<&Node> = doc.ancestors(id).collect();
    let inside = doc.descendants(id).len() - 1;
    let mut lines = vec![match above.first() {
        Some(parent) => format!("{id} {}, in {} {}; {inside} inside.", tag(node), parent.id, tag(parent)),
        None => format!("{id} {}, the drawing's root; {inside} inside.", tag(node)),
    }];
    lines.push(match node.attrs.as_slice() {
        [] => "No attributes.".to_owned(),
        attrs => format!("Attributes: {}", attrs.iter().map(|a| format!("{}=\"{}\"", a.name, shown(&a.value))).collect::<Vec<_>>().join(" ")),
    });
    // What the drawing's <style> rules say of it: each property once,
    // as the rule that counts for most has it.
    let mut ruled: Vec<(&str, &str)> = Vec::new();
    for said in node.rules().iter().rev().filter(|said| !said.name.is_empty()) {
        if !ruled.iter().any(|(name, _)| *name == &*said.name) {
            ruled.push((&said.name, &said.value));
        }
    }
    if !ruled.is_empty() {
        lines.push(format!("From <style> rules: {} (its own style=\"…\" outvotes these; its attributes don't)", ruled.iter().rev().map(|(name, value)| format!("{name}: {value}")).collect::<Vec<_>>().join("; ")));
    }
    // What it's drawn with that it doesn't say itself: from the nearest
    // group above that does.
    let handed: Vec<String> = INHERITED.iter().filter(|name| prop(node, name).is_none()).filter_map(|name| above.iter().find_map(|a| prop(a, name).map(|v| format!("{name}=\"{v}\" ({})", a.id)))).collect();
    if !handed.is_empty() {
        lines.push(format!("From the groups above: {}", handed.join(", ")));
    }
    let boxes = page_bounds(doc);
    lines.push(match (boxes.get(&id), undrawn(node)) {
        (_, Some(why)) => format!("Shows nowhere ({why})."),
        (Some(b), None) => format!("Shows at {} in the drawing's coordinates (strokes aside).", rect(b)),
        (None, None) if node.kind.is_shape() || node.kind.is_group() => "Shows nowhere: it draws nothing as it is.".to_owned(),
        (None, None) => "Shows nowhere itself: it's something others use, or words about the picture.".to_owned(),
    });
    // The root's own transform isn't one.
    let under: Vec<String> = above.iter().filter(|a| a.parent.is_some()).filter_map(|a| a.attr("transform").map(|t| format!("{} transform=\"{t}\"", a.id))).collect();
    if !under.is_empty() {
        lines.push(format!("Its own numbers are under, nearest first: {}.", under.join("; ")));
    }
    let ids = Ids::of(doc);
    let mut uses: Vec<(String, Option<NodeId>)> = Vec::new();
    for attr in &node.attrs {
        for name in names(&attr.name, &attr.value) {
            uses.push((format!("{} → {}", attr.name, ids.get(name).and_then(|t| doc.get(t)).map_or(format!("nothing (no element has id=\"{name}\")"), |t| format!("{} {}", t.id, tag(t)))), ids.get(name)));
        }
    }
    if !uses.is_empty() {
        lines.push(format!("Uses: {}.", uses.iter().map(|(said, _)| said.as_str()).collect::<Vec<_>>().join("; ")));
    }
    // What names it: only the first element of an id is the one found.
    let own = node.attr("id").filter(|own| ids.get(own) == Some(id));
    let users: Vec<&Node> = own.map_or(Vec::new(), |own| doc.descendants(doc.root()).into_iter().filter_map(|u| doc.get(u)).filter(|u| u.attrs.iter().any(|a| names(&a.name, &a.value).contains(&own))).collect());
    if !users.is_empty() {
        let said: Vec<String> = users.iter().take(MAX_USERS).map(|u| format!("{} {}", u.id, tag(u))).collect();
        let more = users.len().saturating_sub(MAX_USERS);
        lines.push(format!("Used by: {}{}.", said.join(", "), if more > 0 { format!(", and {more} more") } else { String::new() }));
    }
    let mut m = Map::new();
    m.insert("node_id", id.to_string().into());
    m.insert("element", node.name.as_str().into());
    m.insert("parent", above.first().map_or(Doc::Null, |p| p.id.to_string().into()));
    let mut attrs = Map::new();
    for attr in &node.attrs {
        attrs.insert(attr.name.as_str(), attr.value.as_str().into());
    }
    m.insert("attrs", Doc::Map(attrs));
    if let Some(b) = boxes.get(&id) {
        m.insert("box", Doc::List([b.min.x, b.min.y, b.width(), b.height()].into_iter().map(Doc::from).collect()));
    }
    m.insert("uses", Doc::List(uses.iter().filter_map(|(_, t)| t.map(|t| t.to_string().into())).collect()));
    m.insert("used_by", Doc::List(users.iter().map(|u| u.id.to_string().into()).collect()));
    Ok(Reply::text(lines.join("\n")).data(Doc::Map(m)))
}

fn query_schema() -> Doc {
    schema::object(&["doc_id", "point"], vec![("doc_id", common::doc_id()), ("point", schema::list(schema::number(-1e9, 1e9, "A coordinate"), "[x, y] in the drawing's coordinates"))])
}

fn query(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let doc: &Document = ctx.core.doc(input.doc()?).map_err(refused)?;
    let point = match input.args.list("point", "numbers")?.iter().map(|d| d.as_f64().filter(|v| v.is_finite())).collect::<Option<Vec<f64>>>().as_deref() {
        Some(&[x, y]) => Vec2::new(x, y),
        _ => return fail("\"point\" should be [x, y]"),
    };
    let hits = hit::at(doc, point);
    let said: Vec<String> = hits
        .iter()
        .filter_map(|h| Some((doc.get(h.node)?, h.part)))
        .map(|(node, part)| {
            // The groups it's in, the nearest first; the root goes
            // without saying.
            let groups: Vec<String> = doc.ancestors(node.id).filter(|a| a.parent.is_some()).map(|a| a.id.to_string()).collect();
            let within = if groups.is_empty() { String::new() } else { format!(", in {}", groups.join(" in ")) };
            format!("{} {} (its {}){within}", node.id, tag(node), if part == Part::Fill { "fill" } else { "stroke" })
        })
        .collect();
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(hits.iter().map(|h| h.node.to_string().into()).collect()));
    let at = format!("{},{}", n(point.x), n(point.y));
    Ok(Reply::text(if said.is_empty() { format!("Nothing is drawn at {at}.") } else { format!("At {at}, front to back: {}.", said.join("; ")) }).data(Doc::Map(m)))
}
