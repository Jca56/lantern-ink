//! `Command::Transform` (ARCHITECTURE §3.4, D13): where a move, a scale
//! or a turn is written. That each result draws the same picture as the
//! transform itself would is `ink-render`'s test (`tests/settle.rs`);
//! these are about what the file then says.

use ink_doc::{Command, DocError, DocId, Document, NodeId};
use ink_geom::{Affine, Vec2};

const OPEN: &str = "<svg viewBox=\"0 0 24 24\">";

fn doc(inner: &str) -> Document {
    Document::parse(DocId(1), &format!("{OPEN}{inner}</svg>")).unwrap()
}

/// What's in the drawing once `ids` have been through `by`.
fn through(inner: &str, ids: &[u64], by: Affine) -> String {
    let mut d = doc(inner);
    d.apply(&Command::Transform { nodes: ids.iter().map(|&n| NodeId(n)).collect(), by }).unwrap();
    let out = d.to_svg();
    out[OPEN.len()..out.len() - "</svg>".len()].to_owned()
}

fn shift(x: f64, y: f64) -> Affine {
    Affine::translate(x, y)
}

fn turn(degrees: f64, cx: f64, cy: f64) -> Affine {
    Affine::rotate(degrees.to_radians()).about(Vec2::new(cx, cy))
}

fn grow(x: f64, y: f64) -> Affine {
    Affine::scale(x, y)
}

#[test]
fn a_shape_takes_a_move_a_scale_and_a_turn_into_its_own_numbers() {
    let rect = r#"<rect x="4" y="4" width="8" height="8" rx="2"/>"#;
    assert_eq!(through(rect, &[2], shift(2.0, 3.0)), r#"<rect x="6" y="7" width="8" height="8" rx="2"/>"#);
    assert_eq!(through(rect, &[2], grow(2.0, 0.5)), r#"<rect x="8" y="2" width="16" height="4" rx="4" ry="1"/>"#);
    assert_eq!(through(r#"<rect x="4" y="4" width="8" height="2"/>"#, &[2], turn(90.0, 8.0, 5.0)), r#"<rect x="7" y="1" width="2" height="8"/>"#, "a quarter turn swaps its sides");
    assert_eq!(through(r#"<circle cx="5" cy="6" r="2"/>"#, &[2], turn(33.0, 0.0, 0.0).then(&grow(-2.0, 2.0))), r#"<circle cx="-1.851" cy="15.51" r="4"/>"#);
    assert_eq!(through(r#"<ellipse cx="5" cy="6" rx="3" ry="1"/>"#, &[2], turn(-90.0, 5.0, 6.0)), r#"<ellipse cx="5" cy="6" rx="1" ry="3"/>"#);
    assert_eq!(through(r#"<path d="M4 14 H12"/>"#, &[2], turn(90.0, 0.0, 0.0)), r#"<path d="M-14 4 V12"/>"#);
    assert_eq!(through(r#"<line x2="4" y2="4"/>"#, &[2], Affine::skew_x(45f64.to_radians())), r#"<line x2="8" y2="4"/>"#);
    assert_eq!(through(r#"<polyline points="0 0 4 0"/>"#, &[2], grow(1.0, -1.0).then(&shift(1.0, 1.0))), r#"<polyline points="1,1 5,1"/>"#);
    // What changes nothing changes nothing: the file's own way of
    // saying a number stays.
    for same in [r#"<path d="m0 0h4v4z"/>"#, r#"<rect x="2.0" width="1e1" height="4px"/>"#, r#"<g transform="translate(0)"><circle r="2"/></g>"#] {
        assert_eq!(through(same, &[2], Affine::IDENTITY), same);
    }
}

#[test]
fn a_turn_a_rect_cannot_say_stays_as_one_rotate_about_its_middle() {
    let rect = r#"<rect x="4" y="4" width="8" height="8" rx="2"/>"#;
    let turned = r#"<rect x="4" y="4" width="8" height="8" rx="2" transform="rotate(45 8 8)"/>"#;
    assert_eq!(through(rect, &[2], turn(45.0, 8.0, 8.0)), turned);
    // Turned on, a quarter turn in all: a rect again, and no transform.
    assert_eq!(through(turned, &[2], turn(45.0, 8.0, 8.0)), rect);
    // Moved, it moves, and is still turned about its own middle.
    assert_eq!(through(turned, &[2], shift(2.0, 3.0)), r#"<rect x="6" y="7" width="8" height="8" rx="2" transform="rotate(45 10 11)"/>"#);
    // Scaled along its own sides under the turn: its numbers again.
    assert_eq!(through(turned, &[2], turn(-45.0, 8.0, 8.0).then(&grow(2.0, 1.0)).then(&turn(45.0, 16.0, 8.0))), r#"<rect x="8" y="4" width="16" height="8" rx="4" transform="rotate(45 16 8)" ry="2"/>"#);
    // Turned about somewhere else, its middle goes round with it.
    assert_eq!(through(r#"<ellipse cx="6" cy="0" rx="3" ry="1"/>"#, &[2], turn(30.0, 0.0, 0.0)), r#"<ellipse cx="5.196" cy="3" rx="3" ry="1" transform="rotate(30 5.196 3)"/>"#);
    // A skew isn't a turn: it stays whole, and the rect as it was.
    assert_eq!(through(rect, &[2], Affine::skew_x(45f64.to_radians())), r#"<rect x="4" y="4" width="8" height="8" rx="2" transform="matrix(1 0 1 1 0 0)"/>"#);
}

#[test]
fn a_stroke_grows_with_its_shape() {
    assert_eq!(through(r##"<circle cx="5" cy="6" r="2" stroke="#000"/>"##, &[2], grow(2.0, 2.0)), r##"<circle cx="10" cy="12" r="4" stroke="#000" stroke-width="2"/>"##, "the default width of 1, now said");
    // Where the node has it: in its style, or from the group above.
    assert_eq!(through(r#"<path d="M0 0H4" style="stroke:red; stroke-width:1.5; stroke-dasharray: 4,2; stroke-dashoffset:1"/>"#, &[2], grow(2.0, 2.0)), r#"<path d="M0 0 H8" style="stroke:red; stroke-width:3; stroke-dasharray: 8 4; stroke-dashoffset:2"/>"#);
    assert_eq!(through(r##"<g stroke="#000" stroke-width="2" clip-path="url(#c)"><path d="M0 0H4"/></g>"##, &[3], grow(0.5, 0.5)), r##"<g stroke="#000" stroke-width="2" clip-path="url(#c)"><path d="M0 0 H2" stroke-width="1"/></g>"##);
    // A lopsided scale would make it lopsided: that stays a transform.
    let stroked = r##"<rect width="8" height="8" stroke="#000"/>"##;
    assert_eq!(through(stroked, &[2], grow(2.0, 1.0)), r##"<rect width="8" height="8" stroke="#000" transform="scale(2 1)"/>"##);
    // A move, a turn and a mirror leave it as wide as it was.
    assert_eq!(through(stroked, &[2], grow(-1.0, 1.0).then(&shift(8.0, 2.0))), r##"<rect width="8" height="8" stroke="#000" y="2"/>"##);
    assert_eq!(through(r#"<rect width="8" height="8" stroke="none" stroke-width="3"/>"#, &[2], grow(2.0, 1.0)), r#"<rect width="16" height="8" stroke="none" stroke-width="3"/>"#, "no stroke drawn, none to grow");
    // Dashes start where a rect's outline does: turned or mirrored into
    // its numbers they'd start somewhere else. A path's go with it.
    let dashed = r##"<rect width="8" height="4" stroke="#000" stroke-dasharray="3 2"/>"##;
    assert_eq!(through(dashed, &[2], shift(1.0, 0.0).then(&grow(2.0, 2.0))), r##"<rect width="16" height="8" stroke="#000" stroke-dasharray="6 4" x="2" stroke-width="2"/>"##);
    assert_eq!(through(dashed, &[2], grow(-1.0, 1.0)), r##"<rect width="8" height="4" stroke="#000" stroke-dasharray="3 2" transform="scale(-1 1)"/>"##);
    assert_eq!(through(r##"<circle r="4" stroke="#000" stroke-dasharray="3 2"/>"##, &[2], turn(90.0, 0.0, 0.0)), r##"<circle r="4" stroke="#000" stroke-dasharray="3 2" transform="rotate(90)"/>"##);
    assert_eq!(through(r##"<path d="M0 0H8" stroke="#000" stroke-dasharray="3 2"/>"##, &[2], grow(-1.0, 1.0)), r##"<path d="M0 0 H-8" stroke="#000" stroke-dasharray="3 2"/>"##);
}

#[test]
fn what_a_shape_is_drawn_with_decides_what_its_numbers_can_take() {
    let defs = r##"<defs><linearGradient id="box"/><linearGradient id="user" gradientUnits="userSpaceOnUse"/><linearGradient id="too" href="#user"/><filter id="shadow"><feDropShadow/></filter><filter id="fixed" filterUnits="userSpaceOnUse"><feDropShadow/></filter></defs>"##;
    let shape = |attrs: &str, by: Affine| {
        let out = through(&format!(r#"{defs}<path d="M0 0H4V4Z" {attrs}/>"#), &[10], by);
        out[defs.len()..].to_owned()
    };
    // A gradient across its box goes with a move and a stretch, but
    // wouldn't turn or mirror with baked numbers.
    assert_eq!(shape(r#"fill="url(#box)""#, shift(2.0, 3.0).then(&grow(2.0, 3.0))), r#"<path d="M4 9 H12 V21 Z" fill="url(#box)"/>"#);
    assert_eq!(shape(r#"fill="url(#box)""#, turn(30.0, 0.0, 0.0)), r#"<path d="M-1.268 0.732 H2.732 V4.732 Z" fill="url(#box)" transform="rotate(30 0.732 2.732)"/>"#);
    assert_eq!(shape(r#"fill="url(#box)""#, grow(-1.0, 1.0).then(&shift(4.0, 0.0))), r#"<path d="M0 0H4V4Z" fill="url(#box)" transform="matrix(-1 0 0 1 4 0)"/>"#);
    assert_eq!(shape(r#"fill="url(#nothing) red""#, turn(90.0, 0.0, 0.0)), r#"<path d="M0 0 V4 H-4 Z" fill="url(#nothing) red"/>"#, "a gradient that isn't there is a colour");
    // One laid out in its own coordinates would be left behind, like a
    // clip path, a mask and markers.
    for held in [r#"fill="url(#user)""#, r#"fill="url(#too)""#, r#"clip-path="url(#c)""#, r#"style="mask: url(#m)""#, r#"marker-end="url(#arrow)""#, r#"filter="url(#fixed)""#] {
        assert_eq!(shape(held, shift(2.0, 3.0)), format!(r#"<path d="M0 0H4V4Z" {held} transform="translate(2 3)"/>"#));
    }
    assert_eq!(shape(r#"stroke="url(#user)" stroke-width="0""#, shift(2.0, 3.0)), r#"<path d="M2 3 H6 V7 Z" stroke="url(#user)" stroke-width="0"/>"#, "a stroke that isn't drawn holds nothing");
    // A shadow goes with a move, but wouldn't turn or grow.
    assert_eq!(shape(r#"filter="url(#shadow)""#, shift(2.0, 3.0)), r#"<path d="M2 3 H6 V7 Z" filter="url(#shadow)"/>"#);
    assert_eq!(shape(r#"filter="url(#shadow)""#, grow(2.0, 2.0)), r#"<path d="M0 0H4V4Z" filter="url(#shadow)" transform="scale(2)"/>"#);
    assert_eq!(shape(r#"filter="url(#shadow)""#, turn(90.0, 2.0, 2.0).then(&shift(1.0, 0.0))), r#"<path d="M1 0 H5 V4 Z" filter="url(#shadow)" transform="rotate(90 3 2)"/>"#, "moved in its numbers, turned in its transform");
    // What can't all be read is left as written.
    assert_eq!(through(r#"<rect width="50%" height="4"/>"#, &[2], shift(1.0, 1.0)), r#"<rect width="50%" height="4" transform="translate(1 1)"/>"#);
}

#[test]
fn a_gradient_in_a_shapes_own_coordinates_goes_with_it_when_it_is_the_shapes_alone() {
    let own = |gradient: &str, shape: &str, by: Affine| through(&format!("<defs>{gradient}</defs>{shape}"), &[4], by);
    let linear = r#"<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="4" x2="0" y2="12"/>"#;
    let rect = r##"<rect x="4" y="4" width="8" height="8" fill="url(#g)"/>"##;
    // A move, an even scale and a turn: into the gradient's own numbers.
    assert_eq!(own(linear, rect, shift(2.0, 3.0)), r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="2" y1="7" x2="2" y2="15"/></defs><rect x="6" y="7" width="8" height="8" fill="url(#g)"/>"##);
    assert_eq!(own(linear, rect, grow(2.0, 2.0)), r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="8" x2="0" y2="24"/></defs><rect x="8" y="8" width="16" height="16" fill="url(#g)"/>"##);
    assert_eq!(own(linear, rect, turn(90.0, 8.0, 8.0)), r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="12" y1="0" x2="4" y2="0"/></defs><rect x="4" y="4" width="8" height="8" fill="url(#g)"/>"##, "the square is where it was; its gradient has turned");
    // A stretch would leave its colours no longer square to its line:
    // that goes into its gradientTransform, and is added to after.
    let stretched = own(linear, rect, grow(2.0, 1.0));
    assert_eq!(stretched, r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="4" x2="0" y2="12" gradientTransform="scale(2 1)"/></defs><rect x="8" y="4" width="16" height="8" fill="url(#g)"/>"##);
    assert!(through(&stretched, &[4], shift(1.0, 0.0)).starts_with(r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="4" x2="0" y2="12" gradientTransform="matrix(2 0 0 1 1 0)"/>"##));
    // One an editor left a gradientTransform on is taken in whole, when
    // that and the move come to an even scale.
    let boxy = r#"<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="8" x2="0" y2="24" gradientTransform="matrix(0.5, 0, 0, 0.5, 1, 0)"/>"#;
    assert_eq!(own(boxy, rect, shift(1.0, 0.0)), r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="2" y1="4" x2="2" y2="12"/></defs><rect x="5" y="4" width="8" height="8" fill="url(#g)"/>"##);
    // Rings: the middle, the focus and the radius.
    let radial = r#"<radialGradient id="g" gradientUnits="userSpaceOnUse" cx="8" cy="8" r="4" fx="6" fy="8"/>"#;
    assert_eq!(own(radial, rect, turn(90.0, 8.0, 8.0).then(&grow(2.0, 2.0))), r##"<defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="16" cy="16" r="8" fx="16" fy="12"/></defs><rect x="8" y="8" width="16" height="16" fill="url(#g)"/>"##);
    // Numbers that aren't all said plainly stay, under a gradientTransform.
    let unsaid = r#"<linearGradient id="g" gradientUnits="userSpaceOnUse" x2="50%"/>"#;
    assert_eq!(own(unsaid, rect, shift(2.0, 3.0)), r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x2="50%" gradientTransform="translate(2 3)"/></defs><rect x="6" y="7" width="8" height="8" fill="url(#g)"/>"##);
    // A stroke's gradient goes too, and with it the stroke grows.
    let stroked = r##"<path d="M0 8H8" stroke="url(#g)" stroke-width="2"/>"##;
    assert_eq!(own(linear, stroked, grow(2.0, 2.0)), r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="8" x2="0" y2="24"/></defs><path d="M0 16 H16" stroke="url(#g)" stroke-width="4"/>"##);
}

#[test]
fn a_gradient_other_shapes_use_is_never_touched() {
    let gradient = r#"<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="4" x2="0" y2="12"/>"#;
    // Two shapes paint with it: the one moved keeps its move as a
    // transform, and the gradient is as it was for both.
    let shared = format!(r##"<defs>{gradient}</defs><rect x="4" y="4" width="8" height="8" fill="url(#g)"/><rect x="14" y="4" width="8" height="8" fill="url(#g)"/>"##);
    assert_eq!(through(&shared, &[4], shift(2.0, 3.0)), format!(r##"<defs>{gradient}</defs><rect x="4" y="4" width="8" height="8" fill="url(#g)" transform="translate(2 3)"/><rect x="14" y="4" width="8" height="8" fill="url(#g)"/>"##));
    // Even moved together: neither can say the gradient is its own.
    assert_eq!(through(&shared, &[4, 5], shift(2.0, 3.0)), format!(r##"<defs>{gradient}</defs><rect x="4" y="4" width="8" height="8" fill="url(#g)" transform="translate(2 3)"/><rect x="14" y="4" width="8" height="8" fill="url(#g)" transform="translate(2 3)"/>"##));
    // One that another gradient takes its stops from is used by it;
    // one that takes from another isn't all there to move; and one a
    // group hands to what's in it is each of theirs.
    let based = format!(r##"<defs>{gradient}<linearGradient id="h" href="#g"/></defs><rect width="8" height="8" fill="url(#g)"/><rect width="8" height="8" fill="url(#h)"/>"##);
    assert!(through(&based, &[5], shift(1.0, 0.0)).contains(r##"<rect width="8" height="8" fill="url(#g)" transform="translate(1 0)"/>"##));
    assert!(through(&based, &[6], shift(1.0, 0.0)).ends_with(r##"<rect width="8" height="8" fill="url(#h)" transform="translate(1 0)"/>"##));
    let handed = format!(r##"<defs>{gradient}</defs><g fill="url(#g)"><rect width="8" height="8"/></g>"##);
    assert_eq!(through(&handed, &[4], shift(1.0, 0.0)), format!(r##"<defs>{gradient}</defs><g fill="url(#g)" transform="translate(1 0)"><rect width="8" height="8"/></g>"##));
}

#[test]
fn a_group_passes_it_down_when_everything_in_it_can_take_it() {
    let group = r#"<g><rect x="4" y="4" width="8" height="8"/><path d="M4 14 H12"/><title>two</title></g>"#;
    assert_eq!(through(group, &[2], shift(2.0, 3.0)), r#"<g><rect x="6" y="7" width="8" height="8"/><path d="M6 17 H14"/><title>two</title></g>"#);
    // A turn the rect can't say would leave it a transform it didn't
    // have: the group keeps it.
    assert_eq!(through(group, &[2], turn(45.0, 0.0, 0.0)), r#"<g transform="rotate(45)"><rect x="4" y="4" width="8" height="8"/><path d="M4 14 H12"/><title>two</title></g>"#);
    assert_eq!(through(r#"<g><g><path d="M4 14 H12"/></g></g>"#, &[2], turn(90.0, 0.0, 0.0)), r#"<g><g><path d="M-14 4 V12"/></g></g>"#, "all the way down");
    // One already turned keeps its turn, about where it now is.
    assert_eq!(through(r#"<g><rect width="8" height="8" transform="rotate(45 4 4)"/></g>"#, &[2], shift(2.0, 3.0)), r#"<g><rect width="8" height="8" transform="rotate(45 6 7)" x="2" y="3"/></g>"#);
    // What the group itself is drawn with stays where its numbers are.
    assert_eq!(through(r#"<g clip-path="url(#c)"><path d="M4 14 H12"/></g>"#, &[2], shift(2.0, 3.0)), r#"<g clip-path="url(#c)" transform="translate(2 3)"><path d="M4 14 H12"/></g>"#);
    let shadowed = r#"<filter id="s"><feDropShadow/></filter><g filter="url(#s)"><path d="M4 14 H12"/></g>"#;
    assert!(through(shadowed, &[4], shift(2.0, 3.0)).ends_with(r#"<g filter="url(#s)"><path d="M6 17 H14"/></g>"#));
    assert!(through(shadowed, &[4], grow(2.0, 2.0)).ends_with(r#"<g filter="url(#s)" transform="scale(2)"><path d="M4 14 H12"/></g>"#));
    // Nor is anything passed to what can't take it: words, pictures,
    // and what Ink doesn't know.
    for held in ["<text>hi</text>", r##"<use href="#a"/>"##, "<image/>", "<mystery/>", r#"<path d="M0 0H4" clip-path="url(#c)"/>"#] {
        assert_eq!(through(&format!("<g>{held}<path d=\"M4 14 H12\"/></g>"), &[2], shift(2.0, 3.0)), format!("<g transform=\"translate(2 3)\">{held}<path d=\"M4 14 H12\"/></g>"));
    }
}

#[test]
fn a_transform_left_by_another_editor_is_taken_in_when_its_node_is_moved() {
    // Boxy's way of scaling: a matrix beside the old numbers.
    assert_eq!(through(r#"<rect width="10" height="10" transform="matrix(0.5, 0, 0, 0.5, 2, 2)"/>"#, &[2], shift(1.0, 0.0)), r#"<rect width="5" height="5" x="3" y="2"/>"#);
    assert_eq!(through(r#"<g transform="matrix(2 0 0 2 1 1)"><circle cx="1" cy="1" r="1"/><path d="M0 0h1"/></g>"#, &[2], shift(1.0, 0.0)), r#"<g><circle cx="4" cy="3" r="2"/><path d="M2 1 H4"/></g>"#);
    // One that has to stay is added to, and stays.
    assert_eq!(through(r#"<g transform="matrix(2 0 0 2 1 1)" clip-path="url(#c)"><path d="M0 0h1"/></g>"#, &[2], shift(1.0, 0.0)), r#"<g transform="matrix(2 0 0 2 2 1)" clip-path="url(#c)"><path d="M0 0h1"/></g>"#);
    // An origin it turned about is worked in, and not left to move it.
    assert_eq!(through(r#"<rect width="4" height="2" transform="rotate(90)" transform-origin="2 1"/>"#, &[2], shift(1.0, 0.0)), r#"<rect width="2" height="4" x="2" y="-1"/>"#);
    assert_eq!(through(r#"<text transform="scale(2)" style="transform-origin: 1px 1px; fill: red">a</text>"#, &[2], shift(1.0, 0.0)), r#"<text transform="matrix(2 0 0 2 0 -1)" style=" fill: red">a</text>"#);
}

#[test]
fn a_move_is_in_the_documents_coordinates_whatever_the_node_is_under() {
    // Under a group that doubles everything, two across is one in its
    // own numbers.
    let under = r#"<g transform="scale(2)" clip-path="url(#c)"><rect x="1" y="1" width="2" height="2"/></g>"#;
    assert_eq!(through(under, &[3], shift(2.0, 0.0)), r#"<g transform="scale(2)" clip-path="url(#c)"><rect x="2" y="1" width="2" height="2"/></g>"#);
    assert_eq!(through(r#"<g transform="rotate(90)" clip-path="url(#c)"><path d="M0 0H4"/></g>"#, &[3], shift(0.0, 3.0)), r#"<g transform="rotate(90)" clip-path="url(#c)"><path d="M3 0 H7"/></g>"#);
    // The root has no transform to take it: everything in it does, each
    // for itself. Definitions stay as they are.
    let all = r#"<defs><path id="d" d="M0 0H1"/></defs><rect width="4" height="4"/><g><circle r="1"/></g>"#;
    assert_eq!(through(all, &[1], shift(1.0, 2.0)), r#"<defs><path id="d" d="M0 0H1"/></defs><rect width="4" height="4" x="1" y="2"/><g><circle r="1" cx="1" cy="2"/></g>"#);
    // One named along with the group it's in moves once, with it.
    let group = r#"<g><rect width="4" height="4"/></g><rect width="1" height="1"/>"#;
    assert_eq!(through(group, &[3, 2, 4, 4], shift(1.0, 0.0)), r#"<g><rect width="4" height="4" x="1"/></g><rect width="1" height="1" x="1"/>"#);
}

#[test]
fn numbers_are_written_as_finely_as_the_document_says() {
    let third = turn(120.0, 0.0, 0.0);
    assert_eq!(through(r#"<line x2="1"/>"#, &[2], third), r#"<line x2="-0.5" y2="0.866"/>"#);
    let mut fine = Document::parse(DocId(1), r#"<svg xmlns:ink="urn:lantern:ink" ink:decimals="5"><line x2="1"/></svg>"#).unwrap();
    fine.apply(&Command::Transform { nodes: vec![NodeId(2)], by: third }).unwrap();
    assert!(fine.to_svg().ends_with(r#"<line x2="-0.5" y2="0.86603"/></svg>"#), "{}", fine.to_svg());
}

#[test]
fn what_cannot_go_through_a_transform_says_so_and_nothing_changes() {
    let inner = r#"<defs><linearGradient id="g"/></defs><rect width="4" height="4"/><g transform="scale(0)"><circle r="1"/></g>"#;
    let mut d = doc(inner);
    let before = d.to_svg();
    let refused = |d: &mut Document, nodes: &[u64], by: Affine| match d.apply(&Command::Transform { nodes: nodes.iter().map(|&n| NodeId(n)).collect(), by }) {
        Err(DocError::Invalid(why)) => why,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused(&mut d, &[4, 3], shift(1.0, 0.0)), "N3 is a <linearGradient>, which shows nowhere itself: only shapes, groups, text, images and uses go through a transform");
    assert_eq!(refused(&mut d, &[6], shift(1.0, 0.0)), "N6 is under a transform that squashes it flat: nothing moves it from there");
    assert!(refused(&mut d, &[4], grow(0.0, 1.0)).starts_with("that transform squashes everything flat"));
    assert!(refused(&mut d, &[4], shift(f64::NAN, 0.0)).starts_with("that transform squashes everything flat"));
    assert_eq!(d.apply(&Command::Transform { nodes: vec![NodeId(4), NodeId(99)], by: shift(1.0, 0.0) }), Err(DocError::NoSuchNode(NodeId(99))));
    assert_eq!(d.to_svg(), before);
    // And what it did is said.
    let applied = d.apply(&Command::Transform { nodes: vec![NodeId(4)], by: shift(1.0, 0.0) }).unwrap();
    assert_eq!((applied.structure_changed(), applied.changed), (false, vec![NodeId(4)]));
    assert!(d.apply(&Command::Transform { nodes: vec![NodeId(4)], by: Affine::IDENTITY }).unwrap().is_nothing());
}
