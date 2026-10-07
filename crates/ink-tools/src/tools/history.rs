//! History: undo, redo, and `batch`, many edits as one step.

use ink_core::{Actor, Command, NodeId};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Args, Kind, Reply, Tool, ToolError, fail, schema};

use crate::describe::actor;
use crate::input::{In, Names, common, refused, refused_edit};
use crate::tools::{Ctx, Entry, Handler, previewed};

/// The tools a batch can hold: every edit.
pub(crate) const EDITS: [&str; 21] = ["node_add", "node_add_svg", "node_set", "node_move", "node_delete", "node_transform", "node_align", "node_duplicate", "node_group", "node_ungroup", "node_style", "gradient_add", "gradient_set", "clip_set", "filter_set", "path_set", "path_edit", "path_op", "text_add", "text_set", "doc_set"];
/// The most steps one batch holds.
const MAX_BATCH: usize = 200;

pub(super) fn tools() -> Vec<Entry> {
    let direct = |name, title, description, schema, kind, f| Entry { spec: Tool { name, title, description, schema, kind }, handler: Handler::Direct(f) };
    vec![
        direct("history_undo", "Undo", "Undo a drawing's last steps (default 1), whoever made them. A batch is one step. history_redo brings them back until the next edit.", steps_schema as fn() -> Doc, Kind::Set, undo as super::Direct),
        direct("history_redo", "Redo", "Redo steps history_undo undid (default 1). A new edit after an undo clears what could be redone.", steps_schema, Kind::Set, redo),
        direct(
            "batch",
            "Many edits as one",
            "Run many edits on one drawing as ONE undo step, all or nothing: if any step is refused, none happen, and the reply says which step and why. Each step is {tool, args, as}: tool is any edit (node_add, node_add_svg, node_set, node_move, node_delete, node_transform, node_align, node_duplicate, node_group, node_ungroup, node_style, gradient_add, gradient_set, clip_set, filter_set, path_set, path_edit, path_op, doc_set); args are that tool's, without doc_id; `as` names the node the step makes (what it adds, a copy, a group), so later steps can refer to it as \"@name\" before its id exists (e.g. as \"face\", then into: \"@face\"). Attaches a picture of the result unless preview: false.",
            batch_schema,
            Kind::Set,
            batch,
        ),
    ]
}

fn steps_schema() -> Doc {
    schema::object(&["doc_id"], vec![("doc_id", common::doc_id()), ("steps", schema::integer(1, 100, "How many steps (default 1)"))])
}

/// Undo or redo up to `steps` steps, and say which.
fn travel(ctx: &mut Ctx, input: &In, back: bool) -> Result<Reply, ToolError> {
    let id = input.doc()?;
    let steps = input.args.opt_int("steps", 1, 100)?.unwrap_or(1);
    ctx.core.doc(id).map_err(refused)?;
    let mut done = Vec::new();
    for _ in 0..steps {
        match if back { ctx.core.undo(id) } else { ctx.core.redo(id) } {
            Ok(step) => done.push(format!("\"{}\" ({})", step.label, actor(step.actor))),
            Err(_) => break,
        }
    }
    let (verb, did) = if back { ("undo", "Undid") } else { ("redo", "Redid") };
    if done.is_empty() {
        return fail(format!("there's nothing to {verb} in {id}"));
    }
    let history = ctx.core.history(id).map_err(refused)?;
    let (undo, redo) = (history.undoable().count(), history.redoable().count());
    let mut m = Map::new();
    m.insert("steps", Doc::Int(done.len() as i64));
    m.insert("undo", Doc::Int(undo as i64));
    m.insert("redo", Doc::Int(redo as i64));
    Ok(Reply::text(format!("{did} {} step{}: {}. Now {undo} can be undone and {redo} redone.", done.len(), if done.len() == 1 { "" } else { "s" }, done.join(", "))).data(Doc::Map(m)))
}

fn undo(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    travel(ctx, input, true)
}

fn redo(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    travel(ctx, input, false)
}

fn batch_schema() -> Doc {
    let step = schema::object(
        &["tool"],
        vec![
            ("tool", schema::one_of(&EDITS, "The edit")),
            ("args", common::any_object("The tool's arguments, without doc_id")),
            ("as", schema::pattern("^[A-Za-z0-9_-]+$", "Name the node this step adds, for later steps' \"@name\"")),
        ],
    );
    schema::object(&["doc_id", "steps"], vec![("doc_id", common::doc_id()), ("steps", schema::list(step, "The edits, in order")), ("preview", schema::boolean("Attach a picture of the result", true))])
}

fn batch(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let id = input.doc()?;
    let steps = input.args.list("steps", "{tool, args, as}")?;
    if steps.is_empty() || steps.len() > MAX_BATCH {
        return fail(format!("a batch holds 1 to {MAX_BATCH} steps, not {}", steps.len()));
    }
    // A dry run on a copy: resolves "@names" and finds the refused step
    // before anything changes. Ids come out the same when it's applied
    // for real, since the same Commands meet the same document.
    let mut scratch = ctx.core.doc(id).map_err(refused)?.clone();
    let mut names = Names::new();
    let mut named: Vec<(String, NodeId)> = Vec::new();
    let mut commands = Vec::with_capacity(steps.len());
    for (i, step) in steps.iter().enumerate() {
        let n = i + 1;
        let Some(fields) = step.as_map() else { return fail(format!("step {n} should be an object {{tool, args, as}}")) };
        if let Some(k) = fields.keys().find(|k| !["tool", "args", "as"].contains(k)) {
            return fail(format!("step {n}: \"{k}\" isn't a step field; a step is {{tool, args, as}}"));
        }
        let name = step.get("tool").and_then(Doc::as_str).ok_or_else(|| ToolError(format!("step {n} needs a \"tool\"")))?;
        let entry = ctx.entries.iter().find(|e| e.spec.name == name).ok_or_else(|| ToolError(format!("step {n}: there's no tool \"{name}\"")))?;
        let Handler::Edit { build, .. } = &entry.handler else {
            return fail(format!("step {n}: {name} can't go in a batch; only edits can ({})", EDITS.join(", ")));
        };
        let mut args = match step.get("args") {
            None | Some(Doc::Null) => Map::new(),
            Some(Doc::Map(m)) => m.clone(),
            Some(_) => return fail(format!("step {n} ({name}): \"args\" should be an object")),
        };
        if args.contains("preview") {
            return fail(format!("step {n} ({name}): preview goes on the batch, not on a step"));
        }
        match args.get("doc_id").map(Doc::as_str) {
            None => args.insert("doc_id", id.to_string().into()),
            Some(Some(d)) if d == id.to_string() => {}
            Some(_) => return fail(format!("step {n} ({name}): a batch works on one drawing, {id}")),
        }
        let args = Doc::Map(args);
        let step_err = |e: ToolError| ToolError(format!("step {n} ({name}): {}", e.0));
        let checked = Args::new(&args).and_then(|a| a.check(&(entry.spec.schema)()).map(|()| a)).map_err(step_err)?;
        let command = build(&scratch, &In { args: checked, names: Some(&names), env: input.env }).map_err(step_err)?;
        let out = scratch.apply(&command).map_err(|e| step_err(refused_edit(e)))?;
        if let Some(label) = step.get("as") {
            let label = label.as_str().filter(|l| !l.is_empty() && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
            let label = label.ok_or_else(|| ToolError(format!("step {n}: \"as\" should be a name of letters, digits, _ and -")))?;
            let &node = out.created.first().ok_or_else(|| ToolError(format!("step {n} ({name}) adds no node to name \"{label}\"")))?;
            if names.insert(label.to_owned(), node).is_some() {
                return fail(format!("step {n}: the name \"{label}\" is already taken in this batch"));
            }
            named.push((label.to_owned(), node));
        }
        commands.push(command);
    }
    let applied = ctx.core.apply(id, &Command::Batch(commands), Actor::Claude, "batch").map_err(refused)?;
    debug_assert!(ctx.core.doc(id).is_ok_and(|d| d.to_svg() == scratch.to_svg()), "the batch ran differently from its dry run");
    // What a later step took out again (a shape combined into another)
    // isn't there to be told of.
    let (made, unmade): (Vec<NodeId>, Vec<NodeId>) = applied.created.iter().partition(|node| scratch.get(**node).is_some());
    named.retain(|(_, node)| made.contains(node));
    let mut text = format!("Ran {} steps on {id} as one undo step (history_undo undoes all of them).", steps.len());
    if !made.is_empty() {
        text += &format!(" New nodes: {}.", made.iter().map(NodeId::to_string).collect::<Vec<_>>().join(", "));
    }
    if !named.is_empty() {
        text += &format!(" Named: {}.", named.iter().map(|(label, node)| format!("@{label} = {node}")).collect::<Vec<_>>().join(", "));
    }
    if !unmade.is_empty() {
        text += &format!(" Made and taken out again on the way: {}.", unmade.iter().map(NodeId::to_string).collect::<Vec<_>>().join(", "));
    }
    let mut m = Map::new();
    m.insert("node_ids", Doc::List(made.iter().map(|n| n.to_string().into()).collect()));
    let mut by_name = Map::new();
    for (label, node) in &named {
        by_name.insert(label.as_str(), node.to_string().into());
    }
    m.insert("names", Doc::Map(by_name));
    previewed(ctx, id, Reply::text(text).data(Doc::Map(m)), input.args.opt_bool("preview")? != Some(false))
}
