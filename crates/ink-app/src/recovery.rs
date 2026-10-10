//! The window's side of not losing work (`docs/M4.md`, slice f):
//! `autosaving.rs` says when a drawing's copy is due, and here the
//! copies are written (on the job pool), the ones other Inks left
//! behind are asked about as the window opens ("Unsaved work found",
//! as LS3 asks), and the last ones are written when the window ends
//! without being asked to.
//!
//! A drawing that came back from a copy has no file of its own: it's
//! unsaved work from the start, under the name it had, and the copy is
//! this window's to keep until it's saved for real or closed.

use std::path::{Path, PathBuf};

use ink_core::autosave::{Orphan, orphans};
use lntrn_app::Exit;
use lntrn_ui::{Action, Dialog, Ui};

use crate::autosaving::{DocState, WHO};
use crate::ink::Ink;
use crate::lifecycle::file_name;
use crate::menus::{DISCARD_FOUND, RESTORE_FOUND};

/// The window's process, as the system names it: whose copies aren't
/// lost work while it runs.
const PROGRAM: &str = "lantern-ink";
/// How many drawings the question names.
const LISTED: usize = 8;

impl Ink {
    /// Every open drawing as autosave sees it. A new one nothing was
    /// done to has nothing to lose.
    pub(crate) fn autosave_states(&self) -> Vec<DocState> {
        let mut all: Vec<DocState> = self.core.docs().filter_map(|doc| Some(DocState { doc, modified: self.is_modified(doc) && !self.untouched(doc), state: self.core.history(doc).ok()?.stamp(), name: self.label(doc) })).collect();
        all.sort_by_key(|d| d.doc);
        all
    }

    /// Start the copies that are due, and have the window wake when the
    /// next could be.
    pub(crate) fn autosave_tick(&mut self, ui: &mut Ui) {
        let (due, next) = self.autosave.tick(&self.autosave_states(), ui.now());
        for (doc, path) in due {
            // The text is the drawing's as it is now; writing it is the
            // pool's.
            match self.core.doc(doc).map(|drawing| drawing.to_svg()).ok().zip(self.core.history(doc).ok().map(|h| h.stamp())) {
                Some((text, state)) => self.files.copy(doc, path, state, text),
                None => self.autosave.written(doc, &path, 0, false),
            }
        }
        if let Some(next) = next {
            ui.state.request_redraw_after(next);
        }
    }

    /// Look in `dir` for copies other Inks left behind: asked about
    /// with the next frame.
    pub(crate) fn look_for_lost(&mut self, dir: &Path) {
        self.found = orphans(dir, WHO, PROGRAM);
        self.ask_found = !self.found.is_empty();
    }

    /// The copies left behind, asked about: restore them, or let them
    /// go.
    pub(crate) fn recovery_dialog(&self) -> Option<Dialog> {
        let n = self.found.len();
        if n == 0 {
            return None;
        }
        let mut list: Vec<String> = self.found.iter().take(LISTED).map(|f| format!("- {}", f.name)).collect();
        if n > LISTED {
            list.push(format!("\u{2026} and {} more", n - LISTED));
        }
        let count = if n == 1 { "A drawing wasn\u{2019}t saved".to_owned() } else { format!("{n} drawings weren\u{2019}t saved") };
        let body = format!("{count} when Lantern Ink last closed:\n\n{}", list.join("\n"));
        Some(Dialog::new("Unsaved work found", &body).button("Discard All", Some(Action::new(DISCARD_FOUND))).button("Restore All", Some(Action::new(RESTORE_FOUND))).default_button(1))
    }

    /// "Restore All": each copy is read, to open as unsaved work.
    pub(crate) fn restore_found(&mut self) {
        for Orphan { path, name } in std::mem::take(&mut self.found) {
            self.files.recover(path, name);
        }
    }

    /// "Discard All": the copies go.
    pub(crate) fn discard_found(&mut self) {
        for found in std::mem::take(&mut self.found) {
            if let Err(e) = std::fs::remove_file(&found.path) {
                lntrn_core::log_error!("discarding {}: {e}", found.path.display());
            }
        }
    }

    /// A copy left behind was read: open it as unsaved work called
    /// `name`, with no file of its own, and keep the copy as this
    /// window's.
    pub(crate) fn recovered(&mut self, path: &Path, name: String, result: Result<String, String>) {
        match result.and_then(|text| self.core.open_text(&text).map_err(|e| e.to_string())) {
            Ok(opened) => {
                // The new drawing a window starts with gives way, if
                // nothing was done to it.
                let lone = (self.tabs.len() == 1).then(|| self.tabs.active_doc()).flatten().filter(|&d| self.untouched(d));
                self.tabs.add_as(opened.doc, name);
                self.recovered.push(opened.doc);
                if let Some(blank) = lone {
                    self.close(blank);
                }
                if let Some(state) = self.autosave_states().into_iter().find(|d| d.doc == opened.doc) {
                    self.autosave.adopt(&state, path);
                }
            }
            // The file stays: it's offered again next time.
            Err(e) => self.toast(format!("Couldn\u{2019}t restore {}: {e}", file_name(path))),
        }
    }

    /// The loop ended. A quit gives up what wasn't saved (it was asked
    /// about); anything else gets a last copy of every unsaved drawing,
    /// waited for, to be offered back next time.
    pub(crate) fn last_words(&mut self, why: Exit) {
        if why == Exit::Quit {
            self.autosave.clear();
            return;
        }
        let behind: Vec<(ink_core::DocId, PathBuf)> = self.autosave.behind(&self.autosave_states());
        for (doc, path) in behind {
            let made = path.parent().map_or(Ok(()), std::fs::create_dir_all).map_err(|e| e.to_string());
            let written = made.and_then(|()| self.core.doc(doc).map_err(|e| e.to_string())).and_then(|drawing| ink_core::write_atomic(&path, drawing.to_svg().as_bytes()).map_err(|e| e.to_string()));
            match written {
                Ok(()) => lntrn_core::log_info!("autosaved {doc} to {} on the way out", path.display()),
                Err(e) => lntrn_core::log_error!("last autosave of {doc}: {e}"),
            }
        }
    }
}
