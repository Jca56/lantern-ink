//! What a document is made of: elements, their attributes in the file's
//! order, and what's written between them. Each remembers how it was
//! written, so the writer can put back exactly what was read
//! (ARCHITECTURE §3.2).
//!
//! An [`Element`] stands alone (read from text, or about to be put into
//! a document); a [`Node`] is one in a document, with its ID.

use std::borrow::Cow;

use crate::id::NodeId;
use crate::kind::Kind;
use crate::xml::escape::unescape;

/// A short piece of formatting, kept without allocating when it's one of
/// the usual ones.
pub(crate) fn keep(s: &str) -> Cow<'static, str> {
    match s {
        "" => Cow::Borrowed(""),
        " " => Cow::Borrowed(" "),
        "=" => Cow::Borrowed("="),
        _ => Cow::Owned(s.to_owned()),
    }
}

/// An attribute's value as the file had it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Raw {
    /// As written, which is the value itself.
    Verbatim,
    /// As written, where that isn't the value (it has entities).
    Escaped(Box<str>),
    /// Set since it was read: written afresh.
    Fresh,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// As written, prefix and all: `fill`, `xlink:href`.
    pub name: String,
    /// Its value, entities resolved.
    pub value: String,
    /// The whitespace before its name.
    pub(crate) lead: Cow<'static, str>,
    /// What's between its name and its value's quote: `=`, with whatever
    /// space the file had around it.
    pub(crate) eq: Cow<'static, str>,
    /// `"` or `'`.
    pub(crate) quote: u8,
    pub(crate) raw: Raw,
}

impl Attr {
    /// A new attribute, written the plain way: ` name="value"`.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Attr {
        Attr { name: name.into(), value: value.into(), lead: Cow::Borrowed(" "), eq: Cow::Borrowed("="), quote: b'"', raw: Raw::Fresh }
    }

    /// One read from text: `raw` is what stood between its quotes.
    pub(crate) fn read(name: &str, raw: &str, lead: &str, eq: &str, quote: u8) -> Attr {
        let (value, raw) = match unescape(raw) {
            Cow::Borrowed(same) => (same.to_owned(), Raw::Verbatim),
            Cow::Owned(value) => (value, Raw::Escaped(raw.into())),
        };
        Attr { name: name.to_owned(), value, lead: keep(lead), eq: keep(eq), quote, raw }
    }

    /// Give it a new value. The same value again changes nothing, so it's
    /// still written as the file had it.
    pub(crate) fn set(&mut self, value: &str) {
        if self.value != value {
            value.clone_into(&mut self.value);
            self.raw = Raw::Fresh;
        }
    }
}

/// How an element's tags were written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Written {
    /// The whitespace between the last attribute (or the name) and the
    /// end of the opening tag.
    pub tail: Cow<'static, str>,
    /// `<a/>`, not `<a></a>`.
    pub self_closing: bool,
    /// The whitespace in the closing tag, after the name.
    pub close_tail: Cow<'static, str>,
}

/// The first attribute called `name`.
fn find<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.name == name).map(|a| a.value.as_str())
}

/// A name's part after its prefix: `href` of `xlink:href`.
pub(crate) fn local(name: &str) -> &str {
    name.split_once(':').map_or(name, |(_, local)| local)
}

/// A name's prefix, if it has one: `xlink` of `xlink:href`.
pub(crate) fn prefix(name: &str) -> Option<&str> {
    name.split_once(':').map(|(prefix, _)| prefix)
}

/// The character data in a run of raw content: comments and processing
/// instructions left out, CDATA unwrapped, entities resolved.
pub(crate) fn characters(raw: &str, out: &mut String) {
    let mut rest = raw;
    while let Some(lt) = rest.find('<') {
        out.push_str(&unescape(&rest[..lt]));
        let markup = &rest[lt..];
        let (skip, end) = if let Some(cdata) = markup.strip_prefix("<![CDATA[") {
            let len = cdata.find("]]>").unwrap_or(cdata.len());
            out.push_str(&cdata[..len]);
            (9 + len, "]]>")
        } else if let Some(comment) = markup.strip_prefix("<!--") {
            (4 + comment.find("-->").unwrap_or(comment.len()), "-->")
        } else {
            let instruction = markup.get(2..).unwrap_or("");
            (2 + instruction.find("?>").unwrap_or(instruction.len()), "?>")
        };
        rest = markup.get(skip + end.len()..).unwrap_or("");
    }
    out.push_str(&unescape(rest));
}

/// An element on its own: read from text, or made to be put in a
/// document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// As written, prefix and all.
    pub name: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Content>,
    pub(crate) written: Written,
}

/// What an [`Element`] holds, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Element(Element),
    /// Raw text between elements, exactly as written: character data
    /// (entities unresolved), comments, CDATA, processing instructions.
    Text(String),
}

impl Element {
    /// A new, empty element, written `<name/>` until it has children.
    pub fn new(name: impl Into<String>) -> Element {
        Element { name: name.into(), attrs: Vec::new(), children: Vec::new(), written: Written { self_closing: true, ..Written::default() } }
    }

    /// With the attribute `name` set to `value`.
    pub fn with(mut self, name: &str, value: impl Into<String>) -> Element {
        match self.attrs.iter_mut().find(|a| a.name == name) {
            Some(attr) => attr.set(&value.into()),
            None => self.attrs.push(Attr::new(name, value)),
        }
        self
    }

    /// With `child` as its last child.
    pub fn child(mut self, child: Element) -> Element {
        self.children.push(Content::Element(child));
        self
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        find(&self.attrs, name)
    }
}

/// An element in a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: NodeId,
    /// Stamped anew each time the node changes: `(id, rev)` names one
    /// state of it for good, across undo and redo.
    pub rev: u64,
    pub parent: Option<NodeId>,
    /// As written, prefix and all: `path`, `linearGradient`, `bx:grid`.
    pub name: String,
    /// What it is, by its name and namespace.
    pub kind: Kind,
    /// In the file's order.
    pub attrs: Vec<Attr>,
    pub children: Vec<Child>,
    pub(crate) written: Written,
}

/// What a [`Node`] holds, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Child {
    Node(NodeId),
    /// Raw text between elements, exactly as written (see
    /// [`Content::Text`]).
    Text(String),
}

impl Node {
    /// The value of the first attribute called `name`.
    pub fn attr(&self, name: &str) -> Option<&str> {
        find(&self.attrs, name)
    }

    /// Its name without any prefix.
    pub fn local(&self) -> &str {
        local(&self.name)
    }

    /// Its child elements, in order.
    pub fn elements(&self) -> impl DoubleEndedIterator<Item = NodeId> + '_ {
        self.children.iter().filter_map(|c| match c {
            Child::Node(id) => Some(*id),
            Child::Text(_) => None,
        })
    }

    /// The character data directly inside it (not its child elements'):
    /// what a `<title>` or a `<style>` says.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for child in &self.children {
            if let Child::Text(raw) = child {
                characters(raw, &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attribute_keeps_how_it_was_written_until_it_changes() {
        let mut a = Attr::read("d", "M0 0&#10;L1 1", "\n   ", " = ", b'\'');
        assert_eq!((a.value.as_str(), &a.raw), ("M0 0\nL1 1", &Raw::Escaped("M0 0&#10;L1 1".into())));
        a.set("M0 0\nL1 1");
        assert!(matches!(a.raw, Raw::Escaped(_)), "the same value again changes nothing");
        a.set("M2 2");
        assert_eq!((a.value.as_str(), &a.raw), ("M2 2", &Raw::Fresh));
        assert_eq!(Attr::read("x", "12", " ", "=", b'"').raw, Raw::Verbatim);
        assert!(matches!(Attr::read("x", "12", " ", "=", b'"').lead, Cow::Borrowed(_)), "the usual formatting isn't allocated");
    }

    #[test]
    fn names_split_at_their_prefix() {
        assert_eq!((local("xlink:href"), prefix("xlink:href")), ("href", Some("xlink")));
        assert_eq!((local("path"), prefix("path")), ("path", None));
    }

    #[test]
    fn character_data_leaves_markup_out() {
        let mut out = String::new();
        characters("a &amp; b<!-- not this --><![CDATA[<raw> & ]]><?pi nor this?>c", &mut out);
        assert_eq!(out, "a & b<raw> & c");
        let mut cut = String::new();
        characters("x<!-- never closed", &mut cut);
        assert_eq!(cut, "x");
    }

    #[test]
    fn elements_are_built_piece_by_piece() {
        let g = Element::new("g").with("fill", "red").with("fill", "blue").child(Element::new("rect").with("width", "4"));
        assert_eq!(g.attr("fill"), Some("blue"));
        assert_eq!(g.attrs.len(), 1);
        assert!(matches!(&g.children[0], Content::Element(e) if e.name == "rect" && e.attr("width") == Some("4")));
    }
}
