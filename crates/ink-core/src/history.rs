//! Undo, as snapshots (ARCHITECTURE §4.2, LUI2 U016). Every state a
//! document has been in is kept as a [`Snapshot`], which shares every
//! node with its neighbours, so a step costs the nodes it changed and
//! undo is putting a snapshot back. One mechanism for every edit: none
//! can forget it.

use ink_doc::{Document, Snapshot};

/// Who made an edit: history says, so Alva can tell her steps from
/// Claude's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Alva,
    Claude,
}

/// One step of history: what was done, and by whom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    pub actor: Actor,
}

/// A state the document has been in.
struct State {
    snapshot: Snapshot,
    /// The step that led here (`None` for the state the document
    /// opened in).
    step: Option<Step>,
    /// Never given to another state, so "the state that was saved" can
    /// be told from every other, however undo and redo wander.
    stamp: u64,
}

/// The most states kept. Past it the oldest go: they can't be undone to
/// any more.
const MAX_STATES: usize = 1000;

pub struct History {
    states: Vec<State>,
    /// The state the document is in now.
    at: usize,
    next_stamp: u64,
}

impl History {
    /// History beginning with `doc` as it is.
    pub(crate) fn new(doc: &Document) -> History {
        History { states: vec![State { snapshot: doc.snapshot(), step: None, stamp: 0 }], at: 0, next_stamp: 1 }
    }

    /// `doc` has just been changed by `step`: what could be redone is
    /// gone, and this is the newest state.
    pub(crate) fn record(&mut self, doc: &Document, step: Step) {
        self.states.truncate(self.at + 1);
        self.states.push(State { snapshot: doc.snapshot(), step: Some(step), stamp: self.next_stamp });
        self.next_stamp += 1;
        if self.states.len() > MAX_STATES {
            self.states.remove(0);
        }
        self.at = self.states.len() - 1;
    }

    /// Step back: `doc` is put as it was before the latest step, which
    /// is returned.
    pub(crate) fn undo(&mut self, doc: &mut Document) -> Option<Step> {
        let undone = self.states[self.at].step.clone().filter(|_| self.at > 0)?;
        self.at -= 1;
        doc.restore(&self.states[self.at].snapshot);
        Some(undone)
    }

    /// Step forward again: the step redone.
    pub(crate) fn redo(&mut self, doc: &mut Document) -> Option<Step> {
        let next = self.states.get(self.at + 1)?;
        doc.restore(&next.snapshot);
        self.at += 1;
        next.step.clone()
    }

    /// The stamp of the state the document is in: equal to another
    /// moment's only if it's in that very state again.
    pub fn stamp(&self) -> u64 {
        self.states[self.at].stamp
    }

    /// The steps that can be undone, the latest last.
    pub fn undoable(&self) -> impl DoubleEndedIterator<Item = &Step> {
        // The oldest state kept is as far back as undo goes: the step
        // that led to it can't be undone.
        self.states.iter().take(self.at + 1).skip(1).filter_map(|s| s.step.as_ref())
    }

    /// The steps that can be redone, the next first.
    pub fn redoable(&self) -> impl DoubleEndedIterator<Item = &Step> {
        self.states[self.at + 1..].iter().filter_map(|s| s.step.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::{Command, DocId, NodeId};

    use super::*;

    fn set(doc: &mut Document, value: &str) {
        doc.apply(&Command::SetAttr { node: NodeId(1), name: "a".into(), value: Some(value.into()) }).unwrap();
    }

    fn step(label: &str, actor: Actor) -> Step {
        Step { label: label.into(), actor }
    }

    #[test]
    fn steps_go_back_and_forward_and_a_new_one_ends_what_could_be_redone() {
        let mut doc = Document::parse(DocId(1), "<svg/>").unwrap();
        let mut h = History::new(&doc);
        assert_eq!((h.undo(&mut doc), h.redo(&mut doc), h.stamp()), (None, None, 0));
        for (value, actor) in [("1", Actor::Alva), ("2", Actor::Claude), ("3", Actor::Alva)] {
            set(&mut doc, value);
            h.record(&doc, step(value, actor));
        }
        assert_eq!(h.undoable().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["1", "2", "3"]);
        assert_eq!(h.undo(&mut doc), Some(step("3", Actor::Alva)));
        assert_eq!(h.undo(&mut doc), Some(step("2", Actor::Claude)));
        assert_eq!((doc.to_svg().as_str(), h.stamp()), ("<svg a=\"1\"/>", 1));
        assert_eq!(h.redoable().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["2", "3"]);
        assert_eq!(h.redo(&mut doc), Some(step("2", Actor::Claude)));
        assert_eq!((doc.to_svg().as_str(), h.stamp()), ("<svg a=\"2\"/>", 2));
        // A new step from here: "3" can't be redone any more, and the
        // new state's stamp is one no state has had.
        set(&mut doc, "4");
        h.record(&doc, step("4", Actor::Alva));
        assert_eq!((h.redo(&mut doc), h.stamp()), (None, 4));
        assert_eq!(h.undoable().map(|s| s.label.as_str()).collect::<Vec<_>>(), ["1", "2", "4"]);
        while h.undo(&mut doc).is_some() {}
        assert_eq!((doc.to_svg().as_str(), h.stamp()), ("<svg/>", 0), "all the way back is the state it opened in");
    }

    #[test]
    fn the_oldest_states_go_past_the_limit() {
        let mut doc = Document::parse(DocId(1), "<svg/>").unwrap();
        let mut h = History::new(&doc);
        for i in 0..MAX_STATES + 5 {
            set(&mut doc, &i.to_string());
            h.record(&doc, step("set", Actor::Alva));
        }
        let mut undone = 0;
        while h.undo(&mut doc).is_some() {
            undone += 1;
        }
        assert_eq!(undone, MAX_STATES - 1);
        assert_eq!(doc.to_svg(), "<svg a=\"5\"/>", "as far back as is kept");
    }
}
