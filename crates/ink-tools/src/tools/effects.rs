//! Clip paths and filters as tools: a node cut to a shape, a node given
//! a shadow or a blur. Edits: one Command, one undo step.

use ink_core::ink_doc::refs::Ids;
use ink_core::ink_doc::style::prop;
use ink_core::ink_doc::{Document, Element, Kind, Precision, color};
use ink_core::{Applied, Command, NodeId};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Reply, ToolError, fail, schema};

use crate::describe::tag;
use crate::input::{In, common, refused_edit};
use crate::tools::{Entry, edit};

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "clip_set",
            "Clip, release",
            "Cut nodes to a shape. `by` names one or more shapes (a rect, a circle, a path): they leave the drawing and become a clip path in <defs>, staying where they showed, and each of `node_ids` then shows only where they are. To cut several things as one, group them and clip the group. Or release: true takes the clip off instead: a clip path that then cuts nothing is taken apart, its shapes put back in the drawing just over what they cut. A clip path that is one node's alone moves with it (node_transform); one that others use is never changed.",
            clip_schema,
            lntrn_mcp::Kind::Set,
            clip,
            clipped,
        ),
        edit(
            "filter_set",
            "Shadow, blur",
            "Give nodes a drop shadow or a blur. `shadow` {dx, dy, blur, color, opacity} casts the node's shape, blurred, at an offset (dx = dy = 0 is a glow all round); `blur` (a number, the blur's deviation) blurs the node itself. Both are in the node's own units. It makes a <filter> in <defs>, with room around the node's box for the effect to show in, and sets it on every node named. Or remove: true takes a node's filter off. A node with a filter takes a move into its own numbers, but keeps a scale or a turn as a transform, so that its shadow scales and turns with it. Other filters, chains of steps, are written with node_add_svg.",
            filter_schema,
            lntrn_mcp::Kind::Set,
            filter,
            filtered,
        ),
    ]
}

fn node_ids(desc: &str) -> Doc {
    schema::list(common::node_id("A node"), desc)
}

/// The first of `stem-1`, `stem-2`, … that nothing is called; or
/// `wanted`, made different if it's taken.
fn free_id(doc: &Document, wanted: Option<&str>, stem: &str) -> Result<String, ToolError> {
    let ids = Ids::of(doc);
    match wanted.map(str::trim) {
        Some(wanted) if !wanted.is_empty() && wanted.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')) => Ok(ids.free(wanted)),
        Some(wanted) => fail(format!("\"{wanted}\" can't be an id: letters, digits, - and _ only, so it can be written url(#…)")),
        None => Ok((1..).map(|n| format!("{stem}-{n}")).find(|id| ids.get(id).is_none()).expect("there is always another number")),
    }
}

fn named(doc: &Document, input: &In, key: &str) -> Result<Vec<NodeId>, ToolError> {
    let nodes = input.nodes(key)?;
    for &id in &nodes {
        doc.node(id).map_err(refused_edit)?;
    }
    Ok(nodes)
}

fn list(nodes: &[NodeId]) -> String {
    nodes.iter().map(NodeId::to_string).collect::<Vec<_>>().join(", ")
}

fn clip_schema() -> Doc {
    common::edit(
        &["node_ids"],
        vec![
            ("node_ids", node_ids("The nodes to cut (or to release)")),
            ("by", node_ids("The shapes to cut them to")),
            ("release", schema::boolean("Take the clip off instead", false)),
            ("id", schema::string("The clip path's name (default clip-1, clip-2, …); made different if it's taken")),
        ],
    )
}

fn clip(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes = named(doc, input, "node_ids")?;
    let release = input.args.opt_bool("release")? == Some(true);
    let by = if input.args.has("by") { named(doc, input, "by")? } else { Vec::new() };
    match (by.is_empty(), release) {
        (true, false) => fail("say what to cut them to: by (one or more shapes), or release: true to take their clip off"),
        (false, true) => fail("give by or release, not both"),
        (true, true) => Ok(Command::SetClip { nodes, by, id: String::new() }),
        (false, false) => Ok(Command::SetClip { nodes, by, id: free_id(doc, input.args.opt_str("id")?, "clip")? }),
    }
}

fn clipped(doc: &Document, applied: &Applied) -> Reply {
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(applied.moved.iter().map(|id| id.to_string().into()).collect()));
    if let Some(clip) = applied.created.first().and_then(|id| doc.get(*id)) {
        let name = clip.attr("id").unwrap_or_default();
        let cut: Vec<NodeId> = applied.changed.iter().copied().filter(|id| doc.get(*id).is_some_and(|n| prop(n, "clip-path") == Some(&format!("url(#{name})")))).collect();
        m.insert("node_id", clip.id.to_string().into());
        m.insert("id", name.into());
        return Reply::text(format!("Made {} {} from {}: it now cuts {}, which show{} only where {} {}.", clip.id, tag(clip), list(&applied.moved), list(&cut), if cut.len() == 1 { "s" } else { "" }, if applied.moved.len() == 1 { "it" } else { "they" }, if applied.moved.len() == 1 { "is" } else { "are" })).data(Doc::Map(m));
    }
    let freed: Vec<NodeId> = applied.changed.iter().copied().filter(|id| !applied.moved.contains(id)).collect();
    if freed.is_empty() {
        return Reply::text("Nothing changed: they had no clip path to take off.").data(Doc::Map(m));
    }
    let mut text = format!("Took the clip off {}.", list(&freed));
    if !applied.removed.is_empty() {
        text += &format!(" Its clip path cut nothing any more and is gone: {} {} back in the drawing, over what {} cut.", list(&applied.moved), if applied.moved.len() == 1 { "is" } else { "are" }, if applied.moved.len() == 1 { "it" } else { "they" });
    }
    Reply::text(text).data(Doc::Map(m))
}

fn filter_schema() -> Doc {
    let shadow = schema::object(
        &[],
        vec![
            ("dx", schema::number(-1e6, 1e6, "How far right it falls (default 0)")),
            ("dy", schema::number(-1e6, 1e6, "How far down it falls (default 0)")),
            ("blur", schema::number(0.0, 1e6, "How soft it is: the blur's deviation (default 1)")),
            ("color", schema::string("Its colour (default black)")),
            ("opacity", schema::number(0.0, 1.0, "How dark it is (default 0.5)")),
        ],
    );
    common::edit(
        &["node_ids"],
        vec![
            ("node_ids", node_ids("The nodes")),
            ("shadow", shadow),
            ("blur", schema::number(0.0, 1e6, "Blur the node itself: the blur's deviation")),
            ("remove", schema::boolean("Take the node's filter off instead", false)),
            ("id", schema::string("The filter's name (default shadow-1 or blur-1, …); made different if it's taken")),
        ],
    )
}

/// The region a filter needs on `nodes` for an effect that reaches
/// `reach` of their own units past their shapes
/// ([`ink_core::ink_doc::filter::region`]: the window's shadows are
/// given the same).
fn region(doc: &Document, nodes: &[NodeId], reach: f64) -> Result<Vec<(&'static str, String)>, ToolError> {
    ink_core::ink_doc::filter::region(doc, nodes, reach).or_else(|id| {
        let node = doc.node(id).map_err(refused_edit)?;
        fail(format!("{id} {} has no box with both a width and a height, and a filter shows within a region measured by its node's box: group it with what it belongs to, and filter the group", tag(node)))
    })
}

fn filter(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let a = &input.args;
    let nodes = named(doc, input, "node_ids")?;
    let p = Precision::of(doc);
    let (stem, step, reach) = match (a.opt_map("shadow", "dx, dy, blur, color and opacity")?, a.opt_f64("blur")?, a.opt_bool("remove")? == Some(true)) {
        (None, None, true) => return Ok(Command::SetStyle { nodes, set: vec![("filter".to_owned(), None)] }),
        (None, None, false) => return fail("say what to give them: shadow, blur, or remove: true to take their filter off"),
        (Some(_), Some(_), _) | (Some(_), _, true) | (_, Some(_), true) => return fail("give one of shadow, blur or remove"),
        (Some(shadow), None, false) => {
            if let Some(other) = shadow.keys().find(|k| !["dx", "dy", "blur", "color", "opacity"].contains(k)) {
                return fail(format!("a shadow has dx, dy, blur, color and opacity, not \"{other}\""));
            }
            let num = |name: &str, default: f64| match shadow.get(name).filter(|d| !d.is_null()) {
                None => Ok(default),
                Some(d) => d.as_f64().filter(|v| v.is_finite()).ok_or_else(|| ToolError(format!("a shadow's \"{name}\" should be a number"))),
            };
            let (dx, dy, soft, opacity) = (num("dx", 0.0)?, num("dy", 0.0)?, num("blur", 1.0)?, num("opacity", 0.5)?);
            if soft < 0.0 || !(0.0..=1.0).contains(&opacity) {
                return fail("a shadow's blur isn't less than nothing, and its opacity is from 0 to 1");
            }
            let tint = match shadow.get("color").filter(|d| !d.is_null()) {
                None => "#000",
                Some(d) => d.as_str().filter(|c| color::parse(c).is_some()).ok_or_else(|| ToolError("a shadow's \"color\" should be a colour: \"#rrggbb\", a name, rgb(…)".into()))?.trim(),
            };
            let step = Element::new("feDropShadow").with("dx", p.number(dx)).with("dy", p.number(dy)).with("stdDeviation", p.number(soft)).with("flood-color", tint).with("flood-opacity", p.number(opacity));
            ("shadow", step, 3.0 * soft + dx.abs().max(dy.abs()))
        }
        (None, Some(soft), false) => {
            if soft <= 0.0 {
                return fail("a blur of nothing blurs nothing: give more than 0, or remove: true to take the filter off");
            }
            ("blur", Element::new("feGaussianBlur").with("stdDeviation", p.number(soft)), 3.0 * soft)
        }
    };
    let id = free_id(doc, a.opt_str("id")?, stem)?;
    let made = region(doc, &nodes, reach)?.into_iter().fold(Element::new("filter").with("id", id.as_str()), |filter, (name, value)| filter.with(name, value)).child(step);
    Ok(Command::Batch(vec![Command::Define { elements: vec![made] }, Command::SetStyle { nodes, set: vec![("filter".to_owned(), Some(format!("url(#{id})")))] }]))
}

fn filtered(doc: &Document, applied: &Applied) -> Reply {
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(applied.changed.iter().map(|id| id.to_string().into()).collect()));
    let Some(made) = applied.created.first().and_then(|id| doc.get(*id)).filter(|n| n.kind == Kind::Filter) else {
        return Reply::text(if applied.changed.is_empty() { "Nothing changed: they had no filter to take off.".to_owned() } else { format!("Took the filter off {}.", list(&applied.changed)) }).data(Doc::Map(m));
    };
    let name = made.attr("id").unwrap_or_default();
    m.insert("node_id", made.id.to_string().into());
    m.insert("id", name.into());
    Reply::text(format!("Made {} {}: it now filters {}.", made.id, tag(made), if applied.changed.is_empty() { "nothing (they had it already)".to_owned() } else { list(&applied.changed) })).data(Doc::Map(m))
}
