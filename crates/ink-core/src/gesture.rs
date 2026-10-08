//! Gestures (ARCHITECTURE §4.3): a drag shown as it will land, without
//! touching the document until it does.
//!
//! A gesture comes to one [`Command`], said again each time the drag
//! moves on: "these nodes, moved this far from where they were". What a
//! window shows meanwhile ([`Core::shown`]) is the document with that
//! command applied to a copy of it, which shares every node the command
//! didn't touch. The document itself, its history and what's saved of
//! it are as they were: anything else that reads them in the middle of
//! a drag (a save, Claude over MCP) sees only what's been committed.
//! [`Core::commit`] applies the command for real, as one step.
//!
//! Because the command is always the whole of the drag so far, applied
//! to the document as it is, nothing drifts: a hundred small moves
//! aren't a hundred roundings, and a drag that comes back to where it
//! began is no change at all.

use ink_doc::{Applied, Command, DocId, Document};

use crate::core::{Core, Open};
use crate::error::CoreError;
use crate::history::Actor;

/// Which picture of a document is the one to show ([`Core::shown`]):
/// the same as another moment's only if the drawing looked the same
/// then.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Look {
    /// The state the document is in ([`crate::History::stamp`]).
    state: u64,
    /// The preview over it: 0 for none.
    preview: u64,
}

impl Look {
    /// Whether it's a gesture's preview, not the document itself.
    pub fn is_preview(&self) -> bool {
        self.preview != 0
    }
}

/// A drag under way.
pub(crate) struct Gesture {
    actor: Actor,
    /// What it comes to so far, and what that did where it could be
    /// done. `None` until it has moved.
    command: Option<(Command, Result<Applied, CoreError>)>,
    /// The document with the command applied, and the number of this
    /// preview. `None` while the command changes nothing, or is
    /// refused: the document shows as it is.
    preview: Option<(Document, u64)>,
}

impl Open {
    /// The document changed under a gesture (an edit of Claude's, an
    /// undo): what the gesture comes to is worked out again, on the
    /// document as it is now.
    pub(crate) fn rebase(&mut self, previews: &mut u64) {
        let Some(command) = self.gesture.as_ref().and_then(|g| g.command.as_ref()).map(|(c, _)| c.clone()) else { return };
        // What it comes to now is kept with the gesture, to say when it ends.
        let _ = self.preview(&command, previews);
    }

    /// Make the gesture's preview the document with `command` applied.
    fn preview(&mut self, command: &Command, previews: &mut u64) -> Result<Applied, CoreError> {
        let mut copy = self.doc.clone();
        let done = copy.apply(command).map_err(CoreError::from);
        let Some(gesture) = &mut self.gesture else { return done };
        gesture.preview = match &done {
            Ok(applied) if !applied.is_nothing() => {
                *previews += 1;
                Some((copy, *previews - 1))
            }
            _ => None,
        };
        gesture.command = Some((command.clone(), done.clone()));
        done
    }
}

impl Core {
    /// Begin a gesture on `id`: a drag whose every step is shown, and
    /// which lands as one step of history, or none. One already under
    /// way there is given up.
    pub fn begin(&mut self, id: DocId, actor: Actor) -> Result<(), CoreError> {
        self.open_mut(id)?.gesture = Some(Gesture { actor, command: None, preview: None });
        Ok(())
    }

    /// Whether a gesture is under way on `id`.
    pub fn gesturing(&self, id: DocId) -> bool {
        self.open(id).is_ok_and(|open| open.gesture.is_some())
    }

    /// What the gesture on `id` comes to now: `command`, in place of
    /// whatever it came to before. That is what shows
    /// ([`Core::shown`]); the document is as it was. A command the
    /// document refuses shows nothing, and says why: the gesture goes
    /// on, and may yet come to one it takes.
    pub fn update(&mut self, id: DocId, command: &Command) -> Result<Applied, CoreError> {
        let open = self.docs.get_mut(&id).ok_or(CoreError::NoSuchDoc(id))?;
        let gesture = open.gesture.as_ref().ok_or(CoreError::NoGesture(id))?;
        // Asked again where it was (a frame the pointer didn't move
        // in): nothing to work out, and the same picture to show.
        if let Some((was, done)) = &gesture.command
            && was == command
        {
            return done.clone();
        }
        open.preview(command, &mut self.previews)
    }

    /// End the gesture on `id`: what it came to is applied to the
    /// document as one step, `label`led and its actor's. Nothing, if it
    /// came to nothing; and if the document refuses it, that's said and
    /// nothing is done. Either way the gesture is over.
    pub fn commit(&mut self, id: DocId, label: &str) -> Result<Applied, CoreError> {
        let gesture = self.open_mut(id)?.gesture.take().ok_or(CoreError::NoGesture(id))?;
        match gesture.command {
            Some((command, _)) => self.apply(id, &command, gesture.actor, label),
            None => Ok(Applied::default()),
        }
    }

    /// Give the gesture on `id` up: the drawing shows as it is, and
    /// nothing was done.
    pub fn cancel(&mut self, id: DocId) {
        if let Ok(open) = self.open_mut(id) {
            open.gesture = None;
        }
    }

    /// The drawing to show for `id`: as a gesture under way would leave
    /// it, else as it is. And which picture that is: draw again when
    /// the [`Look`] is another.
    pub fn shown(&self, id: DocId) -> Result<(&Document, Look), CoreError> {
        let open = self.open(id)?;
        let state = open.history.stamp();
        Ok(match open.gesture.as_ref().and_then(|g| g.preview.as_ref()) {
            Some((doc, preview)) => (doc, Look { state, preview: *preview }),
            None => (&open.doc, Look { state, preview: 0 }),
        })
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::NodeId;
    use ink_geom::Affine;

    use super::*;

    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\n  <rect id=\"a\" x=\"2\" y=\"2\" width=\"4\" height=\"4\"/>\n  <circle id=\"b\" cx=\"12\" cy=\"12\" r=\"3\"/>\n</svg>\n";

    fn moved(node: u64, dx: f64, dy: f64) -> Command {
        Command::Transform { nodes: vec![NodeId(node)], by: Affine::translate(dx, dy) }
    }

    fn rect_x(doc: &Document) -> &str {
        doc.node(NodeId(2)).unwrap().attr("x").unwrap()
    }

    #[test]
    fn a_gesture_shows_without_touching_the_document_and_lands_as_one_step() {
        let mut core = Core::headless();
        let id = core.open_text(SVG).unwrap().doc;
        let (_, at_rest) = core.shown(id).unwrap();
        assert!(!at_rest.is_preview() && !core.gesturing(id));
        assert_eq!(core.update(id, &moved(2, 1.0, 0.0)), Err(CoreError::NoGesture(id)));

        core.begin(id, Actor::Alva).unwrap();
        assert_eq!(core.shown(id).unwrap().1, at_rest, "begun, and nothing to show yet");
        let mut looks = vec![at_rest];
        for step in 1..=5 {
            let applied = core.update(id, &moved(2, step as f64, 0.5)).unwrap();
            assert_eq!(applied.changed, [NodeId(2)]);
            let (shown, look) = core.shown(id).unwrap();
            // The whole of the drag so far, from where the rect was.
            assert_eq!((rect_x(shown), look.is_preview()), (format!("{}", 2 + step).as_str(), true));
            assert!(!looks.contains(&look), "every preview is another picture");
            looks.push(look);
            // The document, its history and its file's state are as
            // they were.
            assert_eq!((core.doc(id).unwrap().to_svg().as_str(), core.history(id).unwrap().undoable().count(), core.is_modified(id).unwrap()), (SVG, 0, true));
        }
        // The same place again: the same picture, nothing worked out.
        let look = core.shown(id).unwrap().1;
        core.update(id, &moved(2, 5.0, 0.5)).unwrap();
        assert_eq!(core.shown(id).unwrap().1, look);

        let applied = core.commit(id, "Move").unwrap();
        assert_eq!(applied.changed, [NodeId(2)]);
        let (shown, look) = core.shown(id).unwrap();
        assert!(!look.is_preview() && !looks.contains(&look) && !core.gesturing(id));
        assert_eq!((rect_x(shown), rect_x(core.doc(id).unwrap())), ("7", "7"));
        let steps: Vec<_> = core.history(id).unwrap().undoable().map(|s| (s.label.as_str(), s.actor)).collect();
        assert_eq!(steps, [("Move", Actor::Alva)]);
        core.undo(id).unwrap();
        assert_eq!((core.doc(id).unwrap().to_svg().as_str(), core.shown(id).unwrap().1), (SVG, at_rest));
    }

    #[test]
    fn a_gesture_given_up_or_come_to_nothing_leaves_no_trace() {
        let mut core = Core::headless();
        let id = core.open_text(SVG).unwrap().doc;
        let at_rest = core.shown(id).unwrap().1;
        core.begin(id, Actor::Alva).unwrap();
        core.update(id, &moved(3, 4.0, 4.0)).unwrap();
        assert!(core.shown(id).unwrap().1.is_preview());
        core.cancel(id);
        assert_eq!((core.shown(id).unwrap().1, core.doc(id).unwrap().to_svg().as_str(), core.gesturing(id)), (at_rest, SVG, false));
        assert_eq!(core.commit(id, "Move"), Err(CoreError::NoGesture(id)));

        // Dragged out and back to where it began: no change, no step.
        core.begin(id, Actor::Alva).unwrap();
        core.update(id, &moved(3, 4.0, 4.0)).unwrap();
        assert!(core.update(id, &moved(3, 0.0, 0.0)).unwrap().is_nothing());
        assert_eq!(core.shown(id).unwrap().1, at_rest);
        assert!(core.commit(id, "Move").unwrap().is_nothing());
        assert_eq!((core.history(id).unwrap().undoable().count(), core.doc(id).unwrap().to_svg().as_str()), (0, SVG));
        // Begun and ended without moving.
        core.begin(id, Actor::Alva).unwrap();
        assert!(core.commit(id, "Move").unwrap().is_nothing());
        assert_eq!(core.history(id).unwrap().undoable().count(), 0);
    }

    #[test]
    fn what_the_document_refuses_shows_nothing_and_says_why() {
        let mut core = Core::headless();
        let id = core.open_text(SVG).unwrap().doc;
        let at_rest = core.shown(id).unwrap().1;
        core.apply(id, &Command::SetLocked { nodes: vec![NodeId(2)], locked: true }, Actor::Alva, "Lock").unwrap();
        let locked = core.shown(id).unwrap().1;
        assert_ne!(locked, at_rest);
        core.begin(id, Actor::Alva).unwrap();
        core.update(id, &moved(3, 1.0, 1.0)).unwrap();
        // On to something it can't have: the drawing shows as it is.
        let refused = core.update(id, &moved(2, 1.0, 1.0)).unwrap_err();
        assert!(refused.to_string().contains("locked"), "{refused}");
        assert_eq!(core.shown(id).unwrap().1, locked);
        // And back to something it can.
        core.update(id, &moved(3, 2.0, 1.0)).unwrap();
        assert!(core.shown(id).unwrap().1.is_preview());
        // Ended on the refusal, it says so, and nothing is done.
        core.update(id, &moved(2, 1.0, 1.0)).unwrap_err();
        assert!(core.commit(id, "Move").is_err());
        assert_eq!((core.history(id).unwrap().undoable().count(), core.gesturing(id)), (1, false));
    }

    #[test]
    fn a_gesture_follows_the_document_changing_under_it() {
        let mut core = Core::headless();
        let id = core.open_text(SVG).unwrap().doc;
        core.begin(id, Actor::Alva).unwrap();
        core.update(id, &moved(2, 3.0, 0.0)).unwrap();
        let before = core.shown(id).unwrap().1;
        // Claude recolours the circle in the middle of Alva's drag.
        core.apply(id, &Command::SetAttr { node: NodeId(3), name: "fill".into(), value: Some("#ffc800".into()) }, Actor::Claude, "Fill").unwrap();
        let (shown, look) = core.shown(id).unwrap();
        assert!(look.is_preview() && look != before);
        assert_eq!((rect_x(shown), shown.node(NodeId(3)).unwrap().attr("fill")), ("5", Some("#ffc800")), "the drag, on the drawing as it is now");
        assert_eq!(rect_x(core.doc(id).unwrap()), "2");
        // Undone under it, too.
        core.undo(id).unwrap();
        assert_eq!(core.shown(id).unwrap().0.node(NodeId(3)).unwrap().attr("fill"), None);
        core.commit(id, "Move").unwrap();
        let steps: Vec<_> = core.history(id).unwrap().undoable().map(|s| (s.label.as_str(), s.actor)).collect();
        assert_eq!(steps, [("Move", Actor::Alva)]);
    }

    #[test]
    fn what_a_gesture_makes_has_the_ids_it_showed() {
        let mut core = Core::headless();
        let id = core.open_text(SVG).unwrap().doc;
        core.begin(id, Actor::Alva).unwrap();
        let shown = core.update(id, &Command::Duplicate { nodes: vec![NodeId(2)] }).unwrap();
        // However often it's asked again on the way.
        core.update(id, &Command::Batch(vec![Command::Duplicate { nodes: vec![NodeId(3)] }])).unwrap();
        let again = core.update(id, &Command::Duplicate { nodes: vec![NodeId(2)] }).unwrap();
        let landed = core.commit(id, "Duplicate").unwrap();
        assert_eq!((shown.created.len(), &again.created, &landed.created), (1, &shown.created, &shown.created));
        assert!(core.doc(id).unwrap().get(landed.created[0]).is_some());
    }
}
