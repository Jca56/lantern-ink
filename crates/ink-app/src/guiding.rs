//! Guides in the window (`docs/M4.md`, slice f; D19). A guide is
//! dragged out of a ruler (the top one gives a line across the page,
//! the left one a line down it), lands as anything dragged lands (on
//! the grid, or on a shape's line), and stays in the file
//! (`ink_doc::guides`). With the Pointer, one is taken hold of where
//! it crosses bare canvas (where it crosses a shape, a press is the
//! shape's); dragged back onto a ruler, or off the canvas, it's gone.
//! Each is one step: "Add Guide", "Move Guide", "Remove Guide".
//!
//! View > Guides hides them: hidden, nothing lands on them and none is
//! taken hold of. Dragging a new one out shows them again.

use ink_core::{Command, DocId};
use ink_doc::guides::{self, Guide};
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui};

use crate::canvas::CanvasInput;
use crate::handles::{self, HIT, TURN};
use crate::ink::Ink;
use crate::picking::top_at;
use crate::pointer::View;
use crate::theme::ACCENT;

/// How near a guide, logical px, a press takes hold of it.
pub const GRAB: f64 = 6.0;

/// A guide being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    /// Which of the drawing's guides it is (its place among them);
    /// none for a new one, out of a ruler.
    which: Option<usize>,
    /// Where the drag has it.
    guide: Guide,
    /// The pointer is off the canvas: let go here, it's gone (a new
    /// one never was).
    off: bool,
}

/// What the window keeps of guides between frames.
#[derive(Default)]
pub struct Guiding {
    drag: Option<(DocId, Held)>,
    /// The guides as they show this frame, and whether each is in hand
    /// or under the pointer.
    shown: Vec<(Guide, bool)>,
}

impl Guiding {
    /// A guide is being dragged: the view holds still, and no key does
    /// anything but Escape.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }

    /// Give up a guide being dragged: it's where it was, or never was.
    pub fn drop_drag(&mut self) {
        self.drag = None;
    }
}

/// `guides` with each place said once: one dragged onto another is the
/// one guide.
fn once(guides: Vec<Guide>) -> Vec<Guide> {
    let mut out: Vec<Guide> = Vec::with_capacity(guides.len());
    for g in guides {
        let same = |h: &Guide| std::mem::discriminant(h) == std::mem::discriminant(&g) && (h.at() - g.at()).abs() < 1e-9;
        if !out.iter().any(same) {
            out.push(g);
        }
    }
    out
}

/// The guides `all` after the drag `held` is let go, and what the step
/// is called; none where it changes nothing.
fn landed(mut all: Vec<Guide>, held: Held) -> Option<(Vec<Guide>, &'static str)> {
    let label = match (held.which, held.off) {
        (None, true) => return None,
        (None, false) => {
            all.push(held.guide);
            "Add Guide"
        }
        (Some(i), true) if i < all.len() => {
            all.remove(i);
            "Remove Guide"
        }
        (Some(i), false) if i < all.len() && all[i] != held.guide => {
            all[i] = held.guide;
            "Move Guide"
        }
        _ => return None,
    };
    Some((once(all), label))
}

impl Ink {
    /// Whether the pointer is on bare canvas, as the Pointer has it:
    /// on nothing drawn, and clear of the selection's box and its
    /// handles.
    fn on_bare_canvas(&mut self, view: &View, doc: DocId, pointer: Vec2) -> bool {
        let Ok(drawing) = self.core.doc(doc) else { return false };
        let stamp = self.core.history(doc).map_or(0, |h| h.stamp());
        let Some(tab) = self.tabs.iter_mut().find(|t| t.doc == doc) else { return false };
        let tops = tab.selection.tops(drawing);
        let boxes = tab.boxes(drawing, stamp);
        let joint = tops.iter().filter_map(|id| boxes.get(id).copied()).reduce(|a, b| a.union(&b));
        let s = view.scale;
        let on_box = joint.is_some_and(|j| handles::hit(view.to_window.bounds(&j), pointer, HIT * s, TURN * s).is_some());
        let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);
        !on_box && top_at(drawing, view.to_doc.apply(pointer), 3.0 * s / per_unit).is_none()
    }

    /// One frame of the guides of `doc`: a new one out of a ruler, one
    /// dragged with the Pointer (`pointing`: it's in hand), and what
    /// shows. Whether a guide has the pointer this frame: then it's no
    /// tool's.
    pub(crate) fn guide_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, pointing: bool) -> bool {
        let (l, s, pointer) = (self.layout, view.scale, ui.state.pointer);
        let area = l.canvas;
        if self.guiding.drag.is_some_and(|(on, _)| on != doc) {
            self.guiding.drag = None;
        }
        let all = self.core.doc(doc).map(guides::of).unwrap_or_default();
        let mut taken = self.guiding.drag.is_some();
        let mut hover = None;
        // A press on a ruler: a new guide, of the kind that runs along
        // that ruler.
        for (name, rect, guide, icon) in [("ruler-top", l.ruler_top, Guide::Y(0.0), CursorIcon::NsResize), ("ruler-left", l.ruler_left, Guide::X(0.0), CursorIcon::EwResize)] {
            if rect.is_empty() {
                continue;
            }
            let resp = ui.interact(ui.id(name), rect, Sense::DRAG);
            if resp.hovered && self.guiding.drag.is_none() {
                ui.state.cursor_icon = icon;
            }
            if resp.pressed && self.guiding.drag.is_none() {
                self.guiding.drag = Some((doc, Held { which: None, guide, off: true }));
                // (They show, to be dragged out.)
                if !self.settings.guides {
                    self.settings.guides = true;
                    self.settings.save();
                }
                taken = true;
            }
        }
        // With the Pointer over a guide that shows, on bare canvas: a
        // press takes hold of it.
        if self.guiding.drag.is_none() && pointing && self.settings.guides && input.over && !all.is_empty() {
            let far = |g: &Guide| match g {
                Guide::X(x) => (view.to_window.apply(Vec2::new(*x, 0.0)).x - pointer.x).abs(),
                Guide::Y(y) => (view.to_window.apply(Vec2::new(0.0, *y)).y - pointer.y).abs(),
            };
            let near = all.iter().enumerate().filter(|(_, g)| far(g) <= GRAB * s).min_by(|a, b| far(a.1).total_cmp(&far(b.1))).map(|(i, _)| i);
            if let Some(i) = near.filter(|_| self.on_bare_canvas(view, doc, pointer)) {
                hover = Some(i);
                ui.state.cursor_icon = if matches!(all[i], Guide::X(_)) { CursorIcon::EwResize } else { CursorIcon::NsResize };
                if input.pressed {
                    self.guiding.drag = Some((doc, Held { which: Some(i), guide: all[i], off: false }));
                    taken = true;
                }
            }
        }
        // One in hand: where the pointer has it, landed like anything
        // dragged (but not on the other guides: it would never leave
        // where it was).
        if let Some((_, mut held)) = self.guiding.drag {
            let at = view.to_doc.apply(pointer);
            let lines = self.snap_lines(ui, view, doc, &[], false);
            let land = |across: bool, v: f64| lines.as_ref().map_or((v, crate::snap::Landed::default()), |lines| lines.edge(across, v));
            let (guide, on) = match held.guide {
                Guide::X(_) => {
                    let (x, on) = land(true, at.x);
                    (Guide::X(x), on)
                }
                Guide::Y(_) => {
                    let (y, on) = land(false, at.y);
                    (Guide::Y(y), on)
                }
            };
            held = Held { guide, off: !area.contains(pointer), ..held };
            ui.state.cursor_icon = if matches!(guide, Guide::X(_)) { CursorIcon::EwResize } else { CursorIcon::NsResize };
            if ui.state.down {
                if !held.off {
                    self.landed = on;
                }
                self.guiding.drag = Some((doc, held));
            } else {
                self.guiding.drag = None;
                if let Some((guides, label)) = landed(all.clone(), held) {
                    self.edit(doc, &Command::SetGuides { guides }, label);
                }
            }
        }
        // What shows: the drawing's, with the one in hand where the
        // drag has it.
        let all = self.core.doc(doc).map(guides::of).unwrap_or_default();
        let held = self.guiding.drag.map(|(_, held)| held);
        let mut shown: Vec<(Guide, bool)> = all.iter().enumerate().filter(|(i, _)| held.is_none_or(|h| h.which != Some(*i))).map(|(i, g)| (*g, hover == Some(i))).collect();
        if let Some(held) = held.filter(|h| !h.off) {
            shown.push((held.guide, true));
        }
        if !self.settings.guides {
            shown.clear();
        }
        self.guiding.shown = shown;
        taken
    }

    /// The guides, over the canvas `area`: a line right across it each.
    pub(crate) fn draw_guides(&self, ui: &mut Ui, area: Rect, view: &View) {
        let w = (1.5 * ui.m.scale).round().max(1.0);
        ui.draw.push_clip(area);
        for (guide, lit) in &self.guiding.shown {
            let (tint, w) = if *lit { (ACCENT, w * 2.0) } else { (self.settings.guide_color(), w) };
            match guide {
                Guide::X(x) => {
                    let at = view.to_window.apply(Vec2::new(*x, 0.0)).x.round();
                    ui.draw.rect(Rect::from_xywh(at - (w / 2.0).floor(), area.min.y, w, area.height()), tint);
                }
                Guide::Y(y) => {
                    let at = view.to_window.apply(Vec2::new(0.0, *y)).y.round();
                    ui.draw.rect(Rect::from_xywh(area.min.x, at - (w / 2.0).floor(), area.width(), w), tint);
                }
            }
        }
        ui.draw.pop_clip();
    }

    /// View > Guides: shown or hidden, remembered.
    pub(crate) fn toggle_guides(&mut self) {
        self.settings.guides = !self.settings.guides;
        self.settings.save();
    }

    /// View > Rulers: shown or hidden, remembered.
    pub(crate) fn toggle_rulers(&mut self) {
        self.settings.rulers = !self.settings.rulers;
        self.settings.save();
    }

    /// View > Clear Guides: every guide of the drawing that shows goes,
    /// as one step.
    pub(crate) fn clear_guides(&mut self) {
        if let Some(doc) = self.tabs.active_doc() {
            self.edit(doc, &Command::SetGuides { guides: Vec::new() }, "Clear Guides");
        }
    }

    /// Whether the drawing that shows has any guides (to clear).
    pub(crate) fn has_guides(&self) -> bool {
        self.tabs.active_doc().and_then(|doc| self.core.doc(doc).ok()).is_some_and(|d| !guides::of(d).is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_let_go_adds_moves_or_removes_a_guide() {
        let all = vec![Guide::X(2.0), Guide::Y(4.0), Guide::X(22.0)];
        let held = |which: Option<usize>, guide: Guide, off: bool| Held { which, guide, off };
        // A new one, let go on the canvas; let go off it, it never was.
        assert_eq!(landed(all.clone(), held(None, Guide::Y(12.0), false)), Some((vec![Guide::X(2.0), Guide::Y(4.0), Guide::X(22.0), Guide::Y(12.0)], "Add Guide")));
        assert_eq!(landed(all.clone(), held(None, Guide::Y(12.0), true)), None);
        // One of the drawing's: moved, put back where it was, taken off.
        assert_eq!(landed(all.clone(), held(Some(1), Guide::Y(6.5), false)), Some((vec![Guide::X(2.0), Guide::Y(6.5), Guide::X(22.0)], "Move Guide")));
        assert_eq!(landed(all.clone(), held(Some(1), Guide::Y(4.0), false)), None);
        assert_eq!(landed(all.clone(), held(Some(0), Guide::X(9.0), true)), Some((vec![Guide::Y(4.0), Guide::X(22.0)], "Remove Guide")));
        // Dragged onto another, it's the one guide; a line down at 4 and
        // one across at 4 are two.
        assert_eq!(landed(all.clone(), held(Some(2), Guide::X(2.0), false)), Some((vec![Guide::X(2.0), Guide::Y(4.0)], "Move Guide")));
        assert_eq!(landed(all.clone(), held(None, Guide::X(4.0), false)).unwrap().0.len(), 4);
        // One that's gone meanwhile is nothing.
        assert_eq!(landed(all, held(Some(7), Guide::X(4.0), false)), None);
    }
}
