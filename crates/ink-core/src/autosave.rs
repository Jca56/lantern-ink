//! Autosave. A front end can be killed without warning (Claude Code has
//! been seen killing an MCP server without closing its stdin first), so
//! a save on the way out isn't enough. Every drawing with unsaved work
//! is copied into the autosave folder when [`Autosave::run`] is called:
//! once its front end has been idle a few seconds, at least once a
//! minute while it's busy, and at the end. A drawing has one file there,
//! rewritten as it changes and removed once it has no unsaved work
//! (saved for real, or undone back to its saved state) or is closed.

//!
//! A copy is named for the process that wrote it ([`file_name`]), so a
//! front end starting up can tell the copies nobody is keeping any
//! more ([`orphans`]) from a live one's, and offer those back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

/// A copy's name: `{who}-{pid}-{doc}-{name}.svg`. `who` is the kind of
/// front end (`mcp`, `window`) and the pid says which process, so a
/// live one's copies are never offered back as lost work.
pub fn file_name(who: &str, doc: DocId, name: &str) -> String {
    format!("{who}-{}-{doc}-{name}.svg", std::process::id())
}

/// A copy whose writer is gone: work that was never saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Orphan {
    pub path: PathBuf,
    /// What its drawing was called.
    pub name: String,
}

/// Whether process `pid` is one of `program` still running. A number
/// that has gone round to some other program doesn't count.
fn running(pid: u32, program: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|name| name.trim() == program)
}

/// The copies in `dir` that a `who` front end wrote and no longer
/// keeps: its process (a `program`, by the name the system knows it
/// by) is gone. In name order.
pub fn orphans(dir: &Path, who: &str, program: &str) -> Vec<Orphan> {
    orphans_where(dir, who, |pid| running(pid, program))
}

fn orphans_where(dir: &Path, who: &str, running: impl Fn(u32) -> bool) -> Vec<Orphan> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Orphan> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_name()?.to_str()?.strip_suffix(".svg")?.to_owned();
            // (A name may have dashes of its own: it's all that's left.)
            let mut parts = stem.splitn(4, '-');
            let (by, pid, _doc, name) = (parts.next()?, parts.next()?.parse::<u32>().ok()?, parts.next()?, parts.next()?);
            (by == who && !running(pid)).then(|| Orphan { path: path.clone(), name: name.to_owned() })
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
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
        self.dir.join(file_name(self.who, doc, &name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_copies_nobody_keeps_are_found_by_whose_they_were() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/ink-core/orphans");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["window-100-w1-lamp.svg", "window-200-w2-moon-phase.svg", "window-300-w1-live.svg", "mcp-100-d1-claudes.svg", "window-x-w1-odd.svg", "notes.txt", "window-100-w9.svg"] {
            std::fs::write(dir.join(name), "<svg/>").unwrap();
        }
        // 300 is still running: its copy is its own.
        let found = orphans_where(&dir, "window", |pid| pid == 300);
        assert_eq!(found.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["lamp", "moon-phase"]);
        assert_eq!(found[0].path, dir.join("window-100-w1-lamp.svg"));
        assert_eq!(orphans_where(&dir, "mcp", |_| false).len(), 1);
        assert!(orphans_where(&dir.join("none"), "window", |_| false).is_empty());
        // This process's own are named so that it's told from the rest.
        assert_eq!(file_name("window", DocId(3), "lamp"), format!("window-{}-{}-lamp.svg", std::process::id(), DocId(3)));
        assert!(running(std::process::id(), std::fs::read_to_string("/proc/self/comm").unwrap().trim()) && !running(std::process::id(), "something-else"));
    }
}
