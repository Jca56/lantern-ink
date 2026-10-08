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
//!   holds the lock.

use crate::command::Command;
use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::{INK_NS, INK_PREFIX, Kind};
use crate::node::{local, prefix};

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

    /// Refuse reaching `id` as `reach` says, if a lock is in the way.
    fn reach(&self, id: NodeId, reach: Reach) -> Result<(), DocError> {
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
        let each = |nodes: &[NodeId], reach: Reach| nodes.iter().try_for_each(|&id| self.reach(id, reach));
        // Something new beside a node goes into that node's parent.
        let into = |place: Place| self.parent_at(place).map_or(Ok(()), |parent| self.reach(parent, Reach::Own));
        let beside = |nodes: &[NodeId]| nodes.iter().filter_map(|id| self.get(*id).and_then(|n| n.parent)).try_for_each(|parent| self.reach(parent, Reach::Own));
        match command {
            Command::SetAttr { node, name, .. } => {
                // The lock itself, said as an attribute, is locking.
                let is_lock = local(name) == LOCKED && prefix(name).is_some_and(|p| self.namespace(*node, Some(p)) == Some(INK_NS));
                self.reach(*node, if is_lock { Reach::Lock } else { Reach::Own })
            }
            Command::Insert { place, .. } => into(*place),
            Command::Delete { nodes } | Command::Ungroup { nodes, .. } | Command::Transform { nodes, .. } | Command::Boolean { nodes, .. } => each(nodes, Reach::Deep),
            Command::Move { nodes, place } => each(nodes, Reach::Deep).and_then(|()| into(*place)),
            Command::Group { nodes } => each(nodes, Reach::Deep).and_then(|()| beside(nodes)),
            // A copy changes nothing of what it copies: only where it's
            // put matters.
            Command::Duplicate { nodes } => beside(nodes),
            Command::Define { .. } => {
                let defs = self.get(self.root).and_then(|root| root.elements().find(|&id| self.get(id).is_some_and(|n| n.kind == Kind::Defs)));
                self.reach(defs.unwrap_or(self.root), Reach::Own)
            }
            Command::SetClip { nodes, by, .. } => each(nodes, Reach::Own).and_then(|()| each(by, Reach::Deep)),
            Command::SetStyle { nodes, .. } | Command::ToPath { nodes } | Command::Simplify { nodes, .. } | Command::TextToPath { nodes, .. } => each(nodes, Reach::Own),
            // The outline may be a new path beside its shape.
            Command::OutlineStroke { nodes, .. } => each(nodes, Reach::Own).and_then(|()| beside(nodes)),
            Command::EditPath { node, .. } | Command::SetPath { node, .. } | Command::SetText { node, .. } | Command::SetLabel { node, .. } => self.reach(*node, Reach::Own),
            Command::SetLocked { nodes, .. } => each(nodes, Reach::Lock),
            // Tidying steps round what's locked; a batch's Commands
            // are each guarded as their turn comes.
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
        // A copy of a locked node is its own node (and locked like it).
        let copied = ok(Command::Duplicate { nodes: vec![n(2)] });
        assert!(copied.is_locked(n(8)) && copied.is_locked(n(2)));
        // Unlocking by the attribute itself is unlocking.
        let unlocked = ok(Command::SetAttr { node: n(2), name: "ink:locked".into(), value: None });
        assert!(!unlocked.is_locked(n(2)));
        // In a batch, what an earlier step unlocked a later one may change.
        ok(Command::Batch(vec![Command::SetLocked { nodes: vec![n(5)], locked: false }, Command::Transform { nodes: vec![n(4)], by: Affine::translate(1.0, 0.0) }, Command::SetLocked { nodes: vec![n(5)], locked: true }]));
    }

    #[test]
    fn tidying_steps_round_what_is_locked() {
        let mut d = doc(r##"<defs><linearGradient id="spare" ink:locked="true"/><linearGradient id="gone"/></defs><g ink:locked="true"><!-- kept --><g id="hollow"/></g><g/><!-- goes -->"##);
        d.apply(&Command::Tidy { also: vec![Extra::Comments, Extra::Ids] }).unwrap();
        assert_eq!(d.to_svg(), format!(r##"<svg {INK}><defs><linearGradient id="spare" ink:locked="true"/></defs><g ink:locked="true"><!-- kept --><g id="hollow"/></g></svg>"##));
    }
}
