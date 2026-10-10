//! Work that must never hold a frame up (LS3's `files.rs`): pickers on
//! threads of their own, reading and writing on the job pool. Each
//! sends what it did back over a channel and wakes the loop; the window
//! picks it up on its thread, where the core lives.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use ink_core::{DocId, SaveJob};
use lntrn_app::Waker;
use lntrn_core::jobs::Pool;

use crate::picker::{self, Ask};

/// What to do once a save is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// Close the tab: the save came from "save first?".
    Close,
}

/// Something a thread finished.
pub enum Done {
    /// Open's picker closed, with a path or none.
    OpenPicked(Option<PathBuf>),
    /// Save As's picker closed for `doc`.
    SavePicked { doc: DocId, then: Then, path: Option<PathBuf> },
    /// A file's text, to be opened as a drawing.
    Read { path: PathBuf, result: Result<String, String> },
    Written { job: SaveJob, then: Then, result: Result<(), String> },
    /// Export's picker closed.
    ExportPicked(Option<PathBuf>),
    /// An export was written, or wasn't: what to say of it.
    Exported(Result<String, String>),
    /// An autosave copy of `doc` as it was at `state` reached `path`,
    /// or didn't.
    Copied { doc: DocId, path: PathBuf, state: u64, result: Result<(), String> },
    /// A copy left behind (the drawing `name`), read to be restored.
    Recovered { path: PathBuf, name: String, result: Result<String, String> },
}

pub struct Files {
    tx: Sender<Done>,
    rx: Receiver<Done>,
    waker: Option<Waker>,
    /// A picker is open: a second Ctrl+O doesn't stack another.
    picking: bool,
    /// Documents with a save on its way.
    saving: Vec<DocId>,
}

impl Default for Files {
    fn default() -> Self {
        let (tx, rx) = channel();
        Files { tx, rx, waker: None, picking: false, saving: Vec::new() }
    }
}

impl Files {
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// Everything that finished since the last look.
    pub fn finished(&mut self) -> Vec<Done> {
        let done: Vec<Done> = self.rx.try_iter().collect();
        for d in &done {
            match d {
                Done::OpenPicked(_) | Done::SavePicked { .. } | Done::ExportPicked(_) => self.picking = false,
                Done::Written { job, .. } => self.saving.retain(|d| *d != job.doc),
                Done::Read { .. } | Done::Exported(_) | Done::Copied { .. } | Done::Recovered { .. } => {}
            }
        }
        done
    }

    pub fn is_saving(&self, doc: DocId) -> bool {
        self.saving.contains(&doc)
    }

    fn sender(&self) -> impl Fn(Done) + Send + 'static {
        let (tx, waker) = (self.tx.clone(), self.waker.clone());
        move |done| {
            // Only fails once the window is gone: nothing to tell then.
            let _ = tx.send(done);
            if let Some(w) = &waker {
                w.wake();
            }
        }
    }

    pub fn pick_open(&mut self) {
        if std::mem::replace(&mut self.picking, true) {
            return;
        }
        let send = self.sender();
        std::thread::spawn(move || send(Done::OpenPicked(picker::pick(&Ask::Open))));
    }

    pub fn pick_save(&mut self, doc: DocId, name: String, then: Then) {
        if std::mem::replace(&mut self.picking, true) {
            return;
        }
        let send = self.sender();
        std::thread::spawn(move || {
            let path = picker::pick(&Ask::Save { name }).map(picker::with_extension);
            send(Done::SavePicked { doc, then, path });
        });
    }

    /// Ask where an export goes, suggesting `name`.
    pub fn pick_export(&mut self, name: String, png: bool) {
        if std::mem::replace(&mut self.picking, true) {
            return;
        }
        let send = self.sender();
        std::thread::spawn(move || send(Done::ExportPicked(picker::pick(&Ask::Export { name, png }))));
    }

    /// Write an export.
    pub fn export(&mut self, job: crate::exporting::ExportJob) {
        let send = self.sender();
        Pool::global().spawn(move || send(Done::Exported(job.run())));
    }

    /// Write an autosave copy: `text`, the drawing `doc` as it was at
    /// `state`, to `path`.
    pub fn copy(&mut self, doc: DocId, path: PathBuf, state: u64, text: String) {
        let send = self.sender();
        Pool::global().spawn(move || {
            let made = path.parent().map_or(Ok(()), std::fs::create_dir_all).map_err(|e| e.to_string());
            let result = made.and_then(|()| ink_core::write_atomic(&path, text.as_bytes()).map_err(|e| e.to_string()));
            send(Done::Copied { doc, path, state, result });
        });
    }

    /// Read a copy left behind, to restore the drawing `name`.
    pub fn recover(&mut self, path: PathBuf, name: String) {
        let send = self.sender();
        Pool::global().spawn(move || {
            let result = ink_core::read_text(&path).map_err(|e| e.to_string());
            send(Done::Recovered { path, name, result });
        });
    }

    /// Read the text of the file at `path`.
    pub fn read(&mut self, path: PathBuf) {
        let send = self.sender();
        Pool::global().spawn(move || {
            let result = ink_core::read_text(&path).map_err(|e| e.to_string());
            send(Done::Read { path, result });
        });
    }

    /// Write `job`'s file.
    pub fn write(&mut self, job: SaveJob, then: Then) {
        self.saving.push(job.doc);
        let send = self.sender();
        Pool::global().spawn(move || {
            let result = job.write().map_err(|e| e.to_string());
            send(Done::Written { job, then, result });
        });
    }
}
