//! What's done to the anchors picked (ARCHITECTURE §8): made smooth or
//! corners, a path parted at them, two loose ends joined, taken out, a
//! new one put on a segment. Each is one Command (`anchors.rs` says
//! which) and one step, from the Node tool's Box, its right-click menu,
//! its keys, or a double click on the canvas (`noding.rs`).

use std::collections::HashMap;

use ink_core::{DocId, NodeId};
use ink_doc::geometry;
use ink_doc::outline::AnchorId;
use lntrn_math::{Rect, Vec2};
use lntrn_props::Value;
use lntrn_ui::{Action, ContextMenu, FILL, Item, Ui};

use crate::anchors::{self, Picked};
use crate::controls;
use crate::ink::Ink;
use crate::menus::NODE_OP;
use crate::shapebox::Laid;
use crate::toolbox;
use crate::tools::Tool;

/// Something done to the anchors picked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeOp {
    Smooth,
    Corner,
    Break,
    Join,
    Delete,
    /// A new anchor on the segment after this anchor of this shape,
    /// this far along it.
    Add(NodeId, AnchorId, f64),
}

impl NodeOp {
    /// What a menu's row calls it; `Add` is the segment the menu opened
    /// on.
    const NAMED: [(&'static str, NodeOp); 5] = [("smooth", NodeOp::Smooth), ("corner", NodeOp::Corner), ("break", NodeOp::Break), ("join", NodeOp::Join), ("delete", NodeOp::Delete)];
    const ADD: &'static str = "add";
}

/// What there is to do with the anchors picked: which of the Box's
/// buttons are lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Can {
    pub any: bool,
    pub part: bool,
    pub join: bool,
}

impl Ink {
    /// Whether a key is the picked anchors' to take: the Node tool is
    /// in hand, and has some of the drawing that shows.
    pub(crate) fn anchors_in_hand(&self) -> bool {
        self.tools.active() == Tool::Node && !self.noding.picked.is_empty() && self.noding.of == self.tabs.active_doc()
    }

    pub(crate) fn node_can(&self) -> Can {
        let n = &self.noding;
        let Some(drawing) = n.of.filter(|doc| Some(*doc) == self.tabs.active_doc()).and_then(|doc| self.core.doc(doc).ok()) else { return Can::default() };
        Can { any: !n.picked.is_empty(), part: anchors::broken(drawing, &n.would_be, &n.picked).is_some(), join: anchors::joined(drawing, &n.would_be, &n.picked).is_some() }
    }

    /// Do `op` to the anchors picked: one step. What's picked afterwards
    /// is what it left, or made.
    pub(crate) fn node_op(&mut self, op: NodeOp) {
        let Some((doc, drawing)) = self.noding.of.and_then(|doc| Some((doc, self.core.doc(doc).ok()?))) else { return };
        let (would_be, picked) = (&self.noding.would_be, &self.noding.picked);
        let plain = |command| (command, HashMap::new());
        let (made, label) = match op {
            NodeOp::Smooth => (anchors::smoothed(drawing, would_be, picked, true), "Smooth"),
            NodeOp::Corner => (anchors::smoothed(drawing, would_be, picked, false), "Corner"),
            NodeOp::Break => (anchors::broken(drawing, would_be, picked), "Break Path"),
            NodeOp::Join => (anchors::joined(drawing, would_be, picked), "Join"),
            NodeOp::Delete => (anchors::deleted(drawing, would_be, picked).map(plain), if picked.len() == 1 { "Delete Anchor" } else { "Delete Anchors" }),
            NodeOp::Add(node, after, share) => (anchors::added(drawing, would_be, node, after, share).map(plain), "Add Anchor"),
        };
        let Some((command, names)) = made else { return };
        let Some(applied) = self.edit(doc, &command, label) else { return };
        match op {
            NodeOp::Delete => self.noding.picked.clear(),
            // The new one, to be dragged where it's wanted.
            NodeOp::Add(node, ..) => self.noding.picked = applied.anchors.first().map(|made| (node, *made)).into_iter().collect(),
            _ => anchors::renamed(&mut self.noding.picked, &names),
        }
    }

    /// A row of the Node tool's menu.
    pub(crate) fn node_op_named(&mut self, name: &str) {
        let add = self.noding.target.take().filter(|_| name == NodeOp::ADD).map(|(node, after, share)| NodeOp::Add(node, after, share));
        match add.or(NodeOp::NAMED.iter().find(|(named, _)| *named == name).map(|(_, op)| *op)) {
            Some(op) => self.node_op(op),
            None => lntrn_core::log_error!("no such thing to do to an anchor: {name}"),
        }
    }

    /// The arrow keys: the anchors picked moved `by`, in the drawing's
    /// units.
    pub(crate) fn nudge_anchors(&mut self, by: Vec2) {
        let Some((doc, drawing)) = self.noding.of.and_then(|doc| Some((doc, self.core.doc(doc).ok()?))) else { return };
        let nodes: Vec<NodeId> = self.noding.picked.iter().map(|(node, _)| *node).collect();
        let (_, names) = anchors::firsts(drawing, &self.noding.would_be, &nodes);
        let Some(command) = anchors::moved(drawing, &self.noding.would_be, &self.noding.picked, by) else { return };
        if self.edit(doc, &command, "Nudge").is_some() {
            anchors::renamed(&mut self.noding.picked, &names);
        }
    }

    /// Where the anchor `picked` is, in the drawing's coordinates: as
    /// the drawing has it, or (`live`) as a drag under way shows it.
    fn anchor_place(&self, doc: DocId, (node, id): Picked, live: bool) -> Option<Vec2> {
        let drawing = if live { self.core.shown(doc).ok()?.0 } else { self.core.doc(doc).ok()? };
        let outline = anchors::outline_of(drawing, &self.noding.would_be, node)?;
        let at = outline.anchors().find(|a| a.id == id)?.at;
        Some(geometry::to_doc(drawing, node)?.apply(at))
    }

    /// The menu a right-click opens with the Node tool, at `at`: what
    /// there is to do with the anchors picked, and, where it was on a
    /// segment (`on_segment`), an anchor put there.
    pub(crate) fn node_menu(&self, at: Vec2, on_segment: bool) -> ContextMenu {
        let (can, count) = (self.node_can(), self.noding.picked.len());
        let row = |label: &str, op: &str| Item::action(label, Action::new(NODE_OP).with("op", Value::Str(op.to_owned())));
        let mut items = Vec::new();
        if on_segment {
            items.push(row("Add Anchor Here", NodeOp::ADD));
        }
        if can.any {
            if on_segment {
                items.push(Item::Separator);
            }
            items.extend([row("Smooth", "smooth"), row("Corner", "corner")]);
            if can.part {
                items.push(row("Break Path", "break"));
            }
            if can.join {
                items.push(row("Join", "join"));
            }
            items.extend([Item::Separator, Item::danger(if count == 1 { "Delete Anchor" } else { "Delete Anchors" }, Action::new(NODE_OP).with("op", Value::Str("delete".to_owned())))]);
        }
        let title = match count {
            0 => "Segment".to_owned(),
            1 => "Anchor".to_owned(),
            n => format!("{n} Anchors"),
        };
        ContextMenu::new(&title, at).tab("", items)
    }

    /// The Box under the Node tool: where the one anchor picked is, to
    /// type or drag along; and what's done to the anchors picked, each
    /// greyed while there's nothing for it to do.
    pub(crate) fn node_box(&mut self, ui: &mut Ui, canvas: Rect) {
        let Some(doc) = self.tabs.active_doc() else { return self.toolbox.gone() };
        let can = self.node_can();
        let one = match self.noding.picked.as_slice() {
            [one] if self.noding.of == Some(doc) => Some(*one),
            _ => None,
        };
        let mut place = one.and_then(|one| self.anchor_place(doc, one, true));
        let step = self.core.doc(doc).map_or(1.0, |drawing| {
            let page = ink_doc::arrange::page_box(drawing);
            crate::boxes::step_for(page.width().max(page.height()))
        });
        let (mut moved, mut op) = (false, None);
        let mut laid = Laid::new();
        toolbox::draw_with(ui, canvas, &mut self.toolbox, Tool::Node.label(), |ui| {
            if let Some(place) = place.as_mut() {
                // Where it is: across, and down.
                let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
                let (gap, half) = (ui.m.gap * 2.0, ((row.width() - ui.m.gap * 2.0) / 2.0).floor());
                for (k, (name, value)) in [("X", &mut place.x), ("Y", &mut place.y)].into_iter().enumerate() {
                    let r = Rect::from_xywh(row.min.x + (half + gap) * k as f64, row.min.y, half, row.height());
                    laid.push((name, r));
                    moved |= controls::number_in(ui, ui.id(name), r, name, value, step, None, 3);
                }
            }
            for pair in [[("Smooth", NodeOp::Smooth, can.any), ("Corner", NodeOp::Corner, can.any)], [("Break", NodeOp::Break, can.part), ("Join", NodeOp::Join, can.join)]] {
                ui.columns(&[FILL; 2], |ui, i| {
                    let (name, does, on) = pair[i];
                    let (clicked, rect) = controls::button_if(ui, name, on);
                    laid.push((name, rect));
                    if clicked {
                        op = Some(does);
                    }
                });
            }
            let (clicked, rect) = controls::button_if(ui, "Delete", can.any);
            laid.push(("Delete", rect));
            if clicked {
                op = Some(NodeOp::Delete);
            }
        });
        #[cfg(test)]
        {
            self.toolbox.laid = laid;
        }
        // A number dragged along is a gesture, landing when it's let go
        // of; one typed is a step at once. From where the anchor is in
        // the drawing itself.
        let held = ui.state.down;
        let from = one.zip(place).filter(|_| moved).and_then(|(one, to)| Some((one, to - self.anchor_place(doc, one, false)?)));
        if let Some((one, by)) = from
            && let Some(command) = self.core.doc(doc).ok().and_then(|drawing| anchors::moved(drawing, &self.noding.would_be, &[one], by))
        {
            self.box_set(doc, &command, "Move Anchor", held);
        }
        if !held {
            self.tune_settled();
        }
        if let Some(op) = op {
            self.node_op(op);
        }
    }
}
