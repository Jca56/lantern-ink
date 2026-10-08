//! Gradients: making one in the drawing's `<defs>` (and painting with
//! it), and changing one that's there. Edits: one Command, one undo
//! step.

use ink_core::ink_doc::gradient::{self, NewStop};
use ink_core::ink_doc::refs::Ids;
use ink_core::ink_doc::{Document, Kind, Node, Precision, color};
use ink_core::{Applied, Command, NodeId, Place};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Reply, ToolError, fail, schema};

use crate::describe::{attributes, tag};
use crate::input::{In, common, refused_edit};
use crate::tools::{Entry, edit};

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "gradient_add",
            "Add gradient",
            "Make a gradient in the drawing's <defs> (made if there isn't one) and, with `fill` or `stroke`, paint nodes with it. `colors` are spread evenly along it, or `stops` place each ({offset 0..1, color, opacity}). A linear one runs along the line `from` → `to`; a radial one spreads from `center` out to `radius` (from `focus`, if given). With units \"box\" (the default) those are fractions of each painted shape's own box, so [0, 0] → [0, 1] is top to bottom of whatever it paints, it follows a shape that moves, and any number of shapes can share it. With units \"user\" they are in the painted shape's own coordinates (the drawing's, unless it's under a transform): one gradient can then run across several shapes, and a line with no height can be painted (by its box it can't: that's refused). Returns the gradient's node id and its id=\"…\" (say `id` to choose it): paint with it later as \"url(#id)\" through node_style.",
            add_schema,
            lntrn_mcp::Kind::Add,
            add,
            added,
        ),
        edit(
            "gradient_set",
            "Change gradient",
            "Change a gradient that's there (by its node id): its `colors` or `stops` (all of them, replacing the ones it has), its line (`from`, `to`) or circle (`center`, `radius`, `focus`), its `units` and its `spread`. What isn't given stays. One stop alone is a node like any other: node_set changes its offset or stop-color.",
            set_schema,
            lntrn_mcp::Kind::Set,
            set,
            was_set,
        ),
    ]
}

fn point(desc: &str) -> Doc {
    schema::list(schema::number(-1e9, 1e9, "A coordinate"), desc)
}

/// What both tools take to say what a gradient is.
fn shape_props() -> schema::Props {
    let stop = schema::object(
        &["offset", "color"],
        vec![("offset", schema::number(0.0, 1.0, "Where along the gradient, 0 to 1")), ("color", schema::string("Its colour: \"#rrggbb\", a name, rgb(…)")), ("opacity", schema::number(0.0, 1.0, "How solid it is there (default 1)"))],
    );
    vec![
        ("colors", schema::list(schema::string("A colour"), "Colours spread evenly from start to end")),
        ("stops", schema::list(stop, "Instead of colors: each colour and where it sits, in order")),
        ("from", point("Linear: [x, y] where it starts (default [0, 0])")),
        ("to", point("Linear: [x, y] where it ends (default [1, 0]: left to right)")),
        ("center", point("Radial: [x, y] of its middle (default [0.5, 0.5])")),
        ("radius", schema::number(0.0, 1e9, "Radial: how far out it reaches (default 0.5)")),
        ("focus", point("Radial: [x, y] the rings spread from (default: the center)")),
        ("units", schema::one_of(&["box", "user"], "What those numbers are in: fractions of each painted shape's box (default), or the shape's own coordinates")),
        ("spread", schema::one_of(&["pad", "reflect", "repeat"], "What it does past its ends (default pad: the end colours go on)")),
    ]
}

fn add_schema() -> Doc {
    let mut props = vec![("kind", schema::one_of(&["linear", "radial"], "linear (default) or radial"))];
    props.extend(shape_props());
    props.push(("id", schema::string("The name to use it by (default gradient-1, gradient-2, …); made different if it's taken")));
    props.push(("fill", schema::list(common::node_id("A node"), "Nodes to fill with it")));
    props.push(("stroke", schema::list(common::node_id("A node"), "Nodes to stroke with it")));
    common::edit(&[], props)
}

/// `[x, y]` under `key`, if it's there.
fn pair(input: &In, key: &str) -> Result<Option<(f64, f64)>, ToolError> {
    let Some(items) = input.args.opt_list(key, "numbers")? else { return Ok(None) };
    match items.iter().map(|d| d.as_f64().filter(|v| v.is_finite())).collect::<Option<Vec<f64>>>().as_deref() {
        Some(&[x, y]) => Ok(Some((x, y))),
        _ => fail(format!("\"{key}\" should be [x, y]")),
    }
}

/// The stops asked for: `colors` spread evenly, or `stops` as placed.
/// `None` when neither is given.
fn stops(input: &In) -> Result<Option<Vec<NewStop>>, ToolError> {
    let colour = |text: &str| if color::parse(text).is_some() { Ok(text.trim().to_owned()) } else { fail(format!("\"{text}\" isn't a colour: give \"#rrggbb\", a name like \"gold\", or rgb(…)")) };
    let made = match (input.args.opt_list("colors", "colours")?, input.args.opt_list("stops", "{offset, color, opacity}")?) {
        (Some(_), Some(_)) => return fail("give colors or stops, not both"),
        (None, None) => return Ok(None),
        (Some(colors), None) => {
            let last = colors.len().saturating_sub(1).max(1) as f64;
            colors.iter().enumerate().map(|(i, c)| Ok(NewStop { offset: i as f64 / last, color: colour(c.as_str().ok_or_else(|| ToolError("\"colors\" should be a list of colours".into()))?)?, opacity: None })).collect::<Result<Vec<_>, ToolError>>()?
        }
        (None, Some(stops)) => stops
            .iter()
            .map(|stop| {
                let field = |name: &str| stop.as_map().and_then(|m| m.get(name));
                let offset = field("offset").and_then(Doc::as_f64).filter(|v| (0.0..=1.0).contains(v)).ok_or_else(|| ToolError("each stop needs an \"offset\" from 0 to 1".into()))?;
                let color = colour(field("color").and_then(Doc::as_str).ok_or_else(|| ToolError("each stop needs a \"color\"".into()))?)?;
                let opacity = match field("opacity").filter(|d| !d.is_null()) {
                    None => None,
                    Some(d) => Some(d.as_f64().filter(|v| (0.0..=1.0).contains(v)).ok_or_else(|| ToolError("a stop's \"opacity\" is from 0 to 1".into()))?),
                };
                Ok(NewStop { offset, color, opacity })
            })
            .collect::<Result<Vec<_>, ToolError>>()?,
    };
    if made.is_empty() {
        return fail("a gradient needs at least one colour");
    }
    Ok(Some(made))
}

/// What the call says of a gradient's line or circle, units and spread,
/// as attributes to set (`None`: to take off, the default being meant).
/// `radial` is the kind it's for; `new` fills in what isn't said.
fn said(input: &In, radial: bool, new: bool, p: &Precision) -> Result<Vec<(&'static str, Option<String>)>, ToolError> {
    let a = &input.args;
    let (from, to, center, focus) = (pair(input, "from")?, pair(input, "to")?, pair(input, "center")?, pair(input, "focus")?);
    let radius = a.opt_f64("radius")?;
    if radial && (from.is_some() || to.is_some()) {
        return fail("a radial gradient has a center, a radius and a focus, not from and to");
    }
    if !radial && (center.is_some() || radius.is_some() || focus.is_some()) {
        return fail("a linear gradient runs from → to: it has no center, radius or focus");
    }
    let user = match a.opt_str("units")? {
        None => None,
        Some("box") => Some(false),
        Some("user") => Some(true),
        Some(other) => return fail(format!("units is box or user, not \"{other}\"")),
    };
    let mut attrs: Vec<(&'static str, Option<String>)> = Vec::new();
    let n = |v: f64| Some(p.number(v));
    if radial {
        // In the shape's own coordinates there's no middle to presume.
        if new && user == Some(true) && (center.is_none() || radius.is_none()) {
            return fail("with units \"user\" say where it is: center and radius, in the painted shape's coordinates");
        }
        if let Some((x, y)) = center.or(new.then_some((0.5, 0.5))) {
            attrs.extend([("cx", n(x)), ("cy", n(y))]);
        }
        if let Some(r) = radius.or(new.then_some(0.5)) {
            attrs.push(("r", n(r)));
        }
        if let Some((x, y)) = focus {
            attrs.extend([("fx", n(x)), ("fy", n(y))]);
        }
    } else {
        if new && user == Some(true) && (from.is_none() || to.is_none()) {
            return fail("with units \"user\" say where it runs: from and to, in the painted shape's coordinates");
        }
        if let Some((x, y)) = from.or(new.then_some((0.0, 0.0))) {
            attrs.extend([("x1", n(x)), ("y1", n(y))]);
        }
        if let Some((x, y)) = to.or(new.then_some((1.0, 0.0))) {
            attrs.extend([("x2", n(x)), ("y2", n(y))]);
        }
    }
    // The box is what a gradient goes by unless it says otherwise, and
    // padding what it does past its ends: neither needs saying.
    match user {
        Some(true) => attrs.push(("gradientUnits", Some("userSpaceOnUse".to_owned()))),
        Some(false) if !new => attrs.push(("gradientUnits", None)),
        _ => {}
    }
    match a.opt_str("spread")? {
        None => {}
        Some("pad") if new => {}
        Some("pad") => attrs.push(("spreadMethod", None)),
        Some(way @ ("reflect" | "repeat")) => attrs.push(("spreadMethod", Some(way.to_owned()))),
        Some(other) => return fail(format!("spread is pad, reflect or repeat, not \"{other}\"")),
    }
    Ok(attrs)
}

fn add(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let a = &input.args;
    let radial = match a.opt_str("kind")? {
        None | Some("linear") => false,
        Some("radial") => true,
        Some(other) => return fail(format!("kind is linear or radial, not \"{other}\"")),
    };
    let p = Precision::of(doc);
    let Some(stops) = stops(input)? else { return fail("say what colours it has: colors (spread evenly) or stops (each placed)") };
    let attrs: Vec<(&str, String)> = said(input, radial, true, &p)?.into_iter().filter_map(|(name, value)| Some((name, value?))).collect();
    let ids = Ids::of(doc);
    let id = match a.opt_str("id")?.map(str::trim) {
        Some(wanted) if !wanted.is_empty() && wanted.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.')) => ids.free(wanted),
        Some(wanted) => return fail(format!("\"{wanted}\" can't be an id: letters, digits, - and _ only, so it can be written url(#…)")),
        None => (1..).map(|n| format!("gradient-{n}")).find(|id| ids.get(id).is_none()).expect("there is always another number"),
    };
    let mut commands = vec![Command::Define { elements: vec![gradient::element(radial, &id, &attrs, &stops, &p)] }];
    for (key, property) in [("fill", "fill"), ("stroke", "stroke")] {
        if a.has(key) {
            let nodes = input.nodes(key)?;
            for &node in &nodes {
                doc.node(node).map_err(refused_edit)?;
            }
            commands.push(Command::SetStyle { nodes, set: vec![(property.to_owned(), Some(format!("url(#{id})")))] });
        }
    }
    Ok(Command::Batch(commands))
}

/// How many stops `gradient` has.
fn stop_count(doc: &Document, gradient: &Node) -> usize {
    gradient.elements().filter(|&id| doc.get(id).is_some_and(|n| n.kind == Kind::Stop)).count()
}

fn added(doc: &Document, applied: &Applied) -> Reply {
    let Some(gradient) = applied.created.first().and_then(|id| doc.get(*id)) else { return Reply::text("Nothing was made.") };
    let name = gradient.attr("id").unwrap_or_default();
    let stops = stop_count(doc, gradient);
    let mut text = format!("Made {} {} with {stops} stop{}: paint with it as \"url(#{name})\".", gradient.id, tag(gradient), if stops == 1 { "" } else { "s" });
    if !applied.changed.is_empty() {
        text += &format!(" Painted with it: {}.", applied.changed.iter().map(NodeId::to_string).collect::<Vec<_>>().join(", "));
    }
    let mut m = Map::new();
    m.insert("node_id", gradient.id.to_string().into());
    m.insert("id", name.into());
    m.insert("node_ids", Doc::List(applied.changed.iter().map(|id| id.to_string().into()).collect()));
    Reply::text(text).data(Doc::Map(m))
}

fn set_schema() -> Doc {
    let mut props = vec![("node_id", common::node_id("The gradient"))];
    props.extend(shape_props());
    common::edit(&["node_id"], props)
}

fn set(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let id = input.node("node_id")?;
    let node = doc.node(id).map_err(refused_edit)?;
    let radial = match node.kind {
        Kind::LinearGradient => false,
        Kind::RadialGradient => true,
        _ => return fail(format!("{id} is a <{}>, not a gradient (doc_info lists them under <defs>)", node.name)),
    };
    let p = Precision::of(doc);
    let mut commands: Vec<Command> = said(input, radial, false, &p)?.into_iter().map(|(name, value)| Command::SetAttr { node: id, name: name.to_owned(), value }).collect();
    if let Some(stops) = stops(input)? {
        let old: Vec<NodeId> = node.elements().filter(|&c| doc.get(c).is_some_and(|n| n.kind == Kind::Stop)).collect();
        if !old.is_empty() {
            commands.push(Command::Delete { nodes: old });
        }
        commands.push(Command::Insert { place: Place::LastIn(id), elements: stops.iter().map(|s| s.element(&p)).collect() });
    }
    if commands.is_empty() {
        return fail("say what to change: colors or stops, from, to, center, radius, focus, units or spread");
    }
    Ok(Command::Batch(commands))
}

fn was_set(doc: &Document, applied: &Applied) -> Reply {
    // The gradient: what changed, or what the new stops are in.
    let gradient = applied.changed.first().copied().or_else(|| applied.created.first().and_then(|stop| doc.get(*stop)?.parent)).and_then(|id| doc.get(id));
    let Some(gradient) = gradient else { return Reply::text("Nothing changed: it was like that already.") };
    let stops = stop_count(doc, gradient);
    let mut m = Map::new();
    m.insert("node_id", gradient.id.to_string().into());
    Reply::text(format!("Set. {} {} is now:{} with {stops} stop{}.", gradient.id, tag(gradient), attributes(gradient), if stops == 1 { "" } else { "s" })).data(Doc::Map(m))
}
