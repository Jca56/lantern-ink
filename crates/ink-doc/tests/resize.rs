//! `Command::Resize` (ARCHITECTURE §3.4): a shape's outline put through
//! a transform with its line left as it is, which is what a handle on
//! a shape does in the window. A stroke keeps its width, so nothing
//! holds a stretch back from a shape's own numbers. These are about
//! what the file then says; that the outline goes where the transform
//! puts it is checked against `Command::Transform` on every corpus
//! file at the end.

use std::path::PathBuf;

use ink_doc::geometry::page_bounds;
use ink_doc::style::prop;
use ink_doc::{Command, DocId, Document, Kind, NodeId};
use ink_geom::{Affine, Vec2};

const OPEN: &str = "<svg viewBox=\"0 0 24 24\">";

fn doc(inner: &str) -> Document {
    Document::parse(DocId(1), &format!("{OPEN}{inner}</svg>")).unwrap()
}

fn inside(d: &Document) -> String {
    let out = d.to_svg();
    out[OPEN.len()..out.len() - "</svg>".len()].to_owned()
}

/// What's in the drawing once `ids` have been resized by `by`.
fn resized(inner: &str, ids: &[u64], by: Affine) -> String {
    let mut d = doc(inner);
    d.apply(&Command::Resize { nodes: ids.iter().map(|&n| NodeId(n)).collect(), by }).unwrap();
    inside(&d)
}

/// And once they've been through it as a transform, lines and all.
fn transformed(inner: &str, ids: &[u64], by: Affine) -> String {
    let mut d = doc(inner);
    d.apply(&Command::Transform { nodes: ids.iter().map(|&n| NodeId(n)).collect(), by }).unwrap();
    inside(&d)
}

fn grow(x: f64, y: f64) -> Affine {
    Affine::scale(x, y)
}

fn about(x: f64, y: f64, by: Affine) -> Affine {
    by.about(Vec2::new(x, y))
}

#[test]
fn a_stroked_shape_stretched_keeps_its_line_and_its_plain_numbers() {
    let rect = r##"<rect x="4" y="4" width="8" height="8" rx="2" stroke="#000" stroke-width="2"/>"##;
    assert_eq!(resized(rect, &[2], grow(2.0, 0.5)), r##"<rect x="8" y="2" width="16" height="4" rx="2" stroke="#000" stroke-width="2"/>"##);
    // As a transform, its stroke would have to grow more one way than
    // the other: only a `transform` can say that.
    assert!(transformed(rect, &[2], grow(2.0, 0.5)).contains("transform="));
    // Evenly, a transform grows the line with it; a resize doesn't.
    assert_eq!(transformed(rect, &[2], grow(2.0, 2.0)), r##"<rect x="8" y="8" width="16" height="16" rx="4" stroke="#000" stroke-width="4"/>"##);
    assert_eq!(resized(rect, &[2], grow(2.0, 2.0)), r##"<rect x="8" y="8" width="16" height="16" rx="2" stroke="#000" stroke-width="2"/>"##);
    // A path takes anything; its dashes stay the length they were.
    let path = r##"<path d="M0 0 H4 V4 Z" stroke="#000" stroke-width="1.5" stroke-dasharray="2 1"/>"##;
    assert_eq!(resized(path, &[2], grow(3.0, 1.0)), r##"<path d="M0 0 H12 V4 Z" stroke="#000" stroke-width="1.5" stroke-dasharray="2 1"/>"##);
    let ellipse = r##"<ellipse cx="5" cy="6" rx="3" ry="1" stroke="red"/>"##;
    assert_eq!(resized(ellipse, &[2], about(5.0, 6.0, grow(1.0, 4.0))), r##"<ellipse cx="5" cy="6" rx="3" ry="4" stroke="red"/>"##);
    // Mirrored by a handle dragged through its far side.
    assert_eq!(resized(rect, &[2], about(4.0, 4.0, grow(-1.0, 1.0))), r##"<rect x="-4" y="4" width="8" height="8" rx="2" stroke="#000" stroke-width="2"/>"##);
    // What a resize leaves as it is, it leaves as the file says it.
    let same = r##"<rect x="2.0" width="1e1" height="4px" stroke="#000"/>"##;
    assert_eq!(resized(same, &[2], Affine::IDENTITY), same);
}

#[test]
fn a_circle_stretched_one_way_becomes_an_ellipse() {
    assert_eq!(resized(r##"<circle cx="5" cy="6" r="2" stroke="#000"/>"##, &[2], grow(2.0, 1.0)), r##"<ellipse cx="10" cy="6" rx="4" ry="2" stroke="#000"/>"##);
    // Its radii stand where its radius did, written as that was.
    assert_eq!(resized("<circle r='2' cx='5' fill=\"red\"></circle>", &[2], grow(1.0, 3.0)), "<ellipse rx='2' ry='6' cx='5' fill=\"red\"></ellipse>");
    // Evenly, it's a circle still.
    assert_eq!(resized(r##"<circle cx="5" cy="6" r="2" stroke="#000"/>"##, &[2], grow(2.0, 2.0)), r##"<circle cx="10" cy="12" r="4" stroke="#000"/>"##);
    // As a transform it stays a circle, under a `transform`.
    assert!(transformed(r##"<circle cx="5" cy="6" r="2"/>"##, &[2], grow(2.0, 1.0)).starts_with("<circle "));
}

#[test]
fn a_turned_rect_takes_a_resize_along_its_own_sides() {
    let turned = r##"<rect x="4" y="4" width="8" height="8" rx="2" stroke="#000" transform="rotate(45 8 8)"/>"##;
    // Evenly about its middle: bigger, and turned as it was.
    assert_eq!(resized(turned, &[2], about(8.0, 8.0, grow(2.0, 2.0))), r##"<rect x="0" y="0" width="16" height="16" rx="2" stroke="#000" transform="rotate(45 8 8)"/>"##);
    // Across the way it's turned, no rect is what that makes of it:
    // its `transform` says it, as a transform's would.
    let across = resized(turned, &[2], grow(2.0, 1.0));
    assert_eq!(across, transformed(turned, &[2], grow(2.0, 1.0)));
    assert!(across.contains("width=\"8\"") && across.contains("matrix("), "{across}");
}

#[test]
fn a_group_passes_a_resize_down_and_what_holds_a_shape_still_holds_it() {
    let group = r##"<g stroke="#000"><rect x="2" y="2" width="4" height="4"/><circle cx="10" cy="4" r="2"/></g>"##;
    assert_eq!(resized(group, &[2], grow(2.0, 1.0)), r##"<g stroke="#000"><rect x="4" y="2" width="8" height="4"/><ellipse cx="20" cy="4" rx="4" ry="2"/></g>"##);
    // Under a group that scales, a resize is still in the document's
    // coordinates, and the line is the width it showed at.
    let under = r##"<g transform="scale(2)"><rect x="1" y="1" width="2" height="2" stroke="#000" stroke-width="0.5"/></g>"##;
    assert_eq!(resized(under, &[3], grow(2.0, 1.0)), r##"<g transform="scale(2)"><rect x="2" y="1" width="4" height="2" stroke="#000" stroke-width="0.5"/></g>"##);
    // A shadow wouldn't grow with baked numbers: its shape keeps a
    // `transform`, as ever.
    let shadowed = r##"<defs><filter id="s"><feDropShadow/></filter></defs><rect width="4" height="4" stroke="#000" filter="url(#s)"/>"##;
    assert_eq!(resized(shadowed, &[5], grow(2.0, 1.0)), transformed(shadowed, &[5], grow(2.0, 1.0)));
    // A gradient in the shape's own coordinates, its alone, goes with it.
    let painted = r##"<defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="4" y2="0"><stop/></linearGradient></defs><rect width="4" height="4" fill="url(#g)" stroke="#000"/>"##;
    let out = resized(painted, &[5], grow(2.0, 1.0));
    assert!(out.contains(r##"<rect width="8" height="4" fill="url(#g)" stroke="#000"/>"##) && out.contains("gradientTransform") && !out.contains("stroke-width"), "{out}");
    // What's locked isn't resized.
    let mut locked = doc(r##"<rect width="4" height="4"/>"##);
    locked.apply(&Command::SetLocked { nodes: vec![NodeId(2)], locked: true }).unwrap();
    assert!(locked.apply(&Command::Resize { nodes: vec![NodeId(2)], by: grow(2.0, 1.0) }).unwrap_err().to_string().contains("is locked"));
}

/// Every shape and group of every corpus file, resized: it shows where
/// a transform would have put it, and of the shapes with a line, how
/// many kept it at the width it was.
#[test]
fn a_resize_puts_every_outline_where_the_transform_would() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    let (mut resizes, mut lined, mut kept) = (0usize, 0usize, 0usize);
    for path in files.iter().step_by(6) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut d = Document::parse(DocId(1), &std::fs::read_to_string(path).unwrap()).unwrap();
        d.adopt();
        let before = page_bounds(&d);
        let nodes: Vec<NodeId> = d.descendants(d.root()).into_iter().skip(1).filter(|id| before.contains_key(id)).collect();
        for &id in &nodes {
            let was = before[&id];
            let by = Affine::scale(1.5, 0.75).about(was.min);
            let mut resized = d.clone();
            // What a transform refuses, a resize does too.
            let mut moved = d.clone();
            let asked = Command::Transform { nodes: vec![id], by };
            if moved.apply(&asked).is_err() {
                assert!(resized.apply(&Command::Resize { nodes: vec![id], by }).is_err(), "{name}: {id}");
                continue;
            }
            resized.apply(&Command::Resize { nodes: vec![id], by }).unwrap_or_else(|e| panic!("{name}: {id}: {e}"));
            resizes += 1;
            let (Some(is), Some(would)) = (page_bounds(&resized).get(&id).copied(), page_bounds(&moved).get(&id).copied()) else { panic!("{name}: {id} shows nowhere now") };
            let slack = 2e-3 * (1.0 + was.max.abs().max_element().max(was.min.abs().max_element()));
            assert!((is.min - would.min).abs().max_element() <= slack && (is.max - would.max).abs().max_element() <= slack, "{name}: {id}: resized to {is:?}, a transform puts it at {would:?}");
            // A shape with a line, still without a transform of its
            // own: its line is said as it was.
            let node = d.node(id).unwrap();
            if node.kind.is_shape() && prop(node, "stroke").is_some_and(|s| s != "none") {
                lined += 1;
                let now = resized.node(id).unwrap();
                if now.attr("transform") == node.attr("transform") {
                    kept += 1;
                    assert_eq!(prop(now, "stroke-width"), prop(node, "stroke-width"), "{name}: {id}");
                    assert!(now.kind != Kind::Circle || node.kind == Kind::Circle);
                }
            }
        }
    }
    println!("{resizes} resizes; of {lined} shapes with a line of their own, {kept} took it into their numbers and kept the line as it was");
    assert!(resizes >= 250 && kept * 10 >= lined * 7, "{resizes} resizes, {kept} of {lined} lines kept");
}
