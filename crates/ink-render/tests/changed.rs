//! An edit costs the parts of the picture it touches, and no others
//! (ARCHITECTURE §8): `Plan::changed_from` says where a drawing's
//! picture can differ from what it was, and the window draws those
//! tiles again and keeps the rest. So everything outside what it says
//! has to be the same to the byte, after any edit there is. Here
//! corpus files are edited a dozen ways (moved, scaled, repainted,
//! faded, restacked, taken out, put in, their definitions changed), and
//! each picture is held against the one before, tile by tile.
//!
//! Every eighth file with `cargo test`; all of them, with more edits
//! each, in a few minutes:
//! `cargo test --release -p ink-render --test changed -- --ignored
//! --nocapture`.

use std::collections::HashMap;
use std::path::PathBuf;

use ink_doc::{Command, DocId, Document, Kind, NodeId, Place, Viewport, elements};
use ink_geom::{Affine, Rect, Vec2};
use ink_render::Plan;

/// A tile's side here, px: small, so that a box said too tight shows.
const SIDE: u32 = 32;
/// The longer side of each picture, px.
const FIT: f64 = 192.0;

fn corpus() -> Vec<(String, Document)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    assert!(files.len() >= 144);
    files
        .iter()
        .map(|path| {
            let mut doc = Document::parse(DocId(1), &std::fs::read_to_string(path).unwrap()).unwrap();
            doc.adopt();
            (path.file_name().unwrap().to_string_lossy().into_owned(), doc)
        })
        .collect()
}

/// `doc` laid out with its page `FIT` px long.
fn laid(doc: &Document) -> Plan {
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    let zoom = FIT / viewport.size.x.max(viewport.size.y).max(1e-9);
    Plan::new(doc, &Affine::scale(zoom, zoom), false)
}

/// A picture's tiles, each drawn when it's first asked for.
struct Tiled<'a> {
    plan: &'a Plan,
    drawn: HashMap<(i64, i64), Vec<u8>>,
}

impl<'a> Tiled<'a> {
    fn of(plan: &'a Plan) -> Tiled<'a> {
        Tiled { plan, drawn: HashMap::new() }
    }

    fn tile(&mut self, i: i64, j: i64) -> &[u8] {
        self.drawn.entry((i, j)).or_insert_with(|| self.plan.part(i * SIDE as i64, j * SIDE as i64, SIDE, SIDE).unwrap().rgba)
    }
}

/// The tiles of both pictures that `changed` doesn't touch, each
/// checked to be the same in `old` and `new`: how many there were, and
/// how many it does touch.
fn held(name: &str, what: &str, old: &mut Tiled, new: &Plan, changed: &[Rect]) -> (usize, usize) {
    let Some(all) = [old.plan.bounds(), new.bounds()].into_iter().flatten().reduce(|a, b| a.union(&b)) else { return (0, 0) };
    // No further out than a screen or so: a filter's region can be
    // far bigger than anything worth looking at.
    let all = all.intersection(&Rect::new(Vec2::splat(-2.0 * FIT), Vec2::splat(3.0 * FIT)));
    let side = SIDE as f64;
    let (i0, j0, i1, j1) = ((all.min.x / side).floor() as i64, (all.min.y / side).floor() as i64, (all.max.x / side).ceil() as i64, (all.max.y / side).ceil() as i64);
    let (mut same, mut touched) = (0, 0);
    for j in j0..j1 {
        for i in i0..i1 {
            let tile = Rect::from_xywh(i as f64 * side, j as f64 * side, side, side);
            if changed.iter().any(|r| r.intersects(&tile)) {
                touched += 1;
                continue;
            }
            let is = new.part(i * SIDE as i64, j * SIDE as i64, SIDE, SIDE).unwrap();
            assert!(old.tile(i, j) == is.rgba, "{name}: {what}: the tile at {i}, {j} changed, outside {changed:?}");
            same += 1;
        }
    }
    (same, touched)
}

/// The nodes of `doc` that draw something: shapes, texts, and groups.
fn drawn(doc: &Document) -> Vec<NodeId> {
    let in_defs = |id: NodeId| doc.ancestors(id).any(|n| matches!(n.kind, Kind::Defs | Kind::ClipPath | Kind::Mask | Kind::Pattern | Kind::Marker | Kind::Symbol));
    doc.descendants(doc.root()).into_iter().skip(1).filter(|&id| doc.get(id).is_some_and(|n| n.kind.is_shape() || n.kind.is_group() || n.kind == Kind::Text) && !in_defs(id)).collect()
}

/// A few of `ids`, spread through them: the first, the last, and some
/// between.
fn some(ids: &[NodeId], n: usize) -> Vec<NodeId> {
    if ids.len() <= n {
        return ids.to_vec();
    }
    (0..n).map(|k| ids[k * (ids.len() - 1) / (n - 1)]).collect()
}

/// Every `every`th corpus file, edited up to `most` ways: how many
/// edits that was, and of the tiles looked at, how many were kept and
/// how many an edit touched.
fn sweep(every: usize, most: usize) -> (usize, usize, usize) {
    let (mut edits, mut same, mut touched, mut nothing) = (0usize, 0usize, 0usize, 0usize);
    for (name, doc) in corpus().into_iter().step_by(every) {
        let before = laid(&doc);
        assert!(before.changed_from(&before).is_empty(), "{name}: a picture differs from itself");
        let nodes = drawn(&doc);
        let Some(&top) = nodes.last() else { continue };
        let unit = Viewport::of(doc.node(doc.root()).unwrap()).view.x.max(1e-6) / 24.0;
        let mut commands: Vec<(String, Command)> = Vec::new();
        for id in some(&nodes, if most > 16 { 4 } else { 2 }) {
            commands.push((format!("{id} moved"), Command::Transform { nodes: vec![id], by: Affine::translate(1.5 * unit, -0.75 * unit) }));
            commands.push((format!("{id} scaled"), Command::Transform { nodes: vec![id], by: Affine::scale(1.25, 0.8) }));
            commands.push((format!("{id} faded"), Command::SetStyle { nodes: vec![id], set: vec![("opacity".into(), Some("0.4".into()))] }));
            commands.push((format!("{id} repainted"), Command::SetStyle { nodes: vec![id], set: vec![("fill".into(), Some("#1e90ff".into())), ("stroke".into(), Some("#102030".into()))] }));
            commands.push((format!("{id} taken out"), Command::Delete { nodes: vec![id] }));
            commands.push((format!("{id} hidden"), Command::SetAttr { node: id, name: "display".into(), value: Some("none".into()) }));
            if id != top {
                commands.push((format!("{id} brought to the front"), Command::Move { nodes: vec![id], place: Place::LastIn(doc.root()) }));
            }
        }
        // Two things at once, apart in the stack.
        if nodes.len() >= 3 {
            commands.push(("the first and the last moved".into(), Command::Transform { nodes: vec![nodes[0], top], by: Affine::translate(-unit, 2.0 * unit) }));
        }
        let square = format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#e94560\" stroke=\"#000\" stroke-width=\"{}\"/>", 3.0 * unit, 4.0 * unit, 5.0 * unit, 2.0 * unit, 0.5 * unit);
        commands.push(("a square put on top".into(), Command::Insert { place: Place::LastIn(doc.root()), elements: elements(&square).unwrap() }));
        commands.push(("a square put underneath".into(), Command::Insert { place: Place::FirstIn(doc.root()), elements: elements(&square).unwrap() }));
        // What others are drawn with: a gradient's stop, a filter's
        // blur, a clip path's shape.
        for id in doc.descendants(doc.root()) {
            let node = doc.get(id).unwrap();
            match node.name.as_str() {
                "stop" => commands.push((format!("{id}, a stop, recoloured"), Command::SetAttr { node: id, name: "stop-color".into(), value: Some("#00ff88".into()) })),
                "feGaussianBlur" | "feDropShadow" => commands.push((format!("{id}, a blur, widened"), Command::SetAttr { node: id, name: "stdDeviation".into(), value: Some(format!("{}", 0.9 * unit)) })),
                _ => {}
            }
        }
        commands.truncate(most);
        let mut was = Tiled::of(&before);
        for (what, command) in commands {
            let mut after = doc.clone();
            // An edit this drawing refuses (a locked node, a shape that
            // can't take it) is no edit.
            if after.apply(&command).is_err() {
                continue;
            }
            let now = laid(&after);
            let changed = now.changed_from(&before);
            let (s, t) = held(&name, &what, &mut was, &now, &changed);
            // An undo is an edit too, and touches the same tiles.
            let back = before.changed_from(&now);
            let tiles = |rects: &[Rect]| {
                let mut all: Vec<(i64, i64)> = rects.iter().flat_map(|r| ((r.min.x / SIDE as f64).floor() as i64..(r.max.x / SIDE as f64).ceil() as i64).flat_map(move |i| ((r.min.y / SIDE as f64).floor() as i64..(r.max.y / SIDE as f64).ceil() as i64).map(move |j| (i, j)))).collect();
                all.sort();
                all.dedup();
                all
            };
            assert!(tiles(&changed) == tiles(&back), "{name}: {what}: undone, it touches other tiles");
            edits += 1;
            same += s;
            touched += t;
            nothing += usize::from(changed.is_empty());
        }
    }
    println!("{edits} edits: {same} tiles kept, {touched} drawn again ({:.0} % kept); {nothing} edits changed nothing that shows", 100.0 * same as f64 / (same + touched).max(1) as f64);
    (edits, same, touched)
}

#[test]
fn what_an_edit_leaves_alone_is_the_same_to_the_byte() {
    let (edits, same, touched) = sweep(8, 12);
    assert!(edits >= 150, "{edits} edits");
    // Said too wide, everything is drawn again every time: most of
    // what these edits leave alone has to be seen to be left alone.
    assert!(same >= touched, "{same} tiles kept, {touched} drawn again");
}

#[test]
#[ignore = "every corpus file: a few minutes, in release"]
fn every_corpus_file() {
    let (edits, same, touched) = sweep(1, 48);
    assert!(edits >= 3000 && same >= touched * 2, "{edits} edits: {same} tiles kept, {touched} drawn again");
}

#[test]
fn a_small_thing_moved_costs_the_tiles_round_it() {
    let doc = Document::parse(DocId(1), r##"<svg viewBox="0 0 96 96"><defs><filter id="s" x="-50%" y="-50%" width="200%" height="200%"><feDropShadow dx="1" dy="1" stdDeviation="1"/></filter></defs><rect width="96" height="96" fill="#234"/><g filter="url(#s)"><rect x="8" y="8" width="6" height="6" fill="#0af"/><circle cx="30" cy="30" r="6" fill="#fc0"/><rect x="60" y="60" width="20" height="20" fill="#e94560"/></g><circle cx="80" cy="16" r="5" fill="#fff"/></svg>"##).unwrap();
    let ids = drawn(&doc);
    let (corner, dot, small) = (ids[2], ids[3], ids[5]);
    let before = laid(&doc);
    let area = |rects: &[Rect]| rects.iter().map(Rect::area).sum::<f64>();
    let whole = FIT * FIT;
    // On its own at the top of the stack: where it was and where it is.
    let mut moved = doc.clone();
    moved.apply(&Command::Transform { nodes: vec![small], by: Affine::translate(-3.0, 2.0) }).unwrap();
    let changed = laid(&moved).changed_from(&before);
    assert!(changed.len() == 2 && area(&changed) < whole / 20.0, "{changed:?}");
    // Inside a group with a shadow: its shadow goes with it, as far
    // as the shadow reaches, and the square beside it stays.
    let mut moved = doc.clone();
    moved.apply(&Command::Transform { nodes: vec![dot], by: Affine::translate(2.0, 0.0) }).unwrap();
    let after = laid(&moved);
    let changed = after.changed_from(&before);
    // The group's box is the same (the squares hold its corners), so
    // its filter's region is too: only what's in it changed.
    let square = Rect::from_xywh(60.0 * 2.0, 60.0 * 2.0, 40.0, 40.0);
    assert!(!changed.is_empty() && area(&changed) < whole / 6.0 && changed.iter().all(|r| !r.intersects(&square)), "{changed:?}");
    let (same, touched) = held("inline", "a dot in a shadowed group", &mut Tiled::of(&before), &after, &changed);
    assert!(same > touched * 3, "{same} kept, {touched} drawn again");
    // The square that holds the group's corner: the group's box moves
    // with it, and its filter's region with that. Still only what's
    // round the square, and the edge of the region (where nothing is).
    let mut moved = doc.clone();
    moved.apply(&Command::Transform { nodes: vec![corner], by: Affine::translate(-2.0, -1.0) }).unwrap();
    let after = laid(&moved);
    let changed = after.changed_from(&before);
    assert!(!changed.is_empty() && area(&changed) < whole / 8.0 && changed.iter().all(|r| !r.intersects(&square)), "{changed:?}");
    let (same, touched) = held("inline", "the corner of a shadowed group", &mut Tiled::of(&before), &after, &changed);
    assert!(same > touched * 3, "{same} kept, {touched} drawn again");
}
