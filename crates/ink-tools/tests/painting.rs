//! The tools that say how things are painted, over the wire
//! (ARCHITECTURE §10). What each writes into the file is `ink-doc`'s to
//! test; these are the calls, their replies and their refusals.

mod common;

use common::{ok, refused, server, text};
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
