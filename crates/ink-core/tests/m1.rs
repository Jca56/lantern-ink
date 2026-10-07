//! M1's done-test (ARCHITECTURE §11), on every file of the corpus, and
//! the core's own behaviour around files and history.

use std::path::{Path, PathBuf};

use ink_core::ink_doc::{Element, Viewport};
use ink_core::{Actor, Autosave, Command, Core, CoreError, DocId, NodeId, Place, View};

/// A folder of this test's own, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ink-core-m1").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Every corpus file's path.
fn corpus() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    assert!(files.len() >= 144, "the corpus is {} files", files.len());
    files
}

fn edit(core: &Core, doc: DocId) -> Command {
    let root = core.doc(doc).unwrap().root();
    Command::Batch(vec![
        Command::SetAttr { node: root, name: "data-ink-test".into(), value: Some("m1".into()) },
        Command::Insert { place: Place::LastIn(root), elements: vec![Element::new("circle").with("cx", "3").with("cy", "3").with("r", "2").with("fill", "#ffc800")] },
    ])
}

#[test]
fn every_corpus_file_opens_edits_saves_and_comes_back() {
    let dir = scratch("corpus");
    let (mut taken_over, mut untouched) = (0, 0);
    for source in corpus() {
        let name = source.file_name().unwrap().to_string_lossy().into_owned();
        let original = std::fs::read(&source).unwrap();
        let path = dir.join(&name);
        std::fs::write(&path, &original).unwrap();

        let mut core = Core::headless();
        let opened = core.open_file(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let doc = opened.doc;
        assert_eq!(core.is_modified(doc), Ok(false), "{name}: just opened");
        let as_opened = core.doc(doc).unwrap().to_svg();
        if opened.adopted.is_nothing() {
            // Untouched means byte-identical: in memory, and saved.
            assert!(as_opened.as_bytes() == original, "{name} changed on the way in");
            let copy = dir.join(format!("copy-{name}"));
            core.save(doc, Some(&copy)).unwrap();
            assert!(std::fs::read(&copy).unwrap() == original, "{name} saved differently");
            core.save(doc, Some(&path)).unwrap();
            untouched += 1;
        } else {
            assert!(!as_opened.contains("boxy-svg"), "{name}");
            assert!(std::fs::read(&path).unwrap() == original, "{name}: opening must not touch the file");
            taken_over += 1;
        }

        // An edit, by Claude: one step, saved, and the same when read back.
        let applied = core.apply(doc, &edit(&core, doc), Actor::Claude, "test edit").unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(applied.created.len(), 1, "{name}");
        assert_eq!(core.is_modified(doc), Ok(true), "{name}");
        assert_eq!(core.save(doc, None).as_deref(), Ok(path.as_path()), "{name}");
        assert_eq!(core.is_modified(doc), Ok(false), "{name}");
        let edited = core.doc(doc).unwrap().to_svg();
        assert!(std::fs::read_to_string(&path).unwrap() == edited, "{name}: the file isn't the document");
        let mut again = Core::headless();
        let reopened = again.open_file(&path).unwrap_or_else(|e| panic!("{name}, saved: {e}"));
        assert!(reopened.adopted.is_nothing(), "{name}: nothing of another editor's is left to take out");
        assert!(again.doc(reopened.doc).unwrap().to_svg() == edited, "{name}");
        let view = View::icon(&core.viewport(doc).unwrap(), 64);
        assert!(again.render(reopened.doc, &view).unwrap() == core.render(doc, &view).unwrap(), "{name}: saved and loaded, it draws the same");

        // Undone, it's as it was opened; and so is the file, saved again.
        assert_eq!(core.undo(doc).map(|s| (s.label, s.actor)), Ok(("test edit".to_owned(), Actor::Claude)), "{name}");
        assert_eq!(core.is_modified(doc), Ok(true), "{name}: undone, it's no longer what's on disk");
        assert!(core.doc(doc).unwrap().to_svg() == as_opened, "{name}: undo didn't bring it back");
        assert_eq!(core.undo(doc), Err(CoreError::NothingToUndo), "{name}");
        core.save(doc, None).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap() == as_opened, "{name}");
        if opened.adopted.is_nothing() {
            assert!(std::fs::read(&path).unwrap() == original, "{name}: edit, save, undo, save: not the bytes it started as");
        }
        core.redo(doc).unwrap();
        assert!(core.doc(doc).unwrap().to_svg() == edited, "{name}");

        // A picture of it, on disk.
        let png = dir.join(format!("{name}.png"));
        core.export_png(doc, &view, &png).unwrap();
        let picture = lntrn_image::png::decode(&std::fs::read(&png).unwrap()).unwrap();
        assert!(picture == core.render(doc, &view).unwrap(), "{name}: the PNG isn't the picture");
        assert_eq!((picture.width, picture.height), (64, 64));
    }
    assert!(taken_over > 50 && untouched > 30, "{taken_over} taken over from another editor, {untouched} untouched");
    // No half-written files left behind.
    assert!(std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).all(|e| !e.file_name().to_string_lossy().contains(".ink-")));
}

#[test]
fn a_new_drawing_has_no_file_until_it_is_given_one() {
    let dir = scratch("new");
    let mut core = Core::headless();
    let doc = core.new_doc(24.0, 24.0);
    assert_eq!(doc.to_string(), "d1");
    assert_eq!(core.is_modified(doc), Ok(true), "never saved");
    assert_eq!(core.save(doc, None), Err(CoreError::NoPath(doc)));
    let path = dir.join("new.svg");
    assert_eq!(core.save(doc, Some(&path)), Ok(path.clone()));
    assert_eq!((core.is_modified(doc), core.path(doc)), (Ok(false), Ok(Some(path.as_path()))));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\">\n</svg>\n");
    let viewport = core.viewport(doc).unwrap();
    assert_eq!(viewport, Viewport { size: (24.0, 24.0).into(), to_page: Default::default(), view: (24.0, 24.0).into() });
    // The window's documents are told from a server's by their names.
    let mut window = Core::window(7);
    let (a, b) = (window.new_doc(8.0, 8.0), window.new_doc(8.0, 8.0));
    assert_eq!((a.to_string(), b.to_string(), window.docs().collect::<Vec<_>>()), ("w7".to_owned(), "w8".to_owned(), vec![a, b]));
    assert_eq!(window.close(a), Ok(()));
    assert_eq!(window.close(a), Err(CoreError::NoSuchDoc(a)));
    assert_eq!(window.doc(a).unwrap_err(), CoreError::NoSuchDoc(a));
}

#[test]
fn a_file_has_one_drawing_at_a_time() {
    let dir = scratch("one-a-file");
    let mut core = Core::headless();
    let first = core.new_doc(24.0, 24.0);
    let path = dir.join("a.svg");
    core.save(first, Some(&path)).unwrap();
    // By whatever name the path gives it.
    let spelled = dir.join("sub/../a.svg");
    std::fs::create_dir(dir.join("sub")).unwrap();
    assert_eq!((core.doc_at(&path), core.doc_at(&spelled), core.doc_at(&dir.join("b.svg"))), (Some(first), Some(first), None));
    assert_eq!(core.open_file(&spelled), Err(CoreError::AlreadyOpen { path: spelled.clone(), doc: first }));
    assert_eq!(core.docs().count(), 1, "nothing was opened");
    // Nor can another drawing be saved over it, though it can over itself.
    let second = core.new_doc(8.0, 8.0);
    assert_eq!(core.save(second, Some(&spelled)), Err(CoreError::AlreadyOpen { path: spelled.clone(), doc: first }));
    assert_eq!(core.path(second), Ok(None));
    assert_eq!(core.save(first, Some(&spelled)), Ok(spelled.clone()));
    assert_eq!(core.save(DocId(9), Some(&path)), Err(CoreError::NoSuchDoc(DocId(9))));
    // Closed, its file is anyone's.
    core.close(first).unwrap();
    assert_eq!(core.doc_at(&path), None);
    assert_eq!(core.save(second, Some(&path)), Ok(path.clone()));
    assert_eq!(core.open_file(&path).unwrap_err().to_string(), format!("{} is open as d2: a file has one drawing at a time", path.display()));
}

#[test]
fn what_cannot_be_done_says_why_and_leaves_no_step() {
    let dir = scratch("refused");
    let mut core = Core::headless();
    let doc = core.new_doc(24.0, 24.0);
    let nowhere = Command::SetAttr { node: NodeId(99), name: "x".into(), value: Some("1".into()) };
    assert!(matches!(core.apply(doc, &nowhere, Actor::Claude, "nowhere"), Err(CoreError::Doc(_))));
    assert_eq!(core.undo(doc), Err(CoreError::NothingToUndo), "a refused command is no step");
    assert_eq!(core.redo(doc), Err(CoreError::NothingToRedo));
    // A command that changes nothing is no step either.
    let root = core.doc(doc).unwrap().root();
    let same = Command::SetAttr { node: root, name: "width".into(), value: Some("24".into()) };
    assert!(core.apply(doc, &same, Actor::Alva, "same").unwrap().is_nothing());
    assert_eq!(core.history(doc).unwrap().undoable().count(), 0);
    // Files that aren't drawings.
    let missing = core.open_file(&dir.join("missing.svg")).unwrap_err();
    assert!(matches!(&missing, CoreError::File { path, .. } if path.ends_with("missing.svg")), "{missing}");
    for (name, bytes, says) in [("latin1.svg", b"<svg a=\"\xe9\"/>".as_slice(), "UTF-8"), ("page.svg", b"<html/>".as_slice(), "not an SVG"), ("broken.svg", b"<svg><g></svg>".as_slice(), "line 1, column 9")] {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        let e = core.open_file(&path).unwrap_err().to_string();
        assert!(e.contains(says), "{name}: {e}");
    }
    assert_eq!(core.docs().count(), 1, "nothing that failed to open is open");
    // A picture of no size.
    let none = View { width: 0, height: 10, ..View::icon(&core.viewport(doc).unwrap(), 16) };
    assert!(matches!(core.render(doc, &none), Err(CoreError::Size(_))));
    assert!(matches!(core.save(doc, Some(&dir.join("no/such/folder/x.svg"))), Err(CoreError::File { .. })));
}

#[test]
fn the_autosave_folder_follows_the_drawings() {
    let dir = scratch("autosave");
    let mut core = Core::headless();
    let mut saver = Autosave::new(dir.join("autosave"), "test");
    let lines = std::cell::RefCell::new(Vec::<String>::new());
    let log = |l: &str| lines.borrow_mut().push(l.to_owned());
    let files = || std::fs::read_dir(dir.join("autosave")).map_or(Vec::new(), |d| d.filter_map(Result::ok).map(|e| e.file_name().to_string_lossy().into_owned()).collect::<Vec<_>>());
    let touch = |core: &mut Core, doc, value: &str| {
        let root = core.doc(doc).unwrap().root();
        core.apply(doc, &Command::SetAttr { node: root, name: "data-n".into(), value: Some(value.into()) }, Actor::Claude, "touch").unwrap();
    };

    let doc = core.new_doc(8.0, 8.0);
    saver.run(&core, &log);
    assert!(files().is_empty(), "a new drawing nothing was done to has nothing to lose");

    touch(&mut core, doc, "1");
    saver.run(&core, &log);
    saver.run(&core, &log);
    assert_eq!(files(), [format!("test-{}-d1-untitled.svg", std::process::id())]);
    assert_eq!(lines.borrow().len(), 1, "written once, not again while unchanged");
    assert_eq!(std::fs::read_to_string(dir.join("autosave").join(&files()[0])).unwrap(), core.doc(doc).unwrap().to_svg());

    core.save(doc, Some(&dir.join("real.svg"))).unwrap();
    saver.run(&core, &log);
    assert!(files().is_empty(), "a real save retires the autosave");

    touch(&mut core, doc, "2");
    saver.run(&core, &log);
    assert_eq!(files(), [format!("test-{}-d1-real.svg", std::process::id())], "under its file's name now");
    core.undo(doc).unwrap();
    saver.run(&core, &log);
    assert!(files().is_empty(), "undone back to what's saved: nothing to lose again");

    touch(&mut core, doc, "3");
    saver.run(&core, &log);
    core.close(doc).unwrap();
    saver.run(&core, &log);
    assert!(files().is_empty(), "closing retires it too");
    assert!(!saver.overdue());
}
