//! Copies, groups, and moves between groups (ARCHITECTURE §3.4): what
//! each leaves in the file. None changes how the drawing looks.

use ink_doc::{Command, DocError, DocId, Document, NodeId, Place};

const OPEN: &str = "<svg viewBox=\"0 0 24 24\">";

fn doc(inner: &str) -> Document {
    Document::parse(DocId(1), &format!("{OPEN}{inner}</svg>")).unwrap()
}

fn ids(nodes: &[u64]) -> Vec<NodeId> {
    nodes.iter().map(|&n| NodeId(n)).collect()
}

/// What's in the drawing after `command`.
fn after(inner: &str, command: Command) -> String {
    let mut d = doc(inner);
    d.apply(&command).unwrap();
    let out = d.to_svg();
    out[OPEN.len()..out.len() - "</svg>".len()].to_owned()
}

fn refused(inner: &str, command: Command) -> String {
    let mut d = doc(inner);
    let before = d.to_svg();
    let why = match d.apply(&command) {
        Err(DocError::Invalid(why)) => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(d.to_svg(), before, "a refused command changes nothing");
    why
}

#[test]
fn a_copy_lies_on_its_original_with_names_of_its_own() {
    let inner = "\n  <g id=\"sun\">\n    <linearGradient id=\"glow\"/>\n    <circle r=\"4\" fill=\"url(#glow)\" stroke=\"url(#sky)\"/>\n  </g>\n  <rect width=\"2\" height=\"2\"/>\n";
    let mut d = doc(inner);
    let applied = d.apply(&Command::Duplicate { nodes: ids(&[2, 5]) }).unwrap();
    assert_eq!(applied.created, ids(&[6, 9]));
    assert_eq!(
        d.to_svg(),
        format!("{OPEN}\n  <g id=\"sun\">\n    <linearGradient id=\"glow\"/>\n    <circle r=\"4\" fill=\"url(#glow)\" stroke=\"url(#sky)\"/>\n  </g>\n  <g id=\"sun-2\">\n    <linearGradient id=\"glow-2\"/>\n    <circle r=\"4\" fill=\"url(#glow-2)\" stroke=\"url(#sky)\"/>\n  </g>\n  <rect width=\"2\" height=\"2\"/>\n  <rect width=\"2\" height=\"2\"/>\n</svg>")
    );
    // A copy of the copy counts on.
    d.apply(&Command::Duplicate { nodes: ids(&[6]) }).unwrap();
    assert!(d.to_svg().contains("<g id=\"sun-3\">\n    <linearGradient id=\"glow-3\"/>"));
    assert_eq!(refused(inner, Command::Duplicate { nodes: ids(&[1]) }), "the root <svg> can't be copied into itself");
    assert_eq!(refused(inner, Command::Duplicate { nodes: vec![] }), "there's nothing to copy");
}

#[test]
fn nodes_go_into_a_group_where_the_topmost_was() {
    let inner = "\n  <a/>\n  <path id=\"p\"/>\n  <b/>\n  <g>\n    <rect/>\n  </g>\n  <c/>\n";
    let mut d = doc(inner);
    // Named in any order: grouped in the file's.
    let applied = d.apply(&Command::Group { nodes: ids(&[5, 3]) }).unwrap();
    assert_eq!((applied.created.clone(), applied.moved.clone()), (ids(&[8]), ids(&[3, 5])));
    assert_eq!(d.to_svg(), format!("{OPEN}\n  <a/>\n  <b/>\n  <g>\n    <path id=\"p\"/>\n    <g>\n      <rect/>\n    </g>\n  </g>\n  <c/>\n</svg>"), "what's in them goes in a step too");
    assert_eq!(refused(inner, Command::Group { nodes: ids(&[3, 6]) }), "N3 and N6 aren't in the same group: nodes to group must share a parent (move them together first)");
    assert_eq!(refused(inner, Command::Group { nodes: ids(&[1]) }), "the root <svg> can't go into a group: it's what everything is in");
    assert_eq!(refused(inner, Command::Group { nodes: vec![] }), "there's nothing to group");
    // Where SVG has a prefix, the group has it too.
    let mut prefixed = Document::parse(DocId(1), "<s:svg xmlns:s=\"http://www.w3.org/2000/svg\"><s:path/></s:svg>").unwrap();
    prefixed.apply(&Command::Group { nodes: ids(&[2]) }).unwrap();
    assert_eq!(prefixed.to_svg(), "<s:svg xmlns:s=\"http://www.w3.org/2000/svg\"><s:g><s:path/></s:g></s:svg>");
}

#[test]
fn a_group_goes_and_what_was_in_it_looks_as_it_did() {
    // Its transform goes to each of them, into their numbers where it
    // can; what they had from it by inheritance is said on them.
    let inner = "\n  <g transform=\"translate(2 3)\" fill=\"red\" style=\"stroke: #000; stroke-width: 2\" display=\"none\" id=\"both\">\n    <!-- two -->\n    <rect width=\"4\" height=\"4\" fill=\"blue\"/>\n    <text>hi</text>\n    <title>two</title>\n  </g>\n  <path/>\n";
    let mut d = doc(inner);
    let applied = d.apply(&Command::Ungroup { nodes: ids(&[2]), drop: false }).unwrap();
    assert_eq!((applied.removed.clone(), applied.moved.clone(), applied.changed.clone()), (ids(&[2]), ids(&[3, 4, 5]), ids(&[3, 4])));
    assert_eq!(
        d.to_svg(),
        format!("{OPEN}\n  <rect width=\"4\" height=\"4\" fill=\"blue\" x=\"2\" y=\"3\" stroke=\"#000\" stroke-width=\"2\" display=\"none\"/>\n  <text transform=\"translate(2 3)\" fill=\"red\" stroke=\"#000\" stroke-width=\"2\" display=\"none\">hi</text>\n  <title>two</title>\n  <path/>\n</svg>")
    );
    // A stroke scaled along with its shape is the shape's own, and the
    // group's isn't said over it.
    assert_eq!(after(r##"<g transform="scale(2)" stroke="#000" stroke-width="3"><path d="M0 0H4"/></g>"##, Command::Ungroup { nodes: ids(&[2]), drop: false }), r##"<path d="M0 0 H8" stroke-width="6" stroke="#000"/>"##);
    // Groups in groups, one at a time.
    assert_eq!(after(r#"<g transform="translate(1 0)"><g transform="translate(0 1)"><circle r="1"/></g></g>"#, Command::Ungroup { nodes: ids(&[2]), drop: false }), r#"<g><circle r="1" cx="1" cy="1"/></g>"#);
    assert_eq!(after("<g/>", Command::Ungroup { nodes: ids(&[2]), drop: false }), "");
}

#[test]
fn what_only_a_group_can_hold_is_not_lost_without_being_told_to() {
    let shadowed = r#"<g filter="url(#s)" clip-path="url(#c)"><path/><path/></g>"#;
    assert_eq!(refused(shadowed, Command::Ungroup { nodes: ids(&[2]), drop: false }), "N2 has a filter and a clip path, which only a group can hold for what's in it: ungrouping would lose them. Take them off first, or say to drop them");
    assert_eq!(after(shadowed, Command::Ungroup { nodes: ids(&[2]), drop: true }), "<path/><path/>");
    // An opacity over several things isn't each one's opacity; over one
    // thing, it is.
    let faded = r#"<g opacity="0.5"><path/><path/></g>"#;
    assert_eq!(refused(faded, Command::Ungroup { nodes: ids(&[2]), drop: false }), "N2 has an opacity over all that's in it, which only a group can hold for what's in it: ungrouping would lose it. Take it off first, or say to drop it");
    assert_eq!(after(faded, Command::Ungroup { nodes: ids(&[2]), drop: true }), "<path/><path/>");
    assert_eq!(after(r#"<g opacity="0.5"><path style="opacity: 0.5"/><title>t</title></g>"#, Command::Ungroup { nodes: ids(&[2]), drop: false }), r#"<path style="opacity: 0.25"/><title>t</title>"#);
    assert_eq!(after(r#"<g opacity="50%"><path/></g>"#, Command::Ungroup { nodes: ids(&[2]), drop: false }), r#"<path opacity="0.5"/>"#);
    assert_eq!(refused("<a><path/></a>", Command::Ungroup { nodes: ids(&[2]), drop: false }), "N2 is a <a>, not a group (<g>)");
    assert_eq!(refused("<path/>", Command::Ungroup { nodes: ids(&[1]), drop: false }), "the root <svg> isn't a group that can be taken away: it's what everything is in");
    assert_eq!(refused("<path/>", Command::Ungroup { nodes: vec![], drop: false }), "there's nothing to ungroup");
}

#[test]
fn a_node_moved_between_groups_stays_where_it_shows() {
    // Into a group that doubles everything: its numbers halve.
    let inner = r#"<g transform="scale(2)" clip-path="url(#c)"/><rect x="4" y="4" width="8" height="8"/><text>hi</text>"#;
    assert_eq!(after(inner, Command::Move { nodes: ids(&[3]), place: Place::LastIn(NodeId(2)) }), r#"<g transform="scale(2)" clip-path="url(#c)"><rect x="2" y="2" width="4" height="4"/></g><text>hi</text>"#);
    // What has no numbers for it takes a transform that undoes the
    // group's.
    assert_eq!(after(inner, Command::Move { nodes: ids(&[4]), place: Place::LastIn(NodeId(2)) }), r#"<g transform="scale(2)" clip-path="url(#c)"><text transform="scale(0.5)">hi</text></g><rect x="4" y="4" width="8" height="8"/>"#);
    // Out again, to where it showed inside.
    let inside = r#"<g transform="translate(5 5)" clip-path="url(#c)"><circle r="1"/></g>"#;
    let mut d = doc(inside);
    let applied = d.apply(&Command::Move { nodes: ids(&[3]), place: Place::After(NodeId(2)) }).unwrap();
    assert_eq!((applied.moved.clone(), applied.changed.clone()), (ids(&[3]), ids(&[3])));
    assert!(d.to_svg().ends_with(r#"<g transform="translate(5 5)" clip-path="url(#c)"></g><circle r="1" cx="5" cy="5"/></svg>"#), "{}", d.to_svg());
    // Among its own siblings nothing about it changes.
    assert_eq!(after(inside, Command::Move { nodes: ids(&[3]), place: Place::FirstIn(NodeId(2)) }), inside);
    // Nothing can stay where it shows under a transform that shows
    // nothing.
    assert_eq!(refused(r#"<g transform="scale(0)"/><rect width="1" height="1"/>"#, Command::Move { nodes: ids(&[3]), place: Place::LastIn(NodeId(2)) }), "N3 can't stay where it shows there: it would be under a transform that squashes everything flat");
    // What shows nowhere itself just moves.
    assert_eq!(after(r#"<g transform="scale(2)"/><linearGradient id="g"/>"#, Command::Move { nodes: ids(&[3]), place: Place::LastIn(NodeId(2)) }), r#"<g transform="scale(2)"><linearGradient id="g"/></g>"#);
}
