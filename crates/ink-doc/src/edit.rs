//! The document's edits, at their smallest: set an attribute, put a node
//! in, take one out, move one. Commands (`command.rs`) are made of these.
//!
//! Each keeps the file tidy around what it does (ARCHITECTURE §3.2): a
//! node put in starts a line of its own, indented like its siblings; one
//! taken out takes its line with it; nothing else moves.

use std::sync::Arc;

use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::layout::{indent_before, indentation};
use crate::node::{Attr, Child, Node};
use crate::xml::parse::MAX_DEPTH;

/// Where among a parent's child elements a node goes. Later in the file
/// is further up the picture: SVG paints in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// As this node's first child: under everything in it.
    FirstIn(NodeId),
    /// As its last child: on top of everything in it.
    LastIn(NodeId),
    /// Just before this node, in its parent: right under it.
    Before(NodeId),
    /// Just after it: right on top of it.
    After(NodeId),
}

/// Join texts left side by side, and drop empty ones.
fn merge_texts(children: &mut Vec<Child>) {
    let mut out: Vec<Child> = Vec::with_capacity(children.len());
    for child in children.drain(..) {
        match (out.last_mut(), child) {
            (_, Child::Text(text)) if text.is_empty() => {}
            (Some(Child::Text(last)), Child::Text(text)) => last.push_str(&text),
            (_, child) => out.push(child),
        }
    }
    *children = out;
}

/// A name XML can take as an attribute's.
fn check_name(name: &str) -> Result<(), DocError> {
    let bad = |c: char| c.is_whitespace() || matches!(c, '<' | '>' | '=' | '"' | '\'' | '/' | '&');
    if name.is_empty() || name.contains(bad) || name.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '.') {
        return invalid(format!("\"{name}\" can't be an attribute's name"));
    }
    Ok(())
}

impl Document {
    /// A node to change: copied if a snapshot shares it, and stamped with
    /// a new revision.
    pub(crate) fn edit(&mut self, id: NodeId) -> Result<&mut Node, DocError> {
        let rev = self.next.rev();
        let node = Arc::make_mut(self.nodes.get_mut(&id).ok_or(DocError::NoSuchNode(id))?);
        node.rev = rev;
        Ok(node)
    }

    /// How deep `id` is: 1 for the root.
    pub(crate) fn depth(&self, id: NodeId) -> usize {
        self.ancestors(id).count() + 1
    }

    /// How many levels `id`'s subtree has: 1 for a childless node.
    fn height(&self, id: NodeId) -> usize {
        let mut deepest = 1;
        let mut stack = vec![(id, 1)];
        while let Some((id, level)) = stack.pop() {
            deepest = deepest.max(level);
            if let Some(node) = self.get(id) {
                stack.extend(node.elements().map(|c| (c, level + 1)));
            }
        }
        deepest
    }

    /// Set `name` on `id` to `value` as written, or take it off (`None`).
    /// Whether that changed anything.
    pub(crate) fn set_attr(&mut self, id: NodeId, name: &str, value: Option<&str>) -> Result<bool, DocError> {
        check_name(name)?;
        let node = self.node(id)?;
        let at = node.attrs.iter().position(|a| a.name == name);
        match (at, value) {
            (None, None) => return Ok(false),
            (Some(i), Some(value)) if node.attrs[i].value == value => return Ok(false),
            (Some(i), Some(value)) => self.edit(id)?.attrs[i].set(value),
            (Some(i), None) => {
                self.edit(id)?.attrs.remove(i);
            }
            (None, Some(value)) => {
                let node = self.edit(id)?;
                let mut attr = Attr::new(name, value);
                // Where each attribute has a line to itself, so does this.
                if let Some(last) = node.attrs.last().filter(|a| a.lead.contains('\n')) {
                    attr.lead = last.lead.clone();
                }
                node.attrs.push(attr);
            }
        }
        if name == "xmlns" || name.starts_with("xmlns:") {
            self.rekind(id);
        }
        if name == "d" {
            self.reanchor(id);
        }
        Ok(true)
    }

    /// Unlink `id` from its parent, its line going with it; its nodes
    /// stay in the document. Returns the parent it had.
    fn detach(&mut self, id: NodeId) -> Result<NodeId, DocError> {
        let Some(parent) = self.node(id)?.parent else {
            return invalid("the root <svg> can't be moved or deleted");
        };
        let p = self.edit(parent)?;
        let Some(idx) = p.children.iter().position(|c| *c == Child::Node(id)) else {
            return invalid(format!("{id} isn't where its parent {parent} says"));
        };
        p.children.remove(idx);
        // Alone on its line (nothing but a line break follows): the
        // indentation before it goes too.
        let alone = match p.children.get(idx) {
            None => true,
            Some(Child::Text(text)) => text.trim_start_matches([' ', '\t', '\r']).starts_with('\n'),
            Some(Child::Node(_)) => false,
        };
        if alone
            && idx > 0
            && let Child::Text(text) = &mut p.children[idx - 1]
            && let Some(indent) = indentation(text)
        {
            text.truncate(text.len() - indent.len());
        }
        merge_texts(&mut p.children);
        Ok(parent)
    }

    /// Take `id` out, with everything in it.
    pub(crate) fn remove(&mut self, id: NodeId) -> Result<(), DocError> {
        let gone = self.descendants(id);
        self.detach(id)?;
        for id in gone {
            self.nodes.remove(&id);
        }
        Ok(())
    }

    /// The parent a place is in, and the sibling it is beside (`None`
    /// when the parent has no child elements to be beside).
    fn resolve(&self, place: Place) -> Result<(NodeId, Option<(NodeId, bool)>), DocError> {
        match place {
            Place::FirstIn(parent) => Ok((parent, self.node(parent)?.elements().next().map(|first| (first, false)))),
            Place::LastIn(parent) => Ok((parent, self.node(parent)?.elements().next_back().map(|last| (last, true)))),
            Place::Before(sibling) | Place::After(sibling) => match self.node(sibling)?.parent {
                Some(parent) => Ok((parent, Some((sibling, matches!(place, Place::After(_)))))),
                None => invalid("nothing can go beside the root <svg>, only in it"),
            },
        }
    }

    /// Link `id` (in the document, in no parent's children) in at
    /// `place`.
    fn attach(&mut self, place: Place, id: NodeId) -> Result<(), DocError> {
        let (parent, beside) = self.resolve(place)?;
        if self.depth(parent) + self.height(id) > MAX_DEPTH {
            return Err(DocError::TooBig(format!("elements nested more than {MAX_DEPTH} deep")));
        }
        // The parent's own indentation: the root starts its line, and
        // anything else has what's written before it.
        let own = match self.node(parent)?.parent {
            None => Some("\n".to_owned()),
            Some(grand) => {
                let siblings = &self.node(grand)?.children;
                siblings.iter().position(|c| *c == Child::Node(parent)).and_then(|i| indent_before(siblings, i))
            }
        };
        let unit = self.unit(parent, own.as_deref().unwrap_or("").trim_start_matches(['\r', '\n']));
        let first = matches!(place, Place::FirstIn(_));
        let p = self.edit(parent)?;
        let position = |children: &[Child], sibling: NodeId| children.iter().position(|c| *c == Child::Node(sibling)).ok_or_else(|| DocError::Invalid(format!("{sibling} isn't where its parent says")));
        match beside {
            // After a sibling: on a line of its own, indented like it.
            Some((sibling, true)) => {
                let idx = position(&p.children, sibling)?;
                let indent = indent_before(&p.children, idx);
                p.children.insert(idx + 1, Child::Node(id));
                if let Some(indent) = indent {
                    p.children.insert(idx + 1, Child::Text(indent));
                }
            }
            // Before one: it takes the sibling's line, which starts anew.
            Some((sibling, false)) => {
                let idx = position(&p.children, sibling)?;
                if let Some(indent) = indent_before(&p.children, idx) {
                    p.children.insert(idx, Child::Text(indent));
                }
                p.children.insert(idx, Child::Node(id));
            }
            // The parent's first child element.
            None => {
                let text: String = p.children.iter().map(|c| if let Child::Text(t) = c { t.as_str() } else { "" }).collect();
                if !text.trim().is_empty() {
                    // Among real text (a <text>'s words): at its start
                    // or its end, with no line of its own.
                    p.children.insert(if first { 0 } else { p.children.len() }, Child::Node(id));
                } else if let Some(closing) = indentation(&text).map(str::to_owned) {
                    // Laid out over lines already: `<g>⏎</g>`.
                    let kept = &text[..text.len() - closing.len()];
                    p.children = vec![Child::Text(format!("{kept}{closing}{unit}")), Child::Node(id), Child::Text(closing)];
                } else if let Some(own) = own {
                    p.children = vec![Child::Text(format!("{own}{unit}")), Child::Node(id), Child::Text(own)];
                } else {
                    // All on one line, and it stays so.
                    p.children.push(Child::Node(id));
                }
            }
        }
        merge_texts(&mut p.children);
        self.edit(id)?.parent = Some(parent);
        Ok(())
    }

    /// Put `element` in at `place`, with new IDs. Returns its ID.
    pub(crate) fn insert(&mut self, place: Place, element: crate::node::Element) -> Result<NodeId, DocError> {
        let (parent, _) = self.resolve(place)?;
        let id = self.graft(element, Some(parent))?;
        if let Err(e) = self.attach(place, id) {
            for id in self.descendants(id) {
                self.nodes.remove(&id);
            }
            return Err(e);
        }
        Ok(id)
    }

    /// Whether `id` already is at `place`.
    fn is_at(&self, id: NodeId, place: Place) -> bool {
        let Some(parent) = self.get(id).and_then(|n| n.parent).and_then(|p| self.get(p)) else { return false };
        let siblings: Vec<NodeId> = parent.elements().collect();
        let Some(i) = siblings.iter().position(|&s| s == id) else { return false };
        match place {
            Place::FirstIn(p) => p == parent.id && i == 0,
            Place::LastIn(p) => p == parent.id && i + 1 == siblings.len(),
            Place::Before(s) => s == id || siblings.get(i + 1) == Some(&s),
            Place::After(s) => s == id || (i > 0 && siblings[i - 1] == s),
        }
    }

    /// Move `id`, with everything in it, to `place`. Whether it moved
    /// (it may be there already).
    pub(crate) fn relocate(&mut self, id: NodeId, place: Place) -> Result<bool, DocError> {
        self.node(id)?;
        let target = match place {
            Place::FirstIn(n) | Place::LastIn(n) | Place::Before(n) | Place::After(n) => n,
        };
        self.node(target)?;
        if self.is_at(id, place) {
            return Ok(false);
        }
        if self.is_within(target, id) {
            return invalid(format!("{id} can't go inside itself"));
        }
        self.resolve(place)?;
        let was = self.own_line(id);
        self.detach(id)?;
        self.attach(place, id)?;
        // What's inside it goes in or out as far as it did.
        if let (Some(was), Some(now)) = (was, self.own_line(id)) {
            self.shift(id, &was, &now)?;
        }
        // Its names may mean something else under other declarations.
        self.rekind(id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;
    use crate::node::Element;

    const N: fn(u64) -> NodeId = NodeId;

    fn doc(text: &str) -> Document {
        Document::parse(DocId(1), text).unwrap()
    }

    #[test]
    fn an_attribute_changes_and_nothing_else_moves() {
        let mut d = doc("<svg>\n  <rect  x='1'\n         y=\"2\" />\n</svg>\n");
        assert_eq!(d.set_attr(N(2), "x", Some("10")), Ok(true));
        assert_eq!(d.to_svg(), "<svg>\n  <rect  x='10'\n         y=\"2\" />\n</svg>\n");
        assert_eq!(d.set_attr(N(2), "x", Some("10")), Ok(false), "the same value changes nothing");
        // A new one goes last, on a line of its own where the others have one.
        d.set_attr(N(2), "fill", Some("a\"b")).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <rect  x='10'\n         y=\"2\"\n         fill=\"a&quot;b\" />\n</svg>\n");
        d.set_attr(N(1), "viewBox", Some("0 0 1 1")).unwrap();
        assert!(d.to_svg().starts_with("<svg viewBox=\"0 0 1 1\">"));
        assert_eq!(d.set_attr(N(2), "y", None), Ok(true));
        assert_eq!(d.set_attr(N(2), "y", None), Ok(false));
        assert!(d.to_svg().contains("<rect  x='10'\n         fill="));
        for bad in ["", "a b", "a=\"b", "1x", "x>"] {
            assert!(d.set_attr(N(2), bad, Some("v")).is_err(), "{bad:?}");
        }
        assert_eq!(d.set_attr(N(9), "x", Some("1")), Err(DocError::NoSuchNode(N(9))));
    }

    #[test]
    fn a_changed_node_has_a_new_revision_and_the_others_keep_theirs() {
        let mut d = doc("<svg><g/><rect/></svg>");
        let (g, rect) = (d.node(N(2)).unwrap().rev, d.node(N(3)).unwrap().rev);
        d.set_attr(N(3), "x", Some("1")).unwrap();
        assert_eq!(d.node(N(2)).unwrap().rev, g);
        assert!(d.node(N(3)).unwrap().rev > rect);
        let was = d.node(N(3)).unwrap().rev;
        d.set_attr(N(3), "x", Some("1")).unwrap();
        assert_eq!(d.node(N(3)).unwrap().rev, was, "nothing changed, nothing stamped");
    }

    #[test]
    fn a_node_put_in_has_a_line_of_its_own_like_its_siblings() {
        let mut d = doc("<svg>\n  <a/>\n  <b/>\n</svg>\n");
        let c = d.insert(Place::LastIn(N(1)), Element::new("c")).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <a/>\n  <b/>\n  <c/>\n</svg>\n");
        assert_eq!(d.node(c).unwrap().parent, Some(N(1)));
        d.insert(Place::FirstIn(N(1)), Element::new("z")).unwrap();
        d.insert(Place::After(N(2)), Element::new("m")).unwrap();
        d.insert(Place::Before(c), Element::new("y")).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <z/>\n  <a/>\n  <m/>\n  <b/>\n  <y/>\n  <c/>\n</svg>\n");
        // Tabs, and Windows line ends, are copied as they are.
        let mut t = doc("<svg>\r\n\t<a/>\r\n</svg>");
        t.insert(Place::LastIn(N(1)), Element::new("b")).unwrap();
        assert_eq!(t.to_svg(), "<svg>\r\n\t<a/>\r\n\t<b/>\r\n</svg>");
    }

    #[test]
    fn a_first_child_opens_its_parent_up() {
        // Ink's own empty drawing: laid out over lines already.
        let mut d = doc("<svg>\n</svg>\n");
        let g = d.insert(Place::LastIn(N(1)), Element::new("g")).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <g/>\n</svg>\n");
        // A childless group on its own line: its child goes one step in.
        d.insert(Place::FirstIn(g), Element::new("rect")).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <g>\n    <rect/>\n  </g>\n</svg>\n");
        // A file that indents by four keeps to four.
        let mut four = doc("<svg>\n    <g>\n        <g></g>\n    </g>\n</svg>");
        four.insert(Place::LastIn(N(3)), Element::new("a")).unwrap();
        assert_eq!(four.to_svg(), "<svg>\n    <g>\n        <g>\n            <a/>\n        </g>\n    </g>\n</svg>");
        // A file all on one line stays on one line; an empty root doesn't.
        let mut flat = doc("<svg><g/></svg>");
        flat.insert(Place::LastIn(N(2)), Element::new("a")).unwrap();
        flat.insert(Place::LastIn(N(1)), Element::new("b")).unwrap();
        assert_eq!(flat.to_svg(), "<svg><g><a/></g><b/></svg>");
        let mut bare = doc("<svg/>");
        bare.insert(Place::LastIn(N(1)), Element::new("a")).unwrap();
        assert_eq!(bare.to_svg(), "<svg>\n  <a/>\n</svg>");
        // Among a <text>'s words, no line of its own.
        let mut words = doc("<svg><text>Hello </text></svg>");
        words.insert(Place::LastIn(N(2)), Element::new("tspan")).unwrap();
        assert_eq!(words.to_svg(), "<svg><text>Hello <tspan/></text></svg>");
    }

    #[test]
    fn a_node_taken_out_takes_its_line_with_it() {
        let mut d = doc("<svg>\n  <a/>\n  <g>\n    <b/>\n  </g>\n  <!-- note -->\n  <c/>\n</svg>\n");
        d.remove(N(3)).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <a/>\n  <!-- note -->\n  <c/>\n</svg>\n");
        assert_eq!(d.len(), 3, "what was in it went too");
        assert!(d.get(N(4)).is_none());
        d.remove(N(5)).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <a/>\n  <!-- note -->\n</svg>\n", "a comment keeps its line");
        d.remove(N(2)).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <!-- note -->\n</svg>\n");
        assert!(d.remove(N(1)).is_err(), "the root stays");
        // One of two on a line leaves the other where it is.
        let mut pair = doc("<svg>\n  <a/><b/>\n</svg>");
        pair.remove(N(2)).unwrap();
        assert_eq!(pair.to_svg(), "<svg>\n  <b/>\n</svg>");
    }

    #[test]
    fn a_node_moves_with_everything_in_it() {
        let mut d = doc("<svg>\n  <g>\n    <a/>\n  </g>\n  <b/>\n  <c/>\n</svg>\n");
        assert_eq!(d.relocate(N(4), Place::LastIn(N(2))), Ok(true));
        assert_eq!(d.to_svg(), "<svg>\n  <g>\n    <a/>\n    <b/>\n  </g>\n  <c/>\n</svg>\n");
        assert_eq!(d.node(N(4)).unwrap().parent, Some(N(2)));
        assert_eq!(d.relocate(N(2), Place::After(N(5))), Ok(true));
        assert_eq!(d.to_svg(), "<svg>\n  <c/>\n  <g>\n    <a/>\n    <b/>\n  </g>\n</svg>\n");
        // Already there: nothing moves, nothing is stamped.
        let rev = d.node(N(1)).unwrap().rev;
        for place in [Place::LastIn(N(1)), Place::After(N(5)), Place::After(N(2)), Place::Before(N(2))] {
            assert_eq!(d.relocate(N(2), place), Ok(false), "{place:?}");
        }
        assert_eq!(d.node(N(1)).unwrap().rev, rev);
        assert!(d.relocate(N(2), Place::LastIn(N(3))).is_err(), "not into itself");
        assert!(d.relocate(N(1), Place::LastIn(N(2))).is_err(), "the root stays");
        assert!(d.relocate(N(5), Place::Before(N(1))).is_err(), "nothing goes beside the root");
        assert_eq!(d.relocate(N(5), Place::LastIn(N(9))), Err(DocError::NoSuchNode(N(9))));
    }

    #[test]
    fn nesting_stops_at_the_limit() {
        let mut d = doc(&format!("<svg>{}{}</svg>", "<g>".repeat(MAX_DEPTH - 1), "</g>".repeat(MAX_DEPTH - 1)));
        let deepest = NodeId(MAX_DEPTH as u64);
        let before = d.len();
        assert!(matches!(d.insert(Place::LastIn(deepest), Element::new("a")), Err(DocError::TooBig(_))));
        assert_eq!(d.len(), before, "what couldn't go in isn't left lying about");
        assert!(d.insert(Place::After(deepest), Element::new("a")).is_ok());
    }
}
