//! Asking about a drawing, and setting its page, over the wire
//! (ARCHITECTURE §10): `node_info`, `doc_query` and `doc_set`.

mod common;

use common::{ok, refused, server, text};
use lntrn_data::Doc;

const DRAWING: &str = r##"<defs><linearGradient id='glow'><stop offset='0' stop-color='#fff'/></linearGradient><clipPath id='left'><rect width='12' height='24'/></clipPath></defs><g id='lamp' transform='translate(2 3)' stroke='#12100e' stroke-width='2'><rect id='pane' x='4' y='4' width='8' height='8' fill='url(#glow)' clip-path='url(#left)' style='stroke: url(#nothing)'/><text id='label' font-family='Ink Test' font-size='10'>hi</text></g><circle id='dot' cx='20' cy='20' r='2' stroke='#fff' stroke-width='2'/><use href='#pane'/>"##;

#[test]
fn a_node_says_everything_about_itself() {
    let (mut s, _) = server("info");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", &format!(r#"{{"doc_id":"d1","svg":"{}"}}"#, DRAWING.replace('"', "\\\"")));
    let info = ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N8"}"#);
    assert_eq!(
        text(&info).lines().collect::<Vec<_>>(),
        [
            "N8 <rect id=\"pane\">, in N7 <g id=\"lamp\">; 0 inside.",
            "Attributes: id=\"pane\" x=\"4\" y=\"4\" width=\"8\" height=\"8\" fill=\"url(#glow)\" clip-path=\"url(#left)\" style=\"stroke: url(#nothing)\"",
            "From the groups above: stroke-width=\"2\" (N7)",
            "Shows at 6,7 8×8 in the drawing's coordinates (strokes aside).",
            "Its own numbers are under, nearest first: N7 transform=\"translate(2 3)\".",
            "Uses: fill → N3 <linearGradient id=\"glow\">; clip-path → N5 <clipPath id=\"left\">; style → nothing (no element has id=\"nothing\").",
            "Used by: N11 <use>.",
            "No anchors of its own: path_edit (or path_op to_path) makes it a path that has.",
        ]
    );
    assert_eq!((info.path("structuredContent.parent").and_then(Doc::as_str), info.path("structuredContent.attrs.x").and_then(Doc::as_str), info.path("structuredContent.uses[1]").and_then(Doc::as_str)), (Some("N7"), Some("4"), Some("N5")));
    // What shows nowhere says why; the root is the root.
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N3"}"#)).contains("\nShows nowhere itself: it's something others use, or words about the picture.\nUsed by: N8 <rect id=\"pane\">."));
    // A text shows where its glyphs are (the test font's h and i at
    // size 10, through the group's move).
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N9"}"#)).contains("From the groups above: stroke=\"#12100e\" (N7), stroke-width=\"2\" (N7)\nShows at 3,-4 7×7 in the drawing's coordinates (strokes aside)."));
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N11"}"#)).contains("\nShows nowhere (not drawn yet)."));
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N1"}"#)).starts_with("N1 <svg>, the drawing's root; 10 inside."));
    // A long value is cut, and says where to read it whole.
    ok(&mut s, "node_add", &format!(r#"{{"doc_id":"d1","element":"path","attrs":{{"d":"M0 0{}"}}}}"#, " L1 1".repeat(60)));
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N12"}"#)).contains("L1 1 … (304 characters: doc_source shows all)\""));
    assert_eq!(refused(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N99"}"#), "no node N99 in this document (doc_info lists its nodes)");
}

#[test]
fn a_point_says_what_is_drawn_there() {
    let (mut s, _) = server("query");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", &format!(r#"{{"doc_id":"d1","svg":"{}"}}"#, DRAWING.replace('"', "\\\"")));
    ok(&mut s, "node_add", r##"{"doc_id":"d1","element":"rect","attrs":{"width":24,"height":24,"fill":"#201a14"},"at":"bottom"}"##);
    let at = |s: &mut _, x: f64, y: f64| text(&ok(s, "doc_query", &format!(r#"{{"doc_id":"d1","point":[{x},{y}]}}"#))).to_owned();
    // Through the group's move, over the background.
    assert_eq!(at(&mut s, 8.0, 9.0), "At 8,9, front to back: N8 <rect id=\"pane\"> (its fill), in N7; N12 <rect> (its fill).");
    // A stroke is hit within half its width of the outline; one with
    // nothing to paint it isn't there.
    assert_eq!(at(&mut s, 22.5, 20.0), "At 22.5,20, front to back: N10 <circle id=\"dot\"> (its stroke); N12 <rect> (its fill).");
    assert_eq!(at(&mut s, 5.5, 12.0), "At 5.5,12, front to back: N12 <rect> (its fill).");
    // Where its clip path cuts it away (past x = 12 in its own numbers).
    assert_eq!(at(&mut s, 14.5, 9.0), "At 14.5,9, front to back: N12 <rect> (its fill).");
    assert_eq!(at(&mut s, 20.0, 20.0), "At 20,20, front to back: N10 <circle id=\"dot\"> (its fill); N12 <rect> (its fill).");
    assert_eq!(at(&mut s, 30.0, 30.0), "Nothing is drawn at 30,30.");
    let hit = ok(&mut s, "doc_query", r#"{"doc_id":"d1","point":[8,9]}"#);
    assert_eq!(hit.path("structuredContent.node_ids[0]").and_then(Doc::as_str), Some("N8"));
    assert_eq!(refused(&mut s, "doc_query", r#"{"doc_id":"d1","point":[1]}"#), "\"point\" should be [x, y]");
}

#[test]
fn the_page_is_set_and_the_drawing_fitted_to_it() {
    let (mut s, _) = server("page");
    ok(&mut s, "doc_new", r#"{"width":500}"#);
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect x='100' y='100' width='300' height='300' rx='50' stroke='#000' stroke-width='25'/>"}"##);
    let source = |s: &mut _| text(&ok(s, "doc_source", r#"{"doc_id":"d1"}"#)).to_owned();
    // The page alone: nothing in the drawing moves.
    assert_eq!(text(&ok(&mut s, "doc_set", r#"{"doc_id":"d1","width":48,"height":48}"#)), "Set. Page: 48 × 48, viewBox 0 0 500 500; numbers to 3 decimals.");
    assert!(source(&mut s).contains("width=\"48\" height=\"48\" viewBox=\"0 0 500 500\">\n  <rect x='100' y='100' width='300' height='300' rx='50' stroke='#000' stroke-width='25'/>"));
    // A new viewBox with the drawing fitted to it: a 500-unit drawing
    // becomes a 24-unit one, its stroke with it.
    let fitted = ok(&mut s, "doc_set", r#"{"doc_id":"d1","view_box":[0,0,24,24],"content":"fit"}"#);
    assert_eq!(text(&fitted), "Set. Page: 48 × 48, viewBox 0 0 24 24; numbers to 3 decimals. 1 node was fitted to the new coordinates.");
    assert!(source(&mut s).contains("viewBox=\"0 0 24 24\">\n  <rect x='4.8' y='4.8' width='14.4' height='14.4' rx='2.4' stroke='#000' stroke-width='1.2'/>"), "{}", source(&mut s));
    // How finely its numbers are written, in Ink's own attribute.
    assert_eq!(text(&ok(&mut s, "doc_set", r#"{"doc_id":"d1","decimals":1}"#)), "Set. Page: 48 × 48, viewBox 0 0 24 24; numbers to 1 decimals.");
    assert!(source(&mut s).contains(" ink:decimals=\"1\">"));
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N2"],"move":[0.26,0]}"#);
    assert!(source(&mut s).contains("<rect x='5.1' y='4.8'"), "{}", source(&mut s));
    ok(&mut s, "node_set", r#"{"doc_id":"d1","node_id":"N2","attrs":{"y":1.2345}}"#);
    assert!(source(&mut s).contains("<rect x='5.1' y='1.2'"));
    assert_eq!(text(&ok(&mut s, "doc_set", r#"{"doc_id":"d1","decimals":1}"#)), "Nothing changed: the page was like that already.");
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1","steps":3}"#)), "Undid 3 steps: \"node_set\" (Claude), \"node_transform\" (Claude), \"doc_set\" (Claude). Now 3 can be undone and 3 redone.");
    for (args, says) in [
        ("", "say what to set: width, height, view_box or decimals"),
        (r#","view_box":[0,0,24]"#, "\"view_box\" should be [x, y, width, height], its width and height more than nothing"),
        (r#","view_box":[0,0,0,24]"#, "\"view_box\" should be [x, y, width, height], its width and height more than nothing"),
        (r#","content":"fit""#, "content: \"fit\" goes with a view_box to fit the content to"),
        (r#","decimals":12"#, "\"decimals\" should be a whole number from 0 to 8"),
    ] {
        assert_eq!(refused(&mut s, "doc_set", &format!(r#"{{"doc_id":"d1"{args}}}"#)), says);
    }
    // A drawing from elsewhere gets Ink's namespace when it first needs it.
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ink-transcripts/page");
    std::fs::write(dir.join("plain.svg"), "<svg viewBox=\"0 0 8 8\"/>").unwrap();
    ok(&mut s, "doc_open", r#"{"path":"plain.svg"}"#);
    ok(&mut s, "doc_set", r#"{"doc_id":"d2","decimals":2}"#);
    assert_eq!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d2"}"#)), "<svg viewBox=\"0 0 8 8\" xmlns:ink=\"urn:lantern:ink\" ink:decimals=\"2\"/>");
}

/// Set in the tests' own font: at size 10 a capital is a box 4 by 7
/// standing on the line a unit in from its pen, and an `o` a ring 5
/// across.
#[test]
fn a_text_says_its_words_and_shows_where_its_glyphs_are() {
    let (mut s, _) = server("text");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<g font-family='Ink Test' font-size='10'><text id='label' x='2' y='12' fill='#223'>\n  Hop <tspan fill='#c33'>on</tspan>\n</text><text x='1 2'>no</text><text> </text></g>"}"##);
    let listed = text(&ok(&mut s, "doc_info", r#"{"doc_id":"d1"}"#)).to_owned();
    for line in [
        "    N3 text #label \"Hop on\"  fill #223  at 3,5 31×9",
        "      N4 tspan  fill #c33  (drawn as part of its <text>)",
        "    N5 text \"no\"  (not drawn: its characters are placed or turned one by one (rotate, or a list in x, y, dx or dy))",
        "    N6 text \"\"",
    ] {
        assert!(listed.lines().any(|l| l == line), "no line {line:?} in:\n{listed}");
    }
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N3"}"#)).contains("\nShows at 3,5 31×9 in the drawing's coordinates (strokes aside)."));
    assert!(text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N6"}"#)).contains("\nShows nowhere: it draws nothing as it is."));
    let at = |s: &mut _, x: f64, y: f64| text(&ok(s, "doc_query", &format!(r#"{{"doc_id":"d1","point":[{x},{y}]}}"#))).to_owned();
    assert_eq!(at(&mut s, 5.0, 8.0), "At 5,8, front to back: N3 <text id=\"label\"> (its fill), in N2.");
    assert_eq!(at(&mut s, 26.0, 8.5), "Nothing is drawn at 26,8.5.", "the hole in the o");
    assert_eq!(at(&mut s, 24.0, 8.5), "At 24,8.5, front to back: N3 <text id=\"label\"> (its fill), in N2.", "a span's glyphs are its text's");
    // A text lines up by its box like anything else.
    ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N3"],"x":"left","to":"page"}"#);
    assert_eq!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1","node_id":"N3"}"#)).lines().next(), Some("<text id='label' x='2' y='12' fill='#223' transform=\"translate(-3 0)\">"));
}
