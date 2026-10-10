//! A drawing's comings and goings in the window (LS3's `lifecycle.rs`):
//! new, opened, saved, closed; what the threads finished; and the
//! questions asked before work is let go.

use std::path::{Path, PathBuf};

use ink_core::{CoreError, DocId};
use lntrn_ui::{Action, Dialog, HostCx, ShellRequest};

use crate::actions::doc_action;
use crate::files::{Done, Then};
use crate::ink::Ink;
use crate::menus::*;

impl Ink {
    /// A new, empty drawing the size of the last one asked for (File >
    /// New… asks; this is the tab bar's +, and what a window starts
    /// with), in a new tab.
    pub fn new_document(&mut self) {
        let doc = self.core.new_doc(self.settings.new_width, self.settings.new_height);
        self.tabs.add(doc);
    }

    /// Open the file at `path`, or show its tab if it's open already.
    pub fn open(&mut self, path: PathBuf) {
        match self.core.doc_at(&path) {
            Some(doc) => {
                self.tabs.show(doc);
            }
            None => self.files.read(path),
        }
    }

    /// Save to its file, or ask where if it has none.
    pub(crate) fn save(&mut self, doc: DocId, then: Then) {
        match self.core.path(doc).ok().flatten().map(Path::to_owned) {
            Some(path) => self.write(doc, path, then),
            None => self.save_as(doc, then),
        }
    }

    pub(crate) fn save_as(&mut self, doc: DocId, then: Then) {
        let name = format!("{}.svg", self.label(doc));
        self.files.pick_save(doc, name, then);
    }

    pub(crate) fn write(&mut self, doc: DocId, path: PathBuf, then: Then) {
        if self.files.is_saving(doc) {
            return;
        }
        match self.core.begin_save(doc, Some(&path)) {
            Ok(job) => self.files.write(job, then),
            Err(e) => self.toast(format!("Couldn't save: {}", said(&e, self))),
        }
    }

    /// Close `doc`'s tab, first asking about unsaved work.
    pub fn ask_close(&mut self, doc: DocId, cx: &mut HostCx) {
        if !self.is_modified(doc) || self.untouched(doc) {
            self.close(doc);
            return;
        }
        let dialog = Dialog::new(&format!("Save changes to \u{201c}{}\u{201d}?", self.label(doc)), "Your changes will be lost if you don't save them.")
            .button("Don't Save", Some(doc_action(DISCARD_AND_CLOSE, doc)))
            .button("Cancel", None)
            .button("Save", Some(doc_action(SAVE_AND_CLOSE, doc)))
            .default_button(2);
        cx.request(ShellRequest::Dialog(dialog));
    }

    /// A drawing made here that nothing has been done to: there's
    /// nothing in it to lose, though it has never been saved (so its
    /// tab shows no `•`, and closing it asks nothing).
    pub(crate) fn untouched(&self, doc: DocId) -> bool {
        matches!(self.core.path(doc), Ok(None)) && self.core.history(doc).is_ok_and(|h| h.undoable().next().is_none() && h.redoable().next().is_none())
    }

    pub(crate) fn close(&mut self, doc: DocId) {
        self.tiles.forget(doc);
        self.tabs.remove(doc);
        if let Err(e) = self.core.close(doc) {
            lntrn_core::log_error!("closing {doc}: {e}");
        }
        // The window always has a drawing in it.
        if self.tabs.len() == 0 {
            self.new_document();
        }
    }

    /// Whether the window may close now; if work is unsaved, asks first.
    pub fn may_quit(&mut self, cx: &mut HostCx) -> bool {
        let unsaved: Vec<String> = self.tabs.iter().filter(|t| self.is_modified(t.doc) && !self.untouched(t.doc)).map(|t| self.label(t.doc)).collect();
        if self.quitting || unsaved.is_empty() {
            return true;
        }
        let list = unsaved.iter().map(|n| format!("- {n}")).collect::<Vec<_>>().join("\n");
        let dialog = Dialog::new("Quit with unsaved changes?", &format!("These have changes that aren't saved:\n\n{list}"))
            .button("Cancel", None)
            .button("Quit Without Saving", Some(Action::new(QUIT_ANYWAY)))
            .default_button(0);
        cx.request(ShellRequest::Dialog(dialog));
        false
    }

    /// Pick up what the threads finished.
    pub fn finished(&mut self) {
        for done in self.files.finished() {
            match done {
                Done::OpenPicked(Some(path)) => self.open(path),
                Done::SavePicked { doc, then, path: Some(path) } => {
                    if self.tabs.index_of(doc).is_some() {
                        self.write(doc, path, then);
                    }
                }
                Done::OpenPicked(None) | Done::SavePicked { path: None, .. } | Done::ExportPicked(None) => {}
                Done::ExportPicked(Some(path)) => self.export_to(path),
                Done::Exported(Ok(said) | Err(said)) => self.toast(said),
                Done::Read { path, result } => self.opened(&path, result),
                Done::Written { job, then, result } => {
                    let name = file_name(&job.path);
                    match result.and_then(|()| self.core.saved(&job).map_err(|e| said(&e, self))) {
                        Ok(()) => {
                            self.recent.add(&job.path);
                            self.toast(format!("Saved {name}"));
                            if then == Then::Close {
                                self.close(job.doc);
                            }
                        }
                        Err(e) => self.toast(format!("Couldn't save {name}: {e}")),
                    }
                }
            }
        }
    }

    /// A file's text came back: open it as a drawing, in a tab.
    fn opened(&mut self, path: &Path, result: Result<String, String>) {
        let name = file_name(path);
        let opened = result.and_then(|text| match self.core.open_read(path, &text) {
            // Read twice before the first had come in: it's open.
            Err(CoreError::AlreadyOpen { doc, .. }) => {
                self.tabs.show(doc);
                Ok(None)
            }
            other => other.map(Some).map_err(|e| e.to_string()),
        });
        match opened {
            Ok(Some(opened)) => {
                // The new drawing a window starts with gives way to the
                // first file opened, if nothing was done to it.
                let lone = (self.tabs.len() == 1).then(|| self.tabs.active_doc()).flatten().filter(|&d| self.untouched(d));
                self.tabs.add_named(opened.doc);
                if let Some(blank) = lone {
                    self.close(blank);
                }
                self.recent.add(path);
                if !opened.adopted.is_nothing() {
                    self.toast(format!("Opened {name}: {}\u{2019}s own marks come out when it\u{2019}s saved", opened.adopted.editors.join(" and ")));
                }
            }
            Ok(None) => {}
            Err(e) => self.toast(format!("Couldn't open {name}: {e}")),
        }
        // The window always has a drawing in it (it began with files
        // to open, and none did).
        if self.tabs.len() == 0 {
            self.new_document();
        }
    }
}

/// `e` as the status bar says it: another drawing by its tab's name,
/// where the core knows it by its id.
fn said(e: &CoreError, ink: &Ink) -> String {
    match e {
        CoreError::AlreadyOpen { path, doc } => format!("{} is open in the tab \u{201c}{}\u{201d}", file_name(path), ink.label(*doc)),
        other => other.to_string(),
    }
}

pub(crate) fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}
