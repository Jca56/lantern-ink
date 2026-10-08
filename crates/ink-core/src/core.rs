//! The core (ARCHITECTURE §4): the documents that are open, and
//! everything done to them. The window, the MCP server and the live
//! bridge are front ends on one of these.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ink_doc::tidy::Shipped;
use ink_doc::{Adopted, Applied, Command, DocId, Document, Viewport};
use ink_render::View;
use lntrn_image::{Compression, Image};

use crate::error::CoreError;
use crate::file;
use crate::history::{Actor, History, Step};

/// An open document and what the core keeps about it.
struct Open {
    doc: Document,
    history: History,
    /// Its file, once it has one.
    path: Option<PathBuf>,
    /// The state that is on disk (see [`History::stamp`]); `None` when
    /// it has never been saved.
    saved: Option<u64>,
}

/// What opening a file found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    pub doc: DocId,
    /// What of another editor's was taken out on the way in (D25). The
    /// file itself is as it was until the document is saved.
    pub adopted: Adopted,
}

/// A save set going ([`Core::begin_save`]): the drawing as it was
/// then, to be written wherever there's time for the disk.
#[derive(Clone, Debug)]
pub struct SaveJob {
    pub doc: DocId,
    /// The state the file will hold (see [`History::stamp`]).
    pub stamp: u64,
    pub path: PathBuf,
    text: String,
}

impl SaveJob {
    /// Write the file, whole or not at all.
    pub fn write(&self) -> Result<(), CoreError> {
        file::write(&self.path, self.text.as_bytes())
    }
}

pub struct Core {
    docs: BTreeMap<DocId, Open>,
    /// Whether this is the window's core (its documents are `w…`) or a
    /// headless server's (`d…`).
    window: bool,
    next: u64,
}

impl Core {
    /// A headless server's core: its documents are `d1`, `d2`, …
    pub fn headless() -> Core {
        Core { docs: BTreeMap::new(), window: false, next: 1 }
    }

    /// The window's core: its documents are `w{first}` and on.
    pub fn window(first: u64) -> Core {
        Core { docs: BTreeMap::new(), window: true, next: first.max(1) }
    }

    fn next_id(&mut self) -> DocId {
        let n = self.next;
        self.next += 1;
        if self.window { DocId::window(n) } else { DocId(n) }
    }

    fn open(&self, id: DocId) -> Result<&Open, CoreError> {
        self.docs.get(&id).ok_or(CoreError::NoSuchDoc(id))
    }

    fn open_mut(&mut self, id: DocId) -> Result<&mut Open, CoreError> {
        self.docs.get_mut(&id).ok_or(CoreError::NoSuchDoc(id))
    }

    /// A new, empty drawing `width` × `height` user units.
    pub fn new_doc(&mut self, width: f64, height: f64) -> DocId {
        let id = self.next_id();
        let doc = Document::new(id, width, height);
        self.docs.insert(id, Open { history: History::new(&doc), doc, path: None, saved: None });
        id
    }

    /// Open the drawing in `text`, which has no file (yet).
    pub fn open_text(&mut self, text: &str) -> Result<Opened, CoreError> {
        let id = self.next_id();
        let mut doc = Document::parse(id, text)?;
        // Taken over before history begins: undo never brings another
        // editor's marks back.
        let adopted = doc.adopt();
        self.docs.insert(id, Open { history: History::new(&doc), doc, path: None, saved: None });
        Ok(Opened { doc: id, adopted })
    }

    /// The open document whose file is `path`, by whatever name the
    /// path gives it.
    pub fn doc_at(&self, path: &Path) -> Option<DocId> {
        let wanted = file::canonical(path);
        self.docs.iter().find(|(_, open)| open.path.as_deref().is_some_and(|own| file::canonical(own) == wanted)).map(|(&id, _)| id)
    }

    /// Open the file at `path`. One that is open already is refused
    /// ([`Core::doc_at`] finds it): a file has one document at a time.
    pub fn open_file(&mut self, path: &Path) -> Result<Opened, CoreError> {
        if let Some(doc) = self.doc_at(path) {
            return Err(CoreError::AlreadyOpen { path: path.to_owned(), doc });
        }
        self.open_read(path, &file::read(path)?)
    }

    /// Open the file at `path`, whose `text` was read already (by
    /// [`read_text`], off a window's thread). Refused as
    /// [`Core::open_file`] refuses.
    pub fn open_read(&mut self, path: &Path, text: &str) -> Result<Opened, CoreError> {
        if let Some(doc) = self.doc_at(path) {
            return Err(CoreError::AlreadyOpen { path: path.to_owned(), doc });
        }
        let opened = self.open_text(text)?;
        let open = self.open_mut(opened.doc)?;
        // As opened, it's what's on disk: nothing to save until it's
        // edited, even if marks were taken out of it.
        (open.path, open.saved) = (Some(path.to_owned()), Some(open.history.stamp()));
        Ok(opened)
    }

    /// Close `id`, whatever state it's in: whoever asks has checked
    /// [`Core::is_modified`].
    pub fn close(&mut self, id: DocId) -> Result<(), CoreError> {
        self.docs.remove(&id).map(|_| ()).ok_or(CoreError::NoSuchDoc(id))
    }

    /// The open documents, in the order they were opened.
    pub fn docs(&self) -> impl Iterator<Item = DocId> + '_ {
        self.docs.keys().copied()
    }

    pub fn doc(&self, id: DocId) -> Result<&Document, CoreError> {
        Ok(&self.open(id)?.doc)
    }

    pub fn history(&self, id: DocId) -> Result<&History, CoreError> {
        Ok(&self.open(id)?.history)
    }

    /// The document's file, if it has one.
    pub fn path(&self, id: DocId) -> Result<Option<&Path>, CoreError> {
        Ok(self.open(id)?.path.as_deref())
    }

    /// Whether the document is other than what's on disk. Derived, never
    /// set: undoing back to the saved state makes it unmodified again.
    pub fn is_modified(&self, id: DocId) -> Result<bool, CoreError> {
        let open = self.open(id)?;
        Ok(open.saved != Some(open.history.stamp()))
    }

    /// Apply `command` to `id` as one step of its history, `label`led
    /// and `actor`'s: all of it, or (when any part is refused) none. A
    /// command that changes nothing leaves no step.
    pub fn apply(&mut self, id: DocId, command: &Command, actor: Actor, label: &str) -> Result<Applied, CoreError> {
        let open = self.open_mut(id)?;
        let applied = open.doc.apply(command)?;
        if !applied.is_nothing() {
            open.history.record(&open.doc, Step { label: label.to_owned(), actor });
        }
        Ok(applied)
    }

    /// Undo the latest step, whoever made it. Returns the step undone.
    pub fn undo(&mut self, id: DocId) -> Result<Step, CoreError> {
        let open = self.open_mut(id)?;
        open.history.undo(&mut open.doc).ok_or(CoreError::NothingToUndo)
    }

    /// Redo the step last undone. Returns it.
    pub fn redo(&mut self, id: DocId) -> Result<Step, CoreError> {
        let open = self.open_mut(id)?;
        open.history.redo(&mut open.doc).ok_or(CoreError::NothingToRedo)
    }

    /// Save `id` to `path`, or to its own file again. The file is the
    /// document as Ink holds it, written whole or not at all. Another
    /// open document's file is refused.
    pub fn save(&mut self, id: DocId, path: Option<&Path>) -> Result<PathBuf, CoreError> {
        let job = self.begin_save(id, path)?;
        job.write()?;
        self.saved(&job)?;
        Ok(job.path)
    }

    /// A save of `id` as it is now, to `path` or to its own file
    /// again, for whoever can't wait for the disk (a window's frame):
    /// [`SaveJob::write`] on any thread, then [`Core::saved`]. Another
    /// open document's file is refused.
    pub fn begin_save(&self, id: DocId, path: Option<&Path>) -> Result<SaveJob, CoreError> {
        let open = self.open(id)?;
        let path = path.map(Path::to_owned).or_else(|| open.path.clone()).ok_or(CoreError::NoPath(id))?;
        if let Some(doc) = self.doc_at(&path).filter(|&other| other != id) {
            return Err(CoreError::AlreadyOpen { path, doc });
        }
        Ok(SaveJob { doc: id, stamp: open.history.stamp(), path, text: open.doc.to_svg() })
    }

    /// `job` was written: its file is the document's now, and the
    /// state it held is the one on disk (what's been done since is
    /// still to save).
    pub fn saved(&mut self, job: &SaveJob) -> Result<(), CoreError> {
        // Opened by another document while the save was on its way.
        if let Some(doc) = self.doc_at(&job.path).filter(|&other| other != job.doc) {
            return Err(CoreError::AlreadyOpen { path: job.path.clone(), doc });
        }
        let open = self.open_mut(job.doc)?;
        (open.path, open.saved) = (Some(job.path.clone()), Some(job.stamp));
        Ok(())
    }

    /// Write a clean copy of `id` to `path`, to ship
    /// ([`ink_doc::tidy::shipped`]): tidied, without its comments or
    /// Ink's own marks. The drawing isn't touched, and `path` doesn't
    /// become its file. An open document's file is refused, its own
    /// included: what's open would no longer be what's on disk.
    pub fn export_svg(&self, id: DocId, path: &Path) -> Result<Shipped, CoreError> {
        let clean = ink_doc::tidy::shipped(self.doc(id)?);
        if let Some(doc) = self.doc_at(path) {
            return Err(CoreError::AlreadyOpen { path: path.to_owned(), doc });
        }
        file::write(path, clean.svg.as_bytes())?;
        Ok(clean)
    }

    /// How `id` sits on its page.
    pub fn viewport(&self, id: DocId) -> Result<Viewport, CoreError> {
        let doc = self.doc(id)?;
        Ok(Viewport::of(doc.node(doc.root())?))
    }

    /// A picture of `id`, as `view` says.
    pub fn render(&self, id: DocId, view: &View) -> Result<Image, CoreError> {
        Ok(ink_render::render(self.doc(id)?, view)?)
    }

    /// Write a picture of `id` to `path` as a PNG, as small as one gets.
    pub fn export_png(&self, id: DocId, view: &View, path: &Path) -> Result<(), CoreError> {
        let image = self.render(id, view)?;
        file::write(path, &lntrn_image::png::encode_with(&image, Compression::Best))
    }
}
