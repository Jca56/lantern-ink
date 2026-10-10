//! The shape tools on the canvas (ARCHITECTURE §8; LS3's `shaping.rs`):
//! a drag draws one new shape, always, on top of the level the Pointer
//! is in. It's a gesture in the core: the drawing shows the shape as
//! it's dragged out, and it lands as one step. The shape just drawn is
//! selected, and the tool stays in hand for the next.

use ink_core::{Actor, Command, DocId, Place};
use ink_doc::{Precision, elements, geometry};
use ink_geom::Affine;
use lntrn_math::Vec2;
use lntrn_ui::Ui;

use crate::canvas::CanvasInput;
use crate::edits::in_row_names;
use crate::handles::Keys;
use crate::ink::Ink;
use crate::pointer::View;
use crate::shapes::{self, Kind};

/// How short a drag, logical px, draws nothing.
const DRAW_MIN: f64 = 3.0;

/// A new shape being dragged out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shaping {
    doc: DocId,
    kind: Kind,
    /// Where the drag began, in the drawing's coordinates.
    from: Vec2,
}

impl Ink {
    /// Give up a shape being dragged out: as if it never began.
    pub(crate) fn drop_shape(&mut self) {
        if let Some(shaping) = self.shaping.take() {
            self.core.cancel(shaping.doc);
        }
    }

    /// One frame of the shape tool that draws `kind`, on `doc`.
    pub(crate) fn shape_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, kind: Kind) {
        // One begun on another drawing, or with another tool: given up.
        if self.shaping.is_some_and(|s| s.doc != doc || s.kind != kind) {
            self.drop_shape();
        }
        // On the grid, so a shape's numbers are plain ones, or on a line
        // of what's there already (`snap.rs`); freely, with Ctrl.
        let lines = if input.pressed || self.shaping.is_some() { self.snap_to(ui, view, doc, &[]) } else { None };
        let mut at = view.to_doc.apply(ui.state.pointer);
        if let Some(lines) = &lines {
            (at, self.landed) = lines.point(at);
        }
        let grid = lines.map_or(0.0, |lines| lines.step);
        if input.pressed && self.shaping.is_none() && self.core.begin(doc, Actor::Alva).is_ok() {
            self.shaping = Some(Shaping { doc, kind, from: at });
        }
        let Some(shaping) = self.shaping else { return };
        let keys = Keys { shift: ui.state.mods.shift(), alt: ui.state.mods.alt() };
        let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);
        let Ok(drawing) = self.core.doc(doc) else { return };
        // On top of the level the Pointer is in, in that level's own
        // coordinates.
        let context = self.tabs.iter().find(|t| t.doc == doc).map_or(drawing.root(), |tab| tab.selection.context(drawing));
        let into = geometry::to_doc(drawing, context).and_then(|t| t.inverse()).unwrap_or(Affine::IDENTITY);
        // (Half a step of the grid at least: two corners on one crossing
        // are no shape.)
        let command = match shapes::dragged(kind, shaping.from, at, keys, (DRAW_MIN * view.scale / per_unit).max(grid / 2.0)) {
            Some((a, b)) => {
                let markup = shapes::markup(kind, into.apply(a), into.apply(b), keys.shift, &self.shape_settings, &self.paints, &Precision::of(drawing));
                match elements(&markup) {
                    Ok(elements) => Command::Insert { place: Place::LastIn(context), elements },
                    Err(_) => Command::Batch(Vec::new()),
                }
            }
            // Not dragged far enough to be anything yet.
            None => Command::Batch(Vec::new()),
        };
        let shown = self.core.update(doc, &command);
        if !input.released && input.held {
            return;
        }
        // Let go: it lands, and it's what's selected.
        self.shaping = None;
        match shown.and(self.core.commit(doc, kind.label())) {
            Ok(applied) => {
                if let Some(&made) = applied.created.first() {
                    if let (Ok(drawing), Some(tab)) = (self.core.doc(doc), self.tabs.iter_mut().find(|t| t.doc == doc)) {
                        tab.selection.select_only(made);
                        tab.selection.reveal(drawing, made);
                    }
                    self.tree.show(made);
                }
            }
            Err(e) => {
                let why = e.to_string();
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
            }
        }
    }
}
