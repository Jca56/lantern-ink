//! Text: `text_add` makes a `<text>`, `text_set` changes what one says
//! and how it's lettered, `font_list` says which fonts this machine
//! has. Lines and mixed lettering are `<tspan>`s, which
//! `Command::SetText` writes.

use ink_core::ink_doc::geometry::page_bounds;
use ink_core::ink_doc::lettering::{self, LEADING, Span};
use ink_core::ink_doc::{Document, Kind as NodeKind, Node, Precision, fonts, text};
use ink_core::{Applied, Command, NodeId};
use lntrn_data::{Doc, Map};
use lntrn_mcp::{Kind, Reply, Tool, ToolError, fail, schema};

use crate::describe::{n, rect, tag, words};
use crate::input::{In, common, refused_edit};
use crate::tools::{Ctx, Entry, Handler, edit};

/// The most families one listing names.
const MAX_FAMILIES: usize = 80;

pub(super) fn tools() -> Vec<Entry> {
    vec![
        edit(
            "text_add",
            "Add text",
            "Add a <text>: `text` in one style, or `runs` [{text, fill, bold, italic, font, size}] to mix them; \"\\n\" breaks a line. `x`, `y` is where its first line's baseline starts (letters stand on y; a descender hangs below). `font` is a family, or several to try in order (\"Inter, sans-serif\"): sans-serif, monospace and serif are Lantern's own, and a family that isn't installed here gives way to the next (the reply says which font it ended up in; font_list says what's installed). `size` is in the drawing's units (default 16). `anchor` says which end of each line x is: start (default), middle or end. Lines are `line_height` ems apart (default 1.2). It goes on top of the drawing unless above, below, into or at says. It is drawn as outlines in this machine's fonts, and its box is around its glyphs; Lantern's apps don't draw <text> (doc_preview with renderer \"lantern\" shows what they'll show).",
            add_schema,
            Kind::Add,
            add,
            said,
        ),
        edit(
            "text_set",
            "Set text",
            "Change a <text>: what it says (`text`, or `runs` [{text, fill, bold, italic, font, size}] to mix styles; \"\\n\" breaks a line; everything that was in it is replaced), and how all of it is lettered and painted (font, size, bold, italic, fill, anchor, letter_spacing). Anything not given stays. `line_height` (ems, default 1.2) goes with text or runs: it's written into the lines. `font` is a family, or several to try in order; one that isn't installed here gives way to the next, and the reply says which font it ended up in. To move a text use node_transform or node_align: a move goes into its x and y, and its lines'.",
            set_schema,
            Kind::Set,
            set,
            said,
        ),
        Entry {
            spec: Tool {
                name: "font_list",
                title: "List fonts",
                description: "The font families installed on this machine, in alphabetical order (`query`: only those with it in their name), and what sans-serif, monospace and serif stand for here. A family not in the list can still be named in a drawing: text asking for it is set in the next family it names, or in the sans.",
                schema: fonts_schema,
                kind: Kind::Read,
            },
            handler: Handler::Direct(font_list),
        },
    ]
}

/// What both tools take about lettering.
fn lettering_props() -> Vec<(&'static str, Doc)> {
    let run = schema::object(
        &["text"],
        vec![
            ("text", schema::string("Its words; \"\\n\" breaks a line")),
            ("fill", schema::string("Its paint: \"#rrggbb\", \"none\", \"url(#id)\"")),
            ("bold", schema::boolean("Bold", false)),
            ("italic", schema::boolean("Italic", false)),
            ("font", schema::string("Its family, or several to try in order")),
            ("size", schema::number(0.0, 100000.0, "Its size, in the drawing's units")),
        ],
    );
    vec![
        ("text", schema::string("The words, in one style; \"\\n\" breaks a line")),
        ("runs", schema::list(run, "Stretches with styles of their own, in order")),
        ("font", schema::string("The family, or several to try in order: \"Inter, sans-serif\"")),
        ("size", schema::number(0.0, 100000.0, "Font size, in the drawing's units")),
        ("bold", schema::boolean("Bold", false)),
        ("italic", schema::boolean("Italic", false)),
        ("fill", schema::string("Its paint: \"#rrggbb\", \"none\", \"url(#id)\" (default black)")),
        ("anchor", schema::one_of(&["start", "middle", "end"], "Which end of each line its position is")),
        ("letter_spacing", schema::number(-100000.0, 100000.0, "Space added after each character, in the drawing's units")),
        ("line_height", schema::number(0.01, 100.0, "How far apart lines are, in ems (default 1.2)")),
    ]
}

fn add_schema() -> Doc {
    let mut props = vec![("x", schema::number(-1e9, 1e9, "Where its first line starts: the left end of its baseline (its middle or right end, with anchor)")), ("y", schema::number(-1e9, 1e9, "Its first line's baseline"))];
    props.extend(lettering_props());
    props.extend(common::placement());
    common::edit(&["x", "y"], props)
}

fn set_schema() -> Doc {
    let mut props = vec![("node_id", common::node_id("The <text>"))];
    props.extend(lettering_props());
    common::edit(&["node_id"], props)
}

fn fonts_schema() -> Doc {
    schema::object(&[], vec![("query", schema::string("Only families with this in their name, whatever its case"))])
}

/// The lines `text` or `runs` says, if either was given. A stretch's
/// own style is the properties it sets for itself.
fn lines(input: &In, decimals: usize) -> Result<Option<Vec<Vec<Span>>>, ToolError> {
    let stretches: Vec<(String, Vec<(String, String)>)> = match (input.args.opt_str("text")?, input.args.opt_list("runs", "stretches like {\"text\": \"…\", \"fill\": \"#c33\"}")?) {
        (Some(_), Some(_)) => return fail("give text (one style) or runs (several), not both"),
        (None, None) => return Ok(None),
        (Some(text), None) => vec![(text.to_owned(), Vec::new())],
        (None, Some(runs)) => runs.iter().map(|run| run_of(run, decimals)).collect::<Result<_, _>>()?,
    };
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    for (said, set) in stretches {
        for (i, part) in said.replace("\r\n", "\n").split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if !part.is_empty() {
                lines.last_mut().expect("it starts with one").push(Span { text: part.to_owned(), set: set.clone() });
            }
        }
    }
    Ok(Some(lines))
}

/// One of `runs`: its words, and the properties it sets.
fn run_of(run: &Doc, decimals: usize) -> Result<(String, Vec<(String, String)>), ToolError> {
    let Some(map) = run.as_map() else { return fail("each of \"runs\" should be like {\"text\": \"…\", \"fill\": \"#c33\"}") };
    let mut said = None;
    let mut set = Vec::new();
    for (key, value) in map.iter() {
        let wrong = |what: &str| ToolError(format!("a run's \"{key}\" should be {what}"));
        match key {
            "text" => said = Some(value.as_str().ok_or_else(|| wrong("its words, a string"))?.to_owned()),
            "fill" => set.push(("fill".to_owned(), value.as_str().ok_or_else(|| wrong("a paint, like \"#c33\""))?.to_owned())),
            "font" => set.push(("font-family".to_owned(), value.as_str().ok_or_else(|| wrong("a family's name"))?.to_owned())),
            "bold" => set.push(("font-weight".to_owned(), if value.as_bool().ok_or_else(|| wrong("true or false"))? { "bold" } else { "normal" }.to_owned())),
            "italic" => set.push(("font-style".to_owned(), if value.as_bool().ok_or_else(|| wrong("true or false"))? { "italic" } else { "normal" }.to_owned())),
            "size" => set.push(("font-size".to_owned(), ink_geom::number::format(value.as_f64().filter(|v| *v > 0.0).ok_or_else(|| wrong("a size over 0"))?, decimals))),
            other => return fail(format!("a run has no \"{other}\": it takes text, fill, bold, italic, font and size")),
        }
    }
    match said {
        Some(said) => Ok((said, set)),
        None => fail("each of \"runs\" needs its \"text\""),
    }
}

/// The properties the lettering arguments set on the text itself.
fn lettered(input: &In, decimals: usize) -> Result<Vec<(String, String)>, ToolError> {
    let mut set = Vec::new();
    let number = |v: f64| ink_geom::number::format(v, decimals);
    if let Some(font) = input.args.opt_str("font")? {
        if font.trim().is_empty() {
            return fail("\"font\" is empty: name a family (font_list says which are installed)");
        }
        set.push(("font-family".to_owned(), font.to_owned()));
    }
    if let Some(size) = input.args.opt_f64("size")? {
        if !(size.is_finite() && size > 0.0) {
            return fail("\"size\" should be over 0, in the drawing's units");
        }
        set.push(("font-size".to_owned(), number(size)));
    }
    if let Some(bold) = input.args.opt_bool("bold")? {
        set.push(("font-weight".to_owned(), if bold { "bold" } else { "normal" }.to_owned()));
    }
    if let Some(italic) = input.args.opt_bool("italic")? {
        set.push(("font-style".to_owned(), if italic { "italic" } else { "normal" }.to_owned()));
    }
    if let Some(fill) = input.args.opt_str("fill")? {
        set.push(("fill".to_owned(), fill.to_owned()));
    }
    if let Some(anchor) = input.args.opt_str("anchor")? {
        set.push(("text-anchor".to_owned(), anchor.to_owned()));
    }
    if let Some(gap) = input.args.opt_f64("letter_spacing")? {
        set.push(("letter-spacing".to_owned(), number(gap)));
    }
    Ok(set)
}

fn add(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let decimals = Precision::of(doc).decimals;
    let Some(lines) = lines(input, decimals)? else { return fail("give the words: text (one style) or runs (several)") };
    let mut attrs = vec![("x".to_owned(), ink_geom::number::format(input.args.f64("x")?, decimals)), ("y".to_owned(), ink_geom::number::format(input.args.f64("y")?, decimals))];
    attrs.extend(lettered(input, decimals)?);
    for (name, value) in &attrs[2..] {
        ink_core::ink_doc::styling::check(name, value).map_err(ToolError)?;
    }
    let leading = input.args.opt_f64("line_height")?.unwrap_or(LEADING);
    let element = lettering::element(&attrs, &lines, leading).map_err(refused_edit)?;
    Ok(Command::Insert { place: input.place(doc)?, elements: vec![element] })
}

fn set(doc: &Document, input: &In) -> Result<Command, ToolError> {
    let id = input.node("node_id")?;
    let node = doc.node(id).map_err(refused_edit)?;
    if node.kind != NodeKind::Text {
        return fail(format!("{id} is a <{}>, not a <text>: text_add makes a text, and node_style paints anything", node.name));
    }
    let decimals = Precision::of(doc).decimals;
    let mut steps = Vec::new();
    let leading = input.args.opt_f64("line_height")?;
    match lines(input, decimals)? {
        Some(lines) => steps.push(Command::SetText { node: id, lines, leading }),
        None if leading.is_some() => return fail("line_height goes with text or runs: it's written into the lines they make"),
        None => {}
    }
    let style: Vec<(String, Option<String>)> = lettered(input, decimals)?.into_iter().map(|(name, value)| (name, Some(value))).collect();
    if !style.is_empty() {
        steps.push(Command::SetStyle { nodes: vec![id], set: style });
    }
    if steps.is_empty() {
        return fail("say what to set: text or runs, or font, size, bold, italic, fill, anchor, letter_spacing");
    }
    Ok(Command::Batch(steps))
}

/// The fonts a text is set in, in words: `Inter bold 18`, and which of
/// the families it asked for aren't here.
pub(crate) fn set_in(doc: &Document, node: &Node) -> String {
    let fonts: Vec<String> = text::lettered(doc, node)
        .iter()
        .map(|font| {
            let style = match (font.bold, font.italic) {
                (true, true) => " bold italic",
                (true, false) => " bold",
                (false, true) => " italic",
                (false, false) => "",
            };
            let missing = if font.missing.is_empty() { String::new() } else { format!(" ({} not installed here)", font.missing.join(", ")) };
            format!("{}{style} {}{missing}", font.family, n(font.size))
        })
        .collect();
    if fonts.is_empty() { "it says nothing yet".to_owned() } else { format!("set in {}", fonts.join(" and ")) }
}

/// What a text is now: what it says, where it shows, and its fonts.
pub(crate) fn about(doc: &Document, id: NodeId) -> Option<String> {
    let node = doc.get(id).filter(|node| node.kind == NodeKind::Text)?;
    let place = match (page_bounds(doc).get(&id), text::unset(doc, node)) {
        (_, Some(why)) => format!(", not drawn: {why}"),
        (Some(b), None) => format!(" at {}", rect(b)),
        (None, None) => String::new(),
    };
    Some(format!("{id} {} {}{place}, {}", tag(node), words(doc, node), set_in(doc, node)))
}

/// What `text_add` and `text_set` say: the text as it now is.
fn said(doc: &Document, applied: &Applied) -> Reply {
    let (verb, id) = match (applied.created.iter().find(|id| doc.get(**id).is_some_and(|node| node.kind == NodeKind::Text)), applied.changed.first()) {
        (Some(id), _) => ("Added", *id),
        (None, Some(id)) => ("Set. Now:", *id),
        (None, None) => return Reply::text("Nothing changed: it said that already, lettered that way."),
    };
    let mut m = Map::new();
    m.insert("node_id", id.to_string().into());
    Reply::text(format!("{verb} {}.", about(doc, id).unwrap_or_else(|| id.to_string()))).data(Doc::Map(m))
}

fn font_list(_: &mut Ctx, input: &In) -> Result<Reply, ToolError> {
    let query = input.args.opt_str("query")?.map(str::to_lowercase).filter(|q| !q.trim().is_empty());
    let all = fonts::families();
    let found: Vec<&String> = all.iter().filter(|family| query.as_ref().is_none_or(|q| family.to_lowercase().contains(q.trim()))).collect();
    let stands = |generic: &str| fonts::family(generic).map_or_else(|| "nothing".to_owned(), |family| fonts::called(&family));
    let mut text = format!("Here sans-serif is {}, monospace is {} and serif is {}. ", stands("sans-serif"), stands("monospace"), stands("serif"));
    text += &match (&query, found.len()) {
        (Some(q), 0) => format!("No family of the {} installed has \"{}\" in its name.", all.len(), q.trim()),
        (Some(q), count) => format!("{count} of the {} families installed have \"{}\" in their name: ", all.len(), q.trim()),
        (None, count) => format!("{count} families are installed: "),
    };
    if !found.is_empty() {
        text += &found.iter().take(MAX_FAMILIES).map(|family| family.as_str()).collect::<Vec<_>>().join(", ");
        text += &match found.len().saturating_sub(MAX_FAMILIES) {
            0 => ".".to_owned(),
            more => format!(", and {more} more (narrow it with query)."),
        };
    }
    let mut m = Map::new();
    m.insert("families", Doc::List(found.iter().take(MAX_FAMILIES).map(|family| family.as_str().into()).collect()));
    m.insert("count", Doc::Int(found.len() as i64));
    Ok(Reply::text(text).data(Doc::Map(m)))
}
