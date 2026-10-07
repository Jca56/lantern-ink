//! The tools that move things about and put them together, over the
//! wire (ARCHITECTURE §10): transforms, alignment, copies and groups.
//! What each writes into the file is `ink-doc`'s to test; these are the
//! calls, their replies and their refusals.

mod common;

use common::{data, ok, refused, server, text};
use ink_tools::Ink;
use lntrn_mcp::Server;

#[test]
fn nodes_move_scale_and_turn_on_the_page() {
    let (mut s, _) = server("transform");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect id='a' x='4' y='4' width='8' height='8' rx='2'/><g id='g'><path id='p' d='M4 14 H12' stroke='#000'/></g><text id='t'> </text>"}"##);
    let node = |s: &mut Server<Ink>, id: &str| text(&ok(s, "doc_source", &format!(r#"{{"doc_id":"d1","node_id":"{id}"}}"#))).to_owned();
    // A move goes into a shape's own numbers.
    let moved = ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N2"],"move":[2,3]}"#);
    assert_eq!(text(&moved), "Done. Now: N2 <rect id=\"a\"> at 6,7 8×8.");
    assert_eq!(node(&mut s, "N2"), "<rect id='a' x='6' y='7' width='8' height='8' rx='2'/>");
    // A turn a rect can't say stays as one rotate about its middle (the
    // middle of its box, when no pivot is given).
    let turned = ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N2"],"rotate":45}"#);
    assert_eq!(text(&turned), "Done. Now: N2 <rect id=\"a\"> at 5.17,6.17 9.66×9.66, transform=\"rotate(45 10 11)\".", "the box of its rounded outline");
    // A group passes it down; a stroke grows with its shape.
    let grown = ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N3"],"scale":[2],"pivot":[0,0]}"#);
    assert_eq!(text(&grown), "Done. Now: N4 <path id=\"p\"> at 8,28 16×0.");
    assert_eq!(node(&mut s, "N3"), "<g id='g'>\n    <path id='p' d='M8 28 H24' stroke='#000' stroke-width=\"2\"/>\n  </g>");
    // Scale, then turn, then flip, about a pivot; then the move.
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N4"],"scale":[0.5,0.5],"rotate":90,"flip":"horizontal","pivot":[8,28],"move":[-8,-28]}"#);
    assert_eq!(node(&mut s, "N4"), "<path id='p' d='M0 0 V8' stroke='#000' stroke-width=\"1\"/>");
    // What has no numbers to take it keeps it as a transform; a matrix
    // says anything.
    let held = ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N5"],"matrix":[1,0,0.5,1,2,0]}"#);
    assert_eq!(text(&held), "Done. Now: N5 <text id=\"t\">, transform=\"matrix(1 0 0.5 1 2 0)\".");
    // Naming the root moves everything in the drawing.
    let all = ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N1"],"move":[1,1]}"#);
    assert!(text(&all).starts_with("Done. Now: N2 <rect id=\"a\"> at 6.17,7.17 9.66×9.66, transform=\"rotate(45 11 12)\"; N4 <path id=\"p\"> at 1,1 0×8; N5 <text id=\"t\">, transform=\"matrix(1 0 0.5 1 3 1)\"."), "{}", text(&all));
    assert_eq!(text(&ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N2"],"move":[0,0]}"#)), "Nothing changed: that leaves them where they are.");
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)), "Undid 1 step: \"node_transform\" (Claude). Now 6 can be undone and 1 redone.");

    for (args, says) in [
        (r#""node_ids":["N2"]"#, "say what to do: move, scale, rotate, skew or flip (or matrix)"),
        (r#""node_ids":["N2"],"move":[1]"#, "\"move\" should be [dx, dy]"),
        (r#""node_ids":["N2"],"scale":[2,0]"#, "a scale of 0 squashes things flat: give one that leaves them some size"),
        (r#""node_ids":["N2"],"skew":[90,0]"#, "a skew of 90° lays things flat: give a smaller one"),
        (r#""node_ids":["N2"],"flip":"sideways""#, "flip is horizontal, vertical or both, not \"sideways\""),
        (r#""node_ids":["N2"],"matrix":[1,0,0,1,0,0],"rotate":3"#, "give matrix alone: it says everything the others would"),
        (r#""node_ids":["N2"],"matrix":[1,0,0,0,0,0]"#, "that transform squashes everything flat (or isn't numbers): give one that leaves things some size"),
        (r#""node_ids":["N5"],"rotate":10"#, "none of these nodes shows anywhere, so they have no middle to turn about: give pivot"),
        (r#""node_ids":["N9"],"move":[1,1]"#, "no node N9 in this document (doc_info lists its nodes)"),
    ] {
        assert_eq!(refused(&mut s, "node_transform", &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
    // In a batch, with the node an earlier step made.
    let steps = r#"[{"tool":"node_add","args":{"element":"circle","attrs":{"r":2}},"as":"dot"},{"tool":"node_transform","args":{"node_ids":["@dot"],"move":[5,5]}}]"#;
    ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","preview":false,"steps":{steps}}}"#));
    assert_eq!(node(&mut s, "N6"), "<circle r=\"2\" cx=\"5\" cy=\"5\"/>");
}

#[test]
fn nodes_line_up_and_spread_out() {
    let (mut s, _) = server("align");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<rect width='4' height='4'/><rect x='6' y='2' width='2' height='8'/><circle cx='20' cy='20' r='2'/><defs/>"}"##);
    let boxes = |s: &mut Server<Ink>| -> Vec<String> { text(&ok(s, "doc_info", r#"{"doc_id":"d1"}"#)).lines().skip(4).filter_map(|l| l.split("  at ").nth(1).map(str::to_owned)).collect() };
    assert_eq!(boxes(&mut s), ["18,18 4×4", "6,2 2×8", "0,0 4×4"], "front to back");
    // Against the box around them all.
    let lined = ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N2","N3","N4"],"y":"bottom"}"#);
    assert_eq!(text(&lined), "Done. Now: N2 <rect> at 0,18 4×4; N3 <rect> at 6,14 2×8.");
    assert_eq!(boxes(&mut s), ["18,18 4×4", "6,14 2×8", "0,18 4×4"]);
    // The gaps between them made the same, the outer two staying put.
    ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N4","N2","N3"],"spread":"horizontal"}"#);
    assert_eq!(boxes(&mut s), ["18,18 4×4", "10,14 2×8", "0,18 4×4"]);
    // One node against the page; several against a node that stays put.
    ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N4"],"x":"center","y":"middle"}"#);
    assert_eq!(boxes(&mut s)[0], "10,10 4×4");
    ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N2","N3","N4"],"x":"left","y":"top","to":"N4"}"#);
    assert_eq!(boxes(&mut s), ["10,10 4×4", "10,10 2×8", "10,10 4×4"]);
    ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N2","N3"],"x":"right","to":"page"}"#);
    assert_eq!(boxes(&mut s), ["10,10 4×4", "22,10 2×8", "20,10 4×4"]);
    assert_eq!(text(&ok(&mut s, "node_align", r#"{"doc_id":"d1","node_ids":["N2","N3"],"x":"right","to":"page"}"#)), "Nothing changed: that leaves them where they are.");
    for (args, says) in [
        (r#""node_ids":["N2"]"#, "say how: x (left, center, right), y (top, middle, bottom) or spread (horizontal, vertical)"),
        (r#""node_ids":["N2","N3"],"spread":"vertical""#, "spread shares out the space between the first and the last: it takes three nodes or more"),
        (r#""node_ids":["N5"],"x":"left""#, "N5 <defs> shows nowhere, so there's nothing of it to line up"),
        (r#""node_ids":["N2"],"x":"left","to":"the middle""#, "\"to\" is \"page\" or a node id like \"N3\", not \"the middle\""),
        (r#""node_ids":["N2"],"x":"middle""#, "x is left, center or right, not \"middle\""),
    ] {
        assert_eq!(refused(&mut s, "node_align", &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
}

#[test]
fn nodes_are_copied_grouped_and_ungrouped() {
    let (mut s, _) = server("groups");
    ok(&mut s, "doc_new", "{}");
    ok(&mut s, "node_add_svg", r##"{"doc_id":"d1","svg":"<circle id='dot' cx='4' cy='4' r='2' fill='#ffc800'/><rect id='bar' x='2' y='10' width='8' height='2'/>"}"##);
    let source = |s: &mut Server<Ink>| text(&ok(s, "doc_source", r#"{"doc_id":"d1"}"#)).lines().skip(1).map(str::to_owned).collect::<Vec<_>>().join("\n");
    // A copy lies on its original, under a name of its own.
    let copy = ok(&mut s, "node_duplicate", r#"{"doc_id":"d1","node_ids":["N2"]}"#);
    assert_eq!(text(&copy), "Copied: N4 <circle id=\"dot-2\"> at 2,2 4×4. Each lies right on top of its original.");
    assert_eq!(data(&copy, "node_id"), "N4");
    ok(&mut s, "node_transform", r#"{"doc_id":"d1","node_ids":["N4"],"move":[6,0]}"#);
    // Into a group, where the topmost was; nothing looks different.
    let grouped = ok(&mut s, "node_group", r#"{"doc_id":"d1","node_ids":["N4","N2"]}"#);
    assert_eq!(text(&grouped), "Grouped N2, N4 into N5 <g> at 2,2 10×4, in N1.");
    assert_eq!(source(&mut s), "  <g>\n    <circle id='dot' cx='4' cy='4' r='2' fill='#ffc800'/>\n    <circle id='dot-2' cx='10' cy='4' r='2' fill='#ffc800'/>\n  </g>\n  <rect id='bar' x='2' y='10' width='8' height='2'/>\n</svg>");
    // The group moves as one, by passing it down; a node moved into it
    // stays where it shows.
    ok(&mut s, "node_set", r##"{"doc_id":"d1","node_id":"N5","attrs":{"transform":"translate(0 2)","stroke":"#12100e","clip-path":"url(#none)"}}"##);
    let moved = ok(&mut s, "node_move", r#"{"doc_id":"d1","node_ids":["N3"],"into":"N5"}"#);
    assert_eq!(text(&moved), "Moved N3 (now in N5). To stay where it showed, N3 changed to make up for the transforms there.");
    assert!(source(&mut s).contains("<rect id='bar' x='2' y='8' width='8' height='2'/>"), "{}", source(&mut s));
    // Ungrouped: what only the group could hold stops it, until told.
    assert_eq!(refused(&mut s, "node_ungroup", r#"{"doc_id":"d1","node_ids":["N5"]}"#), "N5 has a clip path, which only a group can hold for what's in it: ungrouping would lose it. Take it off first, or say to drop it");
    let ungrouped = ok(&mut s, "node_ungroup", r#"{"doc_id":"d1","node_ids":["N5"],"drop":true}"#);
    assert_eq!(text(&ungrouped), "Ungrouped N5. What was in it: N2 (in N1), N4 (in N1), N3 (in N1).");
    assert_eq!(source(&mut s), "  <circle id='dot' cx='4' cy='6' r='2' fill='#ffc800' stroke=\"#12100e\"/>\n  <circle id='dot-2' cx='10' cy='6' r='2' fill='#ffc800' stroke=\"#12100e\"/>\n  <rect id='bar' x='2' y='10' width='8' height='2' stroke=\"#12100e\"/>\n</svg>");
    assert_eq!(text(&ok(&mut s, "history_undo", r#"{"doc_id":"d1"}"#)), "Undid 1 step: \"node_ungroup\" (Claude). Now 6 can be undone and 1 redone.");
    for (tool, args, says) in [
        ("node_group", r#""node_ids":["N2","N1"]"#, "the root <svg> can't go into a group: it's what everything is in"),
        ("node_group", r#""node_ids":["N2","N5"]"#, "N2 and N5 aren't in the same group: nodes to group must share a parent (move them together first)"),
        ("node_ungroup", r#""node_ids":["N2"]"#, "N2 is a <circle>, not a group (<g>)"),
        ("node_duplicate", r#""node_ids":["N1"]"#, "the root <svg> can't be copied into itself"),
        ("node_duplicate", r#""node_ids":["N9"]"#, "no node N9 in this document (doc_info lists its nodes)"),
    ] {
        assert_eq!(refused(&mut s, tool, &format!(r#"{{"doc_id":"d1",{args}}}"#)), says);
    }
    // In a batch, a copy and a group are named like anything a step makes.
    let steps = r#"[{"tool":"node_duplicate","args":{"node_ids":["N3"]},"as":"twin"},{"tool":"node_transform","args":{"node_ids":["@twin"],"move":[0,4]}},{"tool":"node_group","args":{"node_ids":["N3","@twin"]},"as":"bars"},{"tool":"node_set","args":{"node_id":"@bars","attrs":{"id":"bars"}}}]"#;
    let ran = ok(&mut s, "batch", &format!(r#"{{"doc_id":"d1","preview":false,"steps":{steps}}}"#));
    assert!(text(&ran).contains("Named: @twin = N6, @bars = N7."), "{}", text(&ran));
    assert!(source(&mut s).contains("<g id=\"bars\">\n      <rect id='bar' x='2' y='8' width='8' height='2'/>\n      <rect id='bar-2' x='2' y='12' width='8' height='2'/>\n    </g>"), "{}", source(&mut s));
}
