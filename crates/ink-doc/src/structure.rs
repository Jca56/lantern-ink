//! Copies and groups (ARCHITECTURE §3.4): a node copied beside itself,
//! nodes put into a group of their own, a group taken away from around
//! what's in it. None of them changes how the drawing looks, but for
//! where a copy lies over its original.

use std::collections::{HashMap, HashSet};

use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::length::unit;
use crate::node::{Attr, Child, Content, Element, prefix};
use crate::settle::{self, Edit};
use crate::style::{INHERITED, prop};
use crate::value::Precision;

/// What only a group can hold for what's in it, and how to say so.
const EFFECTS: [(&str, &str); 3] = [("filter", "a filter"), ("clip-path", "a clip path"), ("mask", "a mask")];

/// What taking a group away did.
pub(crate) struct Ungrouped {
    /// What was in it, in order.
    pub inside: Vec<NodeId>,
    /// Which of those were changed to look as they did.
    pub changed: Vec<NodeId>,
    /// What only the group could hold, gone with it as it was told.
    pub lost: Vec<String>,
}

/// `value` with every `url(#old)` in it naming `new` instead, for each
/// pair of `names`; `None` when it names none of them.
fn renamed(value: &str, names: &HashMap<String, String>) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let (mut rest, mut changed) = (value, false);
    while let Some(open) = rest.find("url(") {
        let Some(close) = rest[open..].find(')') else { break };
        let inside = &rest[open + 4..open + close];
        let name = inside.trim().trim_matches(['"', '\'']).trim();
        out.push_str(&rest[..open]);
        match name.strip_prefix('#').and_then(|id| names.get(id)) {
            Some(new) => {
                out.push_str(&format!("url(#{new})"));
                changed = true;
            }
            None => out.push_str(&rest[open..=open + close]),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

/// Give everything in `el` that has an `id` a new one (`taken` knows
/// the ones in use, and comes to know these), and point what `el` says
/// of the old ones at the new: a copy must not answer to its
/// original's name, and what's in it should go on using what's in it.
fn rename(el: &mut Element, taken: &mut HashSet<String>) {
    fn collect(el: &Element, taken: &mut HashSet<String>, names: &mut HashMap<String, String>) {
        if let Some(old) = el.attr("id") {
            // "sun" becomes "sun-2", and that "sun-3".
            let stem = old.rsplit_once('-').filter(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).map_or(old, |(stem, _)| stem);
            let new = (2..).map(|n| format!("{stem}-{n}")).find(|name| !taken.contains(name)).expect("there is always another number");
            taken.insert(new.clone());
            names.insert(old.to_owned(), new);
        }
        for child in &el.children {
            if let Content::Element(child) = child {
                collect(child, taken, names);
            }
        }
    }
    fn apply(el: &mut Element, names: &HashMap<String, String>) {
        for attr in &mut el.attrs {
            let new = match attr.name.as_str() {
                "id" => names.get(&attr.value).cloned(),
                name if name == "href" || name.ends_with(":href") => attr.value.trim().strip_prefix('#').and_then(|id| names.get(id)).map(|new| format!("#{new}")),
                _ => renamed(&attr.value, names),
            };
            if let Some(new) = new {
                attr.set(&new);
            }
        }
        for child in &mut el.children {
            if let Content::Element(child) = child {
                apply(child, names);
            }
        }
    }
    let mut names = HashMap::new();
    collect(el, taken, &mut names);
    if !names.is_empty() {
        apply(el, &names);
    }
}

impl Document {
    /// `id` and everything in it as an element on its own, written as
    /// the file writes it.
    pub(crate) fn element_of(&self, id: NodeId) -> Result<Element, DocError> {
        let node = self.node(id)?;
        let children = node
            .children
            .iter()
            .map(|child| match child {
                Child::Node(child) => self.element_of(*child).map(Content::Element),
                Child::Text(raw) => Ok(Content::Text(raw.clone())),
            })
            .collect::<Result<_, _>>()?;
        Ok(Element { name: node.name.clone(), attrs: node.attrs.clone(), children, written: node.written.clone() })
    }

    /// Make the changes `edits` ask for. The nodes any of them changed.
    pub(crate) fn make(&mut self, edits: &[Edit]) -> Result<Vec<NodeId>, DocError> {
        let mut changed = Vec::new();
        for edit in edits {
            let (node, did) = match edit {
                Edit::Attr { node, name, value } => (*node, self.set_attr(*node, name, value.as_deref())?),
                Edit::Prop { node, name, value } => (*node, self.set_prop(*node, name, value.as_deref())?),
                Edit::Ellipse { node, rx, ry } => (*node, self.make_ellipse(*node, rx, ry)?),
            };
            if did && !changed.contains(&node) {
                changed.push(node);
            }
        }
        Ok(changed)
    }

    /// Make the `<circle>` `id` an `<ellipse>` with the radii `rx` and
    /// `ry`, said where its `r` was and written as that was.
    fn make_ellipse(&mut self, id: NodeId, rx: &str, ry: &str) -> Result<bool, DocError> {
        let node = self.node(id)?;
        if node.kind != Kind::Circle {
            return invalid(format!("{id} is a <{}>: only a circle is made an ellipse", node.name));
        }
        let name = prefix(&node.name).map_or("ellipse".to_owned(), |p| format!("{p}:ellipse"));
        let node = self.edit(id)?;
        let at = node.attrs.iter().position(|a| a.name == "r");
        let mut radii = [Attr::new("rx", rx), Attr::new("ry", ry)];
        if let Some(was) = at.map(|i| &node.attrs[i]) {
            for radius in &mut radii {
                (radius.lead, radius.eq, radius.quote) = (was.lead.clone(), was.eq.clone(), was.quote);
            }
        }
        node.attrs.retain(|a| a.name != "r");
        let at = at.unwrap_or(node.attrs.len()).min(node.attrs.len());
        node.attrs.splice(at..at, radii);
        (node.name, node.kind) = (name, Kind::Ellipse);
        Ok(true)
    }

    /// Copy `id`, with everything in it, right on top of itself (just
    /// after it in the file). What in the copy has an `id` gets one of
    /// its own, and the copy goes by those. A lock stays with what was
    /// locked: nobody locked the copy, and one that couldn't be moved
    /// off its original would be no use. Returns the copy.
    pub(crate) fn duplicate(&mut self, id: NodeId) -> Result<NodeId, DocError> {
        if self.node(id)?.parent.is_none() {
            return invalid("the root <svg> can't be copied into itself");
        }
        let mut copy = self.element_of(id)?;
        let mut taken: HashSet<String> = self.nodes.values().filter_map(|n| n.attr("id").map(str::to_owned)).collect();
        rename(&mut copy, &mut taken);
        let made = self.insert(Place::After(id), copy)?;
        for part in self.descendants(made) {
            if self.is_locked(part) {
                self.set_locked(part, false)?;
            }
        }
        Ok(made)
    }

    /// Put `elements` into the drawing's `<defs>`, after what's there:
    /// the first `<defs>` directly in the root, or a new one made as the
    /// root's first child. Each must have an `id` (what it's used by)
    /// that nothing else has. Returns them.
    pub(crate) fn define(&mut self, elements: &[Element]) -> Result<Vec<NodeId>, DocError> {
        if elements.is_empty() {
            return invalid("there's nothing to define");
        }
        let mut taken: HashSet<String> = self.nodes.values().filter_map(|n| n.attr("id").map(str::to_owned)).collect();
        for element in elements {
            match element.attr("id").map(str::trim) {
                None | Some("") => return invalid(format!("a <{}> put in <defs> needs an id for others to use it by", element.name)),
                Some(id) if !taken.insert(id.to_owned()) => return invalid(format!("something is called \"{id}\" already: an id is one thing's name")),
                Some(_) => {}
            }
        }
        let root = self.root;
        let there = self.node(root)?.elements().find(|&c| self.get(c).is_some_and(|n| n.kind == Kind::Defs));
        let defs = match there {
            Some(defs) => defs,
            None => {
                let name = prefix(&self.node(root)?.name).map_or("defs".to_owned(), |p| format!("{p}:defs"));
                self.insert(Place::FirstIn(root), Element::new(name))?
            }
        };
        let mut made = Vec::with_capacity(elements.len());
        for element in elements {
            let id = self.insert(Place::LastIn(defs), element.clone())?;
            self.lay_out(id)?;
            made.push(id);
        }
        Ok(made)
    }

    /// Put `nodes` (which share a parent) into a new group, where the
    /// topmost of them was, in the order they were in. Returns the group.
    pub(crate) fn group(&mut self, nodes: &[NodeId]) -> Result<NodeId, DocError> {
        let Some(&first) = nodes.first() else { return invalid("there's nothing to group") };
        let Some(parent) = self.node(first)?.parent else { return invalid("the root <svg> can't go into a group: it's what everything is in") };
        for &id in nodes {
            match self.node(id)?.parent {
                Some(p) if p == parent => {}
                None => return invalid("the root <svg> can't go into a group: it's what everything is in"),
                Some(_) => return invalid(format!("{first} and {id} aren't in the same group: nodes to group must share a parent (move them together first)")),
            }
        }
        // In the file's order, whatever order they were named in.
        let inside: Vec<NodeId> = self.node(parent)?.elements().filter(|id| nodes.contains(id)).collect();
        // Named as its parent is, so it's an SVG group where the file
        // gives SVG a prefix.
        let name = prefix(&self.node(parent)?.name).map_or("g".to_owned(), |p| format!("{p}:g"));
        let group = self.insert(Place::After(*inside.last().expect("at least the first")), Element::new(name))?;
        for id in inside {
            self.relocate(id, Place::LastIn(group))?;
        }
        Ok(group)
    }

    /// Take the group `id` away from around what's in it: its children
    /// take its place, looking as they did. Its transform goes to each
    /// of them (into their numbers where it can), and so does what they
    /// had from it by inheritance. A filter, a clip path, a mask and an
    /// opacity over several children are the group's alone: with them
    /// it is refused, unless `drop` says to lose them.
    pub(crate) fn ungroup(&mut self, id: NodeId, drop: bool) -> Result<Ungrouped, DocError> {
        let group = self.node(id)?;
        if group.parent.is_none() {
            return invalid("the root <svg> isn't a group that can be taken away: it's what everything is in");
        }
        if group.kind != Kind::G {
            return invalid(format!("{id} is a <{}>, not a group (<g>)", group.name));
        }
        let inside: Vec<NodeId> = group.elements().collect();
        let drawn: Vec<NodeId> = inside.iter().copied().filter(|&c| self.get(c).is_some_and(|n| !n.kind.is_never_drawn() || n.kind == Kind::Other)).collect();
        let opacity = prop(group, "opacity").and_then(unit).filter(|o| *o < 1.0);
        let mut alone: Vec<&str> = EFFECTS.iter().filter(|(name, _)| prop(group, name).is_some_and(|v| v != "none")).map(|(_, what)| *what).collect();
        // An opacity over one thing is that thing's just as well.
        if opacity.is_some() && drawn.len() > 1 {
            alone.push("an opacity over all that's in it");
        }
        if !alone.is_empty() && !drop {
            return invalid(format!("{id} has {}, which only a group can hold for what's in it: ungrouping would lose {}. Take {} off first, or say to drop {}", alone.join(" and "), if alone.len() == 1 { "it" } else { "them" }, if alone.len() == 1 { "it" } else { "them" }, if alone.len() == 1 { "it" } else { "them" }));
        }
        let lost: Vec<String> = alone.iter().map(|what| format!("{id} had {what}")).collect();
        let handed: Vec<(&'static str, String)> = INHERITED.iter().chain(["display"].iter()).filter_map(|name| prop(group, name).map(|v| (*name, v.to_owned()))).filter(|(name, v)| *name != "display" || v == "none").collect();
        let fade = opacity.filter(|_| drawn.len() == 1).map(|o| (drawn[0], o));
        // Its transform, to each of them.
        let mut changed = self.make(&settle::dissolve(self, id))?;
        let decimals = Precision::of(self).decimals;
        for &child in &inside {
            let mut touched = false;
            for (name, value) in &handed {
                if self.node(child)?.kind != Kind::Other && !self.node(child)?.kind.is_never_drawn() && prop(self.node(child)?, name).is_none() {
                    touched |= self.set_attr(child, name, Some(value))?;
                }
            }
            if let Some((_, o)) = fade.filter(|(only, _)| *only == child) {
                let own = prop(self.node(child)?, "opacity").and_then(unit).unwrap_or(1.0);
                touched |= self.set_prop(child, "opacity", Some(&ink_geom::number::format(own * o, decimals)))?;
            }
            if touched && !changed.contains(&child) {
                changed.push(child);
            }
        }
        for &child in &inside {
            self.relocate(child, Place::Before(id))?;
        }
        self.remove(id)?;
        Ok(Ungrouped { inside, changed, lost })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    #[test]
    fn a_copy_goes_by_names_of_its_own() {
        let names = HashMap::from([("a".to_owned(), "a-2".to_owned()), ("b".to_owned(), "b-2".to_owned())]);
        assert_eq!(renamed("url(#a)", &names).as_deref(), Some("url(#a-2)"));
        assert_eq!(renamed("fill: url( \"#a\" ) red; stroke: url('#b'); clip-path: url(#c)", &names).as_deref(), Some("fill: url(#a-2) red; stroke: url(#b-2); clip-path: url(#c)"));
        assert_eq!(renamed("url(#c) #a", &names), None);
        let mut taken: HashSet<String> = ["sun", "sun-2", "ray"].map(str::to_owned).into();
        let mut el = crate::command::elements(r##"<g id="sun-2"><linearGradient id="ray"/><path fill="url(#ray)" stroke="url(#sky)"/><use href="#sun-2" xlink:href="#ray"/></g>"##).unwrap().remove(0);
        rename(&mut el, &mut taken);
        assert_eq!(el.to_markup(), r##"<g id="sun-3"><linearGradient id="ray-2"/><path fill="url(#ray-2)" stroke="url(#sky)"/><use href="#sun-3" xlink:href="#ray-2"/></g>"##);
        assert!(taken.contains("sun-3") && taken.contains("ray-2"));
    }

    #[test]
    fn a_node_is_read_back_out_as_it_is_written() {
        let text = "<svg>\n  <g  fill='red' >\n    <!-- a --><path d=\"M0 0\"/>\n  </g >\n</svg>";
        let d = Document::parse(DocId(1), text).unwrap();
        assert_eq!(d.element_of(NodeId(2)).unwrap().to_markup(), "<g  fill='red' >\n    <!-- a --><path d=\"M0 0\"/>\n  </g >");
        assert_eq!(d.element_of(d.root()).unwrap().to_markup(), text);
    }
}
