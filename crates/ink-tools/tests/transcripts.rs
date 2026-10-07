//! Talking to the server the way Claude Code does: lines in, lines out
//! (ARCHITECTURE §10). The protocol itself is tested in `lntrn-mcp`;
//! these are Ink's tools.

mod common;

use common::{data, ok, picture, refused, server, text};
use ink_tools::{INSTRUCTIONS, Ink};
use lntrn_data::{Doc, json};
use lntrn_mcp::{MAX_INSTRUCTIONS, Server};

#[test]
fn the_tool_list_is_fixed_and_fits_claude_codes_limits() {
    let (mut s, _) = server("list");
    let out = s.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
    assert_eq!(out, s.handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#), "deterministic, for prompt caching");
    let list = json::parse(&out[0]).unwrap();
    let tools = list.path("result.tools").and_then(Doc::as_list).unwrap();
    let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(Doc::as_str)).collect();
    assert_eq!(names, ["doc_new", "doc_open", "doc_list", "doc_info", "doc_source", "doc_preview", "doc_save", "doc_export", "doc_close", "node_add", "node_add_svg", "node_set", "node_move", "node_delete", "node_transform", "node_align", "node_duplicate", "node_group", "node_ungroup", "history_undo", "history_redo", "batch"]);
    for t in tools {
        let name = t.get("name").and_then(Doc::as_str).unwrap();
        assert!(t.get("description").and_then(Doc::as_str).is_some_and(|d| d.len() <= 2048), "{name}'s description is too long");
        assert_eq!(t.path("inputSchema.type").and_then(Doc::as_str), Some("object"), "{name}");
        assert_eq!(t.path("inputSchema.additionalProperties").and_then(Doc::as_bool), Some(false), "{name}");
    }
    // A schema says what the server takes: an attribute is a string or
    // a number, and a batch step's arguments are whatever its tool's are.
    let schema = |tool: &str, path: &str| tools.iter().find(|t| t.get("name").and_then(Doc::as_str) == Some(tool)).and_then(|t| t.path(path)).map(json::write);
    assert_eq!(schema("node_add", "inputSchema.properties.attrs.additionalProperties.type").as_deref(), Some(r#"["string","number"]"#));
    assert_eq!(schema("node_set", "inputSchema.properties.attrs.additionalProperties.type").as_deref(), Some(r#"["string","number","null"]"#));
    assert_eq!(schema("batch", "inputSchema.properties.steps.items.properties.args").as_deref(), Some(r#"{"type":"object","description":"The tool's arguments, without doc_id"}"#));
    assert!(INSTRUCTIONS.len() <= MAX_INSTRUCTIONS, "{} characters of instructions", INSTRUCTIONS.len());
    let hello = json::parse(&s.handle_line(r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}"#)[0]).unwrap();
    assert_eq!(hello.path("result.serverInfo.name").and_then(Doc::as_str), Some("lantern-ink"));
}

#[test]
fn a_drawing_is_made_looked_at_and_saved() {
    let (mut s, dir) = server("session");
    assert!(text(&ok(&mut s, "doc_list", "{}")).starts_with("No drawings are open"));
    let made = ok(&mut s, "doc_new", "{}");
    assert_eq!((data(&made, "doc_id"), data(&made, "root")), ("d1", "N1"));
    assert!(text(&made).starts_with("Made d1: an empty drawing 24 × 24 (viewBox 0 0 24 24)."), "{}", text(&made));

    // One element and its attributes, as the file will write them.
    let disc = ok(&mut s, "node_add", r##"{"doc_id":"d1","element":"circle","attrs":{"cx":12,"cy":12,"r":9.50004,"fill":"#ffc800"}}"##);
    assert_eq!(text(&disc), "Added N2 <circle> at 2.5,2.5 19×19.");
    assert_eq!(data(&disc, "node_id"), "N2");
    // Markup as it's written, nested, under the disc.
    let face = ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<g id='face' fill='none' stroke='#12100e' stroke-linecap='round'>\n  <path d='M8 14 Q12 18 16 14'/>\n  <line x1='9' y1='9' x2='9' y2='10'/>\n</g>","above":"N2"}"##);
    assert_eq!(text(&face), "Added N3 <g id=\"face\"> with 2 inside at 8,9 8×7.");
    // Any attribute, set or taken off.
    let set = ok(&mut s, "node_set", r##"{"doc_id":"d1","node_id":"N3","attrs":{"stroke-width":1.5,"stroke-linecap":null,"opacity":0.9}}"##);
    assert!(text(&set).starts_with("Set. It's now N3 <g id=\"face\">"), "{}", text(&set));
    assert!(text(&ok(&mut s, "node_set", r##"{"doc_id":"d1","node_id":"N3","attrs":{"opacity":"0.9"}}"##)).starts_with("Nothing changed"));

    let source = ok(&mut s, "doc_source", r#"{"doc_id":"d1"}"#);
    assert_eq!(
        text(&source),
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\n  <circle cx=\"12\" cy=\"12\" r=\"9.5\" fill=\"#ffc800\"/>\n  <g id='face' fill='none' stroke='#12100e' stroke-width=\"1.5\" opacity=\"0.9\">\n    <path d='M8 14 Q12 18 16 14'/>\n    <line x1='9' y1='9' x2='9' y2='10'/>\n  </g>\n</svg>\n"
    );
    assert_eq!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1","node_id":"N5"}"#)), "<line x1='9' y1='9' x2='9' y2='10'/>");
    assert!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1","max_chars":200}"#)).contains("… cut at 200 of"));

    // The nodes, front to back.
    let info = ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#);
    let lines: Vec<&str> = text(&info).lines().collect();
    assert_eq!(lines[0], "d1 (no file yet), never saved.");
    assert_eq!(lines[1], "Page 24 × 24, viewBox 0 0 24 24. 5 nodes. Undo: 3 (latest: \"node_set\" by Claude). Redo: 0.");
    assert_eq!(&lines[3..], ["N1 svg  at 2.5,2.5 19×19", "  N3 g #face  fill none  stroke #12100e 1.5  opacity 0.9  at 8,9 8×7", "    N5 line  at 9,9 0×1", "    N4 path  at 8,14 8×2", "  N2 circle  fill #ffc800  at 2.5,2.5 19×19"]);
    assert_eq!(info.path("structuredContent.nodes[1].id").and_then(Doc::as_str), Some("N3"));

    // A look, at the size asked for.
    let seen = ok(&mut s, "doc_preview", r#"{"doc_id":"d1","max_edge":96}"#);
    let (mime, image) = picture(&seen);
    assert_eq!((mime.as_str(), image.width, image.height), ("image/png", 96, 96));
    assert_eq!(image.pixel(48, 20), [255, 200, 0, 255], "the disc");
    assert_eq!(image.pixel(1, 1), [255, 255, 255, 255], "the checkerboard shows where nothing is drawn");
    assert!(text(&seen).starts_with("Preview: all of d1 (page 24 × 24) → 96×96 PNG"), "{}", text(&seen));
    assert!(text(&seen).ends_with(&format!("On disk: {}", dir.join("previews/test-d1-1.png").display())), "{}", text(&seen));
    // A part of it, on black, as a JPEG.
    let part = ok(&mut s, "doc_preview", r#"{"doc_id":"d1","max_edge":64,"region":{"x":0,"y":0,"width":12,"height":6},"background":"black","format":"jpeg"}"#);
    let (mime, image) = picture(&part);
    assert_eq!((mime.as_str(), image.width, image.height), ("image/jpeg", 64, 32));
    assert!(dir.join("previews/test-d1-1.png").exists() && dir.join("previews/test-d1-2.png").exists(), "each look has a file of its own");
    // As Lantern's apps will draw it: five sizes, side by side.
    let lantern = ok(&mut s, "doc_preview", r#"{"doc_id":"d1","renderer":"lantern"}"#);
    let (_, strip) = picture(&lantern);
    assert_eq!((strip.width, strip.height), (128 + 120 + 128 + 96 + 128 + 6 * 12, 152));
    assert!(text(&lantern).starts_with("Lantern preview of d1: as lntrn-svg") && text(&lantern).contains("a strip 672×152; "), "{}", text(&lantern));
    assert!(!text(&lantern).contains("max_edge"));
    let sized = ok(&mut s, "doc_preview", r#"{"doc_id":"d1","renderer":"lantern","max_edge":1000}"#);
    assert_eq!(picture(&sized).1.width, 672);
    assert!(text(&sized).contains("a strip 672×152; transparency as a checkerboard. max_edge doesn't apply to it: the strip is always that size."), "{}", text(&sized));

    // Saved: the file is the markup. Then nothing is unsaved.
    assert_eq!(refused(&mut s, "doc_save", r#"{"doc_id":"d1"}"#), "d1 has no file yet: give doc_save a path ending in .svg");
    assert!(refused(&mut s, "doc_save", r#"{"doc_id":"d1","path":"smile.png"}"#).contains("isn't an .svg name"));
    let saved = ok(&mut s, "doc_save", r#"{"doc_id":"d1","path":"smile.svg"}"#);
    assert_eq!(data(&saved, "path"), dir.join("smile.svg").display().to_string());
    assert_eq!(std::fs::read_to_string(dir.join("smile.svg")).unwrap(), text(&source));
    assert!(text(&ok(&mut s, "doc_list", "{}")).ends_with("5 nodes, saved"), "{}", text(&ok(&mut s, "doc_list", "{}")));
    ok(&mut s, "doc_save", r#"{"doc_id":"d1"}"#);
    // Another drawing can't take its file while it's open, and takes any
    // other file only when told to.
    ok(&mut s, "doc_new", r#"{"width":16}"#);
    for args in [r#"{"doc_id":"d2","path":"smile.svg"}"#, r#"{"doc_id":"d2","path":"./smile.svg","overwrite":true}"#] {
        assert!(refused(&mut s, "doc_save", args).ends_with("smile.svg is d1's file, and d1 is open: save d2 under another name, or doc_close d1 first"));
    }
    std::fs::write(dir.join("other.svg"), "<svg/>").unwrap();
    assert!(refused(&mut s, "doc_save", r#"{"doc_id":"d2","path":"other.svg"}"#).ends_with("is already there: pass overwrite: true to replace it"));

    // Pictures of it.
    let png = ok(&mut s, "doc_export", r#"{"doc_id":"d1","path":"smile.png","size":48}"#);
    assert!(text(&png).contains("48×48 png"), "{}", text(&png));
    let exported = lntrn_image::decode(&std::fs::read(dir.join("smile.png")).unwrap()).unwrap();
    assert_eq!((exported.width, exported.pixel(0, 0)[3], exported.pixel(24, 10)), (48, 0, [255, 200, 0, 255]), "transparent where nothing is drawn");
    ok(&mut s, "doc_export", r#"{"doc_id":"d1","path":"smile.jpg","scale":2}"#);
    assert_eq!(lntrn_image::decode(&std::fs::read(dir.join("smile.jpg")).unwrap()).unwrap().width, 48);
    // A name that doesn't say, and a format that does.
    ok(&mut s, "doc_export", r#"{"doc_id":"d1","path":"smile-picture","format":"webp"}"#);
    assert_eq!(lntrn_image::decode(&std::fs::read(dir.join("smile-picture")).unwrap()).unwrap().width, 24);
    assert!(refused(&mut s, "doc_export", r#"{"doc_id":"d1","path":"smile.png"}"#).contains("already there"));
    assert!(refused(&mut s, "doc_export", r#"{"doc_id":"d1","path":"smile.tiff"}"#).contains("name it .png, .jpg or .webp"));
    assert!(refused(&mut s, "doc_export", r#"{"doc_id":"d1","path":"a.png","size":8,"scale":2}"#).contains("size or scale, not both"));

    // Closing: not with unsaved changes, unless told to lose them.
    assert_eq!(refused(&mut s, "doc_close", r#"{"doc_id":"d2"}"#), "d2 has unsaved changes: doc_save it first, or pass discard: true to lose them");
    ok(&mut s, "doc_close", r#"{"doc_id":"d2","discard":true}"#);
    ok(&mut s, "doc_close", r#"{"doc_id":"d1"}"#);
    assert_eq!(refused(&mut s, "doc_info", r#"{"doc_id":"d1"}"#), "no open document d1 (doc_list shows the open ones)");
}

#[test]
fn nodes_move_go_and_come_back() {
    let (mut s, _) = server("edits");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r#"{"doc_id":"d1","svg":"<rect id='a' width='4' height='4'/><g id='g'><rect id='b' width='2' height='2'/></g><rect id='c' width='8' height='8'/>"}"#);
    let order = |s: &mut Server<Ink>| -> String { text(&ok(s, "doc_info", r#"{"doc_id":"d1"}"#)).lines().skip(3).map(|l| l.split_whitespace().nth(2).unwrap_or("").to_owned()).collect::<Vec<_>>().join(" ") };
    assert_eq!(order(&mut s), "at #c #g #b #a", "front to back");
    // Into the group, on top of what's there; then to the bottom of all.
    assert_eq!(text(&ok(&mut s, "node_move", r#"{"doc_id":"d1","node_ids":["N2"],"into":"N3"}"#)), "Moved N2 (now in N3).");
    assert_eq!(order(&mut s), "at #c #g #a #b");
    ok(&mut s, "node_move", r#"{"doc_id":"d1","node_ids":["N5"],"at":"bottom"}"#);
    assert_eq!(order(&mut s), "at #g #a #b #c");
    assert_eq!(text(&ok(&mut s, "node_move", r#"{"doc_id":"d1","node_ids":["N5"],"below":"N3"}"#)), "Nothing moved: they were there already.");
    assert_eq!(refused(&mut s, "node_move", r#"{"doc_id":"d1","node_ids":["N3"]}"#), "say where to: one of above, below, into or at");
    assert_eq!(refused(&mut s, "node_move", r#"{"doc_id":"d1","node_ids":["N3"],"into":"N4"}"#), "N3 can't go inside itself");

    assert_eq!(text(&ok(&mut s, "node_delete", r#"{"doc_id":"d1","node_ids":["N3"]}"#)), "Deleted N3 (and what was in it). 2 nodes are left.");
    assert_eq!(refused(&mut s, "node_delete", r#"{"doc_id":"d1","node_ids":["N1"]}"#), "N1 is the drawing's root <svg>: it can't be deleted (doc_close closes the drawing)");
    assert_eq!(refused(&mut s, "node_delete", r#"{"doc_id":"d1","node_ids":["N3"]}"#), "no node N3 in this document (doc_info lists its nodes)");

    // Back, and forward again; a refused or empty call left no step.
    let undone = ok(&mut s, "history_undo", r#"{"doc_id":"d1","steps":2}"#);
    assert_eq!(text(&undone), "Undid 2 steps: \"node_delete\" (Claude), \"node_move\" (Claude). Now 2 can be undone and 2 redone.");
    assert_eq!(order(&mut s), "at #c #g #a #b");
    assert_eq!(text(&ok(&mut s, "history_redo", r#"{"doc_id":"d1"}"#)), "Redid 1 step: \"node_move\" (Claude). Now 3 can be undone and 1 redone.");
    ok(&mut s, "history_undo", r#"{"doc_id":"d1","steps":100}"#);
    assert_eq!(refused(&mut s, "history_undo", r#"{"doc_id":"d1"}"#), "there's nothing to undo in d1");
    assert_eq!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1"}"#)), text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1"}"#)));
    assert!(text(&ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#)).contains("1 nodes. Undo: 0. Redo: 4."));
}

#[test]
fn a_batch_is_one_step_with_names_between_its_steps() {
    let (mut s, _) = server("batch");
    ok(&mut s, "doc_new", "{}");
    let steps = r##"[
        {"tool":"node_add","args":{"element":"g","attrs":{"id":"sun","fill":"#ffc800"}},"as":"sun"},
        {"tool":"node_add","args":{"element":"circle","attrs":{"cx":12,"cy":12,"r":5},"into":"@sun"},"as":"disc"},
        {"tool":"node_add_svg","args":{"svg":"<path d='M12 2v3'/><path d='M12 19v3'/>","into":"@sun"}},
        {"tool":"node_set","args":{"node_id":"@disc","attrs":{"r":6}}}
    ]"##;
    let ran = ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","steps":{steps}}}"#));
    assert!(text(&ran).starts_with("Ran 4 steps on d1 as one undo step (history_undo undoes all of them). New nodes: N2, N3, N4, N5. Named: @sun = N2, @disc = N3."), "{}", text(&ran));
    assert_eq!(ran.path("structuredContent.names.disc").and_then(Doc::as_str), Some("N3"));
    let (mime, image) = picture(&ran);
    assert_eq!((mime.as_str(), image.width), ("image/png", 512), "a picture of the result, unless told not to");
    assert!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1"}"#)).contains("<circle cx=\"12\" cy=\"12\" r=\"6\"/>"));
    assert!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)).starts_with("Undid 1 step: \"batch\" (Claude). Now 0 can be undone"));

    // All or nothing: the step that's refused is named, and nothing of
    // the batch is left.
    let bad = r#"{"doc_id":"d1","preview":false,"steps":[{"tool":"node_add","args":{"element":"rect"},"as":"r"},{"tool":"node_set","args":{"node_id":"N99","attrs":{"x":1}}}]}"#;
    assert_eq!(refused(&mut s, "batch", bad), "step 2 (node_set): no node N99 in this document (doc_info lists its nodes)");
    assert!(text(&ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#)).contains("1 nodes. Undo: 0."));
    for (steps, says) in [
        (r#"[]"#, "a batch holds 1 to 200 steps, not 0"),
        (r#"[{"tool":"doc_save","args":{}}]"#, "step 1: doc_save can't go in a batch; only edits can (node_add, node_add_svg, node_set, node_move, node_delete, node_transform, node_align, node_duplicate, node_group, node_ungroup)"),
        (r#"[{"tool":"node_add","args":{"element":"g","preview":true}}]"#, "step 1 (node_add): preview goes on the batch, not on a step"),
        (r#"[{"tool":"node_add","args":{"element":"g","doc_id":"d2"}}]"#, "step 1 (node_add): a batch works on one drawing, d1"),
        (r#"[{"tool":"node_add","args":{"element":"g","into":"@nobody"}}]"#, "step 1 (node_add): \"@nobody\": no earlier step of this batch is named \"nobody\" (name one with \"as\")"),
        (r#"[{"tool":"node_add","args":{"element":"g"},"as":"a"},{"tool":"node_add","args":{"element":"g"},"as":"a"}]"#, "step 2: the name \"a\" is already taken in this batch"),
        (r#"[{"tool":"node_delete","args":{"node_ids":["N1"]},"as":"gone"}]"#, "step 1 (node_delete): N1 is the drawing's root <svg>: it can't be deleted (doc_close closes the drawing)"),
        (r#"[{"tool":"node_add","args":{"element":"g"},"then":"x"}]"#, "step 1: \"then\" isn't a step field; a step is {tool, args, as}"),
    ] {
        assert_eq!(refused(&mut s, "batch", &format!(r#"{{"doc_id":"d1","steps":{steps}}}"#)), says);
    }
    // Outside a batch a name means nothing.
    assert!(refused(&mut s, "node_set", r#"{"doc_id":"d1","node_id":"@sun","attrs":{"x":1}}"#).contains("@names work only between the steps of a batch"));
}

#[test]
fn mistakes_say_how_to_fix_them() {
    let (mut s, dir) = server("mistakes");
    ok(&mut s, "doc_new", "{}");
    assert_eq!(refused(&mut s, "node_add", r#"{"doc_id":"d1","element":"my shape"}"#), "\"my shape\" can't be an element's name: give an SVG one, like rect, path or g");
    assert_eq!(refused(&mut s, "node_add", r#"{"doc_id":"d1","element":"rect","attrs":{"x":null}}"#), "the attribute \"x\" is null: a new element has nothing to take off");
    assert_eq!(refused(&mut s, "node_add", r#"{"doc_id":"d1","element":"rect","attrs":{"x":[1]}}"#), "the attribute \"x\" should be a string or a number (or null, to take it off)");
    assert_eq!(refused(&mut s, "node_add", r#"{"doc_id":"d1","element":"rect","fill":"red"}"#), "unknown argument \"fill\"; this tool takes: doc_id, element, attrs, above, below, into, at, preview");
    assert_eq!(refused(&mut s, "node_add_svg", r#"{"doc_id":"d1","svg":"<g><rect></g>"}"#), "the markup can't be read: line 1, column 10: </g> closes <rect>: the names don't match");
    assert_eq!(refused(&mut s, "node_add_svg", r#"{"doc_id":"d1","svg":"just words"}"#), "the markup can't be read: text outside any element can't be inserted: \"just words\"");
    assert_eq!(refused(&mut s, "node_add_svg", r#"{"doc_id":"d1","svg":"  "}"#), "the markup holds no element");
    assert_eq!(refused(&mut s, "node_set", r#"{"doc_id":"d1","node_id":"N1","attrs":{}}"#), "\"attrs\" is empty: name at least one attribute to set");
    assert_eq!(refused(&mut s, "node_set", r#"{"doc_id":"d1","node_id":"N1","attrs":{"a b":"1"}}"#), "\"a b\" can't be an attribute's name");
    assert_eq!(refused(&mut s, "node_set", r#"{"doc_id":"d7","node_id":"N1","attrs":{"x":1}}"#), "no open document d7 (doc_list shows the open ones)");
    assert_eq!(refused(&mut s, "doc_preview", r#"{"doc_id":"d1","background":"none","format":"jpeg"}"#), "JPEG can't be transparent: use background checker, white or black, or format png");
    assert_eq!(refused(&mut s, "doc_preview", r#"{"doc_id":"d1","renderer":"lantern","region":{"x":0,"y":0,"width":4,"height":4}}"#), "the lantern renderer shows the whole icon at its sizes: leave region out");
    assert!(refused(&mut s, "doc_open", r#"{"path":"nowhere.svg"}"#).starts_with(&format!("{}: ", dir.join("nowhere.svg").display())));
    std::fs::write(dir.join("page.svg"), "<html/>").unwrap();
    assert!(refused(&mut s, "doc_open", r#"{"path":"page.svg"}"#).contains("not an SVG"));
    // A style that outvotes an attribute is pointed out.
    ok(&mut s, "node_add_svg", r#"{"doc_id":"d1","svg":"<rect width='4' height='4' style='fill: red'/>"}"#);
    let set = ok(&mut s, "node_set", r##"{"doc_id":"d1","node_id":"N2","attrs":{"fill":"#00f"}}"##);
    assert!(text(&set).contains("Note: its style=\"…\" also sets fill, and a style wins"), "{}", text(&set));
}

#[test]
fn a_file_from_boxy_is_taken_over_and_saved_as_inks() {
    let (mut s, dir) = server("boxy");
    let original = "<svg viewBox=\"0 0 24 24\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:bx=\"https://boxy-svg.com\">\n  <defs>\n    <bx:export>\n      <bx:file format=\"svg\" path=\"line.svg\"/>\n    </bx:export>\n  </defs>\n  <path d=\"M4 12h16\" stroke=\"#e8dcc8\" bx:shape=\"line\"/>\n</svg>\n";
    std::fs::write(dir.join("line.svg"), original).unwrap();
    let opened = ok(&mut s, "doc_open", r#"{"path":"line.svg"}"#);
    assert!(text(&opened).ends_with("Boxy SVG's own marks were taken out (2 elements, 1 attributes) and Ink's namespace put where theirs was; the file itself changes only when you doc_save."), "{}", text(&opened));
    assert_eq!(std::fs::read_to_string(dir.join("line.svg")).unwrap(), original, "opening doesn't touch the file");
    assert!(text(&ok(&mut s, "doc_list", "{}")).ends_with("saved"), "nor does it count as a change");
    // The ids are the ones the file's elements were given as it was read:
    // what was taken out leaves a gap, and doc_info says what there is.
    assert!(text(&ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#)).ends_with("N1 svg  at 4,12 16×0\n  N5 path  stroke #e8dcc8  at 4,12 16×0"));
    ok(&mut s, "node_set", r#"{"doc_id":"d1","node_id":"N5","attrs":{"stroke-width":2}}"#);
    ok(&mut s, "doc_save", r#"{"doc_id":"d1"}"#);
    assert_eq!(std::fs::read_to_string(dir.join("line.svg")).unwrap(), "<svg viewBox=\"0 0 24 24\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\">\n  <path d=\"M4 12h16\" stroke=\"#e8dcc8\" stroke-width=\"2\"/>\n</svg>\n");
}

#[test]
fn a_file_has_one_drawing_at_a_time() {
    let (mut s, dir) = server("one-a-file");
    std::fs::write(dir.join("dot.svg"), "<svg viewBox=\"0 0 8 8\">\n  <circle cx=\"4\" cy=\"4\" r=\"2\"/>\n</svg>\n").unwrap();
    assert_eq!(data(&ok(&mut s, "doc_open", r#"{"path":"dot.svg"}"#), "doc_id"), "d1");
    // Opened again, by whatever name: the drawing it already is.
    let again = ok(&mut s, "doc_open", r#"{"path":"./dot.svg"}"#);
    assert_eq!((data(&again, "doc_id"), again.path("structuredContent.already_open").and_then(Doc::as_bool)), ("d1", Some(true)));
    assert!(text(&again).ends_with("dot.svg is already open as d1: work on that one (page 8 × 8, viewBox 0 0 8 8, 2 nodes, root N1). To read the file afresh, doc_close d1 first."), "{}", text(&again));
    ok(&mut s, "node_set", r#"{"doc_id":"d1","node_id":"N2","attrs":{"r":3}}"#);
    assert!(text(&ok(&mut s, "doc_open", r#"{"path":"dot.svg"}"#)).contains("already open as d1, with changes that aren't saved: work on that one"));
    assert_eq!(text(&ok(&mut s, "doc_list", "{}")).lines().count(), 1, "one drawing, however often it was asked for");
    // Closed, the file opens afresh.
    ok(&mut s, "doc_close", r#"{"doc_id":"d1","discard":true}"#);
    assert_eq!(data(&ok(&mut s, "doc_open", r#"{"path":"dot.svg"}"#), "doc_id"), "d2");
}

#[test]
fn definitions_are_laid_out_and_listed_as_they_are_written() {
    let (mut s, _) = server("definitions");
    ok(&mut s, "doc_new", r#"{"width":4}"#);
    // Markup all on one line takes the file's lines, however deep.
    let markup = r##"<defs><linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"><stop offset=\"0\" stop-color=\"#fff\"/><stop offset=\"1\" stop-color=\"#000\"/></linearGradient><filter id=\"f\"><feDropShadow dx=\"0\" dy=\"1\" stdDeviation=\"1\"/></filter></defs><rect width=\"4\" height=\"4\" fill=\"url(#g)\"/>"##;
    ok(&mut s, "node_add_svg", &format!(r#"{{"doc_id":"d1","svg":"{markup}"}}"#));
    assert_eq!(
        text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1","node_id":"N2"}"#)),
        "<defs>\n    <linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\n      <stop offset=\"0\" stop-color=\"#fff\"/>\n      <stop offset=\"1\" stop-color=\"#000\"/>\n    </linearGradient>\n    <filter id=\"f\">\n      <feDropShadow dx=\"0\" dy=\"1\" stdDeviation=\"1\"/>\n    </filter>\n  </defs>"
    );
    // What's drawn is listed front to back; a gradient's stops and a
    // filter's steps in their own order, with what they say.
    let info = ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#);
    let lines: Vec<&str> = text(&info).lines().collect();
    assert!(lines[2].starts_with("Nodes, front to back (the first listed is on top; what a <defs>, a gradient or a filter holds is in the file's order;"), "{}", lines[2]);
    assert_eq!(
        &lines[3..],
        [
            "N1 svg  at 0,0 4×4",
            "  N8 rect  fill url(#g)  at 0,0 4×4",
            "  N2 defs",
            "    N3 linearGradient #g  x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\"",
            "      N4 stop  offset=\"0\" stop-color=\"#fff\"",
            "      N5 stop  offset=\"1\" stop-color=\"#000\"",
            "    N6 filter #f",
            "      N7 feDropShadow  dx=\"0\" dy=\"1\" stdDeviation=\"1\"",
        ]
    );
    // A long value is cut, and a long list of them.
    let long = "M0 0".to_owned() + &" L1 1".repeat(20);
    ok(&mut s, "node_add", &format!(r#"{{"doc_id":"d1","element":"clipPath","attrs":{{"id":"c","a":"{long}","b":"{long}","c":"{long}","d":"{long}","e":"{long}"}},"into":"N2"}}"#));
    let cut = format!("{}…", &long[..40]);
    let info = ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#);
    assert!(text(&info).ends_with(&format!("\n    N9 clipPath #c  a=\"{cut}\" b=\"{cut}\" c=\"{cut}\" d=\"{cut}\" …")), "{}", text(&info));
}

#[test]
fn the_newest_looks_stay_on_disk() {
    let (mut s, dir) = server("looks");
    ok(&mut s, "doc_new", "{}");
    for _ in 0..34 {
        ok(&mut s, "doc_preview", r#"{"doc_id":"d1","max_edge":16}"#);
    }
    let kept = |n: u32| dir.join(format!("previews/test-d1-{n}.png")).exists();
    assert!(!kept(1) && !kept(2) && kept(3) && kept(34), "the oldest of more than 32 go");
    assert_eq!(std::fs::read_dir(dir.join("previews")).unwrap().count(), 32);
}
