//! The page itself: its size, its viewBox, and how finely the drawing's
//! numbers are written. An edit like any other: one undo step.

use ink_core::ink_doc::value::{DECIMALS_ATTR, MAX_DECIMALS};
use ink_core::ink_doc::{Document, INK_NS, INK_PREFIX, Precision, Viewport};
use ink_core::{Applied, Command};
use ink_geom::number::parse_list;
use ink_geom::Affine;
use lntrn_data::Doc;
use lntrn_mcp::{Kind, Reply, ToolError, fail, schema};

use crate::describe::page;
use crate::input::{In, common};
use crate::tools::{Entry, edit};

pub(super) fn tools() -> Vec<Entry> {
    vec![edit(
        "doc_set",
        "Set the page",
        "Set a drawing's page: `width` and `height` (the size it asks to be shown at, px), `view_box` [x, y, width, height] (the coordinates that fill that page), and `decimals` (how finely Ink writes this drawing's numbers; 3 unless set). Nothing in the drawing moves unless `content: \"fit\"` is given with a new view_box: then everything is put through the transform that takes the old viewBox to the new one, as node_transform would, so the picture sits in its new coordinates as it sat in the old (a 500-unit drawing becomes a 24-unit one).",
        set_schema,
        Kind::Set,
        set,
        was_set,
    )]
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
