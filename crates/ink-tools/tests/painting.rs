//! The tools that say how things are painted, over the wire
//! (ARCHITECTURE §10). What each writes into the file is `ink-doc`'s to
//! test; these are the calls, their replies and their refusals.

mod common;

use common::{data, ok, picture, refused, server, text};
use ink_tools::Ink;
use lntrn_mcp::Server;

fn node(s: &mut Server<Ink>, id: &str) -> String {
    text(&ok(s, "doc_source", &format!(r#"{{"doc_id":"d1","node_id":"{id}"}}"#))).to_owned()
}

#[test]
fn a_style_is_set_where_it_will_show() {
    let (mut s, _) = server("style");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<style>.rule { fill: #201a14 }</style><rect id='plain' width='8' height='8'/><rect id='styled' width='8' height='8' style='fill: red; opacity: 0.5'/><rect id='ruled' class='rule' width='8' height='8'/><g id='all'><path d='M0 0H8'/></g>"}"##);
    // One call, three nodes, each written where it has (or needs) it.
    let styled = ok(&mut s, "node_style", r##"{"doc_id":"d1","node_ids":["N3","N4","N5"],"style":{"fill":"#ffc800","stroke-width":1.50004,"opacity":null}}"##);
    assert_eq!(text(&styled), "Styled. Now: N3 <rect id=\"plain\">: fill #ffc800, stroke-width 1.5; N4 <rect id=\"styled\">: fill #ffc800, stroke-width 1.5; N5 <rect id=\"ruled\">: fill #ffc800, stroke-width 1.5.");
    assert_eq!(node(&mut s, "N3"), "<rect id='plain' width='8' height='8' fill=\"#ffc800\" stroke-width=\"1.5\"/>");
    assert_eq!(node(&mut s, "N4"), "<rect id='styled' width='8' height='8' style='fill: #ffc800' stroke-width=\"1.5\"/>");
    assert_eq!(node(&mut s, "N5"), "<rect id='ruled' class='rule' width='8' height='8' style=\"fill: #ffc800\" stroke-width=\"1.5\"/>", "in its style, to outvote the rule");
    // Where node_set writes an attribute that won't show, and says so.
    let set = ok(&mut s, "node_set", r##"{"doc_id":"d1","node_id":"N5","attrs":{"fill":"#00f"}}"##);
    assert!(text(&set).contains("(or a <style> rule) also sets fill"), "{}", text(&set));
    // On a group: what the shapes in it draw with.
    assert_eq!(text(&ok(&mut s, "node_style", r##"{"doc_id":"d1","node_ids":["N6"],"style":{"stroke":"#12100e","stroke-linecap":"round"}}"##)), "Styled. Now: N6 <g id=\"all\">: stroke #12100e.");
    assert_eq!(text(&ok(&mut s, "node_style", r##"{"doc_id":"d1","node_ids":["N6"],"style":{"stroke":"#12100e"}}"##)), "Nothing changed: they were painted that way already.");
    assert_eq!(text(&ok(&mut s, "node_style", r##"{"doc_id":"d1","node_ids":["N6"],"style":{"stroke":null,"stroke-linecap":null}}"##)), "Styled. Now: N6 <g id=\"all\">: no fill, stroke or opacity of its own.");
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)), "Undid 1 step: \"node_style\" (Claude). Now 4 can be undone and 1 redone.");
    // A slip is refused, saying what it should have been.
    for (style, says) in [
        (r#"{"fil":"red"}"#, "there's no property \"fil\" (did you mean \"fill\"?)"),
        (r#"{"fill":"blurple"}"#, "fill can't be \"blurple\": it takes a colour (\"#rrggbb\", a name), \"none\", or \"url(#id)\" for a gradient"),
        (r#"{"stroke-width":-2}"#, "stroke-width can't be \"-2\": it takes a length that isn't less than nothing"),
        (r#"{"opacity":[1]}"#, "the attribute \"opacity\" should be a string or a number (or null, to take it off)"),
        ("{}", "\"style\" is empty: name at least one property to set"),
    ] {
        assert_eq!(refused(&mut s, "node_style", &format!(r#"{{"doc_id":"d1","node_ids":["N3"],"style":{style}}}"#)), says);
    }
    assert_eq!(refused(&mut s, "node_style", r#"{"doc_id":"d1","node_ids":["N99"],"style":{"fill":"red"}}"#), "no node N99 in this document (doc_info lists its nodes)");
    // In a batch, on what an earlier step made.
    let steps = r##"[{"tool":"node_add","args":{"element":"circle","attrs":{"r":2}},"as":"dot"},{"tool":"node_style","args":{"node_ids":["@dot"],"style":{"fill":"none","stroke":"#fff"}}}]"##;
    ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","preview":false,"steps":{steps}}}"#));
    assert_eq!(node(&mut s, "N8"), "<circle r=\"2\" fill=\"none\" stroke=\"#fff\"/>");
}

#[test]
fn a_gradient_is_made_painted_with_and_changed() {
    let (mut s, _) = server("gradients");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect id='pane' x='4' y='2' width='16' height='20' rx='3'/><circle id='dot' cx='12' cy='12' r='4' style='fill: red'/>"}"##);
    let source = |s: &mut Server<Ink>| text(&ok(s, "doc_source", r#"{"doc_id":"d1"}"#)).lines().skip(1).map(str::to_owned).collect::<Vec<_>>().join("\n");
    // Colours spread evenly, top to bottom of whatever it paints; the
    // <defs> is made for it, under everything.
    let made = ok(&mut s, "gradient_add", r##"{"doc_id":"d1","colors":["#ffe9a8","#ffb000"],"from":[0,0],"to":[0,1],"id":"glow","fill":["N2"]}"##);
    assert_eq!(text(&made), "Made N5 <linearGradient id=\"glow\"> with 2 stops: paint with it as \"url(#glow)\". Painted with it: N2.");
    assert_eq!((data(&made, "node_id"), data(&made, "id")), ("N5", "glow"));
    assert_eq!(
        source(&mut s),
        "  <defs>\n    <linearGradient id=\"glow\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\n      <stop offset=\"0\" stop-color=\"#ffe9a8\"/>\n      <stop offset=\"1\" stop-color=\"#ffb000\"/>\n    </linearGradient>\n  </defs>\n  <rect id='pane' x='4' y='2' width='16' height='20' rx='3' fill=\"url(#glow)\"/>\n  <circle id='dot' cx='12' cy='12' r='4' style='fill: red'/>\n</svg>"
    );
    // A second one, radial, with placed stops, in the shape's own
    // coordinates; a name that's taken is made different; its fill goes
    // where the node says its fill (D14).
    let ring = ok(&mut s, "gradient_add", r##"{"doc_id":"d1","kind":"radial","stops":[{"offset":0,"color":"white"},{"offset":0.5,"color":"#ff2e5b","opacity":0.5},{"offset":1,"color":"#ff2e5b","opacity":0}],"center":[12,12],"radius":4,"focus":[11,11],"units":"user","spread":"reflect","id":"glow","fill":["N3"],"stroke":["N2"]}"##);
    assert_eq!(text(&ring), "Made N8 <radialGradient id=\"glow-2\"> with 3 stops: paint with it as \"url(#glow-2)\". Painted with it: N3, N2.");
    let after = source(&mut s);
    assert!(after.contains("    <radialGradient id=\"glow-2\" cx=\"12\" cy=\"12\" r=\"4\" fx=\"11\" fy=\"11\" gradientUnits=\"userSpaceOnUse\" spreadMethod=\"reflect\">\n      <stop offset=\"0\" stop-color=\"white\"/>\n      <stop offset=\"0.5\" stop-color=\"#ff2e5b\" stop-opacity=\"0.5\"/>\n      <stop offset=\"1\" stop-color=\"#ff2e5b\" stop-opacity=\"0\"/>\n    </radialGradient>\n  </defs>"), "{after}");
    assert!(after.contains("fill=\"url(#glow)\" stroke=\"url(#glow-2)\"/>\n  <circle id='dot' cx='12' cy='12' r='4' style='fill: url(#glow-2)'/>"), "{after}");
    // It draws: the pane's top is the first colour, its foot the last.
    let (_, image) = picture(&ok(&mut s, "doc_preview", r#"{"doc_id":"d1","max_edge":96}"#));
    let (top, foot) = (image.pixel(40, 12), image.pixel(40, 84));
    assert!(top[0] == 255 && top[1] > 225 && top[2] > 150 && foot[2] < 30, "{top:?} {foot:?}");
    // The dot's gradient is the pane's stroke too. One that's shared is
    // never touched: the dot keeps its move as a transform.
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N3"],"move":[2,-1]}"#);
    let shared = source(&mut s);
    assert!(shared.contains("<radialGradient id=\"glow-2\" cx=\"12\" cy=\"12\" r=\"4\" fx=\"11\" fy=\"11\"") && shared.contains("<circle id='dot' cx='12' cy='12' r='4' style='fill: url(#glow-2)' transform=\"translate(2 -1)\"/>"), "{shared}");
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)), "Undid 1 step: \"node_transform\" (Claude). Now 3 can be undone and 1 redone.");
    // Once it's the dot's alone, it goes where the dot goes.
    ok(&mut s, "node_style", r#"{"doc_id":"d1","node_ids":["N2"],"style":{"stroke":null}}"#);
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N3"],"move":[2,-1]}"#);
    let alone = source(&mut s);
    assert!(alone.contains("<radialGradient id=\"glow-2\" cx=\"14\" cy=\"11\" r=\"4\" fx=\"13\" fy=\"10\" gradientUnits") && alone.contains("<circle id='dot' cx='14' cy='11' r='4' style='fill: url(#glow-2)'/>"), "{alone}");

    // Changed: only what's given. New stops replace the old ones.
    let set = ok(&mut s, "gradient_set", r##"{"doc_id":"d1","node_id":"N5","to":[1,1],"colors":["#201a14","#5a4a3a","#ffe9a8"]}"##);
    assert_eq!(text(&set), "Set. N5 <linearGradient id=\"glow\"> is now: x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\" with 3 stops.");
    assert!(source(&mut s).contains("x2=\"1\" y2=\"1\">\n      <stop offset=\"0\" stop-color=\"#201a14\"/>\n      <stop offset=\"0.5\" stop-color=\"#5a4a3a\"/>\n      <stop offset=\"1\" stop-color=\"#ffe9a8\"/>\n    </linearGradient>"));
    let back = ok(&mut s, "gradient_set", r#"{"doc_id":"d1","node_id":"N8","units":"box","spread":"pad","center":[0.5,0.5],"radius":0.5}"#);
    assert_eq!(text(&back), "Set. N8 <radialGradient id=\"glow-2\"> is now: cx=\"0.5\" cy=\"0.5\" r=\"0.5\" fx=\"13\" fy=\"10\" with 3 stops.", "what goes without saying is taken off");
    assert_eq!(text(&ok(&mut s, "gradient_set", r#"{"doc_id":"d1","node_id":"N8","radius":0.5}"#)), "Nothing changed: it was like that already.");

    for (tool, args, says) in [
        ("gradient_add", r#""from":[0,0]"#, "say what colours it has: colors (spread evenly) or stops (each placed)"),
        ("gradient_add", r##""colors":["#fff","blurple"]"##, "\"blurple\" isn't a colour: give \"#rrggbb\", a name like \"gold\", or rgb(…)"),
        ("gradient_add", r##""colors":["#fff"],"stops":[]"##, "give colors or stops, not both"),
        ("gradient_add", r##""stops":[{"color":"#fff"}]"##, "each stop needs an \"offset\" from 0 to 1"),
        ("gradient_add", r##""colors":["#fff","#000"],"center":[1,1]"##, "a linear gradient runs from → to: it has no center, radius or focus"),
        ("gradient_add", r##""kind":"radial","colors":["#fff","#000"],"units":"user""##, "with units \"user\" say where it is: center and radius, in the painted shape's coordinates"),
        ("gradient_add", r##""colors":["#fff","#000"],"id":"my glow""##, "\"my glow\" can't be an id: letters, digits, - and _ only, so it can be written url(#…)"),
        ("gradient_add", r##""colors":["#fff","#000"],"fill":["N99"]"##, "no node N99 in this document (doc_info lists its nodes)"),
        ("gradient_set", r#""node_id":"N2","to":[1,1]"#, "N2 is a <rect>, not a gradient (doc_info lists them under <defs>)"),
        ("gradient_set", r#""node_id":"N5""#, "say what to change: colors or stops, from, to, center, radius, focus, units or spread"),
        ("gradient_set", r#""node_id":"N8","from":[0,0]"#, "a radial gradient has a center, a radius and a focus, not from and to"),
    ] {
        assert_eq!(refused(&mut s, tool, &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
    // With no name asked for, they're numbered; in a batch one is named
    // like anything a step makes.
    let steps = r##"[{"tool":"gradient_add","args":{"colors":["#fff","#000"]},"as":"g"},{"tool":"gradient_set","args":{"node_id":"@g","spread":"repeat"}}]"##;
    let ran = ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","preview":false,"steps":{steps}}}"#));
    assert!(text(&ran).contains("Named: @g = N"), "{}", text(&ran));
    assert!(source(&mut s).contains("<linearGradient id=\"gradient-1\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"0\" spreadMethod=\"repeat\">"), "{}", source(&mut s));
}

#[test]
fn nodes_are_cut_to_shapes_and_let_go_again() {
    let (mut s, _) = server("clips");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect id='sky' width='24' height='24' fill='#2a9df4'/><g id='hills'><path d='M0 24 L8 10 L16 24 Z' fill='#3a3'/></g><circle id='hole' cx='12' cy='12' r='8'/>"}"##);
    let source = |s: &mut Server<Ink>| text(&ok(s, "doc_source", r#"{"doc_id":"d1"}"#)).lines().skip(1).map(str::to_owned).collect::<Vec<_>>().join("\n");
    let alpha = |s: &mut Server<Ink>, x: u32, y: u32| picture(&ok(s, "doc_preview", r#"{"doc_id":"d1","max_edge":96,"background":"none"}"#)).1.pixel(x, y)[3];
    assert_eq!(alpha(&mut s, 4, 4), 255, "the sky, into its corner");
    // The circle leaves the drawing and cuts the sky and the hills.
    let cut = ok(&mut s, "clip_set", r#"{"doc_id":"d1","node_ids":["N2","N3"],"by":["N5"],"id":"porthole"}"#);
    assert_eq!(text(&cut), "Made N7 <clipPath id=\"porthole\"> from N5: it now cuts N2, N3, which show only where it is.");
    assert_eq!((data(&cut, "node_id"), data(&cut, "id")), ("N7", "porthole"));
    assert_eq!(
        source(&mut s),
        "  <defs>\n    <clipPath id=\"porthole\">\n      <circle id='hole' cx='12' cy='12' r='8'/>\n    </clipPath>\n  </defs>\n  <rect id='sky' width='24' height='24' fill='#2a9df4' clip-path=\"url(#porthole)\"/>\n  <g id='hills' clip-path=\"url(#porthole)\">\n    <path d='M0 24 L8 10 L16 24 Z' fill='#3a3'/>\n  </g>\n</svg>"
    );
    assert_eq!((alpha(&mut s, 4, 4), alpha(&mut s, 48, 48)), (0, 255), "only inside the circle now");
    // Two nodes use it, so neither takes it along: a move stays a
    // transform. (One node's alone, it would go with it.)
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N3"],"move":[2,0]}"#);
    assert!(source(&mut s).contains("<g id='hills' clip-path=\"url(#porthole)\" transform=\"translate(2 0)\">"));
    ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#);
    // Let go, one at a time: the last brings the circle back, over what
    // it cut.
    assert_eq!(text(&ok(&mut s, "clip_set", r#"{"doc_id":"d1","node_ids":["N2"],"release":true}"#)), "Took the clip off N2.");
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N3"],"move":[2,0]}"#);
    assert!(source(&mut s).contains("<circle id='hole' cx='14' cy='12' r='8'/>\n    </clipPath>") && source(&mut s).contains("<path d='M2 24 L10 10 L18 24 Z' fill='#3a3'/>"), "its alone now, the clip goes where it goes: {}", source(&mut s));
    assert_eq!(text(&ok(&mut s, "clip_set", r#"{"doc_id":"d1","node_ids":["N3"],"release":true}"#)), "Took the clip off N3. Its clip path cut nothing any more and is gone: N5 is back in the drawing, over what it cut.");
    assert!(source(&mut s).ends_with("  <g id='hills'>\n    <path d='M2 24 L10 10 L18 24 Z' fill='#3a3'/>\n  </g>\n  <circle id='hole' cx='14' cy='12' r='8'/>\n</svg>"), "{}", source(&mut s));
    assert_eq!(text(&ok(&mut s, "clip_set", r#"{"doc_id":"d1","node_ids":["N3"],"release":true}"#)), "Nothing changed: they had no clip path to take off.");
    for (args, says) in [
        (r#""node_ids":["N2"]"#, "say what to cut them to: by (one or more shapes), or release: true to take their clip off"),
        (r#""node_ids":["N2"],"by":["N5"],"release":true"#, "give by or release, not both"),
        (r#""node_ids":["N2"],"by":["N3"]"#, "N3 is a <g>: a clip path is cut from shapes (a rect, a circle, a path, …), one or several"),
        (r#""node_ids":["N2"],"by":["N2"]"#, "N2 is one of the nodes to clip: a node can't be cut by itself or by what's in it"),
        (r#""node_ids":["N2"],"by":["N5"],"id":"a b""#, "\"a b\" can't be an id: letters, digits, - and _ only, so it can be written url(#…)"),
    ] {
        assert_eq!(refused(&mut s, "clip_set", &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
}

#[test]
fn nodes_are_given_shadows_and_blurs() {
    let (mut s, _) = server("filters");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect id='card' x='6' y='6' width='12' height='8' rx='2' fill='#ffe9a8'/><circle id='dot' cx='12' cy='19' r='2' fill='#ff2e5b'/><path id='line' d='M2 2 H10' stroke='#fff'/>"}"##);
    let source = |s: &mut Server<Ink>| text(&ok(s, "doc_source", r#"{"doc_id":"d1"}"#)).lines().skip(1).map(str::to_owned).collect::<Vec<_>>().join("\n");
    let alpha = |s: &mut Server<Ink>, x: u32, y: u32| picture(&ok(s, "doc_preview", r#"{"doc_id":"d1","max_edge":96,"background":"none"}"#)).1.pixel(x, y)[3];
    assert_eq!(alpha(&mut s, 48, 62), 0, "nothing under the card yet");
    // A shadow, with room in its region for how far it reaches: 3 × the
    // blur and the offset, against the card's 12 × 8 box.
    let cast = ok(&mut s, "filter_set", r##"{"doc_id":"d1","node_ids":["N2"],"shadow":{"dy":1.5,"blur":1,"color":"#12100e","opacity":0.6}}"##);
    assert_eq!(text(&cast), "Made N6 <filter id=\"shadow-1\">: it now filters N2.");
    assert_eq!(
        source(&mut s),
        "  <defs>\n    <filter id=\"shadow-1\" x=\"-48%\" y=\"-67%\" width=\"195%\" height=\"233%\">\n      <feDropShadow dx=\"0\" dy=\"1.5\" stdDeviation=\"1\" flood-color=\"#12100e\" flood-opacity=\"0.6\"/>\n    </filter>\n  </defs>\n  <rect id='card' x='6' y='6' width='12' height='8' rx='2' fill='#ffe9a8' filter=\"url(#shadow-1)\"/>\n  <circle id='dot' cx='12' cy='19' r='2' fill='#ff2e5b'/>\n  <path id='line' d='M2 2 H10' stroke='#fff'/>\n</svg>"
    );
    assert!(alpha(&mut s, 48, 62) > 60, "its shadow falls under it");
    // A blur, of the node itself; the same filter on two nodes at once.
    let soft = ok(&mut s, "filter_set", r#"{"doc_id":"d1","node_ids":["N3","N2"],"blur":0.5,"id":"soft"}"#);
    assert_eq!(text(&soft), "Made N8 <filter id=\"soft\">: it now filters N3, N2.");
    assert!(source(&mut s).contains("<filter id=\"soft\" x=\"-48%\" y=\"-48%\" width=\"195%\" height=\"195%\">\n      <feGaussianBlur stdDeviation=\"0.5\"/>"), "room for the smaller of the two: {}", source(&mut s));
    // Taken off.
    assert_eq!(text(&ok(&mut s, "filter_set", r#"{"doc_id":"d1","node_ids":["N2","N3"],"remove":true}"#)), "Took the filter off N2, N3.");
    assert_eq!(text(&ok(&mut s, "filter_set", r#"{"doc_id":"d1","node_ids":["N2"],"remove":true}"#)), "Nothing changed: they had no filter to take off.");
    for (args, says) in [
        (r#""node_ids":["N2"]"#, "say what to give them: shadow, blur, or remove: true to take their filter off"),
        (r#""node_ids":["N2"],"shadow":{},"blur":1"#, "give one of shadow, blur or remove"),
        (r#""node_ids":["N2"],"shadow":{"spread":2}"#, "a shadow has dx, dy, blur, color and opacity, not \"spread\""),
        (r#""node_ids":["N2"],"shadow":{"color":"blurple"}"#, "a shadow's \"color\" should be a colour: \"#rrggbb\", a name, rgb(…)"),
        (r#""node_ids":["N2"],"shadow":{"opacity":2}"#, "a shadow's blur isn't less than nothing, and its opacity is from 0 to 1"),
        (r#""node_ids":["N2"],"blur":0"#, "a blur of nothing blurs nothing: give more than 0, or remove: true to take the filter off"),
        (r#""node_ids":["N4"],"blur":1"#, "N4 <path id=\"line\"> has no box with both a width and a height, and a filter shows within a region measured by its node's box: group it with what it belongs to, and filter the group"),
    ] {
        assert_eq!(refused(&mut s, "filter_set", &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
}

/// Text, over the wire: made, changed, moved, and asked about. Set in
/// the tests' own font: at size 10 a capital is a box 4 by 7 standing
/// on the line a unit in from its pen, and each letter moves the pen 6.
#[test]
fn text_is_added_set_and_asked_about() {
    let (mut s, _) = server("text");
    ok(&mut s, "doc_new", r#"{"width":100,"height":60}"#);
    let added = ok(&mut s, "text_add", r##"{"doc_id":"d1","text":"HH","x":10,"y":20,"font":"No Such Font, Ink Test","size":10,"fill":"#223"}"##);
    assert_eq!(text(&added), "Added N2 <text> \"HH\" at 11,13 10×7, set in Ink Test 10 (No Such Font not installed here).");
    assert_eq!(data(&added, "node_id"), "N2");
    let source = |s: &mut _| text(&ok(s, "doc_source", r#"{"doc_id":"d1","node_id":"N2"}"#)).to_owned();
    assert_eq!(source(&mut s), "<text x=\"10\" y=\"20\" font-family=\"No Such Font, Ink Test\" font-size=\"10\" fill=\"#223\">HH</text>");
    // Lines and stretches with styles of their own are spans: each line
    // starts back at x, a line further down.
    let set = ok(&mut s, "text_set", r##"{"doc_id":"d1","node_id":"N2","runs":[{"text":"H "},{"text":"H\nHH","fill":"#c33","bold":true}],"anchor":"middle","line_height":1.5}"##);
    assert_eq!(text(&set), "Set. Now: N2 <text> \"H H / HH\" at 3.5,13 13.5×22, set in Ink Test 10 (No Such Font not installed here) and Ink Test bold 10 (No Such Font not installed here).");
    assert_eq!(source(&mut s), "<text x=\"10\" y=\"20\" font-family=\"No Such Font, Ink Test\" font-size=\"10\" fill=\"#223\" text-anchor=\"middle\">H <tspan fill=\"#c33\" font-weight=\"bold\">H</tspan><tspan x=\"10\" dy=\"1.5em\"><tspan fill=\"#c33\" font-weight=\"bold\">HH</tspan></tspan></text>");
    // Lettering alone leaves the words as they are.
    let bigger = ok(&mut s, "text_set", r#"{"doc_id":"d1","node_id":"N2","size":20,"italic":false,"letter_spacing":1}"#);
    assert!(text(&bigger).contains("set in Ink Test 20"), "{}", text(&bigger));
    assert!(source(&mut s).contains("font-size=\"20\" fill=\"#223\" text-anchor=\"middle\" font-style=\"normal\" letter-spacing=\"1\">H <tspan"));
    // A move goes into its x and y, and its lines'.
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N2"],"move":[5,-2]}"#);
    assert!(source(&mut s).starts_with("<text x=\"15\" y=\"18\"") && source(&mut s).contains("<tspan x=\"15\" dy=\"1.5em\">"));
    // It says what it is when asked.
    let info = text(&ok(&mut s, "node_info", r#"{"doc_id":"d1","node_id":"N2"}"#)).to_owned();
    assert!(info.contains("\nSays \"H H / HH\", set in Ink Test 20 (No Such Font not installed here) and Ink Test bold 20 (No Such Font not installed here).\nShows at "), "{info}");
    // One step back at a time.
    ok(&mut s, "history_undo", r#"{"doc_id":"d1","steps":3}"#);
    assert_eq!(source(&mut s), "<text x=\"10\" y=\"20\" font-family=\"No Such Font, Ink Test\" font-size=\"10\" fill=\"#223\">HH</text>");
    // In a batch, with a name for later steps.
    let batch = ok(&mut s, "batch", r#"{"doc_id":"d1","steps":[{"tool":"text_add","args":{"text":"I","x":50,"y":40,"font":"Ink Test"},"as":"label"},{"tool":"text_set","args":{"node_id":"@label","text":"II","bold":true}}]}"#);
    assert!(text(&batch).contains("N6"), "{}", text(&batch));
    assert_eq!(text(&ok(&mut s, "doc_source", r#"{"doc_id":"d1","node_id":"N6"}"#)), "<text x=\"50\" y=\"40\" font-family=\"Ink Test\" font-weight=\"bold\">II</text>");

    for (tool, args, says) in [
        ("text_add", r#""x":1,"y":2"#, "give the words: text (one style) or runs (several)"),
        ("text_add", r#""x":1,"y":2,"text":"a","runs":[{"text":"b"}]"#, "give text (one style) or runs (several), not both"),
        ("text_add", r#""x":1,"y":2,"runs":[{"fill":"red"}]"#, "each of \"runs\" needs its \"text\""),
        ("text_add", r#""x":1,"y":2,"runs":[{"text":"a","colour":"red"}]"#, "a run has no \"colour\": it takes text, fill, bold, italic, font and size"),
        ("text_add", r#""x":1,"y":2,"text":"a","font":" ""#, "\"font\" is empty: name a family (font_list says which are installed)"),
        ("text_set", r#""node_id":"N2""#, "say what to set: text or runs, or font, size, bold, italic, fill, anchor, letter_spacing"),
        ("text_set", r#""node_id":"N2","line_height":2"#, "line_height goes with text or runs: it's written into the lines they make"),
        ("text_set", r#""node_id":"N1","text":"a""#, "N1 is a <svg>, not a <text>: text_add makes a text, and node_style paints anything"),
    ] {
        assert_eq!(refused(&mut s, tool, &format!(r#"{{"doc_id":"d1",{args}}}"#)), says, "{tool} {args}");
    }
    assert_eq!(text(&ok(&mut s, "text_set", r#"{"doc_id":"d1","node_id":"N2","text":"HH"}"#)), "Nothing changed: it said that already, lettered that way.");

    // The fonts here: the tests' own is among them.
    let fonts = ok(&mut s, "font_list", r#"{"query":"ink te"}"#);
    assert!(text(&fonts).starts_with("Here sans-serif is ") && text(&fonts).ends_with("have \"ink te\" in their name: Ink Test."), "{}", text(&fonts));
    assert!(text(&ok(&mut s, "font_list", r#"{"query":"no such font anywhere"}"#)).contains("has \"no such font anywhere\" in its name."));
    assert!(text(&ok(&mut s, "font_list", "{}")).contains(" families are installed: "));
}
