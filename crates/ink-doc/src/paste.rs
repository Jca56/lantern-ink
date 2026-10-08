//! The clipboard, as SVG (ARCHITECTURE §3.4): what's copied is a small
//! drawing of its own, text that any SVG app reads; what's pasted is
//! such a drawing put into this one.
//!
//! - [`Document::clipping`] is the copy: the nodes named, each brought
//!   out to the top level showing where it did, with the definitions
//!   they're drawn with and nothing else, under the drawing's own
//!   `<svg>` (its `viewBox`, so it sits where it sat).
//! - [`crate::Command::Paste`] puts a drawing's content in: its
//!   definitions into `<defs>`, what it draws at a place in the tree.
//!   A definition the drawing has already, said the same way, is used
//!   as it is (pasting back where it came from adds no second copy of
//!   a gradient); anything else whose `id` is taken gets another, and
//!   what's pasted goes by the new name.

use std::collections::{HashMap, HashSet};

use crate::command::Command;
use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::{DocId, NodeId};
use crate::kind::Kind;
use crate::node::{Content, Element};
use crate::structure::rename_taken;

/// Whether two elements say the same thing, however each is written
/// out: the same names, attributes and values, and the same elements
/// and words inside, in order.
fn same(a: &Element, b: &Element) -> bool {
    fn inside(el: &Element) -> Vec<&Content> {
        el.children.iter().filter(|c| !matches!(c, Content::Text(t) if t.trim().is_empty())).collect()
    }
    let attrs = |el: &Element| el.attrs.iter().map(|a| (a.name.clone(), a.value.clone())).collect::<Vec<_>>();
    a.name == b.name
        && attrs(a) == attrs(b)
        && inside(a).len() == inside(b).len()
        && inside(a).into_iter().zip(inside(b)).all(|pair| match pair {
            (Content::Element(x), Content::Element(y)) => same(x, y),
            (Content::Text(x), Content::Text(y)) => x.trim() == y.trim(),
            _ => false,
        })
}

impl Document {
    /// `nodes` as a drawing of their own, for the clipboard: each out
    /// at the top level, back to front as they stand, showing where it
    /// showed (what it was under is made up for; what it inherited
    /// from there isn't said on it). The definitions they use come
    /// along, and nothing else. Nothing on the clipboard is locked.
    pub fn clipping(&self, nodes: &[NodeId]) -> Result<String, DocError> {
        let root = self.root();
        let tops: Vec<NodeId> = self.descendants(root).into_iter().filter(|&id| id != root && nodes.contains(&id) && !nodes.iter().any(|&other| other != id && self.is_within(id, other))).collect();
        if tops.is_empty() {
            return invalid("there's nothing to copy");
        }
        let mut cut = self.clone();
        let locked: Vec<NodeId> = cut.descendants(root).into_iter().filter(|&id| cut.is_locked(id)).collect();
        if !locked.is_empty() {
            cut.apply(&Command::SetLocked { nodes: locked, locked: false })?;
        }
        cut.apply(&Command::Move { nodes: tops.clone(), place: Place::LastIn(root) })?;
        // Everything else that's drawn, and the words about the
        // picture, stay behind. Definitions go if nothing copied uses
        // them.
        let words = |kind: Kind| matches!(kind, Kind::Title | Kind::Desc | Kind::Metadata | Kind::Other);
        let rest: Vec<NodeId> = cut.node(root)?.elements().filter(|id| !tops.contains(id) && cut.get(*id).is_some_and(|n| !n.kind.is_never_drawn() || words(n.kind))).collect();
        if !rest.is_empty() {
            cut.apply(&Command::Delete { nodes: rest })?;
        }
        cut.apply(&Command::Tidy { also: Vec::new() })?;
        Ok(cut.to_svg())
    }

    /// Put what the drawing `svg` holds into this one: what it draws
    /// at `place`, in order, and its definitions into `<defs>`. The
    /// nodes made, definitions first.
    pub(crate) fn paste(&mut self, svg: &str, place: Place) -> Result<Vec<NodeId>, DocError> {
        let mut from = match Document::parse(DocId(0), svg) {
            Ok(from) => from,
            Err(_) => return invalid("what's on the clipboard isn't a drawing: copy something in Ink, or SVG from another app"),
        };
        from.adopt();
        let (mut defs, mut drawn) = (Vec::new(), Vec::new());
        for id in from.node(from.root())?.elements() {
            let Some(node) = from.get(id) else { continue };
            match node.kind {
                Kind::Defs => {
                    for inner in node.elements() {
                        defs.push(from.element_of(inner)?);
                    }
                }
                // Rules for a whole drawing would restyle this one, and
                // words about that picture aren't about this one.
                Kind::Style | Kind::Title | Kind::Desc | Kind::Metadata => {}
                kind if kind.is_never_drawn() && kind != Kind::Other => defs.push(from.element_of(id)?),
                _ => drawn.push(from.element_of(id)?),
            }
        }
        if drawn.is_empty() {
            return invalid("the drawing on the clipboard has nothing in it to paste");
        }
        // A definition this drawing has already, said the same way, is
        // used as it is. One nothing could use (it has no name) is left.
        let there: HashMap<String, NodeId> = self.nodes.values().filter_map(|n| Some((n.attr("id")?.to_owned(), n.id))).collect();
        let mut kept = Vec::new();
        for def in defs {
            let Some(id) = def.attr("id").filter(|id| !id.trim().is_empty()) else { continue };
            let have = match there.get(id) {
                Some(&node) => same(&self.element_of(node)?, &def),
                None => false,
            };
            if !have {
                kept.push(def);
            }
        }
        // Whatever else answers to a name in use gets another, and
        // what's pasted goes by that.
        let definitions = kept.len();
        let mut all = Element::new("g");
        all.children = kept.into_iter().chain(drawn).map(Content::Element).collect();
        let mut taken: HashSet<String> = there.into_keys().collect();
        rename_taken(&mut all, &mut taken);
        let mut elements: Vec<Element> = all.children.into_iter().filter_map(|c| if let Content::Element(el) = c { Some(el) } else { None }).collect();
        let drawn = elements.split_off(definitions);
        let mut made = if elements.is_empty() { Vec::new() } else { self.define(&elements)? };
        let mut at = place;
        for element in drawn {
            let id = self.insert(at, element)?;
            self.lay_out(id)?;
            made.push(id);
            at = Place::After(id);
        }
        Ok(made)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: fn(u64) -> NodeId = NodeId;

    /// N2 defs (N3 the gradient "sky", N4 its stop, N5 the gradient
    /// "unused"), N6 the rect "a" painted with sky, N7 a group moved
    /// and locked holding N8 the circle "b", N9 a title.
    const LAMP: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 24 24\">\n  <defs>\n    <linearGradient id=\"sky\">\n      <stop stop-color=\"#08f\"/>\n    </linearGradient>\n    <linearGradient id=\"unused\"/>\n  </defs>\n  <rect id=\"a\" width=\"4\" height=\"4\" fill=\"url(#sky)\"/>\n  <g transform=\"translate(10 5)\" ink:locked=\"true\">\n    <circle id=\"b\" cx=\"2\" cy=\"2\" r=\"2\"/>\n  </g>\n  <title>Lamp</title>\n</svg>\n";

    fn doc(svg: &str) -> Document {
        Document::parse(DocId(1), svg).unwrap()
    }

    #[test]
    fn a_copy_is_a_drawing_of_what_was_picked_and_what_it_is_drawn_with() {
        let d = doc(LAMP);
        // The rect: with its gradient, and without the other one, the
        // group, or the title.
        assert_eq!(d.clipping(&[N(6)]).unwrap(), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 24 24\">\n  <defs>\n    <linearGradient id=\"sky\">\n      <stop stop-color=\"#08f\"/>\n    </linearGradient>\n  </defs>\n  <rect id=\"a\" width=\"4\" height=\"4\" fill=\"url(#sky)\"/>\n</svg>\n");
        // The circle in the moved group: out at the top level, where it
        // showed. (Its group's lock isn't its own.)
        let circle = d.clipping(&[N(8)]).unwrap();
        assert!(circle.contains("<circle id=\"b\" cx=\"12\" cy=\"7\" r=\"2\"/>") && !circle.contains("<g") && !circle.contains("sky"), "{circle}");
        // Both, and the group named along with what's in it: the group
        // stands for it. Nothing copied is locked.
        let both = d.clipping(&[N(8), N(7), N(6)]).unwrap();
        assert!(both.contains("<rect id=\"a\"") && both.contains("<g transform=\"translate(10 5)\">") && both.find("<rect").unwrap() < both.find("<g ").unwrap(), "{both}");
        // The drawing itself is as it was.
        assert_eq!(d.to_svg(), LAMP);
        assert!(d.clipping(&[]).is_err() && d.clipping(&[d.root()]).is_err());
    }

    #[test]
    fn a_paste_puts_a_drawing_in_and_never_takes_a_name_in_use() {
        let mut d = doc(LAMP);
        let copied = d.clipping(&[N(6)]).unwrap();
        // Back where it came from: on top, called something else, and
        // painted with the gradient that's there (no second "sky").
        let made = d.apply(&Command::Paste { svg: copied.clone(), place: Place::LastIn(d.root()) }).unwrap().created;
        assert_eq!(made.len(), 1);
        let pasted = d.node(made[0]).unwrap();
        assert_eq!((pasted.attr("id"), pasted.attr("fill"), pasted.parent), (Some("a-2"), Some("url(#sky)"), Some(d.root())));
        assert_eq!(d.to_svg().matches("id=\"sky\"").count(), 1);
        // Into a drawing whose "sky" is another gradient: the pasted
        // one comes too, under a name of its own, and the rect uses it.
        let mut other = doc("<svg xmlns=\"http://www.w3.org/2000/svg\"><defs><radialGradient id=\"sky\"/></defs><g id=\"a\"/></svg>");
        let made = other.apply(&Command::Paste { svg: copied.clone(), place: Place::LastIn(N(4)) }).unwrap().created;
        assert_eq!(made.len(), 2);
        let (gradient, rect) = (other.node(made[0]).unwrap(), other.node(made[1]).unwrap());
        assert_eq!((gradient.attr("id"), rect.attr("id"), rect.attr("fill"), rect.parent), (Some("sky-2"), Some("a-2"), Some("url(#sky-2)"), Some(N(4))));
        // Into one with no <defs>: it gets them.
        let mut bare = doc("<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
        bare.apply(&Command::Paste { svg: copied, place: Place::LastIn(bare.root()) }).unwrap();
        assert!(bare.to_svg().contains("<defs>") && bare.to_svg().contains("fill=\"url(#sky)\""));
        // What isn't a drawing, or has nothing to draw, says so.
        let not = |svg: &str| doc(LAMP).apply(&Command::Paste { svg: svg.into(), place: Place::LastIn(N(1)) }).unwrap_err().to_string();
        assert!(not("hello").contains("isn't a drawing") && not("<svg xmlns=\"http://www.w3.org/2000/svg\"><title>t</title></svg>").contains("nothing in it to paste"));
        // Nothing goes into what's locked.
        assert!(doc(LAMP).apply(&Command::Paste { svg: "<svg><rect/></svg>".into(), place: Place::LastIn(N(7)) }).unwrap_err().to_string().contains("locked"));
    }
}
