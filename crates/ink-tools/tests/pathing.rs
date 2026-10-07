//! Paths by their anchors, over the wire (ARCHITECTURE §10):
//! `path_set`, `path_edit` and `path_op`, and the anchors `node_info`
//! lists. What each edit does to an outline is `ink-doc`'s to test;
//! these are the calls, their replies and their refusals.

mod common;

use common::{data, ok, picture, refused, server, text};
use ink_tools::Ink;
use lntrn_data::Doc;
use lntrn_mcp::Server;

const DRAWING: &str = "<path id='tri' d='M2 20 L12 4 L22 20 Z' fill='#ffc800'/><rect id='box' x='2' y='2' width='8' height='6' rx='2'/><path id='two' d='M1 1h4v4zM10 10c2 0 4 2 4 4'/><g id='g'/><path id='bad' d='M0 0 L5 5 X 3'/>";

fn drawing(test: &str) -> Server<Ink> {
    let (mut s, _) = server(test);
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", &format!(r#"{{"doc_id":"d1","svg":"{DRAWING}"}}"#));
    s
}

fn node(s: &mut Server<Ink>, id: &str) -> String {
    text(&ok(s, "doc_source", &format!(r#"{{"doc_id":"d1","node_id":"{id}"}}"#))).to_owned()
}

/// A reply's lines after its first.
fn listed(reply: &Doc) -> Vec<&str> {
    text(reply).lines().skip(1).collect()
}

fn edit(s: &mut Server<Ink>, node: &str, edits: &str) -> Doc {
    ok(s, "path_edit", &format!(r#"{{"doc_id":"d1","node_id":"{node}","edits":{edits}}}"#))
}

#[test]
fn a_path_lists_its_anchors() {
    let mut s = drawing("anchors");
    let info = ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N4"}"#);
    assert_eq!(
        text(&info).lines().skip(3).collect::<Vec<_>>(),
        [
            "Anchors, in its own coordinates (path_edit takes these ids):",
            "  Run 1 of 2, closed, 3 anchors:",
            "    A4 at 1,1  then a line to A5",
            "    A5 at 5,1  then a line to A6",
            "    A6 at 5,5  then a line to A4",
            "  Run 2 of 2, open, 2 anchors:",
            "    A7 at 10,10  out 2,0  then a curve to A8",
            "    A8 at 14,14  in 0,-2  (the end)",
        ]
    );
    // As data too: each anchor's run, place and handles.
    let anchor = |i: usize, key: &str| info.path(&format!("structuredContent.anchors[{i}].{key}")).map(lntrn_data::json::write);
    assert_eq!((anchor(3, "id").as_deref(), anchor(3, "run").as_deref(), anchor(3, "at").as_deref(), anchor(3, "out").as_deref(), anchor(3, "in")), (Some("\"A7\""), Some("2"), Some("[10.0,10.0]"), Some("[2.0,0.0]"), None));
    // What has none says how to get some, or why it can't.
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N3"}"#)).ends_with("\nNo anchors of its own: path_edit (or path_op to_path) makes it a path that has."));
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N6"}"#)).ends_with("\nIts path data can't all be read, so it has no anchors to edit it by."));
    assert!(!text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N5"}"#)).contains("nchors"), "a group isn't an outline");
    // A long path's list is cut, saying how many more there are.
    ok(&mut s, "node_add", &format!(r#"{{"doc_id":"d1","element":"path","attrs":{{"d":"M0 0{}"}}}}"#, " l1 1".repeat(60)));
    let long = ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N7"}"#);
    assert!(text(&long).ends_with("\n    … and 13 more (doc_source shows its d)"), "{}", text(&long));
    assert_eq!(long.path("structuredContent.anchors").and_then(Doc::as_list).map(<[Doc]>::len), Some(61), "all of them, as data");
}

#[test]
fn a_path_is_edited_by_its_anchors() {
    let mut s = drawing("edit");
    // An anchor put on a side, named, and used by the edits after it.
    let added = edit(&mut s, "N2", r#"[{"op":"add","after":"A1","share":0.25,"as":"q"},{"op":"add","after":"@q","near":[10,8],"as":"r"},{"op":"move","anchors":["@q","@r"],"by":[-1,0]}]"#);
    assert_eq!(
        text(&added).lines().collect::<Vec<_>>(),
        [
            "Done. N2 <path id=\"tri\"> is now:",
            "  Run 1 of 1, closed, 5 anchors:",
            "    A1 at 2,20  then a line to A11",
            "    A11 at 3.5,16  then a line to A12",
            "    A12 at 8.64,7.775  then a line to A2",
            "    A2 at 12,4  then a line to A3",
            "    A3 at 22,20  then a line to A1",
            "New anchors: A11, A12.",
        ]
    );
    assert_eq!((data(&added, "node_id"), added.path("structuredContent.made[1]").and_then(Doc::as_str)), ("N2", Some("A12")));
    assert_eq!(node(&mut s, "N2"), "<path id='tri' d='M2 20 L3.5 16 L8.64 7.775 L12 4 L22 20 Z' fill='#ffc800'/>", "its d, in the quotes it had");
    // Curves: a smooth anchor, a bent side, a handle set and taken off.
    let curved = edit(&mut s, "N2", r#"[{"op":"delete","anchors":["A11"]},{"op":"move","anchor":"A12","to":[4,12]},{"op":"smooth","anchor":"A12"},{"op":"bend","after":"A2","through":[20,8]},{"op":"handles","anchor":"A3","in":[0,-4]}]"#);
    assert_eq!(
        listed(&curved),
        [
            "  Run 1 of 1, closed, 4 anchors:",
            "    A1 at 2,20  then a curve to A12",
            "    A12 at 4,12  in -1.457,2.331  out 1.999,-3.198  then a curve to A2",
            "    A2 at 12,4  out 7.333,0  then a curve to A3",
            "    A3 at 22,20  in 0,-4  then a line to A1",
        ]
    );
    let straight = edit(&mut s, "N2", r#"[{"op":"handles","anchor":"A3","in":null},{"op":"line","after":"A2"},{"op":"corner","anchors":["A12"]}]"#);
    assert_eq!(listed(&straight)[1..], ["    A1 at 2,20  then a line to A12", "    A12 at 4,12  then a line to A2", "    A2 at 12,4  then a line to A3", "    A3 at 22,20  then a line to A1"]);
    // Opened where an anchor is (the new one is its other end), turned
    // round, and joined up again.
    let opened = edit(&mut s, "N2", r#"[{"op":"break","anchor":"A2","as":"end"},{"op":"move","anchor":"@end","to":[13,3]},{"op":"reverse"}]"#);
    assert_eq!(listed(&opened), ["  Run 1 of 1, open, 5 anchors:", "    A13 at 13,3  then a line to A12", "    A12 at 4,12  then a line to A1", "    A1 at 2,20  then a line to A3", "    A3 at 22,20  then a line to A2", "    A2 at 12,4  (the end)", "New anchors: A13."]);
    assert_eq!(listed(&edit(&mut s, "N2", r#"[{"op":"join","a":"A2","b":"A13"}]"#))[0], "  Run 1 of 1, closed, 5 anchors:");
    assert_eq!(listed(&edit(&mut s, "N2", r#"[{"op":"break","anchor":"A1"},{"op":"close","anchor":"A3"}]"#))[0], "  Run 1 of 1, closed, 5 anchors:", "the anchor a break made, closed over again, was never there");
    assert_eq!(text(&edit(&mut s, "N2", r#"[{"op":"move","anchor":"A1","by":[0,0]}]"#)), "Nothing changed: the path was like that already.");
    // A shape is made a path by its first edit; each call is one step.
    let rounded = edit(&mut s, "N3", r#"[{"op":"move","anchors":["A15","A16"],"by":[0,-1]}]"#);
    assert_eq!(listed(&rounded)[..3], ["  Run 1 of 1, closed, 8 anchors:", "    A15 at 4,1  then a line to A16", "    A16 at 8,1  then an arc of radii 2 2 to A17"]);
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)), "Undid 1 step: \"path_edit\" (Claude). Now 7 can be undone and 1 redone.");
    assert_eq!(node(&mut s, "N3"), "<rect id='box' x='2' y='2' width='8' height='6' rx='2'/>");
    // In a batch, on what an earlier step made.
    let steps = r#"[{"tool":"node_add","args":{"element":"path","attrs":{"d":"M2 2 H10"}},"as":"rule"},{"tool":"path_edit","args":{"node_id":"@rule","edits":[{"op":"add","after":"A23","as":"mid"},{"op":"move","anchor":"@mid","by":[0,3]}]}}]"#;
    ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","preview":false,"steps":{steps}}}"#));
    assert_eq!(node(&mut s, "N7"), "<path d=\"M2 2 L6 5 L10 2\"/>");
}

#[test]
fn a_slip_in_an_edit_says_which_and_how_to_fix_it() {
    let mut s = drawing("slips");
    for (node, edits, says) in [
        ("N2", r#"[{"op":"add","after":"A1","at":0.3}]"#, "edit 1: add takes after, share, near, not \"at\""),
        ("N2", r#"[{"op":"wiggle"}]"#, "edit 1: there's no edit \"wiggle\": one of move, handles, add, delete, bend, line, smooth, corner, close, break, join, reverse"),
        ("N2", r#"[{"anchor":"A1"}]"#, "edit 1: needs an \"op\": one of move, handles, add, delete, bend, line, smooth, corner, close, break, join, reverse"),
        ("N2", r#"[{"op":"move","anchor":"A1","by":[1,1]},{"op":"delete","anchors":["A99"]}]"#, "edit 2: this path has no anchor A99 (node_info lists the ones it has)"),
        ("N2", r#"[{"op":"delete","anchors":["A4"]}]"#, "this path has no anchor A4 (node_info lists the ones it has)"),
        ("N2", r#"[{"op":"move","anchor":"@nope","by":[1,1]}]"#, "edit 1: no earlier edit of this call is named \"nope\" (an add or a break names its anchor with \"as\")"),
        ("N2", r#"[{"op":"move","anchor":"A1","by":[1,1],"as":"x"}]"#, "edit 1 makes no anchor to name \"x\": add and break do"),
        ("N2", r#"[{"op":"add","after":"A1","as":"x"},{"op":"add","after":"A2","as":"x"}]"#, "edit 2: the name \"x\" is already taken in this call"),
        ("N2", r#"[{"op":"move","anchors":["A1","A2"],"to":[1,1]}]"#, "edit 1: \"to\" puts one anchor at a place: name one, or move several \"by\" [dx, dy]"),
        ("N2", r#"[{"op":"move","anchor":"A1"}]"#, "edit 1: move needs \"by\": [dx, dy] (or \"to\": [x, y], for one anchor)"),
        ("N2", r#"[{"op":"move","by":[1,1]}]"#, "edit 1: move needs \"anchors\": a list of anchor ids like [\"A3\"]"),
        ("N2", r#"[{"op":"move","anchor":"N2","by":[1,1]}]"#, "edit 1: \"N2\" isn't an anchor id like \"A3\""),
        ("N2", r#"[{"op":"move","anchor":"A1","by":[1]}]"#, "edit 1: \"by\" should be [x, y]"),
        ("N2", r#"[{"op":"handles","anchor":"A1"}]"#, "edit 1: handles needs \"in\" or \"out\": [dx, dy] from the anchor, or null to take it off"),
        ("N2", r#"[{"op":"add","after":"A1","share":1.5}]"#, "edit 1: \"share\" is how far along the segment, from 0 to 1"),
        ("N2", r#"[{"op":"join","a":"A1","b":"A3"}]"#, "A1 isn't an end of an open run: only two loose ends can be joined"),
        ("N2", "[]", "\"edits\" holds 0: give 1 to 200 of them"),
        ("N2", "[7]", "edit 1 should be an object {op, …}"),
        ("N5", r#"[{"op":"reverse"}]"#, "N5 is a <g>: only a shape (a rect, a circle, an ellipse, a line, a polyline, a polygon) can be made into a path"),
        ("N6", r#"[{"op":"reverse"}]"#, "N6's path data can't all be read, so it can't be taken point by point: set its d to path data that reads first"),
        ("N99", r#"[{"op":"reverse"}]"#, "no node N99 in this document (doc_info lists its nodes)"),
    ] {
        assert_eq!(refused(&mut s, "path_edit", &format!(r#"{{"doc_id":"d1","node_id":"{node}","edits":{edits}}}"#)), says);
    }
    assert!(text(&ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#)).contains("Undo: 1 (latest: \"node_add_svg\" by Claude). Redo: 0."), "nothing of a refused edit is left");
    assert_eq!(node(&mut s, "N2"), "<path id='tri' d='M2 20 L12 4 L22 20 Z' fill='#ffc800'/>");
}

#[test]
fn an_outline_is_set_whole() {
    let mut s = drawing("set");
    let set = |s: &mut Server<Ink>, node: &str, rest: &str| ok(s, "path_set", &format!(r#"{{"doc_id":"d1","node_id":"{node}",{rest}}}"#));
    // Points joined by lines; the path keeps everything else it had.
    let kite = set(&mut s, "N2", r#""points":[[12,2],[20,12],[12,22],[4,12]],"closed":true"#);
    assert_eq!(listed(&kite), ["  Run 1 of 1, closed, 4 anchors:", "    A11 at 12,2  then a line to A12", "    A12 at 20,12  then a line to A13", "    A13 at 12,22  then a line to A14", "    A14 at 4,12  then a line to A11", "New anchors: A11, A12, A13, A14."]);
    assert_eq!(node(&mut s, "N2"), "<path id='tri' d='M12 2 L20 12 L12 22 L4 12 Z' fill='#ffc800'/>");
    // One flowing curve through them: each leaves along the line from
    // the point before it to the point after.
    let blob = set(&mut s, "N2", r#""points":[[12,2],[20,12],[12,22],[4,12]],"closed":true,"smooth":true"#);
    assert_eq!(listed(&blob)[1], "    A15 at 12,2  in -2.667,0  out 2.667,0  then a curve to A16");
    // Anchors with their handles; one given an id is that anchor still.
    let leaf = set(&mut s, "N2", r#""anchors":[{"at":[12,3],"out":[6,0],"id":"A15"},{"at":[12,21],"in":[6,0],"out":[-6,0]},{"at":[12,3],"in":[-6,0]}]"#);
    assert_eq!(listed(&leaf), ["  Run 1 of 1, open, 3 anchors:", "    A15 at 12,3  out 6,0  then a curve to A19", "    A19 at 12,21  in 6,0  out -6,0  then a curve to A20", "    A20 at 12,3  in -6,0  (the end)", "New anchors: A19, A20."]);
    assert_eq!(node(&mut s, "N2"), "<path id='tri' d='M12 3 C18 3 18 21 12 21 C6 21 6 3 12 3' fill='#ffc800'/>");
    // Closed, a last anchor on the first is the first: the curve that
    // came home is what closes it, and no anchor is made to be lost.
    let closed = set(&mut s, "N2", r#""anchors":[{"at":[12,3],"out":[6,0],"id":"A15"},{"at":[12,21],"in":[6,0],"out":[-6,0],"id":"A19"},{"at":[12,3],"in":[-6,0]}],"closed":true"#);
    assert_eq!(listed(&closed), ["  Run 1 of 1, closed, 2 anchors:", "    A15 at 12,3  in -6,0  out 6,0  then a curve to A19", "    A19 at 12,21  in 6,0  out -6,0  then a curve to A15"]);
    assert_eq!(node(&mut s, "N2"), "<path id='tri' d='M12 3 C18 3 18 21 12 21 C6 21 6 3 12 3 Z' fill='#ffc800'/>");
    // A shape is made a path to take it; the path of two runs has one.
    assert_eq!(listed(&set(&mut s, "N3", r#""points":[[2,2],[10,8]]"#))[0], "  Run 1 of 1, open, 2 anchors:");
    assert_eq!(node(&mut s, "N3"), "<path id='box' d='M2 2 L10 8'/>", "its d where its numbers were, in their quotes");
    assert_eq!(listed(&set(&mut s, "N4", r#""points":[[1,1]]"#)), ["  Run 1 of 1, open, 1 anchor:", "    A31 at 1,1  (the end)", "New anchors: A31."]);
    assert_eq!(text(&set(&mut s, "N4", r#""anchors":[{"at":[1,1],"id":"A31"}]"#)), "Nothing changed: the path was like that already.");
    for (rest, says) in [
        (r#""points":[[1,1]],"anchors":[]"#, "give points or anchors, not both"),
        (r#""closed":true"#, "say what the outline is: points (joined by lines) or anchors (with handles)"),
        (r#""points":[]"#, "an outline needs at least one point"),
        (r#""points":[[1,1],[2]]"#, "each point should be [x, y]"),
        (r#""anchors":[{"at":[1,1]}],"smooth":true"#, "smooth goes with points: anchors say their own handles"),
        (r#""anchors":[{"in":[1,1]}]"#, "each anchor needs an \"at\": [x, y]"),
        (r#""anchors":[{"at":[1,1],"handle":[1,1]}]"#, "an anchor has at, in, out and id, not \"handle\""),
        (r#""anchors":[{"at":[1,1],"id":"A77"}]"#, "this path has no anchor A77 to keep (node_info lists the ones it has)"),
        (r#""anchors":[{"at":[1,1],"id":"A31"},{"at":[2,2],"id":"A31"}]"#, "A31 is named twice: an anchor is in one place"),
        (r#""anchors":[{"at":[1,1],"id":"N4"}]"#, "an anchor's \"id\" should be an anchor id like \"A3\""),
    ] {
        assert_eq!(refused(&mut s, "path_set", &format!(r#"{{"doc_id":"d1","node_id":"N4",{rest}}}"#)), says);
    }
    assert_eq!(refused(&mut s, "path_set", r#"{"doc_id":"d1","node_id":"N5","points":[[1,1]]}"#), "N5 is a <g>: only a shape (a rect, a circle, an ellipse, a line, a polyline, a polygon) can be made into a path");
}

#[test]
fn shapes_become_paths_and_paths_turn_round() {
    let mut s = drawing("op");
    ok(&mut s, "node_add_svg", r#"{"doc_id":"d1","svg":"<circle cx='12' cy='12' r='4' stroke='#fff'/>"}"#);
    let made = ok(&mut s, "path_op", r#"{"doc_id":"d1","node_ids":["N3","N7","N2"],"op":"to_path"}"#);
    assert_eq!(text(&made), "Done: N3 <path id=\"box\">, N7 <path> (node_info lists a path's anchors).", "the path was one already");
    assert_eq!(made.path("structuredContent.node_ids").map(lntrn_data::json::write).as_deref(), Some(r#"["N3","N7"]"#));
    assert_eq!(node(&mut s, "N3"), "<path id='box' d='M4 2 H8 A2 2 0 0 1 10 4 V6 A2 2 0 0 1 8 8 H4 A2 2 0 0 1 2 6 V4 A2 2 0 0 1 4 2 Z'/>", "a rounded corner is an arc still");
    assert_eq!(node(&mut s, "N7"), "<path d='M16 12 A4 4 0 0 1 12 16 A4 4 0 0 1 8 12 A4 4 0 0 1 12 8 A4 4 0 0 1 16 12 Z' stroke='#fff'/>");
    assert_eq!(text(&ok(&mut s, "path_op", r#"{"doc_id":"d1","node_ids":["N3"],"op":"to_path"}"#)), "Nothing changed: they were like that already.");
    // Each run the other way, from where it started.
    ok(&mut s, "path_op", r#"{"doc_id":"d1","node_ids":["N2","N4"],"op":"reverse"}"#);
    assert_eq!(node(&mut s, "N2"), "<path id='tri' d='M2 20 H22 L12 4 Z' fill='#ffc800'/>");
    assert_eq!(node(&mut s, "N4"), "<path id='two' d='M1 1 L5 5 V1 Z M14 14 C14 12 12 10 10 10'/>");
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)), "Undid 1 step: \"path_op\" (Claude). Now 3 can be undone and 1 redone.");
    assert_eq!(node(&mut s, "N4"), "<path id='two' d='M1 1h4v4zM10 10c2 0 4 2 4 4'/>", "to the byte");
    for (args, says) in [
        (r#""node_ids":["N3","N5"],"op":"to_path""#, "N5 is a <g>: only a shape (a rect, a circle, an ellipse, a line, a polyline, a polygon) can be made into a path"),
        (r#""node_ids":["N5"],"op":"reverse""#, "N5 is a <g>, which has no direction to turn round: only a path does (to_path makes a shape one)"),
        (r#""node_ids":["N6"],"op":"reverse""#, "N6's path data can't all be read, so it can't be taken point by point: set its d to path data that reads first"),
        (r#""node_ids":[],"op":"reverse""#, "\"node_ids\" is empty: name at least one node"),
        (r#""node_ids":["N2"],"op":"weld""#, "op is to_path, reverse, union, subtract, intersect or exclude, not \"weld\""),
    ] {
        assert_eq!(refused(&mut s, "path_op", &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
}

#[test]
fn shapes_are_made_one() {
    let (mut s, _) = server("boolean");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect id='card' x='2' y='2' width='14' height='14' rx='2' fill='#ffc800'/><circle id='bite' cx='16' cy='16' r='5'/><g transform='translate(1 1)'><rect id='bar' x='0' y='6' width='22' height='2'/></g><g id='g'/>"}"##);
    let before = text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1"}"#)).to_owned();
    // A bite out of the card: the card keeps its paint and its corners,
    // and takes the circle's arc; the circle is gone.
    let bitten = ok(&mut s, "path_op", r#"{"doc_id":"d1","node_ids":["N2","N3"],"op":"subtract"}"#);
    assert_eq!(text(&bitten), "Done: N2 <path id=\"card\"> is the result at 2,2 14×14; N3 was taken into it and deleted.");
    assert_eq!((bitten.path("structuredContent.node_ids[0]").and_then(Doc::as_str), bitten.path("structuredContent.removed[0]").and_then(Doc::as_str)), (Some("N2"), Some("N3")));
    assert_eq!(node(&mut s, "N2"), "<path id='card' d='M4 2 H14 A2 2 0 0 1 16 4 V11 A5 5 0 0 0 11 16 H4 A2 2 0 0 1 2 14 V4 A2 2 0 0 1 4 2 Z' fill='#ffc800'/>");
    assert_eq!(refused(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N3"}"#), "no node N3 in this document (doc_info lists its nodes)");
    // It draws: yellow in the card, nothing where the bite was.
    let (_, image) = picture(&ok(&mut s, "doc_preview", r#"{"doc_id":"d1","max_edge":96,"background":"none"}"#));
    assert_eq!((image.pixel(24, 24), image.pixel(60, 60)[3]), ([255, 200, 0, 255], 0));
    // With a shape under a group's move: where it shows is what counts.
    let barred = ok(&mut s, "path_op", r#"{"doc_id":"d1","node_ids":["N2","N5"],"op":"exclude"}"#);
    assert_eq!(text(&barred), "Done: N2 <path id=\"card\"> is the result at 1,2 22×14; N5 was taken into it and deleted.");
    assert!(node(&mut s, "N2").contains("M16 9 V7 H23 V9 Z"), "the bar's end, past the card, a unit to the right of its own numbers");
    // One step each, and back to the byte.
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1","steps":2}"#)), "Undid 2 steps: \"path_op\" (Claude), \"path_op\" (Claude). Now 1 can be undone and 2 redone.");
    assert_eq!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1"}"#)), before);
    // Three at once, the one kept in a group: written in its coordinates.
    let all = ok(&mut s, "path_op", r#"{"doc_id":"d1","node_ids":["N5","N2","N3"],"op":"union"}"#);
    assert_eq!(text(&all), "Done: N5 <path id=\"bar\"> is the result at 1,2 22×19; N2, N3 were taken into it and deleted.");
    assert_eq!(node(&mut s, "N5"), "<path id='bar' d='M0 6 H1 V3 A2 2 0 0 1 3 1 H13 A2 2 0 0 1 15 3 V6 H22 V8 H15 V10 A5 5 0 0 1 20 15 A5 5 0 0 1 15 20 A5 5 0 0 1 10 15 H3 A2 2 0 0 1 1 13 V8 H0 Z'/>");
    ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#);
    for (args, says) in [
        (r#""node_ids":["N3","N5"],"op":"intersect""#, "the shapes don't overlap anywhere, so nothing would be left: nothing was changed"),
        (r#""node_ids":["N3","N6"],"op":"union""#, "N6 is a <g>: only shapes (paths, rects, circles, ellipses, polygons) can be combined; for a group, name the shapes in it"),
        (r#""node_ids":["N3"],"op":"subtract""#, "that takes two shapes or more: the first is kept, and the others are taken from it, or met with it (a union of one shape makes its outline simple, where it crosses itself)"),
        (r#""node_ids":["N3","N3"],"op":"union""#, "N3 is named twice: a shape is combined with others, not with itself"),
    ] {
        assert_eq!(refused(&mut s, "path_op", &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
    // In a batch, on what its steps made: what one step takes into
    // another isn't told of as new.
    let steps = r#"[{"tool":"node_add","args":{"element":"circle","attrs":{"cx":4,"cy":4,"r":3}},"as":"a"},{"tool":"node_add","args":{"element":"circle","attrs":{"cx":7,"cy":4,"r":3}},"as":"b"},{"tool":"path_op","args":{"node_ids":["@a","@b"],"op":"intersect"}}]"#;
    let ran = ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","preview":false,"steps":{steps}}}"#));
    assert_eq!(text(&ran), "Ran 3 steps on d1 as one undo step (history_undo undoes all of them). New nodes: N7. Named: @a = N7. Made and taken out again on the way: N8.");
    assert_eq!(node(&mut s, "N7"), "<path d=\"M7 4 A3 3 0 0 1 5.5 6.598 A3 3 0 0 1 4 4 A3 3 0 0 1 5.5 1.402 A3 3 0 0 1 7 4 Z\"/>", "a lens of four arcs");
}
