//! XML, as much as an SVG needs and nothing lost (ARCHITECTURE §3.2):
//! [`parse`] reads text into elements that remember how they were
//! written, and [`write`] puts them back.

pub(crate) mod escape;
pub(crate) mod parse;
pub(crate) mod write;
