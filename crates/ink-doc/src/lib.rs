//! Lantern Ink's document (ARCHITECTURE §3): an SVG's own tree. Elements
//! and attributes are kept as the file wrote them, so a document that
//! hasn't changed saves to the bytes it was read from, and a small edit
//! is a small diff. CPU only.
//!
//! - [`Document`]: the tree, by [`NodeId`]; read with
//!   [`Document::parse`], written with [`Document::to_svg`].
//! - [`Node`], [`Attr`]: an element in it, and its attributes in the
//!   file's order.
//! - [`Element`]: an element on its own, to put in.
//! - [`Command`]: every edit there is, applied whole or not at all
//!   ([`Document::apply`]).
//! - [`Document::adopt`]: another editor's private marks taken out, and
//!   Ink's put in their place.
//!
//! Typed views say what a node means, read from its attributes when
//! asked: [`style`] (the properties a shape inherits), [`geometry`] (the
//! outline it draws), [`transform`], [`gradient`], [`filter`], [`color`],
//! [`length`], [`Viewport`] (how the drawing sits on its page),
//! [`refs`] (what a `url(#…)` points at) and [`text`] (the outlines a
//! `<text>` draws, set in the machine's [`fonts`]).

mod clip;
mod boolean;
pub mod color;
mod command;
mod document;
mod edit;
mod error;
pub mod filter;
pub mod fonts;
mod foreign;
pub mod geometry;
pub mod gradient;
pub mod hit;
mod id;
mod kind;
mod layout;
pub mod length;
pub mod lettering;
mod node;
pub mod outline;
mod outlined;
pub mod pathedit;
pub mod paths;
mod props;
pub mod refs;
mod settle;
mod shape;
pub mod sheet;
mod structure;
mod stroking;
pub mod style;
pub mod styling;
pub mod text;
pub mod tidy;
pub mod transform;
pub mod value;
mod viewport;
mod xml;

pub use command::{Applied, Command, elements};
pub use document::{Document, Snapshot};
pub use edit::Place;
pub use error::DocError;
pub use foreign::Adopted;
pub use id::{DocId, NodeId, ParseIdError};
pub use kind::{INK_NS, INK_PREFIX, Kind, SVG_NS};
pub use node::{Attr, Child, Content, Element, Node};
pub use shape::Geometry;
pub use value::Precision;
pub use viewport::Viewport;
