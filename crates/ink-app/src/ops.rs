//! What the Edit and Object menus do to the selection (ARCHITECTURE §8):
//! each a Command through `Ink::edit`, one step of Alva's, with what's
//! selected afterwards being what the step made or left. The same
//! operations are on the keys, and in the menu a right-click opens.

use ink_core::{Command, DocId, Document, NodeId, Place};
use ink_doc::{Kind, arrange};
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{Action, Dialog, HostCx, ShellRequest};

use crate::actions::doc_action;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::menus;
use crate::select::{self, Selection};

/// Where in its group's stack a thing goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Front,
    Forward,
    Backward,
    Back,
}

/// Something to do to the selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    Deselect,
    Group,
    /// `true`: whatever only the group could hold is let go.
    Ungroup(bool),
    Order(Order),
    /// Which part of each goes in line, across and down (see
    /// [`arrange::line_up`]).
    Align(Option<f64>, Option<f64>),
    /// Share the space out across (`true`) or down.
    Distribute(bool),
    /// Mirror left to right (`true`) or top to bottom.
    Flip(bool),
    /// A quarter turn, clockwise (`true`) or back.
    Quarter(bool),
    /// Lock what's selected; unlock it, if all of it is locked.
    Lock,
}

/// What the menus need to know of the selection to light their rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Picked {
    /// How many things are selected (a group standing for what it
    /// holds), and how many of them are drawn (so can be moved).
    pub count: usize,
    pub drawn: usize,
    /// One of them is a group.
    pub group: bool,
    /// All of them are locked.
    pub locked: bool,
}

/// The selection of the tab that shows, as an operation takes it.
pub(crate) struct Chosen {
    pub doc: DocId,
    /// What's selected, back to front, a group standing for what it
    /// holds.
    pub tops: Vec<NodeId>,
    /// Those of them that are drawn, each with its box.
    pub boxed: Vec<(NodeId, Rect)>,
}

/// The siblings of `id` that are drawn, back to front.
fn stack(doc: &Document, id: NodeId) -> Vec<NodeId> {
    let Some(parent) = doc.get(id).and_then(|n| n.parent).and_then(|p| doc.get(p)) else { return Vec::new() };
    parent.elements().filter(|&c| select::is_drawn(doc, c)).collect()
}

/// The moves that take `nodes` (drawn, back to front) to another place
/// in their groups' stacks: each group's own, for itself.
fn restack(doc: &Document, nodes: &[NodeId], how: Order) -> Vec<Command> {
    let mut out = Vec::new();
    let mut seen: Vec<NodeId> = Vec::new();
    for &first in nodes {
        let Some(parent) = doc.get(first).and_then(|n| n.parent) else { continue };
        if seen.contains(&parent) {
            continue;
        }
        seen.push(parent);
        let mut order = stack(doc, first);
        let picked: Vec<NodeId> = order.iter().copied().filter(|id| nodes.contains(id)).collect();
        match how {
            // All of them to the top, in the order they stand.
            Order::Front => out.push(Command::Move { nodes: picked, place: Place::LastIn(parent) }),
            // Under the lowest thing that isn't one of them.
            Order::Back => {
                if let Some(&under) = order.iter().find(|id| !picked.contains(id)) {
                    out.push(Command::Move { nodes: picked, place: Place::Before(under) });
                }
            }
            // Each past the one thing over it that isn't one of them,
            // the topmost first so none jumps another.
            Order::Forward => {
                for i in (0..order.len().saturating_sub(1)).rev() {
                    if picked.contains(&order[i]) && !picked.contains(&order[i + 1]) {
                        out.push(Command::Move { nodes: vec![order[i]], place: Place::After(order[i + 1]) });
                        order.swap(i, i + 1);
                    }
                }
            }
            Order::Backward => {
                for i in 1..order.len() {
                    if picked.contains(&order[i]) && !picked.contains(&order[i - 1]) {
                        out.push(Command::Move { nodes: vec![order[i]], place: Place::Before(order[i - 1]) });
                        order.swap(i, i - 1);
                    }
                }
            }
        }
    }
    out
}

impl Ink {
    /// What's selected in the tab that shows, as the menus see it.
    pub(crate) fn picked(&self) -> Picked {
        let Some((drawing, sel)) = self.tabs.active().and_then(|tab| Some((self.core.doc(tab.doc).ok()?, &tab.selection))) else { return Picked::default() };
        let tops = sel.tops(drawing);
        Picked {
            count: tops.len(),
            drawn: tops.iter().filter(|&&id| select::is_drawn(drawing, id) && !select::is_hidden(drawing, id)).count(),
            group: tops.iter().any(|&id| drawing.get(id).is_some_and(|n| n.kind == Kind::G)),
            locked: !tops.is_empty() && tops.iter().all(|&id| drawing.is_locked(id)),
        }
    }

    /// What's selected in the tab that shows.
    pub(crate) fn chosen(&mut self) -> Option<Chosen> {
        let tab = self.tabs.active_mut()?;
        let doc = tab.doc;
        let drawing = self.core.doc(doc).ok()?;
        let stamp = self.core.history(doc).ok()?.stamp();
        let tops = tab.selection.tops(drawing);
        let boxes = tab.boxes(drawing, stamp);
        let boxed = tops.iter().filter_map(|id| Some((*id, *boxes.get(id)?))).collect();
        Some(Chosen { doc, tops, boxed })
    }

    fn selection(&mut self) -> Option<&mut Selection> {
        self.tabs.active_mut().map(|tab| &mut tab.selection)
    }

    /// Have `nodes` selected, the last of them in hand, and its row in
    /// sight.
    fn select(&mut self, nodes: Vec<NodeId>) {
        let Some(&last) = nodes.last() else { return };
        if let Some(sel) = self.selection() {
            (sel.active, sel.nodes) = (Some(last), nodes);
        }
        self.tree.show(last);
    }

    /// Do `op` to the selection of the tab that shows.
    pub(crate) fn op(&mut self, op: Op, cx: &mut HostCx) {
        // A drag isn't something to work on in the middle of.
        if self.pointing.busy() {
            return;
        }
        let Some(Chosen { doc, tops, boxed }) = self.chosen() else { return };
        let ids = |boxed: &[(NodeId, Rect)]| boxed.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        match op {
            Op::Copy | Op::Cut => {
                if tops.is_empty() {
                    return;
                }
                let copied = self.core.doc(doc).map_err(|e| e.to_string()).and_then(|d| d.clipping(&tops).map_err(|e| in_row_names(d, &e.to_string())));
                match copied {
                    Ok(svg) => {
                        self.clip_out = Some(svg);
                        if op == Op::Cut {
                            self.edit(doc, &Command::Delete { nodes: tops }, "Cut");
                        }
                    }
                    Err(why) => self.toast(why),
                }
            }
            // The clipboard's text is asked for; it's pasted when the
            // next frame has it.
            Op::Paste => self.pasting = Some(Pasting::Wanted(doc)),
            Op::Delete => {
                if !tops.is_empty() {
                    self.edit(doc, &Command::Delete { nodes: tops }, "Delete");
                }
            }
            Op::Duplicate => {
                if !tops.is_empty()
                    && let Some(applied) = self.edit(doc, &Command::Duplicate { nodes: tops }, "Duplicate")
                {
                    self.select(applied.created);
                }
            }
            Op::SelectAll => {
                let Ok(drawing) = self.core.doc(doc) else { return };
                let Some(tab) = self.tabs.active_mut() else { return };
                let stamp = self.core.history(doc).map_or(0, |h| h.stamp());
                let context = tab.selection.context(drawing);
                let boxes = tab.boxes(drawing, stamp);
                // Everything at the level the Pointer is in that a
                // click there would pick.
                let all: Vec<NodeId> = drawing.get(context).map(|n| n.elements().filter(|id| boxes.contains_key(id) && drawing.lock_over(*id).is_none()).collect()).unwrap_or_default();
                (tab.selection.active, tab.selection.nodes) = (all.last().copied(), all);
            }
            Op::Deselect => {
                if let Some(sel) = self.selection() {
                    sel.clear();
                }
            }
            Op::Group => {
                if !boxed.is_empty()
                    && let Some(applied) = self.edit(doc, &Command::Group { nodes: ids(&boxed) }, "Group")
                {
                    self.select(applied.created);
                }
            }
            Op::Ungroup(drop) => {
                let Ok(drawing) = self.core.doc(doc) else { return };
                let groups: Vec<NodeId> = tops.iter().copied().filter(|&id| drawing.get(id).is_some_and(|n| n.kind == Kind::G)).collect();
                if groups.is_empty() {
                    return;
                }
                let command = Command::Ungroup { nodes: groups, drop };
                match self.core.apply(doc, &command, ink_core::Actor::Alva, "Ungroup") {
                    Ok(applied) => self.select(applied.moved),
                    // What only a group can hold would go with it: ask.
                    Err(e) if !drop && e.to_string().contains("ungrouping would lose") => {
                        let why = e.to_string();
                        let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                        // The document's reason, without its advice
                        // for a tool's caller.
                        let reason = said.split(". Take ").next().unwrap_or(&said).to_owned() + ".";
                        let dialog = Dialog::new("Ungroup anyway?", &reason).button("Cancel", None).button("Ungroup", Some(doc_action(menus::UNGROUP_ANYWAY, doc))).default_button(0);
                        cx.request(ShellRequest::Dialog(dialog));
                    }
                    Err(e) => {
                        let why = e.to_string();
                        let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                        self.toast(said);
                    }
                }
            }
            Op::Order(how) => {
                let Ok(drawing) = self.core.doc(doc) else { return };
                let moves = restack(drawing, &ids(&boxed), how);
                let label = match how {
                    Order::Front => "Bring to Front",
                    Order::Forward => "Bring Forward",
                    Order::Backward => "Send Backward",
                    Order::Back => "Send to Back",
                };
                self.edit(doc, &Command::Batch(moves), label);
            }
            Op::Align(x, y) => {
                let Ok(drawing) = self.core.doc(doc) else { return };
                // Several line up against the box round them all; one
                // alone, or with "to the page" on, against the page.
                let against = match arrange::joint(&boxed) {
                    Some(all) if boxed.len() > 1 && !self.settings.align_to_page => all,
                    Some(_) => arrange::page_box(drawing),
                    None => return,
                };
                self.edit(doc, &arrange::moved(arrange::line_up(&boxed, x, y, against, None)), "Align");
            }
            Op::Distribute(across) => {
                if boxed.len() >= 3 {
                    self.edit(doc, &arrange::moved(arrange::spread(&boxed, across)), "Distribute");
                }
            }
            Op::Flip(across) => {
                let Some(all) = arrange::joint(&boxed) else { return };
                let by = if across { Affine::scale(-1.0, 1.0) } else { Affine::scale(1.0, -1.0) }.about(all.center());
                self.edit(doc, &Command::Transform { nodes: ids(&boxed), by }, if across { "Flip Horizontal" } else { "Flip Vertical" });
            }
            Op::Quarter(clockwise) => {
                let Some(all) = arrange::joint(&boxed) else { return };
                let by = Affine::rotate(if clockwise { 90f64 } else { -90f64 }.to_radians()).about(all.center());
                self.edit(doc, &Command::Transform { nodes: ids(&boxed), by }, "Rotate");
            }
            Op::Lock => {
                let Ok(drawing) = self.core.doc(doc) else { return };
                if tops.is_empty() {
                    return;
                }
                let lock = !tops.iter().all(|&id| drawing.is_locked(id));
                self.edit(doc, &Command::SetLocked { nodes: tops, locked: lock }, if lock { "Lock" } else { "Unlock" });
            }
        }
    }

    /// Paste `svg` (the clipboard's text) into `doc`: on top of what's
    /// at the level the Pointer is in, where it was copied from, and
    /// selected.
    pub(crate) fn paste(&mut self, doc: DocId, svg: String) {
        let Some(context) = self.tabs.iter().find(|t| t.doc == doc).and_then(|tab| Some(tab.selection.context(self.core.doc(doc).ok()?))) else { return };
        let Some(applied) = self.edit(doc, &Command::Paste { svg, place: Place::LastIn(context) }, "Paste") else { return };
        // What it drew, not the definitions that came with it.
        let Ok(drawing) = self.core.doc(doc) else { return };
        let pasted: Vec<NodeId> = applied.created.into_iter().filter(|&id| drawing.get(id).is_some_and(|n| n.parent == Some(context))).collect();
        self.select(pasted);
    }

    /// The menu a right-click opens, at `at`: for what's selected, or
    /// with nothing selected, for the drawing.
    pub(crate) fn selection_menu(&self, at: Vec2) -> lntrn_ui::ContextMenu {
        use lntrn_ui::Item;
        let picked = self.picked();
        let title = match (picked.count, self.tabs.active().and_then(|tab| Some((self.core.doc(tab.doc).ok()?, *tab.selection.nodes.first()?)))) {
            (1, Some((drawing, only))) => select::name_of(drawing, only).0,
            (0, _) => "Drawing".to_owned(),
            (n, _) => format!("{n} Objects"),
        };
        let row = |label: &str, id: &str| Item::action(label, Action::new(id));
        let mut items = Vec::new();
        if picked.count > 0 {
            items.extend([row("Cut", menus::CUT), row("Copy", menus::COPY)]);
        }
        items.push(row("Paste", menus::PASTE));
        if picked.count > 0 {
            items.extend([row("Duplicate", menus::DUPLICATE), Item::danger("Delete", Action::new(menus::DELETE)), Item::Separator]);
        }
        if picked.drawn > 0 {
            items.push(row("Group", menus::GROUP));
            if picked.group {
                items.push(row("Ungroup", menus::UNGROUP));
            }
            items.extend([Item::Separator, row("Bring to Front", menus::TO_FRONT), row("Bring Forward", menus::FORWARD), row("Send Backward", menus::BACKWARD), row("Send to Back", menus::TO_BACK), Item::Separator]);
        }
        if picked.count > 0 {
            items.push(row(if picked.locked { "Unlock" } else { "Lock" }, menus::LOCK));
        } else {
            items.extend([Item::Separator, row("Select All", menus::SELECT_ALL), row("Fit to Window", menus::FIT)]);
        }
        lntrn_ui::ContextMenu::new(&title, at).tab("", items)
    }
}

/// A paste under way: the clipboard's text is asked for in one frame
/// and there in the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pasting {
    /// Asked for by a menu or a key: the next frame asks the system.
    Wanted(DocId),
    /// The system's been asked: this frame has it.
    Asked(DocId),
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    const A: NodeId = NodeId(3);
    const B: NodeId = NodeId(4);
    const C: NodeId = NodeId(5);
    const D: NodeId = NodeId(6);

    /// Back to front under the definitions: a, b, c, d.
    fn doc() -> Document {
        Document::parse(DocId(1), "<svg><defs/><rect id=\"a\"/><rect id=\"b\"/><rect id=\"c\"/><rect id=\"d\"/></svg>").unwrap()
    }

    fn after(nodes: &[NodeId], how: Order) -> Vec<NodeId> {
        let mut d = doc();
        d.apply(&Command::Batch(restack(&d, nodes, how))).unwrap();
        stack(&d, A)
    }

    #[test]
    fn things_go_up_and_down_their_stack_together() {
        assert_eq!(after(&[A], Order::Front), [B, C, D, A]);
        assert_eq!(after(&[A, C], Order::Front), [B, D, A, C]);
        assert_eq!(after(&[D], Order::Back), [D, A, B, C]);
        assert_eq!(after(&[B, D], Order::Back), [B, D, A, C]);
        // One step: past the next thing that isn't one of them.
        assert_eq!(after(&[A], Order::Forward), [B, A, C, D]);
        assert_eq!(after(&[A, B], Order::Forward), [C, A, B, D]);
        assert_eq!(after(&[A, C], Order::Forward), [B, A, D, C]);
        assert_eq!(after(&[C], Order::Backward), [A, C, B, D]);
        assert_eq!(after(&[C, D], Order::Backward), [A, C, D, B]);
        // At the end of the stack already, there's nowhere to go: no
        // step. And "to the back" is over the definitions, not under.
        assert!(restack(&doc(), &[D], Order::Forward).is_empty() && restack(&doc(), &[A], Order::Backward).is_empty());
        let mut d = doc();
        d.apply(&Command::Batch(restack(&d, &[D], Order::Back))).unwrap();
        assert!(d.to_svg().starts_with("<svg><defs/><rect id=\"d\"/>"));
    }
}
