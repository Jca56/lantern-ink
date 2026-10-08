//! Nodes: putting them in, changing their attributes, moving them,
//! taking them out. Each of these is an edit: one Command, one undo
//! step, and a step a batch can hold.

use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::style::prop;
use ink_core::ink_doc::{Document, Element, Precision, elements};
use ink_core::{Applied, Command, NodeId};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, ToolError, fail, schema};

use crate::describe::{rect, tag, undrawn};
use crate::input::{In, attr_value, common, refused_edit};
use crate::tools::{Entry, edit};

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "node_add",
            "Add element",
            "Add one element: `element` is its SVG name (rect, circle, ellipse, line, polyline, polygon, path, g, …) and `attrs` its attributes as the file will write them, e.g. {\"x\": 2, \"y\": 2, \"width\": 20, \"height\": 20, \"rx\": 3, \"fill\": \"#ffc800\"}. Numbers are in the element's own coordinates, as in any SVG: inside whatever transforms its groups have. It goes on top of the drawing unless above, below, into or at says. Returns its node id.",
            add_schema,
            Kind::Add,
            add,
            added,
        ),
        edit(
            "node_add_svg",
            "Add markup",
            "Add SVG markup as written: one element or several side by side, with whatever is nested in them (a <g> of shapes, a <defs> of gradients). It is parsed and checked first; markup that isn't well-formed is refused, saying where. The elements go on top of the drawing, in order, unless above, below, into or at says. Returns their node ids.",
            add_svg_schema,
            Kind::Add,
            add_svg,
            added,
        ),
        edit(
            "node_set",
            "Set attributes",
            "Set attributes on a node, as the file will write them: `attrs` maps each name to a string or a number, or to null to take the attribute off. Any attribute at all: geometry (x, d, points, r), paint (fill, stroke, stroke-width, opacity), transform, id, style. Anything not named stays as it is, to the byte.",
            set_schema,
            Kind::Set,
            set,
            changed,
        ),
        edit("node_move", "Move in the stack", "Move nodes, each with everything in it, to another place in the drawing's order or into another group: just above or below a node, into one, or to the top or bottom of the whole drawing. Later in the file is further up the picture. This moves them in the stack, not on the page: each stays where it shows, so one that lands in a group with a transform has its own numbers (or its transform) changed to make up for it. It takes on its new group's inherited paint.", move_schema, Kind::Set, relocate, moved),
        edit("node_delete", "Delete nodes", "Delete nodes, each with everything in it. Undoable with history_undo.", delete_schema, Kind::Destroy, delete, deleted),
        edit(
            "node_mark",
            "Label, lock",
            "Give nodes a label, or lock them. `label` is a name for a person to know a node by (doc_info shows it in brackets, and a layers panel will): \"\" takes it off. `locked`: true makes each node, and everything in it, refuse every edit until it's unlocked with false: it can't be changed, moved or deleted, and neither can anything in it, though things can still go beside it and a group it's in can be painted. Alva locks what she doesn't want changed, so ask her before unlocking one. Both are Ink's own marks (ink:label, ink:locked): they change nothing of how the drawing draws, other programs ignore them, and an .svg from doc_export leaves them out.",
            mark_schema,
            Kind::Set,
            mark,
            marked,
        ),
    ]
}

fn mark_schema() -> Doc {
    common::edit(
        &["node_ids"],
        vec![
            ("node_ids", schema::list(common::node_id("A node"), "The nodes to label or lock")),
            ("label", schema::string("The name they go by; \"\" takes it off")),
            ("locked", schema::boolean("Lock them (true) or unlock them (false)", false)),
        ],
    )
}

fn mark(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes = input.nodes("node_ids")?;
    for &id in &nodes {
        doc.node(id).map_err(refused_edit)?;
    }
    // No name at all takes the label off.
    let label = input.args.opt_str("label")?.map(|said| Some(said.trim()).filter(|said| !said.is_empty()).map(str::to_owned));
    let locked = input.args.opt_bool("locked")?;
    if label.is_none() && locked.is_none() {
        return fail("say what to mark: label (a name, or \"\" to take it off) or locked (true or false)");
    }
    // A label goes on while the node is open: unlocked first, locked
    // last.
    let mut steps = Vec::new();
    if locked == Some(false) {
        steps.push(Command::SetLocked { nodes: nodes.clone(), locked: false });
    }
    if let Some(label) = label {
        steps.extend(nodes.iter().map(|&node| Command::SetLabel { node, label: label.clone() }));
    }
    if locked == Some(true) {
        steps.push(Command::SetLocked { nodes, locked: true });
    }
    Ok(Command::Batch(steps))
}

fn marked(doc: &Document, applied: &Applied) -> Reply {
    // The root changes only by gaining the declaration Ink's marks need.
    let now: Vec<String> = applied.changed.iter().filter(|&&id| id != doc.root()).filter_map(|id| doc.get(*id)).map(|node| format!("{} {}{}", node.id, tag(node), match crate::describe::marks(doc, node).as_str() { "" => " (no label, unlocked)".to_owned(), marks => marks.to_owned() })).collect();
    if now.is_empty() {
        return Reply::text("Nothing changed: they were marked that way already.");
    }
    let mut m = Map::new();
    m.insert("node_ids", ids(&applied.changed.iter().copied().filter(|&id| id != doc.root()).collect::<Vec<_>>()));
    Reply::text(format!("Marked. Now: {}.", now.join("; "))).data(Doc::Map(m))
}

/// A name an element can have: SVG's are letters, with a prefix maybe.
fn check_element(name: &str) -> Result<(), ToolError> {
    let ok = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '_' | '.'));
    if ok { Ok(()) } else { fail(format!("\"{name}\" can't be an element's name: give an SVG one, like rect, path or g")) }
}

fn add_schema() -> Doc {
    let mut props = vec![("element", schema::string("The element's SVG name: rect, circle, ellipse, line, polyline, polygon, path, g, …")), ("attrs", schema::map(common::value("Its value: a string or a number", false), "Its attributes, by name, as the file will write them"))];
    props.extend(common::placement());
    common::edit(&["element"], props)
}

fn add(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let name = input.args.str("element")?;
    check_element(name)?;
    let decimals = Precision::of(doc).decimals;
    let mut element = Element::new(name);
    for (attr, value) in input.args.opt_map("attrs", "attribute names and values")?.into_iter().flat_map(Map::iter) {
        let Some(value) = attr_value(attr, value, decimals)? else { return fail(format!("the attribute \"{attr}\" is null: a new element has nothing to take off")) };
        element = element.with(attr, value);
    }
    Ok(Command::Insert { place: input.place(doc)?, elements: vec![element] })
}

fn add_svg_schema() -> Doc {
    let mut props = vec![("svg", schema::string("The markup: one or more elements, e.g. \"<g fill='none' stroke='#fff'><path d='M4 12h16'/><path d='M12 4v16'/></g>\""))];
    props.extend(common::placement());
    common::edit(&["svg"], props)
}

fn add_svg(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let parsed = elements(input.args.str("svg")?).map_err(|e| ToolError(format!("the markup can't be read: {e}")))?;
    if parsed.is_empty() {
        return fail("the markup holds no element");
    }
    Ok(Command::Insert { place: input.place(doc)?, elements: parsed })
}

/// What `added` and friends say about one node.
fn about(doc: &Document, id: NodeId, boxes: &std::collections::HashMap<NodeId, ink_geom::Rect>) -> String {
    let Some(node) = doc.get(id) else { return id.to_string() };
    let inside = doc.descendants(id).len() - 1;
    let mut text = format!("{id} {}", tag(node));
    if inside > 0 {
        text += &format!(" with {inside} inside");
    }
    match (boxes.get(&id), undrawn(doc, node)) {
        (_, Some(why)) => text += &format!(" ({why})"),
        (Some(b), None) => text += &format!(" at {}", rect(b)),
        (None, None) if node.kind.is_shape() || node.kind.is_group() || node.kind == ink_core::ink_doc::Kind::Text => text += " (it draws nothing as it is)",
        (None, None) => {}
    }
    text
}

fn ids(nodes: &[NodeId]) -> Doc {
    Doc::List(nodes.iter().map(|id| id.to_string().into()).collect())
}

fn added(doc: &Document, applied: &Applied) -> Reply {
    let boxes = page_bounds(doc);
    let listed: Vec<String> = applied.created.iter().map(|&id| about(doc, id, &boxes)).collect();
    let mut m = Map::new();
    m.insert("node_ids", ids(&applied.created));
    if let [only] = applied.created.as_slice() {
        m.insert("node_id", only.to_string().into());
    }
    Reply::text(format!("Added {}.", listed.join("; "))).data(Doc::Map(m))
}

fn set_schema() -> Doc {
    common::edit(&["node_id", "attrs"], vec![("node_id", common::node_id("The node")), ("attrs", schema::map(common::value("Its new value: a string or a number; null takes the attribute off", true), "The attributes to set, by name"))])
}

fn set(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let node = input.node("node_id")?;
    let decimals = Precision::of(doc).decimals;
    let attrs = input.args.opt_map("attrs", "attribute names and values")?.ok_or_else(|| ToolError("\"attrs\" is required".into()))?;
    if attrs.is_empty() {
        return fail("\"attrs\" is empty: name at least one attribute to set");
    }
    let sets: Vec<Command> = attrs.iter().map(|(name, value)| Ok(Command::SetAttr { node, name: name.to_owned(), value: attr_value(name, value, decimals)? })).collect::<Result<_, ToolError>>()?;
    Ok(Command::Batch(sets))
}

/// The attributes of `node` that don't show: ones its `style` or a
/// `<style>` rule says otherwise about, and so outvotes.
fn overridden(doc: &Document, id: NodeId) -> Vec<String> {
    let Some(node) = doc.get(id) else { return Vec::new() };
    node.attrs.iter().filter(|a| a.name != "style" && prop(node, &a.name).is_some_and(|shows| shows != a.value.trim())).map(|a| a.name.clone()).collect()
}

fn changed(doc: &Document, applied: &Applied) -> Reply {
    let Some(&id) = applied.changed.first() else {
        return Reply::text("Nothing changed: the node already had those values.");
    };
    let boxes = page_bounds(doc);
    let mut text = format!("Set. It's now {}.", about(doc, id, &boxes));
    let shadowed = overridden(doc, id);
    if !shadowed.is_empty() {
        text += &format!(" Note: its style=\"…\" (or a <style> rule) also sets {}, and that wins over the attribute of the same name: node_style sets a property where it will show.", shadowed.join(", "));
    }
    let mut m = Map::new();
    m.insert("node_id", id.to_string().into());
    Reply::text(text).data(Doc::Map(m))
}

fn move_schema() -> Doc {
    let mut props = vec![("node_ids", schema::list(common::node_id("A node to move"), "The nodes to move, in the order they should end up (the first lowest)"))];
    props.extend(common::placement());
    common::edit(&["node_ids"], props)
}

fn relocate(doc: &Document, input: &In) -> Result<Command, ToolError> {
    if !["above", "below", "into", "at"].iter().any(|k| input.args.has(k)) {
        return fail("say where to: one of above, below, into or at");
    }
    Ok(Command::Move { nodes: input.nodes("node_ids")?, place: input.place(doc)? })
}

fn moved(doc: &Document, applied: &Applied) -> Reply {
    if applied.moved.is_empty() {
        return Reply::text("Nothing moved: they were there already.");
    }
    let places: Vec<String> = applied.moved.iter().map(|&id| match doc.get(id).and_then(|n| n.parent) {
        Some(parent) => format!("{id} (now in {parent})"),
        None => id.to_string(),
    }).collect();
    let mut m = Map::new();
    m.insert("node_ids", ids(&applied.moved));
    // One that landed under other transforms was changed to stay put.
    let kept = match applied.changed.as_slice() {
        [] => String::new(),
        changed => format!(" To stay where {} showed, {} changed to make up for the transforms there.", if changed.len() == 1 { "it" } else { "they" }, changed.iter().map(NodeId::to_string).collect::<Vec<_>>().join(", ")),
    };
    Reply::text(format!("Moved {}.{kept}", places.join(", "))).data(Doc::Map(m))
}

fn delete_schema() -> Doc {
    common::edit(&["node_ids"], vec![("node_ids", schema::list(common::node_id("A node to delete"), "The nodes to delete"))])
}

fn delete(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes = input.nodes("node_ids")?;
    // Said here, with the node's name in it, rather than left to the
    // document's refusal.
    for &id in &nodes {
        doc.node(id).map_err(refused_edit)?;
    }
    if nodes.contains(&doc.root()) {
        return fail(format!("{} is the drawing's root <svg>: it can't be deleted (doc_close closes the drawing)", doc.root()));
    }
    Ok(Command::Delete { nodes })
}

fn deleted(doc: &Document, applied: &Applied) -> Reply {
    let gone: Vec<String> = applied.removed.iter().map(NodeId::to_string).collect();
    let mut m = Map::new();
    m.insert("node_ids", ids(&applied.removed));
    Reply::text(format!("Deleted {} (and what was in {}). {} nodes are left.", gone.join(", "), if gone.len() == 1 { "it" } else { "them" }, doc.len())).data(Doc::Map(m))
}
