//! Tidying (ARCHITECTURE §3.4): `Command::Tidy` drops what nothing
//! uses. Nothing that shows ever changes.
//!
//! **Always:** definitions nothing refers to (a gradient, a clip path,
//! a filter, a mask, a pattern, a marker, a symbol, and whatever else is
//! kept in a `<defs>` for others to use), followed down their chains (a
//! gradient only an unused gradient built on goes too); groups and
//! `<defs>` with nothing in them; namespace declarations nothing under
//! them uses.
//!
//! **Only when asked** ([`Extra`]), since someone wrote these on
//! purpose: comments; ids nothing in the drawing refers to (something
//! outside it might); titles, descriptions and metadata.
//!
//! What a `<style>` sheet names counts as used, and Ink's own namespace
//! is never dropped: a drawing Ink made carries it.

use std::collections::HashSet;

use crate::document::Document;
use crate::id::NodeId;
use crate::kind::{INK_NS, INK_PREFIX, Kind};
use crate::node::{Child, Node, prefix};
use crate::refs;

/// What tidying drops only when asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    /// Every comment, in the drawing and around it.
    Comments,
    /// Each `id` nothing in the drawing refers to.
    Ids,
    /// Titles, descriptions and metadata.
    Words,
}

impl Extra {
    /// By the name a tool gives it.
    pub fn named(name: &str) -> Option<Extra> {
        match name {
            "comments" => Some(Extra::Comments),
            "ids" => Some(Extra::Ids),
            "words" => Some(Extra::Words),
            _ => None,
        }
    }
}

/// What tidying dropped (or, from [`plan`], would).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dropped {
    /// Definitions nothing referred to, each with what was in it.
    pub unused: Vec<NodeId>,
    /// Groups and `<defs>` with nothing in them.
    pub empty: Vec<NodeId>,
    /// Namespace declarations nothing used: the element each was on,
    /// and its name (`xmlns:xlink`).
    pub declarations: Vec<(NodeId, String)>,
    /// Titles, descriptions and metadata ([`Extra::Words`]).
    pub words: Vec<NodeId>,
    /// Ids nothing referred to: the element, and the id
    /// ([`Extra::Ids`]).
    pub ids: Vec<(NodeId, String)>,
    /// How many comments ([`Extra::Comments`]).
    pub comments: usize,
    /// Nodes still there that are written differently now.
    pub(crate) changed: Vec<NodeId>,
}

impl Dropped {
    /// There was nothing to tidy.
    pub fn is_nothing(&self) -> bool {
        self.unused.is_empty() && self.empty.is_empty() && self.declarations.is_empty() && self.words.is_empty() && self.ids.is_empty() && self.comments == 0
    }
}

/// Something others refer to, that shows nowhere itself.
fn is_definition(kind: Kind) -> bool {
    matches!(kind, Kind::LinearGradient | Kind::RadialGradient | Kind::ClipPath | Kind::Mask | Kind::Pattern | Kind::Marker | Kind::Filter | Kind::Symbol)
}

/// Something that would show if it weren't kept in a `<defs>`.
fn is_kept_for_use(kind: Kind) -> bool {
    kind.is_shape() || kind.is_group() || matches!(kind, Kind::Text | Kind::Image | Kind::Use)
}

/// Where white space is part of what's said: among these, a comment
/// goes and nothing else does.
fn is_words(kind: Kind) -> bool {
    matches!(kind, Kind::Text | Kind::TSpan | Kind::Style | Kind::Title | Kind::Desc | Kind::Metadata | Kind::Other)
}

/// `raw` (what stands between elements) without its comments, and how
/// many there were. With `lines`, a comment alone on its line takes
/// the line with it.
fn uncommented(raw: &str, lines: bool) -> (String, usize) {
    let (mut out, mut count) = (String::with_capacity(raw.len()), 0);
    let mut rest = raw;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let markup = &rest[lt..];
        let ends = |end: &str| markup.find(end).map_or(markup.len(), |at| at + end.len());
        if markup.starts_with("<!--") {
            rest = &markup[ends("-->")..];
            count += 1;
            // Alone on its line: nothing but space before it on the
            // line, and after it up to the line's end.
            let line = out.rfind('\n').map_or(0, |at| at + 1);
            let to_end = rest.find('\n').map_or(rest.len(), |at| at);
            let alone = lines && out[line..].trim().is_empty() && rest[..to_end].trim().is_empty() && rest.contains('\n') && line > 0;
            if alone {
                out.truncate(line - 1);
                rest = &rest[to_end..];
            }
        } else {
            // A CDATA section or a processing instruction is kept
            // whole, whatever it holds.
            let len = if markup.starts_with("<![CDATA[") { ends("]]>") } else { ends("?>") };
            out.push_str(&markup[..len]);
            rest = &markup[len..];
        }
    }
    out.push_str(rest);
    (out, count)
}

/// The names a `<style>` sheet's text may be pointing at: whatever
/// follows a `#` (a colour among them, which does no harm).
fn named_in_css(css: &str, into: &mut HashSet<String>) {
    for part in css.split('#').skip(1) {
        let end = part.find(|c: char| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))).unwrap_or(part.len());
        if end > 0 {
            into.insert(part[..end].trim_end_matches('.').to_owned());
        }
    }
}

impl Document {
    /// Every id something in the drawing refers to: in an attribute
    /// (`url(#…)`, an `href`), or in a `<style>` sheet.
    fn referred_to(&self) -> HashSet<String> {
        let mut used: HashSet<String> = refs::users(self).into_keys().collect();
        for style in self.descendants(self.root).into_iter().filter_map(|id| self.get(id)).filter(|n| n.kind == Kind::Style) {
            named_in_css(&style.text(), &mut used);
        }
        used
    }

    /// Whether `node`, or anything in it, is referred to.
    fn is_used(&self, node: &Node, used: &HashSet<String>) -> bool {
        self.descendants(node.id).into_iter().filter_map(|id| self.get(id)).any(|n| n.attr("id").is_some_and(|id| used.contains(id)))
    }

    /// Whether `node` holds nothing: no elements, and nothing written
    /// between its tags but space.
    fn is_hollow(node: &Node) -> bool {
        node.children.iter().all(|child| matches!(child, Child::Text(text) if text.trim().is_empty()))
    }

    /// Whether a name in `node` or under it has the prefix `p`.
    fn uses_prefix(&self, node: NodeId, p: &str) -> bool {
        self.descendants(node).into_iter().filter_map(|id| self.get(id)).any(|n| prefix(&n.name) == Some(p) || n.attrs.iter().any(|a| prefix(&a.name) == Some(p)))
    }

    /// Drop what nothing uses, and of `also` what's asked for. Returns
    /// what went.
    pub(crate) fn tidy(&mut self, also: &[Extra]) -> Dropped {
        let mut dropped = Dropped::default();
        let all = |doc: &Document| -> Vec<NodeId> { doc.descendants(doc.root).into_iter().skip(1).collect() };
        if also.contains(&Extra::Words) {
            for id in all(self) {
                if self.get(id).is_some_and(|n| matches!(n.kind, Kind::Title | Kind::Desc | Kind::Metadata)) && self.remove(id).is_ok() {
                    dropped.words.push(id);
                }
            }
        }
        if also.contains(&Extra::Comments) {
            for id in self.descendants(self.root) {
                let Some(node) = self.get(id) else { continue };
                let lines = !is_words(node.kind);
                let cleaned: Vec<(usize, String, usize)> = node.children.iter().enumerate().filter_map(|(i, child)| if let Child::Text(raw) = child { Some((i, uncommented(raw, lines))) } else { None }).filter(|(_, (_, count))| *count > 0).map(|(i, (text, count))| (i, text, count)).collect();
                if cleaned.is_empty() {
                    continue;
                }
                let Ok(node) = self.edit(id) else { continue };
                for (i, text, count) in cleaned {
                    node.children[i] = Child::Text(text);
                    dropped.comments += count;
                }
                dropped.changed.push(id);
            }
            // Around the drawing too: what's left there is its
            // declaration and its doctype.
            for around in [&mut self.before, &mut self.after] {
                let (text, count) = uncommented(around, true);
                if count > 0 {
                    *around = text;
                    dropped.comments += count;
                    dropped.changed.push(self.root);
                }
            }
        }
        // What goes can leave something else unused, or empty: round
        // again until nothing more does.
        loop {
            let before = dropped.unused.len() + dropped.empty.len();
            let used = self.referred_to();
            for id in all(self) {
                let Some(node) = self.get(id) else { continue };
                let in_defs = node.parent.and_then(|p| self.get(p)).is_some_and(|p| p.kind == Kind::Defs);
                if (is_definition(node.kind) || (in_defs && is_kept_for_use(node.kind))) && !self.is_used(node, &used) && self.remove(id).is_ok() {
                    dropped.unused.push(id);
                }
            }
            for id in all(self).into_iter().rev() {
                let Some(node) = self.get(id) else { continue };
                if matches!(node.kind, Kind::G | Kind::Defs) && Document::is_hollow(node) && !self.is_used(node, &used) && self.remove(id).is_ok() {
                    dropped.empty.push(id);
                }
            }
            if dropped.unused.len() + dropped.empty.len() == before {
                break;
            }
        }
        for id in self.descendants(self.root) {
            let Some(node) = self.get(id) else { continue };
            let idle: Vec<String> = node
                .attrs
                .iter()
                .filter_map(|a| Some((a, a.name.strip_prefix("xmlns:")?)))
                .filter(|(a, p)| !(self.uses_prefix(id, p) || *p == INK_PREFIX && a.value == INK_NS))
                .map(|(a, _)| a.name.clone())
                .collect();
            for name in idle {
                if self.set_attr(id, &name, None).is_ok() {
                    dropped.declarations.push((id, name));
                    dropped.changed.push(id);
                }
            }
        }
        if also.contains(&Extra::Ids) {
            let used = self.referred_to();
            for id in self.descendants(self.root) {
                let Some(name) = self.get(id).and_then(|n| n.attr("id")).filter(|name| !used.contains(*name)).map(str::to_owned) else { continue };
                if self.set_attr(id, "id", None).is_ok() {
                    dropped.ids.push((id, name));
                    dropped.changed.push(id);
                }
            }
        }
        dropped
    }
}

/// What tidying `doc` would drop, without touching it.
pub fn plan(doc: &Document, also: &[Extra]) -> Dropped {
    doc.clone().tidy(also)
}

/// A clean copy of a drawing, to ship ([`shipped`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shipped {
    /// The copy, as a file.
    pub svg: String,
    /// What tidying it dropped, its comments among it.
    pub dropped: Dropped,
    /// How many of Ink's own attributes and declarations came out.
    pub marks: usize,
}

/// A clean copy of `doc`, to ship: tidied, with its comments and Ink's
/// own marks (every `ink:` attribute, and the namespace's declaration)
/// out too. Written as the drawing is, where it's the same. The drawing
/// itself isn't touched.
pub fn shipped(doc: &Document) -> Shipped {
    let mut copy = doc.clone();
    let dropped = copy.tidy(&[Extra::Comments]);
    // What's Ink's by the namespace its prefix stands for where it is,
    // all found before any of the declarations that say so goes.
    let attrs = |pick: &dyn Fn(NodeId, &crate::node::Attr) -> bool| -> Vec<(NodeId, String)> {
        let nodes = copy.descendants(copy.root).into_iter().filter_map(|id| copy.get(id));
        nodes.flat_map(|node| node.attrs.iter().filter(|a| pick(node.id, a)).map(|a| (node.id, a.name.clone()))).collect()
    };
    let own = attrs(&|id, a| prefix(&a.name).is_some_and(|p| p != "xmlns" && copy.namespace(id, Some(p)) == Some(INK_NS)));
    let declared = attrs(&|_, a| a.name.starts_with("xmlns:") && a.value == INK_NS);
    let mut marks = 0;
    for (id, name) in own.iter().chain(&declared) {
        marks += usize::from(copy.set_attr(*id, name, None).is_ok());
    }
    Shipped { svg: copy.to_svg(), dropped, marks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;

    /// `svg` once tidied, and what went.
    fn tidied(svg: &str, also: &[Extra]) -> (String, Dropped) {
        let mut d = Document::parse(DocId(1), svg).unwrap();
        let would = plan(&d, also);
        let applied = d.apply(&Command::Tidy { also: also.to_vec() }).unwrap();
        assert_eq!(applied.is_nothing(), would.is_nothing(), "the plan says whether there's anything to do");
        assert_eq!(applied.removed.len(), would.unused.len() + would.empty.len() + would.words.len());
        (d.to_svg(), would)
    }

    #[test]
    fn what_nothing_uses_goes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
  <defs>
    <linearGradient id="base"><stop offset="0"/></linearGradient>
    <linearGradient id="used" xlink:href="#base"/>
    <linearGradient id="spare-base"/>
    <linearGradient id="spare" href="#spare-base"/>
    <clipPath id="cut"><rect width="4" height="4"/></clipPath>
    <filter><feGaussianBlur/></filter>
    <path id="stamp" d="M0 0h4"/>
    <g><circle id="dot" r="1"/></g>
    <rect id="unstamped" width="1" height="1"/>
    <style>.a { fill: url(#styled) } #stamp { stroke: red }</style>
    <radialGradient id="styled"/>
  </defs>
  <rect fill="url(#used)" width="4" height="4"/>
  <use href="#dot"/>
</svg>"##;
        let (out, dropped) = tidied(svg, &[]);
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
  <defs>
    <linearGradient id="base"><stop offset="0"/></linearGradient>
    <linearGradient id="used" xlink:href="#base"/>
    <path id="stamp" d="M0 0h4"/>
    <g><circle id="dot" r="1"/></g>
    <style>.a { fill: url(#styled) } #stamp { stroke: red }</style>
    <radialGradient id="styled"/>
  </defs>
  <rect fill="url(#used)" width="4" height="4"/>
  <use href="#dot"/>
</svg>"##,
            "what's used stays (by a chain, a use, or a sheet); what isn't goes, its line with it"
        );
        assert_eq!(dropped.unused.len(), 5, "a gradient only an unused gradient built on went too: {dropped:?}");
        assert!(dropped.empty.is_empty() && dropped.declarations.is_empty());
    }

    #[test]
    fn empty_groups_and_idle_declarations_go() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:ink="urn:lantern:ink">
  <defs>
    <linearGradient id="spare"/>
  </defs>
  <g fill="red"><g>
  </g></g>
  <g id="kept"><!-- a note --></g>
  <g id="named"/>
  <use href="#named"/>
  <g xmlns:odd="urn:odd"><path odd:mark="1"/></g>
</svg>"##;
        let (out, dropped) = tidied(svg, &[]);
        assert_eq!(
            out,
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:ink="urn:lantern:ink">
  <g id="kept"><!-- a note --></g>
  <g id="named"/>
  <use href="#named"/>
  <g xmlns:odd="urn:odd"><path odd:mark="1"/></g>
</svg>"##,
            "a group left empty by what went out of it goes; one with a note, or a name in use, stays; Ink's own namespace stays"
        );
        assert_eq!((dropped.unused.len(), dropped.empty.len(), dropped.declarations.clone()), (1, 3, vec![(NodeId(1), "xmlns:xlink".to_owned())]));
        // Tidy again: nothing to do, and no step to undo.
        let mut again = Document::parse(DocId(1), &out).unwrap();
        assert!(again.apply(&Command::Tidy { also: Vec::new() }).unwrap().is_nothing());
    }

    #[test]
    fn what_someone_wrote_on_purpose_goes_only_when_asked() {
        let svg = "<?xml version=\"1.0\"?>\n<!-- made by hand -->\n<svg xmlns=\"http://www.w3.org/2000/svg\">\n  <title>Lamp</title>\n  <!-- the glass -->\n  <g id=\"glass\"><!-- only a note --></g>\n  <rect id=\"frame\" width=\"4\" height=\"4\" fill=\"url(#glow)\"/> <!-- beside -->\n  <linearGradient id=\"glow\"/>\n  <text>a<!-- in words -->b<![CDATA[<!-- kept -->]]></text>\n</svg>\n<!-- end -->\n";
        let (as_it_was, nothing) = tidied(svg, &[]);
        assert!(as_it_was == svg && nothing.is_nothing(), "none of it is unused");
        let (out, dropped) = tidied(svg, &[Extra::Comments, Extra::Ids, Extra::Words]);
        assert_eq!(out, "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\">\n  <rect width=\"4\" height=\"4\" fill=\"url(#glow)\"/> \n  <linearGradient id=\"glow\"/>\n  <text>ab<![CDATA[<!-- kept -->]]></text>\n</svg>\n");
        assert_eq!((dropped.comments, dropped.words.len(), dropped.empty.len()), (6, 1, 1), "the group that held only a note is empty without it");
        assert_eq!(dropped.ids.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(), ["frame"], "the gradient's id is in use");
        assert_eq!((Extra::named("comments"), Extra::named("ids"), Extra::named("words"), Extra::named("all")), (Some(Extra::Comments), Some(Extra::Ids), Some(Extra::Words), None));
    }

    #[test]
    fn a_copy_to_ship_is_tidy_and_carries_nothing_of_inks() {
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" ink:decimals=\"2\" viewBox=\"0 0 8 8\">\n  <!-- the lamp -->\n  <defs>\n    <linearGradient id=\"spare\"/>\n  </defs>\n  <g id=\"lamp\" ink:label=\"Lamp\" xmlns:mine=\"urn:lantern:ink\"><rect mine:locked=\"true\" width=\"4\" height=\"4\"/></g>\n  <title>Lamp</title>\n</svg>\n";
        let doc = Document::parse(DocId(1), svg).unwrap();
        let clean = shipped(&doc);
        assert_eq!(clean.svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\">\n  <g id=\"lamp\"><rect width=\"4\" height=\"4\"/></g>\n  <title>Lamp</title>\n</svg>\n", "ids and titles are the drawing's, and stay");
        assert_eq!((clean.marks, clean.dropped.comments, clean.dropped.unused.len(), clean.dropped.empty.len()), (5, 1, 1, 1));
        assert_eq!(doc.to_svg(), svg, "the drawing itself is as it was");
        // A drawing with nothing to drop ships as it's written.
        let plain = "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0h4\"/></svg>";
        let same = shipped(&Document::parse(DocId(1), plain).unwrap());
        assert!(same.svg == plain && same.marks == 0 && same.dropped.is_nothing());
    }

    #[test]
    fn comments_take_their_own_lines_with_them() {
        assert_eq!(uncommented("\n  <!-- a -->\n  ", true), ("\n  ".to_owned(), 1));
        assert_eq!(uncommented("a <!-- b --> c", true), ("a  c".to_owned(), 1));
        assert_eq!(uncommented("<!-- first -->\n  x", true), ("\n  x".to_owned(), 1));
        assert_eq!(uncommented("\n  <!-- a -->\n  <!-- b -->\n", true), ("\n".to_owned(), 2));
        assert_eq!(uncommented("<![CDATA[<!-- no -->]]><?pi <!-- no --> ?>", true), ("<![CDATA[<!-- no -->]]><?pi <!-- no --> ?>".to_owned(), 0));
        assert_eq!(uncommented("x<!-- never closed", true), ("x".to_owned(), 1));
        // Among words the space around a comment may be part of what's
        // said: only the comment goes.
        assert_eq!(uncommented("\n  <!-- a -->\n  b", false), ("\n  \n  b".to_owned(), 1));
        let mut kept = Document::parse(DocId(1), "<svg><text xml:space=\"preserve\">a\n <!-- note -->\n b</text></svg>").unwrap();
        kept.apply(&Command::Tidy { also: vec![Extra::Comments] }).unwrap();
        assert_eq!(kept.to_svg(), "<svg><text xml:space=\"preserve\">a\n \n b</text></svg>");
    }
}
