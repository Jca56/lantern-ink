//! Tidying changes nothing that shows (ARCHITECTURE §3.4): every corpus
//! file drawn before `Command::Tidy` and after it, with nothing asked
//! for and with everything, has to be the same picture to the byte. So
//! does what it then writes, read back. And a drawing tidied once has
//! nothing left to tidy.

use std::path::PathBuf;

use ink_doc::tidy::{Extra, plan};
use ink_doc::{Command, DocId, Document, Viewport};
use ink_render::{View, render};

fn drawn(doc: &Document) -> Vec<u8> {
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    render(doc, &View::icon(&viewport, 96)).unwrap().rgba
}

#[test]
fn a_tidied_drawing_draws_as_it_did() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    assert!(files.len() >= 144);
    let (mut unused, mut empty, mut declarations, mut comments, mut ids, mut tidier) = (0, 0, 0, 0, 0, 0);
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let svg = std::fs::read_to_string(path).unwrap();
        // As a tool opens it: another editor's marks taken out first.
        let mut doc = Document::parse(DocId(1), &svg).unwrap();
        doc.adopt();
        let before = drawn(&doc);
        for also in [vec![], vec![Extra::Comments, Extra::Ids, Extra::Words]] {
            let mut tidy = doc.clone();
            let would = plan(&tidy, &also);
            let applied = tidy.apply(&Command::Tidy { also: also.clone() }).unwrap();
            assert_eq!(applied.is_nothing(), would.is_nothing(), "{name}");
            assert!(drawn(&tidy) == before, "{name}: tidied (also {also:?}), it draws differently");
            let written = tidy.to_svg();
            let again = Document::parse(DocId(2), &written).unwrap_or_else(|e| panic!("{name}: tidied, it doesn't read: {e}"));
            assert!(drawn(&again) == before, "{name}: tidied and read back, it draws differently");
            assert!(plan(&again, &also).is_nothing(), "{name}: there was more to tidy");
            if also.is_empty() {
                unused += would.unused.len();
                empty += would.empty.len();
                declarations += would.declarations.len();
                tidier += usize::from(!would.is_nothing());
                assert!(written.len() <= svg.len(), "{name}: tidied, it's bigger");
            } else {
                comments += would.comments;
                ids += would.ids.len();
                assert!(!written.contains("<!--"), "{name}: a comment is left");
            }
        }
    }
    println!("{tidier} of {} files had something unused: {unused} definitions, {empty} empty groups and <defs>, {declarations} idle namespace declarations; asked, {comments} comments and {ids} ids nothing refers to", files.len());
    assert!(unused >= 100 && empty >= 10 && comments >= 400, "the corpus has that much to tidy: {unused} definitions, {empty} empty, {comments} comments");
}

/// A clean copy to ship draws what the drawing draws, and carries no
/// comment and nothing of Ink's own.
#[test]
fn a_copy_to_ship_draws_as_the_drawing_does() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    let (mut was, mut now, mut marks) = (0, 0, 0);
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let svg = std::fs::read_to_string(path).unwrap();
        let mut doc = Document::parse(DocId(1), &svg).unwrap();
        // Taken over from Boxy, it carries Ink's namespace where Boxy's was.
        doc.adopt();
        let own = doc.to_svg();
        let clean = ink_doc::tidy::shipped(&doc);
        assert!(doc.to_svg() == own, "{name}: shipping it changed the drawing");
        assert!(!clean.svg.contains("<!--") && !clean.svg.contains("xmlns:ink") && !clean.svg.contains("urn:lantern:ink"), "{name}");
        let again = Document::parse(DocId(2), &clean.svg).unwrap_or_else(|e| panic!("{name}: its clean copy doesn't read: {e}"));
        assert!(drawn(&again) == drawn(&doc), "{name}: its clean copy draws differently");
        assert!(clean.svg.len() <= own.len(), "{name}");
        (was, now, marks) = (was + own.len(), now + clean.svg.len(), marks + clean.marks);
    }
    println!("clean copies of the corpus: {} KB where the drawings are {} KB, {marks} of Ink's own marks left out", now / 1024, was / 1024);
    assert!(now < was);
}
