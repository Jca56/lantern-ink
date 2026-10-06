//! Autosave. A front end can be killed without warning (Claude Code has
//! been seen killing an MCP server without closing its stdin first), so
//! a save on the way out isn't enough. Every drawing with unsaved work
//! is copied into the autosave folder when [`Autosave::run`] is called:
//! once its front end has been idle a few seconds, at least once a
//! minute while it's busy, and at the end. A drawing has one file there,
//! rewritten as it changes and removed once it has no unsaved work
//! (saved for real, or undone back to its saved state) or is closed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ink_doc::DocId;

use crate::core::Core;
use crate::file;

/// How long a front end is idle before it autosaves.
pub const IDLE: Duration = Duration::from_secs(5);
/// The longest unsaved work waits while calls keep coming.
const MAX_WAIT: Duration = Duration::from_secs(60);

/// What the folder holds for one drawing.
struct Copy {
    /// The history state it was written at, or last tried at.
    stamp: u64,
    /// Where it was written; none if it never was.
    path: Option<PathBuf>,
}

pub struct Autosave {
    dir: PathBuf,
    /// Whose copies these are: `mcp`, `window`.
    who: &'static str,
    copies: HashMap<DocId, Copy>,
    last_run: Instant,
}

impl Autosave {
    pub fn new(dir: PathBuf, who: &'static str) -> Autosave {
        Autosave { dir, who, copies: HashMap::new(), last_run: Instant::now() }
    }

    /// The front end has been busy for too long since the last run.
    pub fn overdue(&self) -> bool {
        self.last_run.elapsed() >= MAX_WAIT
    }

    /// Bring the folder in step with the drawings.
    pub fn run(&mut self, core: &Core, log: &dyn Fn(&str)) {
        self.last_run = Instant::now();
        let open: Vec<DocId> = core.docs().collect();
        // Closing a drawing with unsaved work is its front end's to
        // refuse: a closed one's work was given up on purpose.
        let closed: Vec<DocId> = self.copies.keys().filter(|d| !open.contains(d)).copied().collect();
        for doc in closed {
            self.remove(doc, "it was closed", log);
        }
        for doc in open {
            let Ok(history) = core.history(doc) else { continue };
            // A new drawing nothing has been done to has nothing to lose.
            let untouched = history.stamp() == 0 && core.path(doc).is_ok_and(|p| p.is_none());
            if untouched || !core.is_modified(doc).unwrap_or(false) {
                self.remove(doc, "it has no unsaved work", log);
                continue;
            }
            let stamp = history.stamp();
            if self.copies.get(&doc).is_some_and(|c| c.stamp == stamp) {
                continue;
            }
            let path = self.path(core, doc);
            let old = self.copies.remove(&doc).and_then(|c| c.path);
            let saved = std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string()).and_then(|()| core.doc(doc).map_err(|e| e.to_string())).and_then(|d| file::write(&path, d.to_svg().as_bytes()).map_err(|e| e.to_string()));
            let path = match saved {
                Ok(()) => {
                    log(&format!("autosaved {doc} to {}", path.display()));
                    // A drawing saved under a new name gets a new file;
                    // the old one goes.
                    if let Some(old) = old.filter(|old| *old != path) {
                        let _ = std::fs::remove_file(old);
                    }
                    Some(path)
                }
                // Tried again at its next change, not every idle moment;
                // the older copy stays, the best there is.
                Err(e) => {
                    log(&format!("autosave of {doc} failed: {e}"));
                    old
                }
            };
            self.copies.insert(doc, Copy { stamp, path });
        }
    }

    fn remove(&mut self, doc: DocId, why: &str, log: &dyn Fn(&str)) {
        if let Some(path) = self.copies.remove(&doc).and_then(|c| c.path) {
            match std::fs::remove_file(&path) {
                Ok(()) => log(&format!("removed {doc}'s autosave: {why}")),
                Err(e) => log(&format!("couldn't remove {}: {e}", path.display())),
            }
        }
    }

    /// `{who}-{pid}-{doc}-{name}.svg`: the pid says which process wrote
    /// it, so a live one's files are never offered as recovery.
    fn path(&self, core: &Core, doc: DocId) -> PathBuf {
        let name = core.path(doc).ok().flatten().and_then(|p| p.file_stem()).map_or("untitled".to_owned(), |s| s.to_string_lossy().into_owned());
        self.dir.join(format!("{}-{}-{doc}-{name}.svg", self.who, std::process::id()))
    }
}
