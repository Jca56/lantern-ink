//! How nodes are painted: `node_style` sets properties the way a file
//! needs them set to show, where `node_set` writes attributes as given.

use ink_core::ink_doc::{Document, Precision};
use ink_core::{Applied, Command};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, ToolError, fail, schema};

use crate::describe::{painted, tag};
use crate::input::{In, attr_value, common, refused_edit};
use crate::tools::{Entry, edit};

/// The most styled nodes a reply spells out.
const MAX_SAID: usize = 12;

pub(super) fn tools() -> Vec<Entry> {
    vec![edit(
        "node_style",
        "Set paint",
        "Set how nodes are painted: `style` maps a property's name (fill, stroke, stroke-width, stroke-linecap, stroke-linejoin, stroke-dasharray, opacity, fill-opacity, fill-rule, …) to its value, or to null to take the property off, on every node named. Each is written where its node already has it: in its style=\"…\" when that's where it is said (the rest of the style is kept), else as an attribute; and a property a <style> rule gives the node goes into its style, which is what outvotes the rule. So what's set here is what shows, where node_set writes an attribute that a style or a rule may outvote. Names are SVG's own, and values Ink draws with are checked: a slip is refused, saying what it should be. Set on a group, what it's given is what the shapes in it draw with unless they say otherwise.",
        style_schema,
        Kind::Set,
        style,
        styled,
    )]
}

fn style_schema() -> Doc {
    common::edit(
        &["node_ids", "style"],
        vec![("node_ids", schema::list(common::node_id("A node"), "The nodes to paint")), ("style", schema::map(common::value("What it's set to: a string or a number; null takes the property off", true), "The properties to set, by name"))],
    )
}

fn style(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes = input.nodes("node_ids")?;
    for &id in &nodes {
        doc.node(id).map_err(refused_edit)?;
    }
    let said = input.args.opt_map("style", "property names and values")?.ok_or_else(|| ToolError("\"style\" is required".into()))?;
    if said.is_empty() {
        return fail("\"style\" is empty: name at least one property to set");
    }
    let decimals = Precision::of(doc).decimals;
    let set = said.iter().map(|(name, value)| Ok((name.to_owned(), attr_value(name, value, decimals)?))).collect::<Result<_, ToolError>>()?;
    Ok(Command::SetStyle { nodes, set })
}

fn styled(doc: &Document, applied: &Applied) -> Reply {
    if applied.changed.is_empty() {
        return Reply::text("Nothing changed: they were painted that way already.");
    }
    let said: Vec<String> = applied.changed.iter().take(MAX_SAID).filter_map(|id| doc.get(*id)).map(|node| format!("{} {}: {}", node.id, tag(node), painted(node))).collect();
    let more = applied.changed.len().saturating_sub(MAX_SAID);
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(applied.changed.iter().map(|id| id.to_string().into()).collect()));
    Reply::text(format!("Styled. Now: {}{}.", said.join("; "), if more > 0 { format!("; and {more} more") } else { String::new() })).data(Doc::Map(m))
}
