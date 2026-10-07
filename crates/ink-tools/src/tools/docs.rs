//! Documents: making, opening, looking at, saving, exporting, closing.

use std::path::Path;

use ink_core::ink_doc::Viewport;
use ink_core::{View, write_atomic};
use lntrn_data::{Doc, Map};
use lntrn_image::Compression;
use lntrn_mcp::{Kind, Reply, Tool, ToolError, fail, schema};

use crate::describe::{self, n};
use crate::input::{In, common, refused};
use crate::preview::{self, Background, MAX_EDGE, Options, flatten};
use crate::tools::{Ctx, Entry, Handler};

fn direct(name: &'static str, title: &'static str, description: &'static str, schema: fn() -> Doc, kind: Kind, f: super::Direct) -> Entry {
    Entry { spec: Tool { name, title, description, schema, kind }, handler: Handler::Direct(f) }
}

pub(super) fn tools() -> Vec<Entry> {
    vec![
        direct("doc_new", "New drawing", "Make an empty drawing: an SVG whose page and viewBox are width × height (default 24 × 24, an icon's grid). It lives in this server until doc_save gives it a file. Returns its doc_id and its root node's id.", new_schema, Kind::Add, new),
        direct("doc_open", "Open drawing", "Open an .svg file. Anything in it Ink doesn't understand is kept and written back as it was; another editor's private marks (Boxy SVG's) are taken out, and the file itself changes only when you doc_save. A file has one drawing at a time: one that's open already is answered with the drawing it is. Returns the doc_id; doc_info lists its nodes.", open_schema, Kind::Add, open),
        direct("doc_list", "List drawings", "The drawings open in this server: id, file, page, how many nodes, and whether they have unsaved changes.", list_schema, Kind::Read, list),
        direct(
            "doc_info",
            "Describe drawing",
            "A drawing's page, file and undo/redo, and its nodes front to back (the first listed is on top), children indented under their parent: each node's id, element, the paint it gives itself, and the box where it shows in the drawing's coordinates. Definitions (what a <defs>, a gradient or a filter holds) are in the file's order, each with its attributes.",
            id_schema,
            Kind::Read,
            info,
        ),
        direct(
            "doc_source",
            "Read markup",
            "The SVG markup itself, exactly as it would be saved: the whole drawing, or one node and what's in it (node_id). Long markup is cut at max_chars (default 20000).",
            source_schema,
            Kind::Read,
            source,
        ),
        direct(
            "doc_preview",
            "Look at drawing",
            "Look at a drawing: a picture of its page (or of `region`, a part of it in the drawing's coordinates) with its longer side max_edge px. A vector is sharp at any size, so ask small for a glance and large for detail. renderer \"lantern\" shows it as Lantern's apps will: drawn by lntrn-svg at 16, 24, 32, 48 and 64 px, each enlarged pixel for pixel, in a strip of its own size (max_edge and region don't apply). Every look is also written to disk, to a file of its own. Ask at milestones rather than after every call.",
            preview_schema,
            Kind::Read,
            look,
        ),
        direct("doc_save", "Save drawing", "Save a drawing as an .svg file: to `path`, or to its own file again. Replacing a file that isn't its own needs overwrite: true, and another open drawing's file can't be taken. The save is atomic: a crash never leaves half a file.", save_schema, Kind::Set, save),
        direct(
            "doc_export",
            "Export picture",
            "Write a picture of a drawing: png (keeps transparency), jpeg (flattened onto white unless background says black) or webp, the format taken from the file name unless `format` says. Its size is the page's own unless `size` (the longer side, px) or `scale` says. Replacing an existing file needs overwrite: true.",
            export_schema,
            Kind::Set,
            export,
        ),
        direct("doc_close", "Close drawing", "Close a drawing, freeing its memory. One with unsaved changes is refused unless discard: true.", close_schema, Kind::Destroy, close),
    ]
}

fn new_schema() -> Doc {
    schema::object(&[], vec![("width", schema::number(0.001, 100_000.0, "The page's width, in the drawing's own units (default 24)")), ("height", schema::number(0.001, 100_000.0, "Its height (default: the width)"))])
}

fn new(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let width = input.args.opt_f64("width")?.unwrap_or(24.0);
    let height = input.args.opt_f64("height")?.unwrap_or(width);
    if !(width >= 0.001 && height >= 0.001 && width <= 100_000.0 && height <= 100_000.0) {
        return fail("width and height are from 0.001 to 100000");
    }
    let id = ctx.core.new_doc(width, height);
    let root = ctx.core.doc(id).map_err(refused)?.root();
    let mut m = Map::new();
    m.insert("doc_id", id.to_string().into());
    m.insert("root", root.to_string().into());
    Ok(Reply::text(format!("Made {id}: an empty drawing {} × {} (viewBox 0 0 {} {}). Its root is {root}; node_add and node_add_svg put things in it. It has no file until doc_save.", n(width), n(height), n(width), n(height))).data(Doc::Map(m)))
}

fn open_schema() -> Doc {
    schema::object(&["path"], vec![("path", common::path("The .svg file"))])
}

fn open(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let path = input.env.resolve(input.args.str("path")?);
    // A file has one drawing at a time: two would save over each other.
    if let Some(id) = ctx.core.doc_at(&path) {
        let doc = ctx.core.doc(id).map_err(refused)?;
        let unsaved = if ctx.core.is_modified(id).map_err(refused)? { ", with changes that aren't saved" } else { "" };
        let text = format!("{} is already open as {id}{unsaved}: work on that one (page {}, {} nodes, root {}). To read the file afresh, doc_close {id} first.", path.display(), describe::page(doc), doc.len(), doc.root());
        let mut m = Map::new();
        m.insert("doc_id", id.to_string().into());
        m.insert("root", doc.root().to_string().into());
        m.insert("node_count", Doc::Int(doc.len() as i64));
        m.insert("already_open", true.into());
        return Ok(Reply::text(text).data(Doc::Map(m)));
    }
    let opened = ctx.core.open_file(&path).map_err(refused)?;
    let doc = ctx.core.doc(opened.doc).map_err(refused)?;
    let mut text = format!("Opened {} as {}: page {}, {} nodes. Its root is {}; doc_info lists the rest.", path.display(), opened.doc, describe::page(doc), doc.len(), doc.root());
    if !opened.adopted.is_nothing() {
        text += &format!(
            " {}'s own marks were taken out ({} elements, {} attributes) and Ink's namespace put where theirs was; the file itself changes only when you doc_save.",
            opened.adopted.editors.join(" and "),
            opened.adopted.elements,
            opened.adopted.attributes
        );
    }
    let mut m = Map::new();
    m.insert("doc_id", opened.doc.to_string().into());
    m.insert("root", doc.root().to_string().into());
    m.insert("node_count", Doc::Int(doc.len() as i64));
    Ok(Reply::text(text).data(Doc::Map(m)))
}

fn list_schema() -> Doc {
    schema::object(&[], vec![])
}

fn list(ctx: &mut Ctx, _: &In) -> Result<Reply, ToolError> {
    let ids: Vec<_> = ctx.core.docs().collect();
    let lines: Vec<String> = ids.iter().map(|&id| describe::summary(ctx.core, id)).collect::<Result<_, _>>()?;
    let mut m = Map::new();
    m.insert("documents", Doc::List(ids.iter().map(|id| id.to_string().into()).collect()));
    let text = if ids.is_empty() { "No drawings are open. doc_new makes one; doc_open opens an .svg file.".to_owned() } else { lines.join("\n") };
    Ok(Reply::text(text).data(Doc::Map(m)))
}

fn id_schema() -> Doc {
    schema::object(&["doc_id"], vec![("doc_id", common::doc_id())])
}

fn info(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let (text, data) = describe::info(ctx.core, input.doc()?)?;
    Ok(Reply::text(text).data(data))
}

fn source_schema() -> Doc {
    schema::object(&["doc_id"], vec![("doc_id", common::doc_id()), ("node_id", common::node_id("Only this node and what's in it")), ("max_chars", schema::integer(200, 200_000, "The most characters to return (default 20000)"))])
}

fn source(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let id = input.doc()?;
    let doc = ctx.core.doc(id).map_err(refused)?;
    let markup = match input.opt_node("node_id")? {
        Some(node) => doc.markup(node).map_err(|e| refused(e.into()))?,
        None => doc.to_svg(),
    };
    let max = input.args.opt_int("max_chars", 200, 200_000)?.unwrap_or(20_000) as usize;
    Ok(Reply::text(match markup.char_indices().nth(max) {
        Some((cut, _)) => format!("{}\n… cut at {max} of {} characters (raise max_chars, or ask for one node with node_id)", &markup[..cut], markup.chars().count()),
        None => markup,
    }))
}

fn preview_schema() -> Doc {
    let region = schema::object(
        &["x", "y", "width", "height"],
        vec![("x", schema::number(-1e9, 1e9, "Left, in the drawing's coordinates")), ("y", schema::number(-1e9, 1e9, "Top")), ("width", schema::number(0.0, 1e9, "Width")), ("height", schema::number(0.0, 1e9, "Height"))],
    );
    schema::object(
        &["doc_id"],
        vec![
            ("doc_id", common::doc_id()),
            ("max_edge", schema::integer(16, MAX_EDGE as i64, "The picture's longer side in px (default 512; the lantern strip has its own size)")),
            ("region", region),
            ("background", schema::one_of(&["checker", "white", "black", "none"], "Under transparent areas (default checker; none needs png)")),
            ("renderer", schema::one_of(&["ink", "lantern"], "ink (default): Ink's own renderer. lantern: as lntrn-svg draws it at icon sizes")),
            ("format", schema::one_of(&["auto", "jpeg", "png"], "auto (default): PNG when it fits max_bytes, else JPEG; or always one")),
            ("quality", schema::integer(1, 100, "JPEG quality to start from (default 82)")),
            ("max_bytes", schema::integer(4096, 8 << 20, "Byte budget for the picture (default 262144). Bytes cost no tokens; the size in px does")),
        ],
    )
}

fn look(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let (image, note) = preview::preview(ctx.core, ctx.env, input.doc()?, &Options::from_args(input)?)?;
    Ok(Reply::text(note).image(image))
}

fn save_schema() -> Doc {
    schema::object(&["doc_id"], vec![("doc_id", common::doc_id()), ("path", common::path("Where, ending in .svg (default: its own file)")), ("overwrite", schema::boolean("Replace an existing file that isn't its own", false))])
}

fn save(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let id = input.doc()?;
    let own = ctx.core.path(id).map_err(refused)?.map(Path::to_owned);
    let path = match input.args.opt_str("path")? {
        Some(path) => input.env.resolve(path),
        None => own.clone().ok_or_else(|| ToolError(format!("{id} has no file yet: give doc_save a path ending in .svg")))?,
    };
    if !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("svg")) {
        return fail(format!("{} isn't an .svg name: doc_save writes the SVG itself (doc_export writes a png, jpeg or webp picture of it)", path.display()));
    }
    if let Some(other) = ctx.core.doc_at(&path).filter(|&other| other != id) {
        return fail(format!("{} is {other}'s file, and {other} is open: save {id} under another name, or doc_close {other} first", path.display()));
    }
    if own.as_deref() != Some(path.as_path()) && path.exists() && input.args.opt_bool("overwrite")? != Some(true) {
        return fail(format!("{} is already there: pass overwrite: true to replace it", path.display()));
    }
    let saved = ctx.core.save(id, Some(&path)).map_err(refused)?;
    let bytes = std::fs::metadata(&saved).map_or(0, |m| m.len());
    let mut m = Map::new();
    m.insert("path", saved.display().to_string().into());
    Ok(Reply::text(format!("Saved {id} to {} ({} bytes).", saved.display(), bytes)).data(Doc::Map(m)))
}

fn export_schema() -> Doc {
    schema::object(
        &["doc_id", "path"],
        vec![
            ("doc_id", common::doc_id()),
            ("path", common::path("Where, e.g. ending in .png")),
            ("format", schema::one_of(&["png", "jpeg", "webp"], "Overrides the file name's")),
            ("size", schema::integer(1, 16384, "The picture's longer side in px (default: the page's own size)")),
            ("scale", schema::number(0.001, 1000.0, "Instead of size: px per px of the page")),
            ("background", schema::one_of(&["none", "white", "black"], "Under transparent areas (default none; jpeg can't be none and takes white)")),
            ("quality", schema::integer(1, 100, "JPEG quality (default 90); WebP: below 100 lossy, 100 lossless (default)")),
            ("overwrite", schema::boolean("Replace an existing file", false)),
        ],
    )
}

fn export(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let (a, id) = (&input.args, input.doc()?);
    let path = input.env.resolve(a.str("path")?);
    let by_name = path.extension().map(|x| x.to_string_lossy().to_ascii_lowercase());
    let format = match a.opt_str("format")?.or(by_name.as_deref()) {
        Some("png") => "png",
        Some("jpeg" | "jpg") => "jpeg",
        Some("webp") => "webp",
        Some("svg") => return fail("doc_export writes a picture; doc_save writes the SVG itself"),
        _ => return fail(format!("{} doesn't say what to write: name it .png, .jpg or .webp, or give format", path.display())),
    };
    if path.exists() && a.opt_bool("overwrite")? != Some(true) {
        return fail(format!("{} is already there: pass overwrite: true to replace it", path.display()));
    }
    let viewport: Viewport = ctx.core.viewport(id).map_err(refused)?;
    let scale = match (a.opt_int("size", 1, 16384)?, a.opt_f64("scale")?) {
        (Some(_), Some(_)) => return fail("give size or scale, not both"),
        (Some(size), None) => size as f64 / viewport.size.x.max(viewport.size.y),
        (None, Some(scale)) if scale > 0.0 => scale,
        (None, Some(_)) => return fail("scale must be more than nothing"),
        (None, None) => 1.0,
    };
    let background = match (a.opt_str("background")?, format) {
        (None | Some("none"), "jpeg") | (Some("white"), _) => Background::White,
        (Some("black"), _) => Background::Black,
        (None | Some("none"), _) => Background::None,
        (Some(other), _) => return fail(format!("background is none, white or black, not \"{other}\"")),
    };
    let image = flatten(&ctx.core.render(id, &View::page(&viewport, scale)).map_err(refused)?, background);
    let quality = a.opt_int("quality", 1, 100)?;
    let bytes = match format {
        "png" => lntrn_image::encode_png_with(&image, Compression::Best),
        "jpeg" => lntrn_image::encode_jpeg(&image, quality.unwrap_or(90) as u8),
        _ => match quality.filter(|q| *q < 100) {
            Some(q) => lntrn_image::encode_webp_lossy(&image, q as u8),
            None => lntrn_image::encode_webp(&image),
        },
    };
    write_atomic(&path, &bytes).map_err(refused)?;
    let mut m = Map::new();
    m.insert("path", path.display().to_string().into());
    m.insert("width", Doc::Int(image.width as i64));
    m.insert("height", Doc::Int(image.height as i64));
    Ok(Reply::text(format!("Exported {id} to {}: {}×{} {format}, {} KB.", path.display(), image.width, image.height, bytes.len().div_ceil(1024))).data(Doc::Map(m)))
}

fn close_schema() -> Doc {
    schema::object(&["doc_id"], vec![("doc_id", common::doc_id()), ("discard", schema::boolean("Close even with unsaved changes, losing them", false))])
}

fn close(ctx: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let id = input.doc()?;
    if ctx.core.is_modified(id).map_err(refused)? && input.args.opt_bool("discard")? != Some(true) {
        return fail(format!("{id} has unsaved changes: doc_save it first, or pass discard: true to lose them"));
    }
    ctx.core.close(id).map_err(refused)?;
    Ok(Reply::text(format!("Closed {id}.")))
}
