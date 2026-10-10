//! The canvas's keys that aren't a menu's: the arrows, which move
//! what's picked, and Escape, which backs out of whatever is under
//! way, one thing at a time.

use ink_core::{Command, NodeId};
use ink_geom::Affine;
use lntrn_math::Vec2;

use crate::ink::Ink;

impl Ink {
    /// The arrow keys: the selection moved `by`, in the drawing's
    /// units.
    pub(crate) fn nudge(&mut self, by: Vec2) {
        // The Node tool's anchors, where it has some picked.
        if self.anchors_in_hand() {
            return self.nudge_anchors(by);
        }
        let Some(tab) = self.tabs.active() else { return };
        let doc = tab.doc;
        let Ok(drawing) = self.core.doc(doc) else { return };
        // What's drawn of it: a definition has nowhere to go.
        let nodes: Vec<NodeId> = tab.selection.tops(drawing).into_iter().filter(|&id| crate::select::is_drawn(drawing, id)).collect();
        if !nodes.is_empty() {
            self.edit(doc, &Command::Transform { nodes, by: Affine::translate(by.x, by.y) }, "Nudge");
        }
    }

    /// Escape: out of a drag; else the Node tool's anchors let go; else
    /// out of the group the Pointer is in, one level, with that group
    /// picked; else nothing picked.
    pub(crate) fn escape(&mut self) {
        if self.shaping.is_some() {
            return self.drop_shape();
        }
        if self.pointing.busy() {
            return self.drop_drag();
        }
        if self.noding.busy() {
            return self.drop_nodes();
        }
        if self.penning.busy() {
            return self.drop_pen();
        }
        if self.grading.busy() {
            return self.drop_grade();
        }
        // A text typed into is let go of (what was typed has landed).
        if self.typing() {
            return self.type_done();
        }
        // The Node tool and the Pen let go of their anchors (and the Pen
        // of a first point not yet a path) before anything else.
        let tool = self.tools.active();
        if matches!(tool, crate::tools::Tool::Node | crate::tools::Tool::Pen) && (self.penning.forget() | self.noding.unpick()) {
            return;
        }
        let Some(tab) = self.tabs.active_mut() else { return };
        let Ok(drawing) = self.core.doc(tab.doc) else { return };
        let sel = &mut tab.selection;
        let context = sel.context(drawing);
        if context == drawing.root() {
            sel.clear();
            sel.within = None;
        } else {
            sel.within = drawing.get(context).and_then(|n| n.parent).filter(|&up| up != drawing.root());
            sel.select_only(context);
        }
    }
}
