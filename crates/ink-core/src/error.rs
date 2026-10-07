//! Why the core couldn't do what was asked, in words that say what to do
//! about it.

use core::fmt;
use std::path::PathBuf;

use ink_doc::{DocError, DocId};
use ink_render::BadSize;

#[derive(Clone, Debug, PartialEq)]
pub enum CoreError {
    NoSuchDoc(DocId),
    /// The document refused: a file that isn't an SVG, an edit that
    /// can't be made.
    Doc(DocError),
    /// A file couldn't be read or written: which, and why.
    File { path: PathBuf, why: String },
    /// The file is another open document's: two on one file would
    /// save over each other.
    AlreadyOpen { path: PathBuf, doc: DocId },
    /// A document that has never been saved was saved without saying
    /// where.
    NoPath(DocId),
    Size(BadSize),
    NothingToUndo,
    NothingToRedo,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::NoSuchDoc(id) => write!(f, "no open document {id}"),
            CoreError::Doc(e) => write!(f, "{e}"),
            CoreError::File { path, why } => write!(f, "{}: {why}", path.display()),
            CoreError::AlreadyOpen { path, doc } => write!(f, "{} is open as {doc}: a file has one drawing at a time", path.display()),
            CoreError::NoPath(id) => write!(f, "{id} has no file yet: say where to save it"),
            CoreError::Size(e) => write!(f, "{e}"),
            CoreError::NothingToUndo => f.write_str("there's nothing to undo"),
            CoreError::NothingToRedo => f.write_str("there's nothing to redo"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<DocError> for CoreError {
    fn from(e: DocError) -> Self {
        CoreError::Doc(e)
    }
}

impl From<BadSize> for CoreError {
    fn from(e: BadSize) -> Self {
        CoreError::Size(e)
    }
}
