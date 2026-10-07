//! Stable IDs. Each is a number that is never reused within its document
//! (the document's counters only go up), shown with a one-letter prefix:
//! `d1`, `N7`. Commands, history, the renderer and the MCP tools all name
//! things by ID. A node's ID lives for as long as its document is open
//! and is never written to the file (ARCHITECTURE §3.3): an element's own
//! `id="…"` is just an attribute.
//!
//! A document open in Ink's window is written `w3`, one in a headless
//! server `d3`: which it is is part of the ID, so the two can never name
//! the same document (the live bridge goes by it).

use core::fmt;
use core::str::FromStr;

/// A string that isn't an ID of the expected kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseIdError {
    pub text: String,
    pub expected: &'static str,
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"{}\" is not a {} ID (like \"{}1\")", self.text, self.expected, self.expected)
    }
}

impl std::error::Error for ParseIdError {}

/// A document, per process: `d3`, or `w3` for one in Ink's window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocId(pub u64);

/// The bit of a [`DocId`] that says it's a window's.
const WINDOW: u64 = 1 << 63;

impl DocId {
    /// The window's document number `n`, written `w{n}`.
    pub const fn window(n: u64) -> DocId {
        DocId(n | WINDOW)
    }

    /// Whether it's a document in Ink's window (not a headless one).
    pub const fn is_window(self) -> bool {
        self.0 & WINDOW != 0
    }

    /// Its number, without which kind it is.
    pub const fn number(self) -> u64 {
        self.0 & !WINDOW
    }
}

impl fmt::Display for DocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", if self.is_window() { 'w' } else { 'd' }, self.number())
    }
}

/// The number after a prefix: digits only, from 1.
fn number(rest: &str) -> Option<u64> {
    rest.parse::<u64>().ok().filter(|&n| n > 0 && n < WINDOW && rest.bytes().all(|b| b.is_ascii_digit()))
}

impl FromStr for DocId {
    type Err = ParseIdError;

    /// `"d7"` → a headless document, `"w7"` → a window's. IDs start at 1.
    fn from_str(s: &str) -> Result<Self, ParseIdError> {
        match (s.strip_prefix('d').and_then(number), s.strip_prefix('w').and_then(number)) {
            (Some(n), _) => Ok(DocId(n)),
            (_, Some(n)) => Ok(DocId::window(n)),
            _ => Err(ParseIdError { text: s.to_owned(), expected: "d" }),
        }
    }
}

/// A node (an element), per document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "N{}", self.0)
    }
}

impl FromStr for NodeId {
    type Err = ParseIdError;

    /// `"N7"` → `NodeId(7)`. IDs start at 1.
    fn from_str(s: &str) -> Result<Self, ParseIdError> {
        s.strip_prefix('N').and_then(number).map(NodeId).ok_or_else(|| ParseIdError { text: s.to_owned(), expected: "N" })
    }
}

/// The next ID a document hands out, and the next revision it stamps on a
/// changed node. Neither goes back, undo or not, so an ID or an `(id,
/// rev)` pair never comes to mean something else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Counters {
    node: u64,
    rev: u64,
    anchor: u64,
}

impl Default for Counters {
    fn default() -> Self {
        Self { node: 1, rev: 1, anchor: 1 }
    }
}

impl Counters {
    pub fn node(&mut self) -> NodeId {
        NodeId(bump(&mut self.node))
    }

    pub fn rev(&mut self) -> u64 {
        bump(&mut self.rev)
    }

    /// The next anchor of a path ([`crate::outline`]).
    pub fn anchor(&mut self) -> crate::outline::AnchorId {
        crate::outline::AnchorId(bump(&mut self.anchor))
    }
}

fn bump(next: &mut u64) -> u64 {
    let id = *next;
    *next += 1;
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_print_and_parse_with_their_prefix() {
        assert_eq!(NodeId(7).to_string(), "N7");
        assert_eq!("N7".parse(), Ok(NodeId(7)));
        assert_eq!("d12".parse(), Ok(DocId(12)));
        for bad in ["", "N", "N0", "7", "n7", "L7", "N-1", "N+1", "N 7", "N7x"] {
            assert_eq!(bad.parse::<NodeId>().unwrap_err().expected, "N", "{bad}");
        }
    }

    #[test]
    fn a_windows_document_is_told_from_a_headless_one() {
        let (headless, window) = (DocId(3), DocId::window(3));
        assert_ne!(headless, window, "the same number, never the same document");
        assert_eq!((headless.to_string(), window.to_string()), ("d3".to_owned(), "w3".to_owned()));
        assert_eq!(("d3".parse(), "w3".parse()), (Ok(headless), Ok(window)));
        assert!(window.is_window() && !headless.is_window());
        assert_eq!((window.number(), headless.number()), (3, 3));
        for bad in ["", "d", "w", "w0", "d0", "W3", "w+3", "w3 ", "x3", "w18446744073709551615", "d9223372036854775808"] {
            assert!(bad.parse::<DocId>().is_err(), "{bad}");
        }
    }

    #[test]
    fn counters_start_at_one_and_never_repeat() {
        let mut c = Counters::default();
        assert_eq!((c.node(), c.node()), (NodeId(1), NodeId(2)));
        assert_eq!((c.rev(), c.rev()), (1, 2));
    }
}
