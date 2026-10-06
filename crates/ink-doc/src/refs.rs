//! What a node points at by name: `fill="url(#sky)"`, `clip-path`,
//! `filter`, a gradient's `href`.

use std::collections::HashMap;

use crate::document::Document;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::Node;
use crate::style::{prop, url_id};

/// The document's elements by their `id` attribute. Of two with one id,
/// the first in the file is the one a name finds.
pub struct Ids<'a> {
    by_id: HashMap<&'a str, NodeId>,
}

impl<'a> Ids<'a> {
    pub fn of(doc: &'a Document) -> Ids<'a> {
        let mut by_id = HashMap::new();
        for id in doc.descendants(doc.root()) {
            if let Some(name) = doc.get(id).and_then(|n| n.attr("id")) {
                by_id.entry(name).or_insert(id);
            }
        }
        Ids { by_id }
    }

    pub fn get(&self, id: &str) -> Option<NodeId> {
        self.by_id.get(id).copied()
    }

    /// The node of `kind` that the property `name` of `node` points at
    /// with a `url(#…)`.
    pub fn target<'d>(&self, doc: &'d Document, node: &Node, name: &str, kind: Kind) -> Option<&'d Node> {
        let (id, _) = url_id(prop(node, name)?)?;
        doc.get(self.get(id)?).filter(|n| n.kind == kind)
    }
}

/// The id an `href` (or the older `xlink:href`) names.
pub fn href(node: &Node) -> Option<&str> {
    node.attr("href").or_else(|| node.attr("xlink:href")).map(|h| h.trim().trim_start_matches('#'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    #[test]
    fn names_find_the_first_element_called_that() {
        let d = Document::parse(DocId(1), r##"<svg><clipPath id="a"/><g id="a"/><path clip-path="url(#a)" filter="url(#a)" style="mask: url(#nope)"/><use xlink:href=" #a"/></svg>"##).unwrap();
        let ids = Ids::of(&d);
        assert_eq!((ids.get("a"), ids.get("b")), (Some(NodeId(2)), None));
        let path = d.node(NodeId(4)).unwrap();
        assert_eq!(ids.target(&d, path, "clip-path", Kind::ClipPath).map(|n| n.id), Some(NodeId(2)));
        assert!(ids.target(&d, path, "filter", Kind::Filter).is_none(), "it's there, but it isn't a filter");
        assert!(ids.target(&d, path, "mask", Kind::Mask).is_none());
        assert_eq!(href(d.node(NodeId(5)).unwrap()), Some("a"));
    }
}
