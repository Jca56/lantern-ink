//! What Object > Clip and Release Clip and Text > Text to Path do to
//! the selection (ARCHITECTURE §8): each a Command the core has had
//! since M3, one step of Alva's. (Drop Shadow and Blur are
//! `shadows.rs`.)
//!
//! **Clip:** the thing on top of what's selected, a shape, is what the
//! rest is cut to. One thing under it is cut itself; several are put in
//! a group, which is cut as one (a clip path is in the coordinates of
//! what it cuts, and several things each have their own).

use ink_core::{Command, DocId, Document, NodeId};
use ink_doc::Kind;
use ink_doc::refs::Ids;
use ink_doc::style::prop;
use lntrn_ui::{Dialog, HostCx, ShellRequest};

use crate::actions::doc_action;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::menus;
use crate::paint;
use crate::select;

/// Something Object or Text does to the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Clip,
    Release,
    /// `true`: even set in another font than it asks for.
    TextToPath(bool),
}

impl Effect {
    pub fn label(self) -> &'static str {
        match self {
            Effect::Clip => "Clip",
            Effect::Release => "Release Clip",
            Effect::TextToPath(_) => "Text to Path",
        }
    }
}

/// What there is to do with the selection: which rows are lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Can {
    pub clip: bool,
    pub release: bool,
    pub text: bool,
}

/// Of the selection `tops` (back to front), what's drawn and may be
/// changed.
pub(crate) fn drawn(doc: &Document, tops: &[NodeId]) -> Vec<NodeId> {
    tops.iter().copied().filter(|&id| select::is_drawn(doc, id) && doc.lock_over(id).is_none()).collect()
}

/// Whether `id` is cut by a clip path.
fn clipped(doc: &Document, id: NodeId) -> bool {
    doc.get(id).and_then(|n| prop(n, "clip-path")).is_some_and(|v| v.trim() != "none")
}

/// The texts the selection is or holds.
fn texts(doc: &Document, tops: &[NodeId]) -> Vec<NodeId> {
    paint::painted(doc, tops).into_iter().filter(|&id| doc.get(id).is_some_and(|n| n.kind == Kind::Text) && doc.lock_over(id).is_none()).collect()
}

/// What there is to do with the selection `tops` of `doc`.
pub fn can(doc: &Document, tops: &[NodeId]) -> Can {
    let stack = drawn(doc, tops);
    let top_is_shape = stack.last().is_some_and(|&top| doc.get(top).is_some_and(|n| n.kind.is_shape()));
    Can { clip: stack.len() >= 2 && top_is_shape, release: stack.iter().any(|&id| clipped(doc, id)), text: !texts(doc, tops).is_empty() }
}

/// The Command that does `effect` to the selection `tops` of `doc`.
/// None where there's nothing for it to do.
pub fn command(doc: &Document, tops: &[NodeId], effect: Effect) -> Option<Command> {
    let stack = drawn(doc, tops);
    match effect {
        Effect::Clip => {
            let (&top, under) = stack.split_last().filter(|_| can(doc, tops).clip)?;
            let ids = Ids::of(doc);
            let id = (1..).map(|n| format!("clip-{n}")).find(|id| ids.get(id).is_none())?;
            if let [one] = under {
                return Some(Command::SetClip { nodes: vec![*one], by: vec![top], id });
            }
            // Several: in a group, cut as one. What the group will be
            // called is what it's called on a copy.
            let group = Command::Group { nodes: under.to_vec() };
            let mut copy = doc.clone();
            let made = copy.apply(&group).ok()?.created.first().copied()?;
            Some(Command::Batch(vec![group, Command::SetClip { nodes: vec![made], by: vec![top], id }]))
        }
        Effect::Release => {
            let nodes: Vec<NodeId> = stack.into_iter().filter(|&id| clipped(doc, id)).collect();
            (!nodes.is_empty()).then(|| Command::SetClip { nodes, by: Vec::new(), id: String::new() })
        }
        Effect::TextToPath(as_drawn) => {
            let nodes = texts(doc, tops);
            (!nodes.is_empty()).then_some(Command::TextToPath { nodes, as_drawn })
        }
    }
}

impl Ink {
    /// What Object's and Text's effects can do with the selection of
    /// the tab that shows.
    pub(crate) fn effect_can(&self) -> Can {
        self.tabs.active().and_then(|tab| self.core.doc(tab.doc).ok().map(|drawing| can(drawing, &tab.selection.tops(drawing)))).unwrap_or_default()
    }

    /// Do `effect` to the selection of the tab that shows: one step.
    /// What's selected afterwards is what it left, with what it made.
    pub(crate) fn effect(&mut self, effect: Effect, cx: &mut HostCx) {
        let Some((doc, drawing, tops)) = self.tabs.active().and_then(|tab| {
            let drawing = self.core.doc(tab.doc).ok()?;
            Some((tab.doc, drawing, tab.selection.tops(drawing)))
        }) else {
            return;
        };
        let Some(command) = command(drawing, &tops, effect) else { return };
        match self.core.apply(doc, &command, ink_core::Actor::Alva, effect.label()) {
            // (What a release puts back in the drawing was moved there,
            // not made.)
            Ok(applied) => self.selected_after(doc, &tops, &[applied.created, applied.moved].concat()),
            // Set in another font than it asks for, its paths would be
            // that font's for good: ask.
            Err(e) if effect == Effect::TextToPath(false) && e.to_string().contains("Say so to make them anyway") => {
                let why = e.to_string();
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                let reason = said.split(". Say so").next().unwrap_or(&said).to_owned() + ".";
                let dialog = Dialog::new("Make paths of it anyway?", &reason).button("Cancel", None).button("Make Paths", Some(doc_action(menus::TEXT_TO_PATH_ANYWAY, doc))).default_button(0);
                cx.request(ShellRequest::Dialog(dialog));
            }
            Err(e) => {
                let why = e.to_string();
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
            }
        }
    }

    /// After a step on the selection `was` that made `created`: what's
    /// left of it that isn't inside something the step made, then what
    /// it made.
    pub(crate) fn selected_after(&mut self, doc: DocId, was: &[NodeId], created: &[NodeId]) {
        let Ok(drawing) = self.core.doc(doc) else { return };
        let inside = |id: NodeId| drawing.ancestors(id).any(|n| created.contains(&n.id));
        let mut now: Vec<NodeId> = was.iter().copied().filter(|&id| drawing.get(id).is_some() && select::is_drawn(drawing, id) && !inside(id)).collect();
        for made in created {
            if !now.contains(made) && select::is_drawn(drawing, *made) {
                now.push(*made);
            }
        }
        if now.is_empty() {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.doc == doc) {
                tab.selection.clear();
            }
        } else {
            self.select(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId as Id;

    use super::*;

    /// Back to front: a card, a badge, a round window over them, a
    /// locked square, and a word.
    fn doc() -> Document {
        Document::parse(Id(1), r##"<svg xmlns:ink="urn:lantern:ink" viewBox="0 0 48 48"><rect id="card" width="20" height="20"/><rect id="badge" x="10" y="10" width="8" height="8"/><circle id="window" cx="10" cy="10" r="8"/><rect width="4" height="4" ink:locked="true"/><text id="word" x="2" y="40" font-family="Ink Test">hi</text></svg>"##).unwrap()
    }

    const CARD: NodeId = NodeId(2);
    const BADGE: NodeId = NodeId(3);
    const WINDOW: NodeId = NodeId(4);
    const LOCKED: NodeId = NodeId(5);
    const WORD: NodeId = NodeId(6);

    #[test]
    fn the_thing_on_top_cuts_the_rest() {
        let mut d = doc();
        assert_eq!((can(&d, &[]), can(&d, &[CARD]), can(&d, &[CARD, WINDOW]).clip, can(&d, &[CARD, WORD]).clip, can(&d, &[LOCKED, WINDOW]).clip), (Can::default(), Can::default(), true, false, false));
        // One thing under it: cut itself, and the shape on top is its
        // clip path now.
        let applied = d.apply(&command(&d, &[CARD, WINDOW], Effect::Clip).unwrap()).unwrap();
        assert_eq!((d.node(CARD).unwrap().attr("clip-path"), applied.changed.contains(&CARD), can(&d, &[CARD])), (Some("url(#clip-1)"), true, Can { clip: false, release: true, text: false }));
        assert!(d.to_svg().contains(r#"<clipPath id="clip-1"><circle id="window" cx="10" cy="10" r="8"/></clipPath>"#), "{}", d.to_svg());
        // Released: no clip path, and the shape is back over it.
        d.apply(&command(&d, &[CARD], Effect::Release).unwrap()).unwrap();
        assert_eq!((d.node(CARD).unwrap().attr("clip-path"), can(&d, &[CARD, WINDOW]).release, d.to_svg().contains("clipPath")), (None, false, false));
        assert_eq!(command(&d, &[CARD], Effect::Release), None);
        // Several under it: put in a group, and the group cut.
        let mut d = doc();
        let applied = d.apply(&command(&d, &[CARD, BADGE, WINDOW], Effect::Clip).unwrap()).unwrap();
        let group = applied.created[0];
        assert_eq!((d.node(group).unwrap().name.as_str(), d.node(group).unwrap().attr("clip-path"), d.node(CARD).unwrap().parent, d.node(CARD).unwrap().attr("clip-path")), ("g", Some("url(#clip-1)"), Some(group), None));
    }

    #[test]
    fn a_text_is_made_paths_of() {
        let d = doc();
        assert_eq!((can(&d, &[WORD]).text, can(&d, &[CARD]).text, command(&d, &[CARD], Effect::TextToPath(false))), (true, false, None));
        assert_eq!(command(&d, &[CARD, WORD], Effect::TextToPath(true)), Some(Command::TextToPath { nodes: vec![WORD], as_drawn: true }));
        assert_eq!((Effect::Clip.label(), Effect::Release.label(), Effect::TextToPath(false).label()), ("Clip", "Release Clip", "Text to Path"));
    }
}
