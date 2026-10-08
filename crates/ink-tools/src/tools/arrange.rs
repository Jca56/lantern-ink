//! Moving things about on the page: through a transform, and into line
//! with each other. Both work in the document's coordinates, wherever
//! in the tree a node is (ARCHITECTURE §3.3), and are edits: one
//! Command, one undo step.

use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::{Document, Viewport};
use ink_core::{Applied, Command, NodeId};
use ink_geom::{Affine, Rect, Vec2};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, ToolError, fail, schema};

use crate::describe::{rect, tag};
use crate::input::{In, common};
use crate::tools::{Entry, edit};

/// The most changed nodes a reply spells out.
const MAX_SAID: usize = 12;

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "node_transform",
            "Move, scale, turn",
            "Move, scale, rotate, skew or flip nodes on the page, any of them together, in the drawing's coordinates whatever groups the nodes are in. Applied in this order: scale, skew, rotate, flip (each about `pivot`, by default the middle of the nodes' joint box), then move. Or give `matrix` [a, b, c, d, e, f] instead. The nodes then look exactly as SVG's transform would make them (strokes, dashes and shadows scale along), and it is written into their own numbers wherever those can say it: a path takes anything, a circle stays a circle, a rect or an ellipse takes a move, a scale along its sides and a quarter turn, and keeps any other turn as one rotate(…) about its middle. What can't be said so (a skew; anything on a node with a clip path) stays in its transform attribute. A group passes it down to what's in it when all of that can take it. Naming the root transforms everything in the drawing.",
            transform_schema,
            Kind::Set,
            transform,
            transformed,
        ),
        edit(
            "node_align",
            "Align, spread",
            "Line nodes up, or spread them out, by the boxes where they show (strokes aside). `x` puts their left edges, centers or right edges in line and `y` their tops, middles or bottoms: with `to: \"page\"` against the page, with `to` a node id against that node (which stays put), otherwise against the box around them all (or the page, for one node). `spread` shares the space between the first and the last evenly along a row (horizontal) or a column (vertical). Each node only moves: nothing is scaled.",
            align_schema,
            Kind::Set,
            align,
            transformed,
        ),
    ]
}

fn node_ids(desc: &str) -> Doc {
    schema::list(common::node_id("A node"), desc)
}

fn numbers(desc: &str) -> Doc {
    schema::list(schema::number(-1e9, 1e9, "A number"), desc)
}

fn transform_schema() -> Doc {
    common::edit(
        &["node_ids"],
        vec![
            ("node_ids", node_ids("The nodes")),
            ("move", numbers("[dx, dy] in the drawing's coordinates")),
            ("scale", numbers("[sx, sy], or [s] for both")),
            ("rotate", schema::number(-36000.0, 36000.0, "Degrees, clockwise")),
            ("skew", numbers("[x, y] degrees: x leans upright lines sideways, y leans level ones")),
            ("flip", schema::one_of(&["horizontal", "vertical", "both"], "Mirror left-right, top-bottom, or both")),
            ("pivot", numbers("[x, y]: what stays put (default the middle of the nodes' joint box)")),
            ("matrix", numbers("Instead of the others: [a, b, c, d, e, f], as SVG's matrix()")),
        ],
    )
}

/// The numbers under `key`, if it's there: `count` of them (any of the
/// counts given).
fn list(input: &In, key: &str, counts: &[usize], what: &str) -> Result<Option<Vec<f64>>, ToolError> {
    let Some(items) = input.args.opt_list(key, "numbers")? else { return Ok(None) };
    let values: Option<Vec<f64>> = items.iter().map(|d| d.as_f64().filter(|v| v.is_finite())).collect();
    match values {
        Some(values) if counts.contains(&values.len()) => Ok(Some(values)),
        _ => fail(format!("\"{key}\" should be {what}")),
    }
}

/// The box around where `nodes` show, in the drawing's coordinates.
fn joint_box(doc: &Document, nodes: &[NodeId]) -> Option<Rect> {
    let boxes = page_bounds(doc);
    nodes.iter().filter_map(|id| boxes.get(id)).fold(None, |all: Option<Rect>, b| Some(all.map_or(*b, |a| a.union(b))))
}

fn transform(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes = input.nodes("node_ids")?;
    for &id in &nodes {
        doc.node(id).map_err(crate::input::refused_edit)?;
    }
    let shift = list(input, "move", &[2], "[dx, dy]")?;
    let scale = list(input, "scale", &[1, 2], "[sx, sy], or [s] for both")?;
    let rotate = input.args.opt_f64("rotate")?;
    let skew = list(input, "skew", &[2], "[x, y] in degrees")?;
    let flip = input.args.opt_str("flip")?;
    let pivot = list(input, "pivot", &[2], "[x, y]")?;
    if let Some(m) = list(input, "matrix", &[6], "[a, b, c, d, e, f]")? {
        if shift.is_some() || scale.is_some() || rotate.is_some() || skew.is_some() || flip.is_some() || pivot.is_some() {
            return fail("give matrix alone: it says everything the others would");
        }
        return Ok(Command::Transform { nodes, by: Affine::new(m[0], m[1], m[2], m[3], m[4], m[5]) });
    }
    let mut about = Affine::IDENTITY;
    if let Some(s) = &scale {
        let (sx, sy) = (s[0], *s.get(1).unwrap_or(&s[0]));
        if sx == 0.0 || sy == 0.0 {
            return fail("a scale of 0 squashes things flat: give one that leaves them some size");
        }
        about = about.then(&Affine::scale(sx, sy));
    }
    if let Some(k) = &skew {
        if k.iter().any(|d| (d.rem_euclid(180.0) - 90.0).abs() < 1e-9) {
            return fail("a skew of 90° lays things flat: give a smaller one");
        }
        about = about.then(&Affine::skew_x(k[0].to_radians())).then(&Affine::skew_y(k[1].to_radians()));
    }
    if let Some(degrees) = rotate {
        about = about.then(&Affine::rotate(degrees.to_radians()));
    }
    match flip {
        None => {}
        Some("horizontal") => about = about.then(&Affine::scale(-1.0, 1.0)),
        Some("vertical") => about = about.then(&Affine::scale(1.0, -1.0)),
        Some("both") => about = about.then(&Affine::scale(-1.0, -1.0)),
        Some(other) => return fail(format!("flip is horizontal, vertical or both, not \"{other}\"")),
    }
    let mut by = Affine::IDENTITY;
    if !about.is_identity() {
        let pivot = match pivot {
            Some(p) => Vec2::new(p[0], p[1]),
            None => joint_box(doc, &nodes).map(|b| b.center()).ok_or_else(|| ToolError("none of these nodes shows anywhere, so they have no middle to turn about: give pivot".into()))?,
        };
        by = about.about(pivot);
    }
    if let Some(d) = &shift {
        by = by.then(&Affine::translate(d[0], d[1]));
    }
    if shift.is_none() && scale.is_none() && rotate.is_none() && skew.is_none() && flip.is_none() {
        return fail("say what to do: move, scale, rotate, skew or flip (or matrix)");
    }
    Ok(Command::Transform { nodes, by })
}

/// What the nodes an edit changed are now, and where: for each, its box
/// in the drawing's coordinates and the `transform` it's left with.
fn transformed(doc: &Document, applied: &Applied) -> Reply {
    if applied.changed.is_empty() {
        return Reply::text("Nothing changed: that leaves them where they are.");
    }
    let boxes = page_bounds(doc);
    let said: Vec<String> = applied
        .changed
        .iter()
        .take(MAX_SAID)
        .filter_map(|id| doc.get(*id))
        .map(|node| {
            let mut text = format!("{} {}", node.id, tag(node));
            if let Some(b) = boxes.get(&node.id) {
                text += &format!(" at {}", rect(b));
            }
            match node.attr("transform") {
                Some(t) => text + &format!(", transform=\"{t}\""),
                None => text,
            }
        })
        .collect();
    let more = applied.changed.len().saturating_sub(MAX_SAID);
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(applied.changed.iter().map(|id| id.to_string().into()).collect()));
    Reply::text(format!("Done. Now: {}{}.", said.join("; "), if more > 0 { format!("; and {more} more") } else { String::new() })).data(Doc::Map(m))
}

fn align_schema() -> Doc {
    common::edit(
        &["node_ids"],
        vec![
            ("node_ids", node_ids("The nodes to move")),
            ("x", schema::one_of(&["left", "center", "right"], "Put these edges (or centers) in line, left to right")),
            ("y", schema::one_of(&["top", "middle", "bottom"], "Put these edges (or middles) in line, top to bottom")),
            ("to", schema::string("What to line up against: \"page\", or a node id like \"N3\" (or \"@name\" inside a batch); default: the box around the nodes (the page, for one node)")),
            ("spread", schema::one_of(&["horizontal", "vertical"], "Share the space between the first and last evenly")),
        ],
    )
}

/// How far along `whole` a box's `part` goes for an alignment: at its
/// start, its middle or its end.
fn along(at: f64, from: f64, to: f64) -> f64 {
    from + (to - from) * at
}

fn align(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes = input.nodes("node_ids")?;
    let boxes = page_bounds(doc);
    let mut shown: Vec<(NodeId, Rect)> = Vec::new();
    for &id in &nodes {
        let node = doc.node(id).map_err(crate::input::refused_edit)?;
        let Some(b) = boxes.get(&id) else { return fail(format!("{id} {} shows nowhere, so there's nothing of it to line up", tag(node))) };
        if !shown.iter().any(|(seen, _)| *seen == id) {
            shown.push((id, *b));
        }
    }
    let x = match input.args.opt_str("x")? {
        None => None,
        Some("left") => Some(0.0),
        Some("center") => Some(0.5),
        Some("right") => Some(1.0),
        Some(other) => return fail(format!("x is left, center or right, not \"{other}\"")),
    };
    let y = match input.args.opt_str("y")? {
        None => None,
        Some("top") => Some(0.0),
        Some("middle") => Some(0.5),
        Some("bottom") => Some(1.0),
        Some(other) => return fail(format!("y is top, middle or bottom, not \"{other}\"")),
    };
    let spread = input.args.opt_str("spread")?;
    if x.is_none() && y.is_none() && spread.is_none() {
        return fail("say how: x (left, center, right), y (top, middle, bottom) or spread (horizontal, vertical)");
    }
    let mut moves: Vec<(NodeId, Vec2)> = shown.iter().map(|(id, _)| (*id, Vec2::ZERO)).collect();
    if x.is_some() || y.is_some() {
        let page = || {
            let v = Viewport::of(doc.node(doc.root()).expect("a document has its root"));
            let (a, b) = v.to_page.inverse().map_or((Vec2::ZERO, v.size), |back| (back.apply(Vec2::ZERO), back.apply(v.size)));
            Rect::new(a.min(b), a.max(b))
        };
        let (against, still) = match input.args.opt_str("to")? {
            Some("page") => (page(), None),
            None if shown.len() == 1 => (page(), None),
            None => (shown.iter().skip(1).fold(shown[0].1, |all, (_, b)| all.union(b)), None),
            Some(other) => {
                // A node an earlier step of the batch made, by its name.
                let id: NodeId = if other.starts_with('@') { input.node("to")? } else { other.parse().map_err(|_| ToolError(format!("\"to\" is \"page\" or a node id like \"N3\", not \"{other}\"")))? };
                let node = doc.node(id).map_err(crate::input::refused_edit)?;
                (*boxes.get(&id).ok_or_else(|| ToolError(format!("{id} {} shows nowhere, so there's nothing to line up against", tag(node))))?, Some(id))
            }
        };
        for ((id, b), (_, shift)) in shown.iter().zip(&mut moves) {
            if still == Some(*id) {
                continue;
            }
            if let Some(at) = x {
                shift.x = along(at, against.min.x, against.max.x) - along(at, b.min.x, b.max.x);
            }
            if let Some(at) = y {
                shift.y = along(at, against.min.y, against.max.y) - along(at, b.min.y, b.max.y);
            }
        }
    }
    if let Some(way) = spread {
        let across = match way {
            "horizontal" => true,
            "vertical" => false,
            other => return fail(format!("spread is horizontal or vertical, not \"{other}\"")),
        };
        if shown.len() < 3 {
            return fail("spread shares out the space between the first and the last: it takes three nodes or more");
        }
        let side = |b: &Rect| if across { (b.min.x, b.max.x) } else { (b.min.y, b.max.y) };
        // In the order they stand, the outer two staying put: the gaps
        // between them are made the same.
        let mut order: Vec<usize> = (0..shown.len()).collect();
        order.sort_by(|&a, &b| side(&shown[a].1).0.total_cmp(&side(&shown[b].1).0));
        let (start, end) = (side(&shown[order[0]].1).0, order.iter().map(|&i| side(&shown[i].1).1).fold(f64::MIN, f64::max));
        let taken: f64 = order.iter().map(|&i| side(&shown[i].1).1 - side(&shown[i].1).0).sum();
        let gap = (end - start - taken) / (shown.len() - 1) as f64;
        let mut at = start;
        for &i in &order {
            let (lo, hi) = side(&shown[i].1);
            if across { moves[i].1.x = at - lo } else { moves[i].1.y = at - lo }
            at += hi - lo + gap;
        }
    }
    let commands: Vec<Command> = moves.into_iter().filter(|(_, d)| *d != Vec2::ZERO).map(|(id, d)| Command::Transform { nodes: vec![id], by: Affine::translate(d.x, d.y) }).collect();
    Ok(Command::Batch(commands))
}
