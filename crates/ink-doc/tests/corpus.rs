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
