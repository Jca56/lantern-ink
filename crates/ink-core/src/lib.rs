//! Lantern Ink's core (ARCHITECTURE §4): the documents that are open,
//! and everything done to them. Headless: no window, no GPU.
//!
//! - [`Core`]: open and new documents by [`DocId`]; [`Core::apply`] runs
//!   a [`Command`] as one step of history; [`Core::undo`] and
//!   [`Core::redo`]; [`Core::save`] (atomic); [`Core::render`] and
//!   [`Core::export_png`].
//! - [`History`]: the steps taken, each with its [`Actor`].

mod core;
mod error;
mod file;
mod history;

pub use ink_doc;
pub use ink_render;

pub use crate::core::{Core, Opened};
pub use error::CoreError;
pub use file::MAX_FILE_BYTES;
pub use history::{Actor, History, Step};
// What a front end needs to drive a core, in one place.
pub use ink_doc::{Applied, Command, DocId, Document, NodeId, Place};
pub use ink_render::View;
