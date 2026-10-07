//! The corpus (`tests/corpus`, ARCHITECTURE §10): the Lantern projects'
//! own SVGs. Whatever Ink does to a document, these are the files it has
//! to do it to without harm.

use std::path::PathBuf;

use ink_doc::{DocId, Document};

/// Every corpus file: its name and its text.
fn corpus() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "svg"))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()))))
        .collect();
    files.sort();
    assert!(files.len() >= 144, "the corpus is {} files", files.len());
    files
}

#[test]
fn every_file_reads_and_writes_back_byte_for_byte() {
    let mut nodes = 0;
    for (name, text) in corpus() {
        let doc = Document::parse(DocId(1), &text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(doc.to_svg() == text, "{name} came back changed");
        nodes += doc.len();
    }
    assert!(nodes > 1000, "{nodes} elements in all");
}

#[test]
fn an_edit_and_its_undo_leave_every_file_as_it_was() {
    use ink_doc::{Command, Element, Place};
    for (name, text) in corpus() {
        let mut doc = Document::parse(DocId(1), &text).unwrap();
        let before = doc.snapshot();
        let last = *doc.descendants(doc.root()).last().unwrap();
        let edit = Command::Batch(vec![
            Command::SetAttr { node: doc.root(), name: "data-ink-test".into(), value: Some("1 & 2".into()) },
            Command::Insert { place: Place::LastIn(doc.root()), elements: vec![Element::new("g").child(Element::new("rect").with("width", "1"))] },
            Command::Delete { nodes: if last == doc.root() { vec![] } else { vec![last] } },
        ]);
        let applied = doc.apply(&edit).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!applied.is_nothing() && doc.to_svg() != text, "{name}");
        // What an edit leaves is still a document, and reads back the same.
        let edited = doc.to_svg();
        assert!(Document::parse(DocId(2), &edited).unwrap_or_else(|e| panic!("{name}, edited: {e}")).to_svg() == edited, "{name}");
        doc.restore(&before);
        assert!(doc.to_svg() == text, "{name} wasn't the same after undo");
    }
}

#[test]
fn one_changed_attribute_is_the_only_bytes_that_differ() {
    use ink_doc::Command;
    const ADDED: &str = "data-ink-test=\"1\"";
    for (name, text) in corpus() {
        let mut doc = Document::parse(DocId(1), &text).unwrap();
        doc.apply(&Command::SetAttr { node: doc.root(), name: "data-ink-test".into(), value: Some("1".into()) }).unwrap();
        let out = doc.to_svg();
        let at = out.find(ADDED).unwrap_or_else(|| panic!("{name}: the attribute isn't there"));
        // The new attribute and the whitespace put before it, cut out
        // again: the file as it was, to the byte.
        let lead = out[..at].len() - out[..at].trim_end().len();
        assert!(lead > 0, "{name}: nothing separates it from what's before");
        assert!(format!("{}{}", &out[..at - lead], &out[at + ADDED.len()..]) == text, "{name}: something else moved");
    }
}

#[test]
fn taking_a_file_over_leaves_nothing_of_boxys_and_a_file_that_still_reads() {
    let (mut taken, mut untouched) = (0, 0);
    for (name, text) in corpus() {
        let mut doc = Document::parse(DocId(1), &text).unwrap();
        let adopted = doc.adopt();
        let out = doc.to_svg();
        assert!(!out.contains("bx:") && !out.contains("boxy-svg"), "{name} still has Boxy's marks");
        if adopted.is_nothing() {
            assert!(out == text, "{name} had nothing to take out, yet changed");
            untouched += 1;
        } else {
            assert!(text.contains("boxy-svg") && out.contains("xmlns:ink=\"urn:lantern:ink\""), "{name}");
            assert!(Document::parse(DocId(2), &out).unwrap_or_else(|e| panic!("{name}, taken over: {e}")).to_svg() == out, "{name}");
            // Taking it over twice finds nothing the second time.
            assert!(doc.adopt().is_nothing(), "{name}");
            taken += 1;
        }
    }
    assert!(taken > 50 && untouched > 30, "{taken} taken over, {untouched} untouched");
}

/// Every file's shapes, each made one with the next by each of the
/// four operations: real outlines, drawn by hand and by Boxy, lying on
/// each other the way icons' parts do. None may be more than Ink can
/// work out, and what the four results hold has to add up.
#[test]
fn neighbouring_shapes_of_every_file_are_made_one() {
    use ink_doc::geometry::{path_of, to_doc};
    use ink_doc::{Command, Kind, NodeId};
    use ink_geom::{Combine, FillRule, Path, combine};

    let length = |path: &Path| -> f64 { path.flatten(0.01).iter().map(|line| line.points.windows(2).map(|p| p[0].distance(p[1])).sum::<f64>()).sum() };
    let (mut pairs, mut made) = (0, 0);
    for (name, text) in corpus() {
        let doc = Document::parse(DocId(1), &text).unwrap();
        // A shape as it shows: its outline in the drawing's coordinates.
        let shown = |doc: &Document, id: NodeId| doc.get(id).zip(to_doc(doc, id)).map(|(node, t)| path_of(node).transformed(&t));
        let shapes: Vec<NodeId> = doc
            .descendants(doc.root())
            .into_iter()
            .filter(|id| doc.get(*id).is_some_and(|n| matches!(n.kind, Kind::Path | Kind::Rect | Kind::Circle | Kind::Ellipse | Kind::Polygon)))
            .filter(|id| shown(&doc, *id).is_some_and(|p| p.bounds().is_some_and(|b| b.width() > 0.0 && b.height() > 0.0)))
            .collect();
        for pair in shapes.windows(2) {
            let (Some(a), Some(b)) = (shown(&doc, pair[0]), shown(&doc, pair[1])) else { continue };
            pairs += 1;
            // What each covers, by the rule the Command fills it with:
            // read back from a union of it alone.
            let covers = |id: NodeId| {
                let mut alone = doc.clone();
                match alone.apply(&Command::Boolean { nodes: vec![id], how: Combine::Union }) {
                    Ok(_) => shown(&alone, id).map_or(0.0, |p| p.area().abs()),
                    Err(e) => {
                        assert!(e.to_string().starts_with("nothing would be left"), "{name} {id} alone: {e}");
                        0.0
                    }
                }
            };
            let held: Vec<f64> = [Combine::Union, Combine::Subtract, Combine::Intersect, Combine::Exclude]
                .into_iter()
                .map(|how| {
                    let mut tried = doc.clone();
                    match tried.apply(&Command::Boolean { nodes: pair.to_vec(), how }) {
                        Ok(_) => {
                            made += 1;
                            shown(&tried, pair[0]).map_or(0.0, |p| p.area().abs())
                        }
                        // Nothing left is an answer; not knowing isn't.
                        Err(e) => {
                            assert!(e.to_string().contains("nothing would be left"), "{name}: {how:?} of {} and {}: {e}", pair[0], pair[1]);
                            0.0
                        }
                    }
                })
                .collect();
            let (hold_a, hold_b) = (covers(pair[0]), covers(pair[1]));
            // To what the file's three decimals and the seams left out
            // can change: a thousandth of a unit along every outline.
            let slack = 2e-3 * (length(&a) + length(&b)) + 1e-9;
            let [union, subtract, intersect, exclude] = held[..] else { unreachable!() };
            let said = || format!("{name}: {} and {}: union {union}, subtract {subtract}, intersect {intersect}, exclude {exclude} of {hold_a} and {hold_b} (to {slack})", pair[0], pair[1]);
            assert!((union + intersect - hold_a - hold_b).abs() <= slack, "{}", said());
            assert!((subtract - (hold_a - intersect)).abs() <= slack, "{}", said());
            assert!((exclude - (union - intersect)).abs() <= slack, "{}", said());
            // The same from plain geometry, with nothing left out: no
            // pair is beyond working out.
            for how in [Combine::Union, Combine::Subtract, Combine::Intersect, Combine::Exclude] {
                assert!(combine(&[(&a, FillRule::NonZero), (&b, FillRule::NonZero)], how, 0.0).is_ok(), "{name}: {how:?} of {} and {}", pair[0], pair[1]);
            }
        }
    }
    assert!(pairs >= 3000 && made >= 10000, "{pairs} pairs, {made} results");
    println!("{pairs} pairs of neighbouring shapes, {made} results");
}
