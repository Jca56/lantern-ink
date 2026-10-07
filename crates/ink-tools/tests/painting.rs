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
