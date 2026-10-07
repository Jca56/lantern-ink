//! Copies and groups: a node copied on top of itself, nodes put into a
//! group, a group taken away from around what's in it. Edits, all
//! three: one Command, one undo step.

use ink_core::ink_doc::Document;
use ink_core::ink_doc::geometry::page_bounds;
use ink_core::{Applied, Command, NodeId};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, ToolError, schema};

use crate::describe::{rect, tag};
use crate::input::{In, common, refused_edit};
use crate::tools::{Entry, edit};

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "node_duplicate",
            "Copy nodes",
            "Copy nodes, each with everything in it, right on top of itself (just above it in the stack). Whatever in a copy has an id=\"…\" gets one of its own (sun becomes sun-2), and what's in the copy refers to those, so the copy stands on its own. Returns the copies' node ids, in the order given: node_transform moves one off its original.",
            ids_schema,
            Kind::Add,
            duplicate,
            copied,
        ),
        edit(
            "node_group",
            "Group nodes",
            "Put nodes that share a parent into a new <g>, where the topmost of them was, in the order they were in. Nothing looks different, except that what lay between them in the stack is now under all of them. Returns the group's node id.",
            ids_schema,
            Kind::Add,
            group,
            grouped,
        ),
        edit(
            "node_ungroup",
            "Ungroup",
            "Take a <g> away from around what's in it: its children take its place and look as they did. Its transform goes to each of them (written as node_transform writes one), and the paint they had from it by inheritance is said on them. A filter, a clip path, a mask, and an opacity over several children are things only a group can hold for what's in it: with any of those it's refused, unless drop: true says to lose them. Returns the nodes that were in the groups.",
            ungroup_schema,
            Kind::Destroy,
            ungroup,
            ungrouped,
        ),
    ]
}

fn ids_schema() -> Doc {
    common::edit(&["node_ids"], vec![("node_ids", schema::list(common::node_id("A node"), "The nodes"))])
}

/// The nodes named, each of which there is.
fn named(doc: &Document, input: &In) -> Result<Vec<NodeId>, ToolError> {
    let nodes = input.nodes("node_ids")?;
    for &id in &nodes {
        doc.node(id).map_err(refused_edit)?;
    }
    Ok(nodes)
}

fn duplicate(doc: &Document, input: &In) -> Result<Command, ToolError> {
    Ok(Command::Duplicate { nodes: named(doc, input)? })
}

/// `id` in a few words: what it is, what's in it, where it shows.
fn about(doc: &Document, id: NodeId, boxes: &std::collections::HashMap<NodeId, ink_geom::Rect>) -> String {
    let Some(node) = doc.get(id) else { return id.to_string() };
    let inside = doc.descendants(id).len() - 1;
    let mut text = format!("{id} {}", tag(node));
    if inside > 0 {
        text += &format!(" with {inside} inside");
    }
    if let Some(b) = boxes.get(&id) {
        text += &format!(" at {}", rect(b));
    }
    text
}

fn ids(nodes: &[NodeId]) -> Doc {
    Doc::List(nodes.iter().map(|id| id.to_string().into()).collect())
}

fn copied(doc: &Document, applied: &Applied) -> Reply {
    let boxes = page_bounds(doc);
    let said: Vec<String> = applied.created.iter().map(|&id| about(doc, id, &boxes)).collect();
    let mut m = Map::new();
    m.insert("node_ids", ids(&applied.created));
    if let [only] = applied.created.as_slice() {
        m.insert("node_id", only.to_string().into());
    }
    Reply::text(format!("Copied: {}. Each lies right on top of its original.", said.join("; "))).data(Doc::Map(m))
}

fn group(doc: &Document, input: &In) -> Result<Command, ToolError> {
    Ok(Command::Group { nodes: named(doc, input)? })
}

fn grouped(doc: &Document, applied: &Applied) -> Reply {
    let Some(&group) = applied.created.first() else { return Reply::text("Nothing was grouped.") };
    let boxes = page_bounds(doc);
    let inside: Vec<String> = applied.moved.iter().map(NodeId::to_string).collect();
    let mut m = Map::new();
    m.insert("node_id", group.to_string().into());
    m.insert("node_ids", ids(&applied.moved));
    let at = boxes.get(&group).map_or(String::new(), |b| format!(" at {}", rect(b)));
    let parent = doc.get(group).and_then(|g| g.parent).map_or(String::new(), |p| format!(", in {p}"));
    Reply::text(format!("Grouped {} into {group} <g>{at}{parent}.", inside.join(", "))).data(Doc::Map(m))
}

fn ungroup_schema() -> Doc {
    common::edit(&["node_ids"], vec![("node_ids", schema::list(common::node_id("A group"), "The groups to take away")), ("drop", schema::boolean("Ungroup even a group with a filter, a clip path, a mask or an opacity only it can hold, losing them", false))])
}

fn ungroup(doc: &Document, input: &In) -> Result<Command, ToolError> {
    Ok(Command::Ungroup { nodes: named(doc, input)?, drop: input.args.opt_bool("drop")? == Some(true) })
}

fn ungrouped(doc: &Document, applied: &Applied) -> Reply {
    let gone: Vec<String> = applied.removed.iter().map(NodeId::to_string).collect();
    let mut m = Map::new();
    m.insert("node_ids", ids(&applied.moved));
    if applied.moved.is_empty() {
        return Reply::text(format!("Ungrouped {}: there was nothing in {}.", gone.join(", "), if gone.len() == 1 { "it" } else { "them" })).data(Doc::Map(m));
    }
    let now: Vec<String> = applied.moved.iter().map(|&id| match doc.get(id).and_then(|n| n.parent) {
        Some(parent) => format!("{id} (in {parent})"),
        None => id.to_string(),
    }).collect();
    Reply::text(format!("Ungrouped {}. What was in {}: {}.", gone.join(", "), if gone.len() == 1 { "it" } else { "them" }, now.join(", "))).data(Doc::Map(m))
}
