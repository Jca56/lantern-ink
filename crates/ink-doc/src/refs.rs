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

    /// An id nothing has yet: `wanted` itself, or with the first number
    /// after it that makes it so (`glow`, `glow-2`, `glow-3`).
    pub fn free(&self, wanted: &str) -> String {
        if self.get(wanted).is_none() {
            return wanted.to_owned();
        }
        (2..).map(|n| format!("{wanted}-{n}")).find(|id| self.get(id).is_none()).expect("there is always another number")
    }

    /// The node of `kind` that the property `name` of `node` points at
    /// with a `url(#…)`.
    pub fn target<'d>(&self, doc: &'d Document, node: &Node, name: &str, kind: Kind) -> Option<&'d Node> {
        let (id, _) = url_id(prop(node, name)?)?;
        doc.get(self.get(id)?).filter(|n| n.kind == kind)
    }
}

/// The ids the attribute `name` names when it says `value`: every
/// `url(#id)` in it, or for an `href` the `#id` it is.
pub fn named<'a>(name: &str, value: &'a str) -> Vec<&'a str> {
    if name == "href" || name.ends_with(":href") {
        return value.trim().strip_prefix('#').into_iter().collect();
    }
    let mut found = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find("url(") {
        let Some(close) = rest[open..].find(')') else { break };
        found.extend(rest[open + 4..open + close].trim().trim_matches(['"', '\'']).trim().strip_prefix('#'));
        rest = &rest[open + close + 1..];
    }
    found
}

/// Which nodes use each `id`: name it in an attribute of theirs (a
/// fill, a clip path, an `href`).
pub fn users(doc: &Document) -> HashMap<String, Vec<NodeId>> {
    let mut by_id: HashMap<String, Vec<NodeId>> = HashMap::new();
    for id in doc.descendants(doc.root()) {
        for attr in doc.get(id).into_iter().flat_map(|n| &n.attrs) {
            for name in named(&attr.name, &attr.value) {
                let users = by_id.entry(name.to_owned()).or_default();
                if !users.contains(&id) {
                    users.push(id);
                }
            }
        }
    }
    by_id
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
        assert_eq!((ids.free("b"), ids.free("a")), ("b".to_owned(), "a-2".to_owned()));
        // What names what: by a url() anywhere in a value, or an href.
        assert_eq!(named("style", "fill: url( '#a' ) red; mask: url(#nope)"), ["a", "nope"]);
        assert_eq!((named("xlink:href", " #a"), named("href", "a.svg#b"), named("fill", "#a")), (vec!["a"], vec![], vec![]));
        let used = users(&d);
        assert_eq!((used["a"].clone(), used["nope"].clone(), used.get("b")), (vec![NodeId(4), NodeId(5)], vec![NodeId(4)], None));
    }
}
