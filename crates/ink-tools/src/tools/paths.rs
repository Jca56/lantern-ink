//! Paths by their anchors: a whole outline set, an outline edited
//! point by point, and the operations that make one path of another.
//! Edits: one Command, one undo step.

use ink_core::ink_doc::outline::AnchorId;
use ink_core::ink_doc::pathedit::{Along, PathEdit};
use ink_core::ink_doc::paths::{NewAnchor, NewRun};
use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::{Document, Kind};
use ink_core::{Applied, Command, NodeId};
use ink_geom::{Combine, Vec2};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Reply, ToolError, fail, schema};

use crate::describe::{anchors, rect, tag};
use crate::input::{In, common, refused_edit};
use crate::tools::{Entry, edit};

/// The edits a `path_edit` takes, each with the keys it reads.
const OPS: [(&str, &[&str]); 12] = [
    ("move", &["anchors", "anchor", "by", "to"]),
    ("handles", &["anchor", "in", "out"]),
    ("add", &["after", "share", "near"]),
    ("delete", &["anchors", "anchor"]),
    ("bend", &["after", "through"]),
    ("line", &["after"]),
    ("smooth", &["anchors", "anchor"]),
    ("corner", &["anchors", "anchor"]),
    ("close", &["anchor"]),
    ("break", &["anchor"]),
    ("join", &["a", "b"]),
    ("reverse", &["anchor"]),
];
/// The most edits one call makes.
const MAX_EDITS: usize = 200;

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "path_set",
            "Set outline",
            "Set a path's whole outline, replacing what it had: `points` [[x, y], …] joined by straight lines (with smooth: true, by one flowing curve through them), or `anchors` [{at: [x, y], in: [dx, dy], out: [dx, dy]}] for exact curves, where `in` and `out` are an anchor's handles as offsets from it (leave one out and the path runs straight on that side). closed: true joins the last back to the first. Numbers are in the path's own coordinates, as its d has them. An anchor given the `id` of one the path has (\"A3\") is that anchor still. A shape that isn't a path yet (a rect, a circle) is made one. For a path of several runs, set its d with node_set.",
            set_schema,
            lntrn_mcp::Kind::Set,
            set,
            outlined,
        ),
        edit(
            "path_edit",
            "Edit anchors",
            "Edit a path point by point: `edits` are made in order, each {op, …} naming anchors by the ids node_info lists (\"A3\"), which stay each anchor's own whatever is added or taken out around it. move {anchors, by: [dx, dy]}, or {anchor, to: [x, y]} (handles go along). handles {anchor, in, out}: an anchor's handles as offsets from it (null takes one off). add {after, share} puts an anchor on the segment that goes out of `after`, a share of the way along (default 0.5) or {after, near: [x, y]} nearest a point, the path keeping its shape. delete {anchors}. bend {after, through: [x, y]} curves that segment so its middle passes through a point; line {after} straightens it. smooth {anchors} and corner {anchors}. close {anchor} closes its run; break {anchor} opens a closed run there, or parts an open one in two; join {a, b} joins two loose ends; reverse {anchor} turns its run round (every run, with no anchor). An add or a break can name the anchor it makes with `as`, for later edits of the same call to use as \"@name\". Numbers are in the path's own coordinates, as its d has them. A shape that isn't a path yet (a rect, a circle) has no anchors to name: path_op to_path makes it one and lists them.",
            edit_schema,
            lntrn_mcp::Kind::Set,
            edit_path,
            outlined,
        ),
        edit(
            "path_op",
            "Path operation",
            "Work on whole outlines. to_path: each shape (a rect, a circle, an ellipse, a line, a polyline, a polygon) becomes a <path> that draws the same outline, a rounded corner still an arc, with everything else about it as it was. reverse: each path runs the other way. union, subtract, intersect and exclude make several shapes one, by what their fills cover wherever each shows in the drawing: union is what any of them covers, intersect what all of them do, subtract the first less the others, exclude what an odd number cover (two shapes less their overlap). The first node named takes the result as its outline (a <path> now) and keeps its place, its paint and its id; the others are deleted. Curves stay the curves they were, cut where the outlines cross: nothing is flattened. A union of one shape makes its outline simple where it crosses itself. outline: each shape's stroke becomes a shape of its own, a path covering what the stroke covered (its width, caps, joins and dashes), filled with what the stroke was painted with; a shape with no fill (or no inside to fill: a line) becomes that path, and one with a fill keeps it and gets the outline as a new path over it. A stroke's edge beside a straight line or a circle's arc is a line or an arc still; beside any other curve it's fitted with curves, to within `tolerance`. simplify: each path is said with as few segments as keep its outline within `tolerance` of where it was (in its own units; by default a five-hundredth of its size): runs of short lines become the arc or the curve they were drawn round (smooth where its pieces meet), a curve in pieces is one curve again, corners stay corners, and the anchors left keep their ids. For one path, to_path, reverse and simplify list its anchors.",
            op_schema,
            lntrn_mcp::Kind::Set,
            op,
            operated,
        ),
    ]
}

fn pair_schema(desc: &str) -> Doc {
    schema::list(schema::number(-1e9, 1e9, "A coordinate"), desc)
}

fn anchor_id(desc: &str) -> Doc {
    schema::pattern("^(A[0-9]+|@[A-Za-z0-9_-]+)$", &format!("{desc}: an anchor id like \"A3\" (or \"@name\" for one an earlier edit of this call made)"))
}

fn set_schema() -> Doc {
    let anchor = schema::object(
        &["at"],
        vec![
            ("at", pair_schema("[x, y]: where it is")),
            ("in", pair_schema("[dx, dy]: its handle on the side the path comes in, from the anchor")),
            ("out", pair_schema("[dx, dy]: its handle on the side the path goes out")),
            ("id", schema::pattern("^A[0-9]+$", "The anchor of the path as it is that this one still is")),
        ],
    );
    common::edit(
        &["node_id"],
        vec![
            ("node_id", common::node_id("The path")),
            ("points", schema::list(pair_schema("[x, y]"), "Anchors joined by straight lines (or a smooth curve)")),
            ("anchors", schema::list(anchor, "Instead of points: anchors with their handles")),
            ("closed", schema::boolean("Join the last anchor back to the first", false)),
            ("smooth", schema::boolean("With points: one flowing curve through them", false)),
        ],
    )
}

/// `[x, y]` in `d`.
fn pair(d: &Doc, what: &str) -> Result<Vec2, ToolError> {
    match d.as_list().map(|l| l.iter().map(|v| v.as_f64().filter(|n| n.is_finite())).collect::<Option<Vec<f64>>>()) {
        Some(Some(xy)) if xy.len() == 2 => Ok(Vec2::new(xy[0], xy[1])),
        _ => fail(format!("{what} should be [x, y]")),
    }
}

fn set(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let node = input.node("node_id")?;
    doc.node(node).map_err(refused_edit)?;
    let a = &input.args;
    let closed = a.opt_bool("closed")? == Some(true);
    let run: Vec<NewAnchor> = match (a.opt_list("points", "[x, y] pairs")?, a.opt_list("anchors", "{at, in, out}")?) {
        (Some(_), Some(_)) => return fail("give points or anchors, not both"),
        (None, None) => return fail("say what the outline is: points (joined by lines) or anchors (with handles)"),
        (Some(points), None) => {
            let at: Vec<Vec2> = points.iter().map(|p| pair(p, "each point")).collect::<Result<_, _>>()?;
            let n = at.len();
            let smooth = a.opt_bool("smooth")? == Some(true) && n >= 3;
            // A curve through every point, leaving each along the line
            // from the point before to the point after.
            let lean = |i: usize| {
                let (before, after) = if closed { (Some((i + n - 1) % n), Some((i + 1) % n)) } else { (i.checked_sub(1), (i + 1 < n).then_some(i + 1)) };
                before.zip(after).filter(|_| smooth).map(|(b, a)| (at[a] - at[b]) * (1.0 / 6.0))
            };
            (0..n).map(|i| NewAnchor { at: at[i], into: lean(i).map(|h| h * -1.0), out: lean(i), id: None }).collect()
        }
        (None, Some(anchors)) => {
            if a.opt_bool("smooth")? == Some(true) {
                return fail("smooth goes with points: anchors say their own handles");
            }
            anchors
                .iter()
                .map(|anchor| {
                    let Some(fields) = anchor.as_map() else { return fail("each anchor should be {at, in, out}") };
                    if let Some(other) = fields.keys().find(|k| !["at", "in", "out", "id"].contains(k)) {
                        return fail(format!("an anchor has at, in, out and id, not \"{other}\""));
                    }
                    let handle = |name: &str| fields.get(name).filter(|d| !d.is_null()).map(|d| pair(d, &format!("an anchor's \"{name}\""))).transpose();
                    let id = match fields.get("id").filter(|d| !d.is_null()) {
                        None => None,
                        Some(d) => Some(d.as_str().and_then(|s| s.parse::<AnchorId>().ok()).ok_or_else(|| ToolError("an anchor's \"id\" should be an anchor id like \"A3\"".into()))?),
                    };
                    Ok(NewAnchor { at: pair(fields.get("at").ok_or_else(|| ToolError("each anchor needs an \"at\": [x, y]".into()))?, "an anchor's \"at\"")?, into: handle("in")?, out: handle("out")?, id })
                })
                .collect::<Result<_, _>>()?
        }
    };
    if run.is_empty() {
        return fail("an outline needs at least one point");
    }
    Ok(Command::SetPath { node, runs: vec![NewRun { anchors: run, closed }] })
}

/// A handle as an edit gives it: `[dx, dy]`, or null to take it off.
fn handle_schema(desc: &str) -> Doc {
    let mut m = Map::new();
    m.insert("type", Doc::List(vec!["array".into(), "null".into()]));
    m.insert("items", schema::number(-1e9, 1e9, "dx or dy"));
    m.insert("description", desc.into());
    Doc::Map(m)
}

fn edit_schema() -> Doc {
    let one = schema::object(
        &["op"],
        vec![
            ("op", schema::one_of(&OPS.map(|(op, _)| op), "What to do")),
            ("anchor", anchor_id("One anchor")),
            ("anchors", schema::list(anchor_id("An anchor"), "Several anchors")),
            ("after", anchor_id("add, bend, line: the anchor the segment goes out of")),
            ("by", pair_schema("move: [dx, dy]")),
            ("to", pair_schema("move: [x, y] to put one anchor at")),
            ("in", handle_schema("handles: [dx, dy] from the anchor, on the side the path comes in (null takes it off)")),
            ("out", handle_schema("handles: [dx, dy] from the anchor, on the side the path goes out (null takes it off)")),
            ("share", schema::number(0.0, 1.0, "add: how far along the segment, 0..1 (default 0.5)")),
            ("near", pair_schema("add: instead of share, the point [x, y] to be nearest")),
            ("through", pair_schema("bend: [x, y] the segment's middle should pass through")),
            ("a", anchor_id("join: one loose end")),
            ("b", anchor_id("join: the other")),
            ("as", schema::pattern("^[A-Za-z0-9_-]+$", "Name the anchor this edit makes, for later edits' \"@name\"")),
        ],
    );
    common::edit(&["node_id", "edits"], vec![("node_id", common::node_id("The path")), ("edits", schema::list(one, "The edits, in order"))])
}

/// The anchors earlier edits of a call made, by the names they gave.
type Named = Vec<(String, AnchorId)>;

/// One edit as the call wrote it, the "@names" among its anchors being
/// `names`'.
fn one_edit(fields: &Map, n: usize, names: &Named) -> Result<PathEdit, ToolError> {
    let here = |why: String| ToolError(format!("edit {n}: {why}"));
    let every = || OPS.map(|(op, _)| op).join(", ");
    let op = fields.get("op").and_then(Doc::as_str).ok_or_else(|| here(format!("needs an \"op\": one of {}", every())))?;
    let Some((_, takes)) = OPS.iter().find(|(known, _)| *known == op) else { return Err(here(format!("there's no edit \"{op}\": one of {}", every()))) };
    // A key the edit has no use for is a slip: read past, it would be
    // made some other way than was meant.
    if let Some((stray, _)) = fields.iter().find(|&(key, _)| !["op", "as"].contains(&key) && !takes.contains(&key)) {
        return Err(here(format!("{op} takes {}, not \"{stray}\"", takes.join(", "))));
    }
    let has = |key: &str| fields.get(key).is_some_and(|d| !d.is_null());
    let named = |d: &Doc, key: &str| -> Result<AnchorId, ToolError> {
        let text = d.as_str().ok_or_else(|| here(format!("\"{key}\" should be an anchor id like \"A3\"")))?;
        match text.strip_prefix('@') {
            Some(name) => names.iter().find(|(known, _)| known == name).map(|(_, id)| *id).ok_or_else(|| here(format!("no earlier edit of this call is named \"{name}\" (an add or a break names its anchor with \"as\")"))),
            None => text.parse().map_err(|_| here(format!("\"{text}\" isn't an anchor id like \"A3\""))),
        }
    };
    let anchor = |key: &str| -> Result<AnchorId, ToolError> { named(fields.get(key).filter(|d| !d.is_null()).ok_or_else(|| here(format!("{op} needs \"{key}\": an anchor id like \"A3\"")))?, key) };
    // Several anchors, or the one.
    let several = || -> Result<Vec<AnchorId>, ToolError> {
        match (fields.get("anchors").filter(|d| !d.is_null()), has("anchor")) {
            (Some(_), true) => Err(here("give \"anchors\" or \"anchor\", not both".into())),
            (None, true) => Ok(vec![anchor("anchor")?]),
            (Some(list), false) => match list.as_list().filter(|l| !l.is_empty()) {
                Some(list) => list.iter().map(|d| named(d, "anchors")).collect(),
                None => Err(here("\"anchors\" should be a list of anchor ids like [\"A3\"]".into())),
            },
            (None, false) => Err(here(format!("{op} needs \"anchors\": a list of anchor ids like [\"A3\"]"))),
        }
    };
    let point = |key: &str| -> Result<Vec2, ToolError> { pair(fields.get(key).ok_or_else(|| here(format!("{op} needs \"{key}\": [x, y]")))?, &format!("\"{key}\"")).map_err(|e| here(e.0)) };
    // A handle: not said, said null (take it off), or [dx, dy].
    let handle = |key: &str| -> Result<Option<Option<Vec2>>, ToolError> {
        match fields.get(key) {
            None => Ok(None),
            Some(Doc::Null) => Ok(Some(None)),
            Some(d) => pair(d, &format!("\"{key}\"")).map(|h| Some(Some(h))).map_err(|e| here(e.0)),
        }
    };
    Ok(match op {
        "move" => match (has("by"), has("to")) {
            (true, true) => return Err(here("move goes \"by\" [dx, dy] or \"to\" [x, y], not both".into())),
            (false, false) => return Err(here("move needs \"by\": [dx, dy] (or \"to\": [x, y], for one anchor)".into())),
            (true, false) => PathEdit::Move { anchors: several()?, by: point("by")? },
            (false, true) => match several()?[..] {
                [anchor] => PathEdit::MoveTo { anchor, to: point("to")? },
                _ => return Err(here("\"to\" puts one anchor at a place: name one, or move several \"by\" [dx, dy]".into())),
            },
        },
        "handles" => {
            let (into, out) = (handle("in")?, handle("out")?);
            if into.is_none() && out.is_none() {
                return Err(here("handles needs \"in\" or \"out\": [dx, dy] from the anchor, or null to take it off".into()));
            }
            PathEdit::Handles { anchor: anchor("anchor")?, into, out }
        }
        "add" => {
            let at = match (has("share"), has("near")) {
                (true, true) => return Err(here("add goes a \"share\" of the way along or \"near\" a point, not both".into())),
                (false, true) => Along::Nearest(point("near")?),
                (true, false) => match fields.get("share").and_then(Doc::as_f64).filter(|s| (0.0..=1.0).contains(s)) {
                    Some(share) => Along::Share(share),
                    None => return Err(here("\"share\" is how far along the segment, from 0 to 1".into())),
                },
                (false, false) => Along::Share(0.5),
            };
            PathEdit::Add { after: anchor("after")?, at }
        }
        "delete" => PathEdit::Delete { anchors: several()? },
        "bend" => PathEdit::Bend { after: anchor("after")?, through: point("through")? },
        "line" => PathEdit::Straighten { after: anchor("after")? },
        "smooth" => PathEdit::Smooth { anchors: several()? },
        "corner" => PathEdit::Corner { anchors: several()? },
        "close" => PathEdit::Close { anchor: anchor("anchor")? },
        "break" => PathEdit::Break { at: anchor("anchor")? },
        "join" => PathEdit::Join { a: anchor("a")?, b: anchor("b")? },
        _ => PathEdit::Reverse { anchor: if has("anchor") { Some(anchor("anchor")?) } else { None } },
    })
}

fn edit_path(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let node = input.node("node_id")?;
    doc.node(node).map_err(refused_edit)?;
    let said = input.args.list("edits", "{op, …}")?;
    if said.is_empty() || said.len() > MAX_EDITS {
        return fail(format!("\"edits\" holds {}: give 1 to {MAX_EDITS} of them", said.len()));
    }
    let mut names: Named = Vec::new();
    let mut edits = Vec::with_capacity(said.len());
    for (i, one) in said.iter().enumerate() {
        let n = i + 1;
        let Some(fields) = one.as_map() else { return fail(format!("edit {n} should be an object {{op, …}}")) };
        edits.push(one_edit(fields, n, &names)?);
        let Some(name) = fields.get("as").filter(|d| !d.is_null()) else { continue };
        let name = name.as_str().filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).ok_or_else(|| ToolError(format!("edit {n}: \"as\" should be a name of letters, digits, _ and -")))?;
        if names.iter().any(|(known, _)| known == name) {
            return fail(format!("edit {n}: the name \"{name}\" is already taken in this call"));
        }
        // The id its anchor will have: the one the document gives it
        // when the edits so far are made on a copy.
        let made = doc.anchors_made(node, &edits).map_err(refused_edit)?;
        let &id = made[i].first().ok_or_else(|| ToolError(format!("edit {n} makes no anchor to name \"{name}\": add and break do")))?;
        names.push((name.to_owned(), id));
    }
    Ok(Command::EditPath { node, edits })
}

/// What an edited path is now, and the anchors the edit made.
fn outlined(doc: &Document, applied: &Applied) -> Reply {
    let Some(node) = applied.changed.first().and_then(|id| doc.get(*id)) else { return Reply::text("Nothing changed: the path was like that already.") };
    let (said, mut data) = anchors(doc, node).unwrap_or_default();
    let mut text = format!("Done. {} {} is now:\n{said}", node.id, tag(node));
    if !applied.anchors.is_empty() {
        text += &format!("\nNew anchors: {}.", applied.anchors.iter().map(AnchorId::to_string).collect::<Vec<_>>().join(", "));
    }
    data.insert("node_id", node.id.to_string().into());
    data.insert("made", Doc::List(applied.anchors.iter().map(|a| a.to_string().into()).collect()));
    Reply::text(text).data(Doc::Map(data))
}

fn op_schema() -> Doc {
    common::edit(
        &["node_ids", "op"],
        vec![
            ("node_ids", schema::list(common::node_id("A node"), "The shapes or paths; for union, subtract, intersect and exclude, the one to keep first")),
            ("op", schema::one_of(&["to_path", "reverse", "union", "subtract", "intersect", "exclude", "outline", "simplify"], "What to do")),
            ("tolerance", schema::number(0.0, 1e9, "simplify: how far the outline may move, in the path's own units (default: a five-hundredth of its size). outline: how near the stroke's true edge the outline's curves keep (default: a two-hundredth of the stroke's width; finer makes more anchors)")),
        ],
    )
}

fn op(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let nodes: Vec<NodeId> = input.nodes("node_ids")?;
    for &id in &nodes {
        doc.node(id).map_err(refused_edit)?;
    }
    match input.args.str("op")? {
        "to_path" => Ok(Command::ToPath { nodes }),
        "reverse" => {
            if let Some(node) = nodes.iter().filter_map(|id| doc.get(*id)).find(|n| n.kind != Kind::Path) {
                return fail(format!("{} is a <{}>, which has no direction to turn round: only a path does (to_path makes a shape one)", node.id, node.name));
            }
            Ok(Command::Batch(nodes.into_iter().map(|node| Command::EditPath { node, edits: vec![PathEdit::Reverse { anchor: None }] }).collect()))
        }
        "union" => Ok(Command::Boolean { nodes, how: Combine::Union }),
        "subtract" => Ok(Command::Boolean { nodes, how: Combine::Subtract }),
        "intersect" => Ok(Command::Boolean { nodes, how: Combine::Intersect }),
        "exclude" => Ok(Command::Boolean { nodes, how: Combine::Exclude }),
        "outline" => {
            let tolerance = input.args.opt_f64("tolerance")?;
            if tolerance.is_some_and(|t| !(t > 0.0 && t.is_finite())) {
                return fail("tolerance is how near the stroke's edge the outline keeps: more than nothing");
            }
            Ok(Command::OutlineStroke { nodes, tolerance })
        }
        "simplify" => {
            let said = input.args.opt_f64("tolerance")?;
            if said.is_some_and(|t| !(t > 0.0 && t.is_finite())) {
                return fail("tolerance is how far the outline may move: more than nothing");
            }
            // Each by its own size, unless told.
            Ok(Command::Batch(nodes.iter().map(|id| Command::Simplify { nodes: vec![*id], tolerance: said.unwrap_or_else(|| ink_core::ink_doc::paths::simplify_tolerance(doc, *id)) }).collect()))
        }
        other => fail(format!("op is to_path, reverse, union, subtract, intersect, exclude, outline or simplify, not \"{other}\"")),
    }
}

fn operated(doc: &Document, applied: &Applied) -> Reply {
    if applied.changed.is_empty() {
        return Reply::text("Nothing changed: they were like that already.");
    }
    // Each with how many anchors it has now.
    let said: Vec<String> = applied
        .changed
        .iter()
        .filter_map(|id| doc.get(*id))
        .map(|n| match doc.outline(n.id).map(|o| o.anchors().count()) {
            Some(count) => format!("{} {} ({count} anchor{})", n.id, tag(n), if count == 1 { "" } else { "s" }),
            None => format!("{} {}", n.id, tag(n)),
        })
        .collect();
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(applied.changed.iter().map(|id| id.to_string().into()).collect()));
    if !applied.created.is_empty() {
        // Strokes outlined beside the shapes that keep their fills.
        let new: Vec<String> = applied.created.iter().filter_map(|id| doc.get(*id)).map(|n| format!("{} {}", n.id, tag(n))).collect();
        m.insert("created", Doc::List(applied.created.iter().map(|id| id.to_string().into()).collect()));
        return Reply::text(format!("Done: {}. A shape that had a fill keeps it, and its stroke's outline is a new path beside it: {}.", said.join(", "), new.join(", "))).data(Doc::Map(m));
    }
    if applied.removed.is_empty() {
        // One path: its anchors, which are what's wanted next. Several
        // are a call each.
        return match applied.changed.as_slice() {
            [only] => match doc.get(*only).and_then(|node| anchors(doc, node)) {
                Some((listed, mut data)) => {
                    data.insert("node_ids", Doc::List(vec![only.to_string().into()]));
                    Reply::text(format!("Done: {}, in its own coordinates (path_edit takes these ids):\n{listed}", said.join(", "))).data(Doc::Map(data))
                }
                None => Reply::text(format!("Done: {}.", said.join(", "))).data(Doc::Map(m)),
            },
            _ => Reply::text(format!("Done: {}. node_info lists a path's anchors.", said.join(", "))).data(Doc::Map(m)),
        };
    }
    // Shapes made one: where the one that's left shows now.
    let at = applied.changed.first().and_then(|id| page_bounds(doc).get(id).map(rect)).map_or(String::new(), |at| format!(" at {at}"));
    let gone: Vec<String> = applied.removed.iter().map(NodeId::to_string).collect();
    m.insert("removed", Doc::List(gone.iter().map(|id| id.as_str().into()).collect()));
    Reply::text(format!("Done: {} is the result{at}; {} {} taken into it and deleted.", said.join(", "), gone.join(", "), if gone.len() == 1 { "was" } else { "were" })).data(Doc::Map(m))
}
