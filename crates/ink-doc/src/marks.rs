//! Ink's own marks on a node (ARCHITECTURE §3.3, D19): a label and a
//! lock, kept in the file as attributes in Ink's namespace, which other
//! programs ignore.
//!
//! - **`ink:label`** is a name for a person to know a node by: a layers
//!   panel shows it, and `doc_info`. It means nothing to how the
//!   drawing draws.
//! - **`ink:locked="true"`** makes a node, and everything in it, refuse
//!   every edit until it's unlocked: it can't be changed, moved,
//!   deleted, or have anything in it changed. Locking and unlocking
//!   are the only Commands that go through. Things can still be put
//!   beside it, and a group it's in can be painted differently; but
//!   what would move it, rewrite it or take it out (a transform, an
//!   ungroup, a delete of that group) is refused, saying which node
//!   holds the lock. So is changing what it's drawn with: the gradient,
//!   clip path or filter it uses by name (and what those use) is held
//!   by its lock too, or a locked node could be recoloured, or blanked,
//!   from the side. A copy of it is a new node, and isn't locked.

use crate::command::Command;
use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::{INK_NS, INK_PREFIX, Kind};
use crate::node::{local, prefix};
use crate::refs::{Ids, named};

/// The names of Ink's marks, after its prefix.
pub const LABEL: &str = "label";
pub const LOCKED: &str = "locked";

/// How a Command reaches a node.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// It changes the node itself.
    Own,
    /// It moves, rewrites or removes what's in the node too.
    Deep,
    /// It only locks or unlocks it.
    Lock,
}

impl Document {
    /// What Ink's mark `name` says on `id`, under whichever prefix
    /// stands for Ink's namespace there.
    pub fn mark(&self, id: NodeId, name: &str) -> Option<&str> {
        let node = self.get(id)?;
        node.attrs.iter().find(|a| local(&a.name) == name && prefix(&a.name).is_some_and(|p| p != "xmlns" && self.namespace(id, Some(p)) == Some(INK_NS))).map(|a| a.value.as_str())
    }

    /// The name `id` was given, if it has one.
    pub fn label(&self, id: NodeId) -> Option<&str> {
        self.mark(id, LABEL).filter(|label| !label.is_empty())
    }

    /// Whether `id` itself is locked.
    pub fn is_locked(&self, id: NodeId) -> bool {
        self.mark(id, LOCKED).is_some_and(|said| said.trim() == "true")
    }

    /// The locked node `id` is, or is in: the nearest.
    pub fn lock_over(&self, id: NodeId) -> Option<NodeId> {
        std::iter::once(id).chain(self.ancestors(id).map(|n| n.id)).find(|&n| self.is_locked(n))
    }

    /// A locked node inside `id`.
    pub fn lock_inside(&self, id: NodeId) -> Option<NodeId> {
        self.descendants(id).into_iter().skip(1).find(|&n| self.is_locked(n))
    }

    /// The attribute Ink's mark `name` is written as on `id`, with the
    /// declaration the drawing needs first (on its root) when nothing
    /// there says what `ink:` stands for.
    fn mark_name(&mut self, id: NodeId, name: &str) -> Result<String, DocError> {
        // Under the prefix it has already, whatever that is.
        if let Some(had) = self.node(id)?.attrs.iter().find(|a| local(&a.name) == name && prefix(&a.name).is_some_and(|p| self.namespace(id, Some(p)) == Some(INK_NS))) {
            return Ok(had.name.clone());
        }
        match self.namespace(id, Some(INK_PREFIX)) {
            Some(INK_NS) => {}
            None => {
                self.set_attr(self.root, &format!("xmlns:{INK_PREFIX}"), Some(INK_NS))?;
            }
            Some(other) => return invalid(format!("this drawing uses the prefix \"{INK_PREFIX}:\" for something else ({other}), so Ink's own marks have nowhere to go")),
        }
        Ok(format!("{INK_PREFIX}:{name}"))
    }

    /// Give `id` the name `label`, or with `None` (or nothing) take its
    /// name off. Whether anything changed.
    pub(crate) fn set_label(&mut self, id: NodeId, label: Option<&str>) -> Result<bool, DocError> {
        let label = label.map(str::trim).filter(|label| !label.is_empty());
        if let Some(c) = label.and_then(|label| label.chars().find(|c| c.is_control())) {
            return invalid(format!("a label is one line of words: it can't hold the control character U+{:04X}", c as u32));
        }
        if label.is_none() && self.mark(id, LABEL).is_none() {
            return Ok(false);
        }
        let name = self.mark_name(id, LABEL)?;
        self.set_attr(id, &name, label)
    }

    /// Lock `id`, or unlock it. Whether anything changed.
    pub(crate) fn set_locked(&mut self, id: NodeId, locked: bool) -> Result<bool, DocError> {
        if self.node(id)?.parent.is_none() {
            return invalid("the root <svg> can't be locked: lock what's in it");
        }
        if locked == self.is_locked(id) {
            return Ok(false);
        }
        let name = self.mark_name(id, LOCKED)?;
        self.set_attr(id, &name, locked.then_some("true"))
    }

    /// What something locked is drawn with: each gradient, clip path,
    /// filter or whatever else it uses by name (in an attribute, or by
    /// a `<style>` rule), and what those use in turn; each with the
    /// locked node it's used under. A lock holds these too.
    fn held(&self) -> Vec<(NodeId, NodeId)> {
        let mut look: Vec<(NodeId, NodeId)> = Vec::new();
        for lock in self.descendants(self.root).into_iter().filter(|&id| self.is_locked(id)) {
            look.extend(self.descendants(lock).into_iter().map(|id| (id, lock)));
        }
        // Nothing locked, nothing held: the usual case is one walk.
        if look.is_empty() {
            return Vec::new();
        }
        let ids = Ids::of(self);
        let mut held: Vec<(NodeId, NodeId)> = Vec::new();
        while let Some((user, lock)) = look.pop() {
            let Some(node) = self.get(user) else { continue };
            let said = node.attrs.iter().map(|a| (a.name.as_str(), a.value.as_str()));
            let ruled = node.ruled.iter().flat_map(|rules| rules.iter()).map(|said| (&*said.name, &*said.value));
            for (name, value) in said.chain(ruled) {
                for used in named(name, value).into_iter().filter_map(|name| ids.get(name)) {
                    // One under a lock of its own is held by that one.
                    if self.lock_over(used).is_none() && !held.iter().any(|(def, _)| *def == used) {
                        held.push((used, lock));
                        look.extend(self.descendants(used).into_iter().map(|id| (id, lock)));
                    }
                }
            }
        }
        held
    }

    /// Refuse reaching `id` as `reach` says, if a lock is in the way:
    /// its own, one over it, one in it, or one that holds it (`held`)
    /// as what a locked node is drawn with.
    fn reach(&self, id: NodeId, reach: Reach, held: &[(NodeId, NodeId)]) -> Result<(), DocError> {
        // A node that isn't there is the Command's own to refuse.
        if self.get(id).is_none() {
            return Ok(());
        }
        // Its own lock is what locking and unlocking are for; a lock
        // further up holds it either way.
        let over = match reach {
            Reach::Lock => self.ancestors(id).map(|n| n.id).find(|&n| self.is_locked(n)),
            Reach::Own | Reach::Deep => self.lock_over(id),
        };
        if let Some(lock) = over {
            return invalid(if lock == id { format!("{id} is locked: nothing about it changes until it's unlocked") } else { format!("{id} is in {lock}, which is locked: nothing in it changes until {lock} is unlocked") });
        }
        if reach == Reach::Deep
            && let Some(lock) = self.lock_inside(id)
        {
            return invalid(format!("{id} holds {lock}, which is locked: that would move or change it. Unlock {lock} first, or leave {id} as it is"));
        }
        if reach == Reach::Lock || held.is_empty() {
            return Ok(());
        }
        let holds = |id: NodeId| held.iter().find(|(def, _)| *def == id).copied();
        if let Some((def, lock)) = std::iter::once(id).chain(self.ancestors(id).map(|n| n.id)).find_map(holds) {
            let what = if def == id { format!("{id} is") } else { format!("{id} is in {def}, which is") };
            return invalid(format!("{what} what {lock} is drawn with, and {lock} is locked: nothing it's drawn with changes until {lock} is unlocked"));
        }
        if reach == Reach::Deep
            && let Some((def, lock)) = self.descendants(id).into_iter().skip(1).find_map(holds)
        {
            return invalid(format!("{id} holds {def}, which {lock} is drawn with, and {lock} is locked: that would move or change it. Unlock {lock} first, or leave {id} as it is"));
        }
        Ok(())
    }

    /// The node a new one put at `place` goes into.
    fn parent_at(&self, place: Place) -> Option<NodeId> {
        match place {
            Place::FirstIn(parent) | Place::LastIn(parent) => Some(parent),
            Place::Before(sibling) | Place::After(sibling) => self.get(sibling).and_then(|n| n.parent),
        }
    }

    /// Refuse `command` (one that isn't a batch) if it would change
    /// something locked.
    pub(crate) fn guard(&self, command: &Command) -> Result<(), DocError> {
        // Tidying steps round what's locked; a batch's Commands are
        // each guarded as their turn comes.
        if matches!(command, Command::Tidy { .. } | Command::Batch(_)) {
            return Ok(());
        }
        let held = self.held();
        let reach = |id: NodeId, reach: Reach| self.reach(id, reach, &held);
        let each = |nodes: &[NodeId], how: Reach| nodes.iter().try_for_each(|&id| reach(id, how));
        // Something new beside a node goes into that node's parent.
        let into = |place: Place| self.parent_at(place).map_or(Ok(()), |parent| reach(parent, Reach::Own));
        let beside = |nodes: &[NodeId]| nodes.iter().filter_map(|id| self.get(*id).and_then(|n| n.parent)).try_for_each(|parent| reach(parent, Reach::Own));
        match command {
            Command::SetAttr { node, name, .. } => {
                // The lock itself, said as an attribute, is locking.
                let is_lock = local(name) == LOCKED && prefix(name).is_some_and(|p| self.namespace(*node, Some(p)) == Some(INK_NS));
                reach(*node, if is_lock { Reach::Lock } else { Reach::Own })
            }
            Command::Insert { place, .. } | Command::Paste { place, .. } => into(*place),
            Command::Delete { nodes } | Command::Ungroup { nodes, .. } | Command::Transform { nodes, .. } | Command::Resize { nodes, .. } | Command::Boolean { nodes, .. } => each(nodes, Reach::Deep),
            Command::Move { nodes, place } => each(nodes, Reach::Deep).and_then(|()| into(*place)),
            Command::Group { nodes } => each(nodes, Reach::Deep).and_then(|()| beside(nodes)),
            // A copy changes nothing of what it copies: only where it's
            // put matters.
            Command::Duplicate { nodes } => beside(nodes),
            Command::Define { .. } => {
                let defs = self.get(self.root).and_then(|root| root.elements().find(|&id| self.get(id).is_some_and(|n| n.kind == Kind::Defs)));
                reach(defs.unwrap_or(self.root), Reach::Own)
            }
            Command::SetClip { nodes, by, .. } => each(nodes, Reach::Own).and_then(|()| each(by, Reach::Deep)),
            Command::SetStyle { nodes, .. } | Command::ToPath { nodes } | Command::Simplify { nodes, .. } | Command::TextToPath { nodes, .. } => each(nodes, Reach::Own),
            // The outline may be a new path beside its shape.
            Command::OutlineStroke { nodes, .. } => each(nodes, Reach::Own).and_then(|()| beside(nodes)),
            Command::EditPath { node, .. } | Command::SetPath { node, .. } | Command::SetText { node, .. } | Command::SetLabel { node, .. } => reach(*node, Reach::Own),
            Command::SetLocked { nodes, .. } => each(nodes, Reach::Lock),
            Command::Tidy { .. } | Command::Batch(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;
    use crate::tidy::Extra;
    use ink_geom::Affine;

    const INK: &str = r#"xmlns:ink="urn:lantern:ink""#;

    fn doc(inner: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg {INK}>{inner}</svg>")).unwrap()
    }

    fn n(id: u64) -> NodeId {
        NodeId(id)
    }

    #[test]
    fn marks_are_read_under_whatever_prefix_is_inks() {
        let d = Document::parse(DocId(1), r#"<svg xmlns:mine="urn:lantern:ink" xmlns:other="urn:other"><g mine:label=" Lamp " mine:locked="true"><path other:locked="true" locked="true"/></g><g mine:locked="false" mine:label=""/></svg>"#).unwrap();
        assert_eq!((d.label(n(2)), d.is_locked(n(2)), d.lock_over(n(3)), d.lock_inside(n(1))), (Some(" Lamp "), true, Some(n(2)), Some(n(2))));
        assert!(!d.is_locked(n(3)), "another namespace's, or no namespace's, isn't Ink's");
        assert_eq!((d.label(n(4)), d.is_locked(n(4)), d.lock_over(n(4)), d.lock_inside(n(2))), (None, false, None, None));
    }

    #[test]
    fn a_label_and_a_lock_are_written_as_inks_own() {
        let mut d = Document::parse(DocId(1), "<svg><g><path/></g></svg>").unwrap();
        d.apply(&Command::SetLabel { node: n(2), label: Some(" Lamp glass ".into()) }).unwrap();
        d.apply(&Command::SetLocked { nodes: vec![n(2), n(3)], locked: true }).unwrap();
        assert_eq!(d.to_svg(), r#"<svg xmlns:ink="urn:lantern:ink"><g ink:label="Lamp glass" ink:locked="true"><path ink:locked="true"/></g></svg>"#, "the namespace is declared the first time it's needed");
        assert!(d.apply(&Command::SetLocked { nodes: vec![n(2)], locked: true }).unwrap().is_nothing(), "locked already");
        // Unlocking is the one thing a locked node lets through; a lock
        // further up still holds what's in it.
        assert_eq!(d.apply(&Command::SetLocked { nodes: vec![n(3)], locked: false }).unwrap_err().to_string(), "N3 is in N2, which is locked: nothing in it changes until N2 is unlocked");
        d.apply(&Command::SetLocked { nodes: vec![n(2)], locked: false }).unwrap();
        d.apply(&Command::SetLabel { node: n(2), label: None }).unwrap();
        assert_eq!(d.to_svg(), r#"<svg xmlns:ink="urn:lantern:ink"><g><path ink:locked="true"/></g></svg>"#);
        assert!(d.apply(&Command::SetLabel { node: n(2), label: Some("  ".into()) }).unwrap().is_nothing(), "no name is no name");
        let refused = |d: &mut Document, c: Command| d.apply(&c).unwrap_err().to_string();
        assert_eq!(refused(&mut d, Command::SetLocked { nodes: vec![n(1)], locked: true }), "the root <svg> can't be locked: lock what's in it");
        assert!(refused(&mut d, Command::SetLabel { node: n(2), label: Some("two\nlines".into()) }).contains("U+000A"));
        let mut taken = Document::parse(DocId(1), r#"<svg xmlns:ink="urn:other"><g/></svg>"#).unwrap();
        assert!(refused(&mut taken, Command::SetLabel { node: n(2), label: Some("a".into()) }).contains("uses the prefix \"ink:\" for something else"));
    }

    #[test]
    fn a_locked_node_refuses_every_edit() {
        let inner = r##"<g id="held" ink:locked="true"><rect id="in" width="4" height="4"/></g><g id="over"><path id="pinned" d="M0 0h4" ink:locked="true"/><circle id="free" r="2"/></g><rect id="loose" width="2" height="2"/>"##;
        // N2 held (locked), N3 in it, N4 over, N5 pinned (locked), N6 free, N7 loose.
        let refused = |c: Command| doc(inner).apply(&c).unwrap_err().to_string();
        let own = "N2 is locked: nothing about it changes until it's unlocked";
        let within = "N3 is in N2, which is locked: nothing in it changes until N2 is unlocked";
        let holds = "N4 holds N5, which is locked: that would move or change it. Unlock N5 first, or leave N4 as it is";
        let attr = |node: u64, name: &str| Command::SetAttr { node: n(node), name: name.into(), value: Some("1".into()) };
        assert_eq!(refused(attr(2, "opacity")), own);
        assert_eq!(refused(attr(3, "x")), within);
        assert_eq!(refused(Command::SetStyle { nodes: vec![n(7), n(3)], set: vec![("fill".into(), Some("red".into()))] }), within, "all of a Command, or none");
        assert_eq!(refused(Command::Delete { nodes: vec![n(2)] }), own);
        assert_eq!(refused(Command::Insert { place: Place::LastIn(n(2)), elements: vec![crate::node::Element::new("path")] }), own);
        assert_eq!(refused(Command::Move { nodes: vec![n(7)], place: Place::After(n(3)) }), own, "nothing is put into it either");
        assert_eq!(refused(Command::Duplicate { nodes: vec![n(3)] }), own);
        assert_eq!(refused(Command::ToPath { nodes: vec![n(3)] }), within);
        assert_eq!(refused(Command::SetLabel { node: n(5), label: Some("a".into()) }), "N5 is locked: nothing about it changes until it's unlocked");
        // What would move, rewrite or remove a locked node from above.
        for deep in [Command::Delete { nodes: vec![n(4)] }, Command::Transform { nodes: vec![n(4)], by: Affine::translate(1.0, 0.0) }, Command::Ungroup { nodes: vec![n(4)], drop: false }, Command::Move { nodes: vec![n(4)], place: Place::FirstIn(n(1)) }, Command::Group { nodes: vec![n(4), n(7)] }] {
            assert_eq!(refused(deep.clone()), holds, "{deep:?}");
        }
        assert_eq!(refused(Command::Batch(vec![attr(7, "x"), attr(5, "opacity")])), "N5 is locked: nothing about it changes until it's unlocked");
    }

    #[test]
    fn what_leaves_a_locked_node_as_it_is_goes_through() {
        let inner = r##"<g id="held" ink:locked="true"><rect id="in" width="4" height="4"/></g><g id="over"><path id="pinned" d="M0 0h4" ink:locked="true"/><circle id="free" r="2"/></g><rect id="loose" width="2" height="2"/>"##;
        let ok = |c: Command| {
            let mut d = doc(inner);
            assert!(!d.apply(&c).unwrap_or_else(|e| panic!("{c:?}: {e}")).is_nothing(), "{c:?}");
            d
        };
        // Beside it, and around it: its group painted, its neighbours changed.
        ok(Command::SetAttr { node: n(4), name: "opacity".into(), value: Some("0.5".into()) });
        ok(Command::SetAttr { node: n(6), name: "r".into(), value: Some("3".into()) });
        ok(Command::Insert { place: Place::After(n(2)), elements: vec![crate::node::Element::new("path")] });
        ok(Command::Delete { nodes: vec![n(6), n(7)] });
        // A copy of a locked node is a new node nobody locked, all the
        // way down: the lock stays where it was put.
        let copied = ok(Command::Duplicate { nodes: vec![n(2), n(4)] });
        assert!(copied.is_locked(n(2)) && copied.is_locked(n(5)) && copied.lock_over(n(8)).is_none() && copied.lock_inside(n(10)).is_none());
        assert!(copied.to_svg().contains(r#"<g id="held-2"><rect id="in-2" width="4" height="4"/></g>"#) && copied.to_svg().contains(r#"<path id="pinned-2" d="M0 0h4"/>"#), "{}", copied.to_svg());
        // Unlocking by the attribute itself is unlocking.
        let unlocked = ok(Command::SetAttr { node: n(2), name: "ink:locked".into(), value: None });
        assert!(!unlocked.is_locked(n(2)));
        // In a batch, what an earlier step unlocked a later one may change.
        ok(Command::Batch(vec![Command::SetLocked { nodes: vec![n(5)], locked: false }, Command::Transform { nodes: vec![n(4)], by: Affine::translate(1.0, 0.0) }, Command::SetLocked { nodes: vec![n(5)], locked: true }]));
    }

    #[test]
    fn a_lock_holds_what_its_node_is_drawn_with() {
        let inner = r##"<defs><linearGradient id="base"><stop offset="0"/></linearGradient><linearGradient id="sky" href="#base"/><clipPath id="cut"><rect id="hole" width="4" height="4"/></clipPath><filter id="free"/></defs><g ink:locked="true"><rect id="kept" width="4" height="4" fill="url(#sky)" clip-path="url(#cut)"/></g><rect id="loose" width="2" height="2" fill="url(#sky)" filter="url(#free)"/>"##;
        // N2 defs, N3 base, N4 its stop, N5 sky, N6 cut, N7 hole, N8
        // free, N9 the locked group, N10 kept (in it), N11 loose.
        let refused = |c: Command| doc(inner).apply(&c).unwrap_err().to_string();
        let attr = |node: u64, name: &str| Command::SetAttr { node: n(node), name: name.into(), value: Some("1".into()) };
        let until = "what N9 is drawn with, and N9 is locked: nothing it's drawn with changes until N9 is unlocked";
        assert_eq!(refused(attr(5, "x2")), format!("N5 is {until}"));
        assert_eq!(refused(Command::Delete { nodes: vec![n(5)] }), format!("N5 is {until}"));
        // What it uses uses, and what's in either.
        assert_eq!(refused(attr(3, "x2")), format!("N3 is {until}"));
        assert_eq!(refused(attr(4, "offset")), format!("N4 is in N3, which is {until}"));
        assert_eq!(refused(Command::Transform { nodes: vec![n(7)], by: Affine::translate(1.0, 0.0) }), format!("N7 is in N6, which is {until}"));
        assert_eq!(refused(Command::Insert { place: Place::LastIn(n(5)), elements: vec![crate::node::Element::new("stop")] }), format!("N5 is {until}"));
        // From above: the <defs> they're in can't go, or be moved.
        let above = refused(Command::Delete { nodes: vec![n(2)] });
        assert!(above.starts_with("N2 holds N") && above.ends_with(", which N9 is drawn with, and N9 is locked: that would move or change it. Unlock N9 first, or leave N2 as it is"), "{above}");
        // What only loose things use is free, and so is painting them
        // with something else, or making more to paint with.
        let ok = |c: Command| assert!(!doc(inner).apply(&c).unwrap_or_else(|e| panic!("{c:?}: {e}")).is_nothing(), "{c:?}");
        ok(attr(8, "x"));
        ok(Command::SetStyle { nodes: vec![n(11)], set: vec![("fill".into(), Some("red".into()))] });
        ok(Command::Define { elements: vec![crate::node::Element::new("linearGradient").with("id", "new")] });
        // Unlocked, it's all free again.
        ok(Command::Batch(vec![Command::SetLocked { nodes: vec![n(9)], locked: false }, attr(5, "x2"), attr(4, "offset")]));
        // What a <style> rule paints a locked node with is held too.
        let mut ruled = doc(r##"<style>.k { fill: url(#sky) }</style><linearGradient id="sky"/><rect class="k" width="4" height="4" ink:locked="true"/>"##);
        assert_eq!(ruled.apply(&attr(3, "x2")).unwrap_err().to_string(), "N3 is what N4 is drawn with, and N4 is locked: nothing it's drawn with changes until N4 is unlocked");
    }

    #[test]
    fn tidying_steps_round_what_is_locked() {
        let mut d = doc(r##"<defs><linearGradient id="spare" ink:locked="true"/><linearGradient id="gone"/></defs><g ink:locked="true"><!-- kept --><g id="hollow"/></g><g/><!-- goes -->"##);
        d.apply(&Command::Tidy { also: vec![Extra::Comments, Extra::Ids] }).unwrap();
        assert_eq!(d.to_svg(), format!(r##"<svg {INK}><defs><linearGradient id="spare" ink:locked="true"/></defs><g ink:locked="true"><!-- kept --><g id="hollow"/></g></svg>"##));
    }
}
