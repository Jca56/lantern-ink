//! Unsaved work kept safe (`docs/M4.md`, slice f; LS3's `autosave.rs`,
//! done the way the MCP server does it): each drawing with unsaved
//! changes has a copy in the autosave folder, written once it's been
//! left alone a few seconds, and after a minute of steady work at the
//! latest. The copy goes when the changes do (saved for real, undone,
//! the tab closed), so what's in the folder is exactly what a crash
//! would lose. The files are named for this process
//! (`ink_core::autosave`), so no other Ink offers them back while this
//! one runs.
//!
//! This only decides when; the copies are written on the job pool.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ink_core::DocId;
use ink_core::autosave::file_name;

/// Whose copies these are, among the front ends that keep some.
pub const WHO: &str = "window";
/// How long a drawing is left alone before its copy is written, seconds.
pub const IDLE: f64 = 5.0;
/// The longest unsaved changes wait while the work goes on.
pub const MAX_WAIT: f64 = 60.0;

/// A drawing as autosave sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct DocState {
    pub doc: DocId,
    /// It has work that's in no file.
    pub modified: bool,
    /// Its history's state: what a copy is a copy of.
    pub state: u64,
    /// What its tab calls it.
    pub name: String,
}

struct Copy {
    /// The state the newest copy was asked for at. No other is asked for
    /// until the document moves on, written or not: a copy that failed
    /// is tried again at the next change, not every few seconds.
    asked: u64,
    /// The file, and the state it holds, once one is written.
    file: Option<(PathBuf, u64)>,
}

/// Changes waiting for their copy.
struct Waiting {
    state: u64,
    /// When they began, and when the latest was made.
    since: f64,
    changed: f64,
}

fn remove(path: &Path) {
    if let Err(e) = std::fs::remove_file(path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        lntrn_core::log_error!("autosave: couldn't remove {}: {e}", path.display());
    }
}

pub struct Autosave {
    /// The folder; none if there's no home to keep one in.
    dir: Option<PathBuf>,
    copies: HashMap<DocId, Copy>,
    waiting: HashMap<DocId, Waiting>,
    /// Documents with a copy being written.
    writing: Vec<DocId>,
    /// Recovered files that couldn't take this process's name: each goes
    /// once its document has a copy of its own.
    leftovers: HashMap<DocId, PathBuf>,
}

impl Autosave {
    pub fn new(dir: Option<PathBuf>) -> Autosave {
        Autosave { dir, copies: HashMap::new(), waiting: HashMap::new(), writing: Vec::new(), leftovers: HashMap::new() }
    }

    fn path(&self, d: &DocState) -> Option<PathBuf> {
        Some(self.dir.as_ref()?.join(file_name(WHO, d.doc, &d.name)))
    }

    /// Bring the folder in step with `docs` at `now` (seconds, any
    /// steady clock). Returns the copies to start writing now, and how
    /// long until the next could be due (none: nothing is waiting).
    pub fn tick(&mut self, docs: &[DocState], now: f64) -> (Vec<(DocId, PathBuf)>, Option<f64>) {
        // A closed document's changes were given up, or saved first.
        let closed: Vec<DocId> = self.copies.keys().filter(|doc| !docs.iter().any(|d| d.doc == **doc)).copied().collect();
        for doc in closed {
            self.forget(doc);
        }
        self.waiting.retain(|doc, _| docs.iter().any(|d| d.doc == *doc));
        let (mut due, mut next) = (Vec::new(), None::<f64>);
        for d in docs {
            if !d.modified {
                self.forget(d.doc);
                self.waiting.remove(&d.doc);
                continue;
            }
            if self.copies.get(&d.doc).is_some_and(|c| c.asked == d.state) {
                self.waiting.remove(&d.doc);
                continue;
            }
            let w = self.waiting.entry(d.doc).or_insert(Waiting { state: d.state, since: now, changed: now });
            if w.state != d.state {
                (w.state, w.changed) = (d.state, now);
            }
            let wait = (IDLE - (now - w.changed)).min(MAX_WAIT - (now - w.since));
            // One at a time: the one on its way lands first.
            if self.writing.contains(&d.doc) {
                continue;
            }
            if wait > 0.0 {
                next = Some(next.map_or(wait, |n| n.min(wait)));
                continue;
            }
            let Some(path) = self.path(d) else { continue };
            self.waiting.remove(&d.doc);
            self.copies.entry(d.doc).or_insert(Copy { asked: d.state, file: None }).asked = d.state;
            self.writing.push(d.doc);
            due.push((d.doc, path));
        }
        (due, next)
    }

    /// A copy of `doc` holding `state` reached `path`, or (`ok` false)
    /// didn't.
    pub fn written(&mut self, doc: DocId, path: &Path, state: u64, ok: bool) {
        self.writing.retain(|d| *d != doc);
        if !ok {
            return;
        }
        match self.copies.get_mut(&doc) {
            Some(copy) => {
                // A renamed document's copy has a new name: the old goes.
                if let Some((old, _)) = copy.file.replace((path.to_owned(), state))
                    && old != path
                {
                    remove(&old);
                }
                if let Some(left) = self.leftovers.remove(&doc) {
                    remove(&left);
                }
            }
            // Closed, saved or undone while it was on its way.
            None => remove(path),
        }
    }

    /// Stop keeping `doc`'s copy.
    fn forget(&mut self, doc: DocId) {
        if let Some(Copy { file: Some((path, _)), .. }) = self.copies.remove(&doc) {
            remove(&path);
        }
        if let Some(left) = self.leftovers.remove(&doc) {
            remove(&left);
        }
    }

    /// `from`, a copy another Ink left behind, was opened as `d`: it
    /// takes this process's name, so it's `d`'s copy as it stands and is
    /// never offered again.
    pub fn adopt(&mut self, d: &DocState, from: &Path) {
        let Some(path) = self.path(d) else { return };
        match std::fs::rename(from, &path) {
            Ok(()) => {
                self.copies.insert(d.doc, Copy { asked: d.state, file: Some((path, d.state)) });
            }
            Err(e) => {
                lntrn_core::log_error!("autosave: {} keeps its name: {e}", from.display());
                self.leftovers.insert(d.doc, from.to_owned());
            }
        }
    }

    /// The documents whose unsaved changes no finished copy holds, with
    /// where each copy goes: what a last save has to write when the app
    /// ends unasked.
    pub fn behind(&self, docs: &[DocState]) -> Vec<(DocId, PathBuf)> {
        let held = |d: &DocState| self.copies.get(&d.doc).and_then(|c| c.file.as_ref()).is_some_and(|(_, state)| *state == d.state);
        docs.iter().filter(|d| d.modified && !held(d)).filter_map(|d| Some((d.doc, self.path(d)?))).collect()
    }

    /// Remove every copy: the app is quitting, and what wasn't saved was
    /// given up.
    pub fn clear(&mut self) {
        let all: Vec<DocId> = self.copies.keys().chain(self.leftovers.keys()).copied().collect();
        for doc in all {
            self.forget(doc);
        }
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own, and one document with unsaved changes.
    fn bench(name: &str) -> (Autosave, PathBuf, DocState) {
        let dir = std::env::temp_dir().join(format!("ink-app-autosave-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (Autosave::new(Some(dir.clone())), dir, DocState { doc: DocId(1), modified: true, state: 1, name: "Sketch".into() })
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    /// Write what `tick` asked for, as the job pool would.
    fn write(a: &mut Autosave, due: &[(DocId, PathBuf)], state: u64) {
        for (doc, path) in due {
            std::fs::write(path, b"copy").unwrap();
            a.written(*doc, path, state, true);
        }
    }

    #[test]
    fn a_copy_is_written_once_the_document_is_left_alone() {
        let (mut a, dir, mut d) = bench("idle");
        let clean = DocState { doc: DocId(2), modified: false, state: 0, name: "Clean".into() };
        assert_eq!(a.tick(std::slice::from_ref(&clean), 0.0), (vec![], None), "nothing unsaved, nothing to do");
        // Changed at 10: due five seconds after the last change.
        assert_eq!(a.tick(&[d.clone(), clean.clone()], 10.0), (vec![], Some(5.0)));
        assert_eq!(a.tick(&[d.clone(), clean.clone()], 13.0), (vec![], Some(2.0)));
        d.state = 2;
        assert_eq!(a.tick(&[d.clone(), clean.clone()], 14.0), (vec![], Some(5.0)), "a change starts the wait again");
        let (due, next) = a.tick(&[d.clone(), clean.clone()], 19.0);
        assert_eq!((due.len(), next), (1, None));
        assert_eq!(due[0], (DocId(1), dir.join(file_name(WHO, DocId(1), "Sketch"))));
        // While it's on its way, and once it's there, nothing more is asked.
        assert_eq!(a.tick(&[d.clone()], 19.5), (vec![], None));
        write(&mut a, &due, 2);
        assert_eq!(a.tick(&[d.clone()], 100.0), (vec![], None));
        assert_eq!(files(&dir), [file_name(WHO, DocId(1), "Sketch")]);
        assert!(a.behind(&[d.clone()]).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn steady_work_is_copied_every_minute() {
        let (mut a, dir, mut d) = bench("steady");
        // A change every second: never idle for five.
        let mut written = Vec::new();
        for t in 0..150 {
            d.state = t + 1;
            let (due, _) = a.tick(std::slice::from_ref(&d), t as f64);
            if !due.is_empty() {
                written.push(t);
                write(&mut a, &due, d.state);
            }
        }
        assert_eq!(written, [60, 121], "a minute after the changes began, and a minute after those since");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_copy_goes_when_the_changes_do() {
        let (mut a, dir, mut d) = bench("gone");
        a.tick(std::slice::from_ref(&d), 0.0);
        let (due, _) = a.tick(std::slice::from_ref(&d), 5.0);
        write(&mut a, &due, 1);
        assert_eq!(files(&dir).len(), 1);
        // Saved for real (or undone back to saved).
        d.modified = false;
        a.tick(std::slice::from_ref(&d), 6.0);
        assert!(files(&dir).is_empty());
        // Changed again, copied again; then its tab closes.
        (d.modified, d.state) = (true, 2);
        a.tick(std::slice::from_ref(&d), 7.0);
        let (due, _) = a.tick(std::slice::from_ref(&d), 12.0);
        write(&mut a, &due, 2);
        assert_eq!(files(&dir).len(), 1);
        a.tick(&[], 13.0);
        assert!(files(&dir).is_empty());
        // A copy that lands after its document closed goes too.
        let mut late = DocState { doc: DocId(7), modified: true, state: 4, name: "Late".into() };
        a.tick(std::slice::from_ref(&late), 20.0);
        let (due, _) = a.tick(std::slice::from_ref(&late), 25.0);
        a.tick(&[], 25.5);
        write(&mut a, &due, 4);
        assert!(files(&dir).is_empty());
        // And one that lands after a save, with the document still open.
        late.state = 5;
        a.tick(std::slice::from_ref(&late), 30.0);
        let (due, _) = a.tick(std::slice::from_ref(&late), 35.0);
        late.modified = false;
        a.tick(std::slice::from_ref(&late), 35.5);
        write(&mut a, &due, 5);
        assert!(files(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_renamed_document_and_a_failed_copy() {
        let (mut a, dir, mut d) = bench("rename");
        a.tick(std::slice::from_ref(&d), 0.0);
        let (due, _) = a.tick(std::slice::from_ref(&d), 5.0);
        write(&mut a, &due, 1);
        // Renamed and changed: the new copy has the new name, alone.
        (d.name, d.state) = ("Final".into(), 2);
        a.tick(std::slice::from_ref(&d), 6.0);
        let (due, _) = a.tick(std::slice::from_ref(&d), 11.0);
        write(&mut a, &due, 2);
        assert_eq!(files(&dir), [file_name(WHO, DocId(1), "Final")]);
        // A copy that fails isn't tried again until the next change, and
        // the last good one stays.
        d.state = 3;
        a.tick(std::slice::from_ref(&d), 12.0);
        let (due, _) = a.tick(std::slice::from_ref(&d), 17.0);
        a.written(due[0].0, &due[0].1, 3, false);
        assert_eq!(a.tick(std::slice::from_ref(&d), 60.0), (vec![], None));
        assert_eq!(files(&dir).len(), 1);
        assert_eq!(a.behind(std::slice::from_ref(&d)).len(), 1, "a last save would write it");
        d.state = 4;
        assert_eq!(a.tick(std::slice::from_ref(&d), 61.0), (vec![], Some(5.0)));
        // Quitting gives up what isn't saved.
        a.clear();
        assert!(files(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_recovered_file_becomes_the_documents_copy() {
        let (mut a, dir, d) = bench("adopt");
        let orphan = dir.join("window-4647-w1-First_Light.svg");
        std::fs::write(&orphan, b"lost work").unwrap();
        a.adopt(&d, &orphan);
        assert_eq!(files(&dir), [file_name(WHO, DocId(1), "Sketch")], "under this process's name now");
        // It's the copy of the document as it stands: nothing to write,
        // nothing behind, until the document changes.
        assert_eq!(a.tick(std::slice::from_ref(&d), 0.0), (vec![], None));
        assert!(a.behind(std::slice::from_ref(&d)).is_empty());
        // Saved somewhere for real, it goes like any copy.
        a.tick(&[DocState { modified: false, ..d.clone() }], 1.0);
        assert!(files(&dir).is_empty());
        // With no folder there's nothing to keep, and nothing breaks.
        let mut none = Autosave::new(None);
        assert_eq!(none.tick(std::slice::from_ref(&d), 0.0), (vec![], Some(5.0)));
        assert_eq!(none.tick(std::slice::from_ref(&d), 9.0), (vec![], None));
        assert!(none.behind(std::slice::from_ref(&d)).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
