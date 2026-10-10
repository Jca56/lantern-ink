//! Not losing work: a drawing with unsaved changes has a copy in the
//! autosave folder once it's been left alone, the copy goes when the
//! changes do, and copies another Ink left behind are offered back.

use ink_core::autosave::file_name;

use super::*;
use crate::autosaving::{Autosave, WHO};
use crate::menus;

/// The names of what's in `dir`, in order.
fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).map(|all| all.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    names.sort();
    names
}

/// A window whose copies go to a folder of the test's own.
fn kept(name: &str) -> (Running, PathBuf) {
    let dir = scratch(name).join("autosave");
    let mut r = Running::start(1280.0, 800.0, 1.0);
    r.ink.autosave = Autosave::new(Some(dir.clone()));
    (r, dir)
}

impl Running {
    /// Draw a rectangle: something to lose.
    fn scribble(&mut self) {
        self.ink.tools.select(Tool::Rect);
        self.drag(self.spot(4.0, 4.0), self.spot(12.0, 10.0));
        self.ink.tools.select(Tool::Pointer);
    }

    /// Let `seconds` go by, and what the pool was asked for land.
    fn wait(&mut self, seconds: f64) {
        self.h.advance(seconds);
        self.frames(2);
        std::thread::sleep(std::time::Duration::from_millis(60));
        self.frames(2);
    }
}

#[test]
fn unsaved_work_is_copied_aside_once_its_left_alone() {
    let (mut r, dir) = kept("saves-copy");
    let doc = r.doc();
    // A new drawing nothing was done to has nothing to lose.
    r.wait(10.0);
    assert!(files(&dir).is_empty());
    r.scribble();
    r.wait(2.0);
    assert!(files(&dir).is_empty(), "not while it's being worked on");
    r.wait(4.0);
    let copy = file_name(WHO, doc, "untitled");
    assert_eq!(files(&dir), std::slice::from_ref(&copy));
    assert_eq!(std::fs::read_to_string(dir.join(&copy)).unwrap(), r.svg());
    // Changed again, the copy follows.
    r.scribble();
    r.wait(6.0);
    assert_eq!(std::fs::read_to_string(dir.join(&copy)).unwrap(), r.svg());
    // Saved for real, the copy goes; so it does when the tab closes
    // with its work given up.
    let saved = dir.parent().unwrap().join("lamp.svg");
    r.ink.write(doc, saved.clone(), Then::Nothing);
    r.until("the save", |r| !r.ink.is_modified(doc));
    r.wait(1.0);
    assert!(files(&dir).is_empty() && saved.exists());
    r.scribble();
    r.wait(6.0);
    assert_eq!(files(&dir), [file_name(WHO, doc, "lamp")]);
    r.ink.close(doc);
    r.wait(1.0);
    assert!(files(&dir).is_empty());
}

#[test]
fn work_another_ink_left_behind_is_offered_back() {
    let (mut r, dir) = kept("saves-found");
    let blank = r.doc();
    std::fs::create_dir_all(&dir).unwrap();
    let lost = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\n  <rect width=\"8\" height=\"8\"/>\n</svg>\n";
    // (No process has number 0: its writer is gone for sure.)
    std::fs::write(dir.join("window-0-w3-lamp.svg"), lost).unwrap();
    std::fs::write(dir.join("window-0-w4-moon.svg"), lost).unwrap();
    r.ink.look_for_lost(&dir);
    assert!(r.ink.ask_found && r.ink.found.len() == 2);
    let asked = r.ink.recovery_dialog().unwrap();
    assert_eq!((asked.title.as_str(), asked.body.as_str()), ("Unsaved work found", "2 drawings weren\u{2019}t saved when Lantern Ink last closed:\n\n- lamp\n- moon"));
    // Restored: each in a tab under its name, unsaved work from the
    // start, in the place of the blank drawing the window began with.
    r.run(menus::RESTORE_FOUND);
    r.until("both back", |r| r.ink.tabs.len() == 2 && r.ink.core.doc(blank).is_err());
    let docs: Vec<DocId> = r.ink.tabs.iter().map(|t| t.doc).collect();
    assert_eq!(docs.iter().map(|d| r.ink.label(*d)).collect::<Vec<_>>(), ["lamp", "moon"]);
    assert!(docs.iter().all(|d| r.ink.is_modified(*d) && !r.ink.untouched(*d) && r.ink.core.doc(*d).unwrap().to_svg() == lost));
    // The copies are this window's now, under its own name.
    assert_eq!(files(&dir), [file_name(WHO, docs[0], "lamp"), file_name(WHO, docs[1], "moon")]);
    assert!(r.ink.found.is_empty() && r.ink.recovery_dialog().is_none());
    // Closed (its work given up), a restored drawing's copy goes.
    r.ink.close(docs[0]);
    r.wait(1.0);
    assert_eq!(files(&dir), [file_name(WHO, docs[1], "moon")]);
    // Discarded instead, they're just gone.
    let (mut r, dir) = kept("saves-discard");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("window-0-w9-old.svg"), lost).unwrap();
    r.ink.look_for_lost(&dir);
    assert_eq!(r.ink.found.len(), 1);
    r.run(menus::DISCARD_FOUND);
    assert!(files(&dir).is_empty() && r.ink.tabs.len() == 1 && r.ink.found.is_empty());
}

#[test]
fn a_window_that_ends_unasked_leaves_its_work_behind() {
    let (mut r, dir) = kept("saves-end");
    let doc = r.doc();
    r.scribble();
    // The compositor went away before the copy was due: one's written
    // on the way out.
    r.ink.last_words(lntrn_app::Exit::Disconnected);
    assert_eq!(files(&dir), [file_name(WHO, doc, "untitled")]);
    // A quit was asked about: what wasn't saved was given up.
    r.wait(6.0);
    r.ink.last_words(lntrn_app::Exit::Quit);
    assert!(files(&dir).is_empty());
}
