//! The window's side of snapping (`snap.rs` has what lands where): the
//! lines a drag on a drawing lands on, and the two switches of the View
//! menu. A tool asks for its lines before it reads the drawing
//! (`Ink::snap_to`), lands its point or its box on them, and leaves
//! what it landed on in `Ink::landed`, drawn over the canvas for that
//! frame; so the lines are gone when the drag is. Guides that show are
//! lines too (`guiding.rs`).
//!
//! View > Snapping turns it all off; Ctrl holds it off while it's down.

use std::collections::HashMap;

use ink_core::{DocId, NodeId};
use ink_doc::guides::Guide;
use ink_doc::outline::AnchorId;
use ink_doc::{Document, Kind};
use lntrn_math::{Rect, Vec2};
use lntrn_ui::Ui;

use crate::ink::Ink;
use crate::nodes::Shown;
use crate::pointer::View;
use crate::shapes;
use crate::snap::{self, REACH, Targets};

/// The lines last worked out, and what they were worked out for: a
/// drag asks for the same ones every frame.
pub(crate) struct Kept {
    doc: DocId,
    stamp: u64,
    level: NodeId,
    skip: Vec<NodeId>,
    per_unit: f64,
    /// The grid, the shapes, the guides: which are snapped to.
    parts: (bool, bool, bool),
    targets: Targets,
}

/// The lines of the shapes of `drawing` (whose boxes are `boxes`): the
/// edges and middle of every shape and text that shows, and of
/// whatever stands at `level` (the group the Pointer is in: its groups
/// are things there). Not of `skip` (what's being dragged) nor of
/// what's in them.
pub fn of_shapes(drawing: &Document, boxes: &HashMap<NodeId, Rect>, level: NodeId, skip: &[NodeId]) -> Targets {
    let mut targets = Targets::default();
    for (&id, &r) in boxes {
        let Some(node) = drawing.get(id) else { continue };
        let stands = node.kind.is_shape() || node.kind == Kind::Text || node.parent == Some(level);
        if stands && id != drawing.root() && !skip.contains(&id) && !drawing.ancestors(id).any(|n| skip.contains(&n.id)) {
            targets.add_box(r);
        }
    }
    targets
}

/// The anchors of the shapes `shown` as lines to land on too: level
/// with one, or straight under it. Not the ones `going` (they're what's
/// dragged).
pub fn add_anchors(lines: &mut Targets, shown: &[Shown], going: &[(NodeId, &[AnchorId])]) {
    for sh in shown {
        let gone = going.iter().find(|(node, _)| *node == sh.node).map_or(&[][..], |(_, ids)| ids);
        for anchor in sh.outline.runs.iter().flat_map(|run| &run.anchors).filter(|a| !gone.contains(&a.id)) {
            lines.add_point(sh.to_doc.apply(anchor.at));
        }
    }
    lines.settle();
}

impl Ink {
    /// What a drag on `doc` lands on, as `view` shows it: the grid, the
    /// page, the shapes but for `skip` (what's dragged), and the guides
    /// that show. None while snapping is off, or Ctrl is down.
    pub(crate) fn snap_to(&mut self, ui: &Ui, view: &View, doc: DocId, skip: &[NodeId]) -> Option<Targets> {
        self.snap_lines(ui, view, doc, skip, true)
    }

    /// [`Ink::snap_to`], with the guides or (for a guide dragged)
    /// without.
    pub(crate) fn snap_lines(&mut self, ui: &Ui, view: &View, doc: DocId, skip: &[NodeId], guides: bool) -> Option<Targets> {
        if !self.settings.snapping || ui.state.mods.ctrl() {
            return None;
        }
        let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);
        let parts = (self.settings.snap_grid, self.settings.snap_shapes, guides && self.settings.guides && self.settings.snap_guides);
        let drawing = self.core.doc(doc).ok()?;
        let stamp = self.core.history(doc).ok()?.stamp();
        let tab = self.tabs.iter_mut().find(|t| t.doc == doc)?;
        let level = tab.selection.context(drawing);
        if let Some(kept) = &self.snaps
            && (kept.doc, kept.stamp, kept.level, kept.per_unit, kept.parts) == (doc, stamp, level, per_unit, parts)
            && kept.skip == skip
        {
            return Some(kept.targets.clone());
        }
        let page = ink_doc::arrange::page_box(drawing);
        let mut targets = Targets::default();
        if parts.1 {
            targets = of_shapes(drawing, tab.boxes(drawing, stamp), level, skip);
            targets.add_box(page);
        }
        if parts.0 {
            targets.step = snap::step_for(shapes::grid_for(page.width().max(page.height())), per_unit, view.scale);
        }
        if parts.2 {
            for guide in ink_doc::guides::of(drawing) {
                match guide {
                    Guide::X(x) => targets.xs.push(x),
                    Guide::Y(y) => targets.ys.push(y),
                }
            }
        }
        targets.reach = REACH * view.scale / per_unit;
        targets.settle();
        self.snaps = Some(Kept { doc, stamp, level, skip: skip.to_vec(), per_unit, parts, targets: targets.clone() });
        Some(targets)
    }

    /// View > Snapping: on or off, remembered.
    pub(crate) fn toggle_snapping(&mut self) {
        self.settings.snapping = !self.settings.snapping;
        self.settings.save();
    }

    /// View > Pixel Grid: shown or not, remembered.
    pub(crate) fn toggle_pixel_grid(&mut self) {
        self.settings.pixel_grid = !self.settings.pixel_grid;
        self.settings.save();
    }
}
