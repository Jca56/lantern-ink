//! The document: an SVG's own tree (ARCHITECTURE §3). Nodes are held by
//! ID behind `Arc`s, so a snapshot of the whole document is a map of
//! pointers, and an edit copies only the nodes it touches.

use std::collections::HashMap;
use std::sync::Arc;

use ink_geom::number;

use crate::error::DocError;
use crate::id::{Counters, DocId, NodeId};
use crate::kind::{INK_NS, INK_PREFIX, Kind, SVG_NS};
use crate::node::{Child, Content, Element, Node, local, prefix};
use crate::xml::parse::{MAX_NODES, parse};
use crate::xml::write::{close_tag, open_tag};

#[derive(Clone, Debug)]
pub struct Document {
    pub id: DocId,
    pub(crate) root: NodeId,
    pub(crate) nodes: HashMap<NodeId, Arc<Node>>,
    /// What the file has before its root element (an XML declaration, a
    /// DOCTYPE, comments) and after it, as written.
    pub(crate) before: String,
    pub(crate) after: String,
    pub(crate) next: Counters,
}

/// A document's content at one moment: what undo goes back to. It shares
/// every node with the document it was taken from.
#[derive(Clone, Debug)]
pub struct Snapshot {
    root: NodeId,
    nodes: HashMap<NodeId, Arc<Node>>,
    before: String,
    after: String,
}

impl Document {
    /// Read an SVG. Anything in it that Ink doesn't understand is kept
    /// as it is; only text that isn't well-formed XML, or isn't an SVG,
    /// is refused.
    pub fn parse(id: DocId, text: &str) -> Result<Document, DocError> {
        let parsed = parse(text)?;
        let mut doc = Document { id, root: NodeId(0), nodes: HashMap::new(), before: parsed.before, after: parsed.after, next: Counters::default() };
        let name = parsed.root.name.clone();
        doc.root = doc.graft(parsed.root, None)?;
        if doc.nodes[&doc.root].kind != Kind::Svg {
            return Err(DocError::NotSvg(format!("its root element is <{name}>, not an SVG's <svg>")));
        }
        doc.restyle();
        Ok(doc)
    }

    /// A new, empty drawing `width` × `height` user units, as Ink writes
    /// one.
    pub fn new(id: DocId, width: f64, height: f64) -> Document {
        let (w, h) = (number::format(width, 3), number::format(height, 3));
        let text = format!("<svg xmlns=\"{SVG_NS}\" xmlns:{INK_PREFIX}=\"{INK_NS}\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n</svg>\n");
        Document::parse(id, &text).expect("Ink's own empty drawing reads")
    }

    /// The document as SVG text: byte for byte what was read, but for
    /// what has changed since.
    pub fn to_svg(&self) -> String {
        let mut out = String::with_capacity(self.before.len() + self.after.len() + self.nodes.len() * 96);
        out.push_str(&self.before);
        self.write(self.root, &mut out);
        out.push_str(&self.after);
        out
    }

    /// One node, and everything in it, as markup.
    pub fn markup(&self, id: NodeId) -> Result<String, DocError> {
        self.node(id)?;
        let mut out = String::new();
        self.write(id, &mut out);
        Ok(out)
    }

    fn write(&self, id: NodeId, out: &mut String) {
        let Some(node) = self.nodes.get(&id) else { return };
        let empty = node.children.is_empty();
        open_tag(&node.name, &node.attrs, &node.written, empty, out);
        for child in &node.children {
            match child {
                Child::Node(child) => self.write(*child, out),
                Child::Text(raw) => out.push_str(raw),
            }
        }
        close_tag(&node.name, &node.written, empty, out);
    }

    /// The `<svg>` everything is in.
    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn node(&self, id: NodeId) -> Result<&Node, DocError> {
        self.nodes.get(&id).map(Arc::as_ref).ok_or(DocError::NoSuchNode(id))
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id).map(Arc::as_ref)
    }

    /// How many elements it holds.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Never: a document always has its root.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// `id`'s ancestors, its parent first, the root last.
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = &Node> {
        let mut at = self.get(id).and_then(|n| n.parent);
        std::iter::from_fn(move || {
            let node = self.get(at?)?;
            at = node.parent;
            Some(node)
        })
    }

    /// `id` and every element under it, in the file's order.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            let Some(node) = self.get(id) else { continue };
            out.push(id);
            stack.extend(node.elements().rev());
        }
        out
    }

    /// Whether `inner` is `outer` or somewhere inside it.
    pub fn is_within(&self, inner: NodeId, outer: NodeId) -> bool {
        inner == outer || self.ancestors(inner).any(|n| n.id == outer)
    }

    /// The namespace a prefix (or, for `None`, an unprefixed element
    /// name) stands for at `node`: the nearest `xmlns` declaration, the
    /// node's own included.
    pub fn namespace(&self, node: NodeId, prefix: Option<&str>) -> Option<&str> {
        std::iter::once(self.get(node)?).chain(self.ancestors(node)).find_map(|n| declared(pairs(n), prefix))
    }

    /// What an element called `name` with `attrs` is, as a child of
    /// `parent`: an SVG element when its namespace is SVG's (or, for an
    /// unprefixed name, when nothing declares a namespace at all, as in
    /// SVG pasted out of a web page).
    fn kind_of<'a>(&'a self, parent: Option<NodeId>, name: &str, attrs: impl Iterator<Item = (&'a str, &'a str)>) -> Kind {
        let prefix = prefix(name);
        let ns = declared(attrs, prefix).or_else(|| parent.and_then(|p| self.namespace(p, prefix)));
        match (ns, prefix) {
            (Some(SVG_NS), _) | (None, None) => Kind::of_svg(local(name)),
            _ => Kind::Other,
        }
    }

    /// Put `el` and everything in it into the document as a child of
    /// `parent`, with new IDs. The caller places it among the parent's
    /// children.
    pub(crate) fn graft(&mut self, el: Element, parent: Option<NodeId>) -> Result<NodeId, DocError> {
        if self.nodes.len() >= MAX_NODES {
            return Err(DocError::TooBig(format!("more than {MAX_NODES} elements")));
        }
        let id = self.next.node();
        let kind = self.kind_of(parent, &el.name, el.attrs.iter().map(|a| (a.name.as_str(), a.value.as_str())));
        let node = Node { id, rev: self.next.rev(), parent, name: el.name, kind, attrs: el.attrs, children: Vec::new(), written: el.written, ruled: None };
        self.nodes.insert(id, Arc::new(node));
        let mut children = Vec::with_capacity(el.children.len());
        for content in el.children {
            children.push(match content {
                Content::Element(child) => Child::Node(self.graft(child, Some(id))?),
                Content::Text(raw) => Child::Text(raw),
            });
        }
        // Nothing else holds the node yet: this copies nothing.
        Arc::make_mut(self.nodes.get_mut(&id).expect("just put in")).children = children;
        Ok(id)
    }

    /// Work out again what `id` and everything under it are, after a
    /// namespace declaration on it changed.
    pub(crate) fn rekind(&mut self, id: NodeId) {
        for id in self.descendants(id) {
            let node = &self.nodes[&id];
            let (kind, was) = (self.kind_of(node.parent, &node.name, pairs(node)), node.kind);
            if kind != was {
                let rev = self.next.rev();
                let node = Arc::make_mut(self.nodes.get_mut(&id).expect("listed"));
                (node.kind, node.rev) = (kind, rev);
            }
        }
    }

    /// The document's content now, to come back to.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { root: self.root, nodes: self.nodes.clone(), before: self.before.clone(), after: self.after.clone() }
    }

    /// Go back (or forward) to `snapshot`. The ID and revision counters
    /// stay where they are, so nothing made since is ever named again.
    pub fn restore(&mut self, snapshot: &Snapshot) {
        self.root = snapshot.root;
        self.nodes.clone_from(&snapshot.nodes);
        self.before.clone_from(&snapshot.before);
        self.after.clone_from(&snapshot.after);
    }
}

/// A node's attributes as names and values.
fn pairs(node: &Node) -> impl Iterator<Item = (&str, &str)> {
    node.attrs.iter().map(|a| (a.name.as_str(), a.value.as_str()))
}

/// The namespace `attrs` declare for `prefix` (`None`: the default one).
fn declared<'a>(mut attrs: impl Iterator<Item = (&'a str, &'a str)>, prefix: Option<&str>) -> Option<&'a str> {
    attrs.find_map(|(name, value)| match (name.strip_prefix("xmlns"), prefix) {
        (Some(""), None) => Some(value),
        (Some(rest), Some(p)) if rest.strip_prefix(':') == Some(p) => Some(value),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: DocId = DocId(1);

    #[test]
    fn a_document_reads_and_writes_back_the_same() {
        let text = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\n  <g fill='red'>\n    <path d=\"M0 0h1\" />\n  </g>\n</svg>\n";
        let doc = Document::parse(DOC, text).unwrap();
        assert_eq!(doc.to_svg(), text);
        assert_eq!(doc.len(), 3);
        let ids = doc.descendants(doc.root());
        assert_eq!(ids, vec![NodeId(1), NodeId(2), NodeId(3)], "IDs in the file's order");
        assert_eq!(ids.iter().map(|&id| doc.node(id).unwrap().kind).collect::<Vec<_>>(), vec![Kind::Svg, Kind::G, Kind::Path]);
        assert_eq!(doc.ancestors(NodeId(3)).map(|n| n.id).collect::<Vec<_>>(), vec![NodeId(2), NodeId(1)]);
        assert!(doc.is_within(NodeId(3), NodeId(1)) && !doc.is_within(NodeId(2), NodeId(3)));
        assert_eq!(doc.markup(NodeId(2)).unwrap(), "<g fill='red'>\n    <path d=\"M0 0h1\" />\n  </g>");
        assert_eq!(doc.node(NodeId(9)).unwrap_err(), DocError::NoSuchNode(NodeId(9)));
    }

    #[test]
    fn only_an_svg_is_a_document() {
        assert!(matches!(Document::parse(DOC, "<html/>"), Err(DocError::NotSvg(_))));
        assert!(matches!(Document::parse(DOC, "<svg xmlns=\"http://www.w3.org/1999/xhtml\"/>"), Err(DocError::NotSvg(_))), "an <svg> that isn't SVG's");
        assert!(Document::parse(DOC, "<svg/>").is_ok(), "no namespace at all: taken as SVG");
        assert!(Document::parse(DOC, "<s:svg xmlns:s=\"http://www.w3.org/2000/svg\"/>").is_ok());
    }

    #[test]
    fn kinds_follow_namespaces() {
        let doc = Document::parse(
            DOC,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:bx=\"https://boxy-svg.com\"><defs><bx:export><bx:file path=\"a.svg\"/></bx:export></defs><g xmlns=\"urn:other\"><path/><s:path xmlns:s=\"http://www.w3.org/2000/svg\"/></g><nope:rect/></svg>",
        )
        .unwrap();
        let kinds: Vec<(String, Kind)> = doc.descendants(doc.root()).iter().map(|&id| (doc.node(id).unwrap().name.clone(), doc.node(id).unwrap().kind)).collect();
        let expect = [("svg", Kind::Svg), ("defs", Kind::Defs), ("bx:export", Kind::Other), ("bx:file", Kind::Other), ("g", Kind::Other), ("path", Kind::Other), ("s:path", Kind::Path), ("nope:rect", Kind::Other)];
        assert_eq!(kinds, expect.map(|(n, k)| (n.to_owned(), k)));
        assert_eq!(doc.namespace(NodeId(4), Some("bx")), Some("https://boxy-svg.com"));
        assert_eq!(doc.namespace(NodeId(6), None), Some("urn:other"), "the nearest declaration wins");
        assert_eq!(doc.namespace(NodeId(2), Some("nope")), None);
    }

    #[test]
    fn a_new_drawing_is_inks_own() {
        let doc = Document::new(DOC, 24.0, 16.5);
        assert_eq!(doc.to_svg(), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" width=\"24\" height=\"16.5\" viewBox=\"0 0 24 16.5\">\n</svg>\n");
        assert_eq!(doc.namespace(doc.root(), Some("ink")), Some(INK_NS));
    }

    #[test]
    fn a_snapshot_brings_the_content_back_but_not_the_counters() {
        let mut doc = Document::parse(DOC, "<svg><g/></svg>").unwrap();
        let before = doc.snapshot();
        let added = doc.graft(Element::new("rect"), Some(doc.root())).unwrap();
        assert_eq!((added, doc.len()), (NodeId(3), 3));
        doc.restore(&before);
        assert_eq!((doc.len(), doc.to_svg().as_str()), (2, "<svg><g/></svg>"));
        assert_eq!(doc.graft(Element::new("rect"), Some(doc.root())).unwrap(), NodeId(4), "an ID is never handed out twice");
    }
}
