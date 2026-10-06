//! Why a document couldn't be read, or an edit couldn't be made. Every
//! message says what was wrong in words a person (or Claude) can act on.

use core::fmt;

use crate::id::NodeId;

#[derive(Clone, Debug, PartialEq)]
pub enum DocError {
    /// The text isn't XML an SVG can be read from: where, and why.
    Syntax { line: usize, column: usize, message: String },
    /// It's XML, but not an SVG.
    NotSvg(String),
    /// More than a document may hold (a hostile or broken file).
    TooBig(String),
    NoSuchNode(NodeId),
    /// An edit that can't be made as asked.
    Invalid(String),
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocError::Syntax { line, column, message } => write!(f, "line {line}, column {column}: {message}"),
            DocError::NotSvg(why) => write!(f, "not an SVG: {why}"),
            DocError::TooBig(why) => write!(f, "too big: {why}"),
            DocError::NoSuchNode(id) => write!(f, "no node {id} in this document"),
            DocError::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for DocError {}

pub(crate) fn invalid<T>(why: impl Into<String>) -> Result<T, DocError> {
    Err(DocError::Invalid(why.into()))
}
