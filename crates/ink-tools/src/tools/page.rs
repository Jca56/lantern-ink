//! The drawing as a whole: its page (its size, its viewBox, how finely
//! its numbers are written) and tidying it. Edits like any other: one
//! undo step each.

use ink_core::ink_doc::tidy::{self, Dropped, Extra};
use ink_core::ink_doc::value::{DECIMALS_ATTR, MAX_DECIMALS};
use ink_core::ink_doc::{Document, INK_NS, INK_PREFIX, Precision, Viewport};
use ink_core::{Actor, Applied, Command, NodeId};
use ink_geom::number::parse_list;
use ink_geom::Affine;
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, Tool, ToolError, fail, schema};

use crate::describe::page;
use crate::input::{In, common, refused};
use crate::tools::{Ctx, Entry, Handler, edit, previewed};

/// The most dropped things of one kind a reply names.
const MAX_NAMED: usize = 6;

pub(super) fn tools() -> Vec<Entry> {
    vec![
        set_tool(),
        Entry {
            spec: Tool {
                name: "doc_tidy",
                title: "Tidy",
                description: "Drop what nothing uses, as one undo step. Nothing that shows changes. Always: definitions nothing refers to (gradients, clip paths, filters, masks, patterns, markers, symbols, and anything else kept in a <defs> for others to use; one that only an unused one builds on goes too), groups and <defs> with nothing in them, and namespace declarations nothing uses. Only when named in `also`, since someone wrote these on purpose: \"comments\"; \"ids\" (each id nothing in the drawing refers to: something outside it still might); \"words\" (<title>, <desc>, <metadata>). The reply says what went. doc_export to an .svg writes a tidied copy without touching the drawing.",
                schema: tidy_schema,
                kind: Kind::Destroy,
            },
            handler: Handler::Direct(tidy_up),
        },
    ]
}

fn set_tool() -> Entry {
    edit(
        "doc_set",
        "Set the page",
        "Set a drawing's page: `width` and `height` (the size it asks to be shown at, px), `view_box` [x, y, width, height] (the coordinates that fill that page), and `decimals` (how finely Ink writes this drawing's numbers; 3 unless set). Nothing in the drawing moves unless `content: \"fit\"` is given with a new view_box: then everything is put through the transform that takes the old viewBox to the new one, as node_transform would, so the picture sits in its new coordinates as it sat in the old (a 500-unit drawing becomes a 24-unit one).",
        set_schema,
        Kind::Set,
        set,
        was_set,
    )
}

fn tidy_schema() -> Doc {
    common::edit(&[], vec![("also", schema::list(schema::one_of(&["comments", "ids", "words"], "What else to drop"), "Also drop these: \"comments\", \"ids\" (ids nothing refers to), \"words\" (titles, descriptions, metadata)"))])
}

/// Some of `nodes` by what they are (`<linearGradient id="glow">`), and
/// how many more there are.
fn some(doc: &Document, nodes: &[NodeId]) -> String {
    let named: Vec<String> = nodes.iter().take(MAX_NAMED).filter_map(|id| doc.get(*id)).map(crate::describe::tag).collect();
    match nodes.len().saturating_sub(MAX_NAMED) {
        0 => named.join(", "),
        more => format!("{}, and {more} more", named.join(", ")),
    }
}

/// What tidying dropped, in words. `doc` is the drawing as it was.
fn dropped_in_words(doc: &Document, dropped: &Dropped) -> Vec<String> {
    let count = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    let mut said = Vec::new();
    if !dropped.unused.is_empty() {
        said.push(format!("{} nothing referred to ({})", count(dropped.unused.len(), "definition", "definitions"), some(doc, &dropped.unused)));
    }
    if !dropped.empty.is_empty() {
        said.push(format!("{} with nothing in {} ({})", count(dropped.empty.len(), "element", "elements"), if dropped.empty.len() == 1 { "it" } else { "them" }, some(doc, &dropped.empty)));
    }
    if !dropped.declarations.is_empty() {
        said.push(format!("{} nothing used ({})", count(dropped.declarations.len(), "namespace declaration", "namespace declarations"), dropped.declarations.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    if dropped.comments > 0 {
        said.push(count(dropped.comments, "comment", "comments"));
    }
    if !dropped.ids.is_empty() {
        let ids: Vec<&str> = dropped.ids.iter().take(MAX_NAMED).map(|(_, id)| id.as_str()).collect();
        let more = dropped.ids.len().saturating_sub(MAX_NAMED);
        said.push(format!("{} nothing referred to ({}{})", count(dropped.ids.len(), "id", "ids"), ids.join(", "), if more > 0 { format!(", and {more} more") } else { String::new() }));
    }
    if !dropped.words.is_empty() {
        said.push(format!("{} ({})", count(dropped.words.len(), "title, description or metadata", "titles, descriptions and metadata"), some(doc, &dropped.words)));
    }
    said
}

fn tidy_up(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let id = input.doc()?;
    let mut also = Vec::new();
    for named in input.args.opt_list("also", "names like \"comments\"")?.unwrap_or(&[]) {
        match named.as_str().and_then(Extra::named) {
            Some(extra) if !also.contains(&extra) => also.push(extra),
            Some(_) => {}
            None => return fail("\"also\" takes \"comments\", \"ids\" and \"words\""),
        }
    }
    // What will go is said from the drawing as it is: afterwards it's
    // not there to be named.
    let doc = ctx.core.doc(id).map_err(refused)?;
    let dropped = tidy::plan(doc, &also);
    if dropped.is_nothing() {
        let unasked = if also.len() < 3 { " (comments, ids nothing refers to, and titles stay unless `also` names them)" } else { "" };
        return Ok(Reply::text(format!("Nothing to tidy: everything here is in use{unasked}.")));
    }
    let said = dropped_in_words(doc, &dropped);
    let before = doc.to_svg().len();
    ctx.core.apply(id, &Command::Tidy { also }, Actor::Claude, "doc_tidy").map_err(refused)?;
    let after = ctx.core.doc(id).map_err(refused)?.to_svg().len();
    let mut m = Map::new();
    m.insert("definitions", Doc::Int(dropped.unused.len() as i64));
    m.insert("empty", Doc::Int(dropped.empty.len() as i64));
    m.insert("declarations", Doc::Int(dropped.declarations.len() as i64));
    m.insert("comments", Doc::Int(dropped.comments as i64));
    m.insert("ids", Doc::Int(dropped.ids.len() as i64));
    m.insert("words", Doc::Int(dropped.words.len() as i64));
    let reply = Reply::text(format!("Tidied {id}: dropped {}. It draws as it did; its markup went from {before} to {after} bytes.", said.join("; "))).data(Doc::Map(m));
    previewed(ctx, id, reply, input.args.opt_bool("preview")? == Some(true))
}

fn set_schema() -> Doc {
    common::edit(
        &[],
        vec![
            ("width", schema::number(0.001, 100_000.0, "The page's width, px")),
            ("height", schema::number(0.001, 100_000.0, "The page's height, px")),
            ("view_box", schema::list(schema::number(-1e9, 1e9, "A number"), "[x, y, width, height]: the coordinates that fill the page")),
            ("content", schema::one_of(&["keep", "fit"], "With view_box: keep everything where its numbers are (default), or fit it to the new coordinates")),
            ("decimals", schema::integer(0, MAX_DECIMALS as i64, "How many decimals this drawing's numbers are written with (default 3)")),
        ],
    )
}

fn set(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let (a, root) = (&input.args, doc.root());
    let node = doc.node(root).map_err(crate::input::refused_edit)?;
    let attr = |name: &str, value: String| Command::SetAttr { node: root, name: name.to_owned(), value: Some(value) };
    let mut commands = Vec::new();
    // First, so that what follows is written as finely as it says.
    let mut precision = Precision::of(doc);
    if let Some(decimals) = a.opt_int("decimals", 0, MAX_DECIMALS as i64)? {
        match doc.namespace(root, Some(INK_PREFIX)) {
            Some(INK_NS) => {}
            None => commands.push(attr(&format!("xmlns:{INK_PREFIX}"), INK_NS.to_owned())),
            Some(other) => return fail(format!("this drawing uses the prefix \"{INK_PREFIX}:\" for something else ({other}), so Ink's own attributes have nowhere to go")),
        }
        commands.push(attr(&format!("{INK_PREFIX}:{DECIMALS_ATTR}"), decimals.to_string()));
        precision.decimals = decimals as usize;
    }
    let view_box = match a.opt_list("view_box", "numbers")? {
        None => None,
        Some(items) => match items.iter().map(|d| d.as_f64().filter(|v| v.is_finite())).collect::<Option<Vec<f64>>>().as_deref() {
            Some(&[x, y, w, h]) if w > 0.0 && h > 0.0 => Some((x, y, w, h)),
            _ => return fail("\"view_box\" should be [x, y, width, height], its width and height more than nothing"),
        },
    };
    match (a.opt_str("content")?, view_box) {
        (None | Some("keep"), _) => {}
        (Some("fit"), Some((x, y, w, h))) => {
            // The coordinates that fill the page now: its viewBox, or
            // without one the page itself.
            let old = node.attr("viewBox").and_then(parse_list).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
            let (ox, oy, ow, oh) = old.map_or_else(|| (0.0, 0.0, Viewport::of(node).size.x, Viewport::of(node).size.y), |v| (v[0], v[1], v[2], v[3]));
            let by = Affine::translate(-ox, -oy).then(&Affine::scale(w / ow, h / oh)).then(&Affine::translate(x, y));
            commands.push(Command::Transform { nodes: vec![root], by });
        }
        (Some("fit"), None) => return fail("content: \"fit\" goes with a view_box to fit the content to"),
        (Some(other), _) => return fail(format!("content is keep or fit, not \"{other}\"")),
    }
    for name in ["width", "height"] {
        if let Some(v) = a.opt_f64(name)? {
            if v <= 0.0 {
                return fail(format!("\"{name}\" must be more than nothing"));
            }
            commands.push(attr(name, precision.number(v)));
        }
    }
    if let Some((x, y, w, h)) = view_box {
        commands.push(attr("viewBox", [x, y, w, h].map(|v| precision.number(v)).join(" ")));
    }
    if commands.is_empty() {
        return fail("say what to set: width, height, view_box or decimals");
    }
    Ok(Command::Batch(commands))
}

fn was_set(doc: &Document, applied: &Applied) -> Reply {
    if applied.changed.is_empty() {
        return Reply::text("Nothing changed: the page was like that already.");
    }
    // The root aside, what changed is what was fitted.
    let fitted = applied.changed.iter().filter(|&&id| id != doc.root()).count();
    let refit = match fitted {
        0 => String::new(),
        1 => " 1 node was fitted to the new coordinates.".to_owned(),
        n => format!(" {n} nodes were fitted to the new coordinates."),
    };
    Reply::text(format!("Set. Page: {}; numbers to {} decimals.{refit}", page(doc), Precision::of(doc).decimals))
}
