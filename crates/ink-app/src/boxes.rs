//! What the Box holds (ARCHITECTURE §8; LS3's `boxes.rs`): with the
//! Pointer in hand, the selection's place and size to type into or
//! drag along, the selected shape's own rows (`shapebox.rs`), and the
//! Pointer's own setting, "Scale strokes". Under a shape tool, that
//! tool's settings. Other tools' settings come with their tools.
//!
//! A number dragged along is a gesture in the core, like a drag on the
//! canvas: the drawing shows it as it goes, and it lands as one step.
//! One typed in is one step at once.

use ink_core::{Actor, Command, DocId, NodeId};
use ink_doc::arrange;
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{FILL, Ui};

use crate::controls;
use crate::ink::Ink;
use crate::ops::Chosen;
use crate::select;
use crate::shapebox;
use crate::shapes::{self, Kind};
use crate::toolbox;
use crate::tools::Tool;

/// One of the selection's four numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    X,
    Y,
    W,
    H,
}

impl Field {
    const ALL: [Field; 4] = [Field::X, Field::Y, Field::W, Field::H];

    fn label(self) -> &'static str {
        match self {
            Field::X => "X",
            Field::Y => "Y",
            Field::W => "W",
            Field::H => "H",
        }
    }

    fn of(self, b: Rect) -> f64 {
        match self {
            Field::X => b.min.x,
            Field::Y => b.min.y,
            Field::W => b.width(),
            Field::H => b.height(),
        }
    }

    /// What takes the box `was` to having this number be `v`: a move,
    /// or a scale about its top left corner. None where there's nothing
    /// to scale (a box with no width can't be given one).
    pub fn set(self, was: Rect, v: f64) -> Option<Affine> {
        let grow = |from: f64| (from.abs() > 1e-12 && v.abs() > 1e-9).then(|| v / from);
        Some(match self {
            Field::X => Affine::translate(v - was.min.x, 0.0),
            Field::Y => Affine::translate(0.0, v - was.min.y),
            Field::W => Affine::scale(grow(was.width())?, 1.0).about(was.min),
            Field::H => Affine::scale(1.0, grow(was.height())?).about(was.min),
        })
    }
}

/// A number of the Box being dragged along.
#[derive(Clone, Debug, PartialEq)]
pub struct Boxing {
    doc: DocId,
    field: Field,
    /// The selection's box, and what's in it, as the drag began.
    was: Rect,
    nodes: Vec<NodeId>,
}

/// How far a pixel along a number field changes it: a tenth of a unit
/// on an icon's grid, a whole one on a page of hundreds.
pub(crate) fn step_for(page: f64) -> f64 {
    10f64.powf((page.max(1e-6) / 240.0).log10().floor()).clamp(0.001, 100.0)
}

impl Ink {
    /// The Box under a shape tool: a rectangle's corners; a polygon's
    /// sides, and whether it's a star and how deep its points go.
    fn shape_box(&mut self, ui: &mut Ui, canvas: Rect, kind: Kind) {
        let step = self.tabs.active().and_then(|tab| self.core.doc(tab.doc).ok()).map_or(1.0, |drawing| {
            let page = arrange::page_box(drawing);
            step_for(page.width().max(page.height()))
        });
        let s = &mut self.shape_settings;
        let mut laid = shapebox::Laid::new();
        toolbox::draw_with(ui, canvas, &mut self.toolbox, kind.label(), |ui| shapebox::rows(ui, kind, s, step, f64::INFINITY, &mut laid));
        #[cfg(test)]
        {
            self.toolbox.laid = laid;
        }
    }

    /// The Box, over `canvas`: before the canvas takes the pointer, so
    /// a press on it is its own.
    pub(crate) fn the_box(&mut self, ui: &mut Ui, canvas: Rect) {
        // Under a shape tool with settings of its own, those: for the
        // shape it draws next.
        if let Some(kind) = shapes::kind_of(self.tools.active()).filter(|k| matches!(k, Kind::Rect | Kind::Polygon)) {
            return self.shape_box(ui, canvas, kind);
        }
        // Under the Node tool: the anchors picked, and what's done to
        // them.
        if self.tools.active() == Tool::Node {
            return self.node_box(ui, canvas);
        }
        let chosen = if self.tools.active() == Tool::Pointer { self.chosen() } else { None };
        let Some(Chosen { doc, tops, boxed }) = chosen else {
            // No settings to show (yet) for the other tools.
            self.toolbox.gone();
            return;
        };
        let Ok(drawing) = self.core.doc(doc) else { return };
        let title = match tops.as_slice() {
            [] => Tool::Pointer.label().to_owned(),
            [one] => select::name_of(drawing, *one).0,
            several => format!("{} Objects", several.len()),
        };
        let page = arrange::page_box(drawing);
        let joint = arrange::joint(&boxed);
        let step = step_for(page.width().max(page.height()));
        // While a drag on the canvas goes, the numbers read where it
        // has the box now.
        let shown = self.pointing.live.take().filter(|_| joint.is_some()).or(joint);
        let mut values = shown.map(|j| Field::ALL.map(|f| f.of(j)));
        let mut changed: Option<Field> = None;
        let mut scale_strokes = self.settings.scale_strokes;
        let dragging = self.boxing.as_ref().map(|b| b.field);
        // The selected shape's own rows, and what one was set to.
        let own = self.own_shown(doc, &tops);
        let (mut tuned, mut own_laid) = (None, shapebox::Laid::new());
        #[cfg(test)]
        let mut laid = Vec::new();
        toolbox::draw_with(ui, canvas, &mut self.toolbox, &title, |ui| {
            if let Some(values) = values.as_mut() {
                // Two rows of two: where it is, then how big.
                for pair in [[0usize, 1], [2, 3]] {
                    let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
                    let (gap, half) = (ui.m.gap * 2.0, ((row.width() - ui.m.gap * 2.0) / 2.0).floor());
                    for (k, i) in pair.into_iter().enumerate() {
                        let field = Field::ALL[i];
                        let r = Rect::from_xywh(row.min.x + (half + gap) * k as f64, row.min.y, half, row.height());
                        let range = matches!(field, Field::W | Field::H).then_some((step.min(0.001), f64::INFINITY));
                        #[cfg(test)]
                        laid.push((field.label(), r));
                        if controls::number_in(ui, ui.id(field.label()), r, field.label(), &mut values[i], step, range, 3) || dragging == Some(field) {
                            changed = Some(field);
                        }
                    }
                }
            }
            if let Some(own) = &own {
                tuned = Ink::own_rows(ui, own, step, &mut own_laid);
            }
            #[cfg(test)]
            laid.append(&mut own_laid);
            #[cfg(test)]
            laid.push(("Scale strokes", Rect::from_min_size(ui.cursor(), Vec2::new(ui.avail_width(), ui.m.widget_h))));
            controls::toggle(ui, "Scale strokes", &mut scale_strokes);
        });
        #[cfg(test)]
        {
            self.toolbox.laid = laid;
        }
        if scale_strokes != self.settings.scale_strokes {
            self.settings.scale_strokes = scale_strokes;
            self.settings.save();
        }

        // A number let go of: its drag lands.
        let held = ui.state.down;
        if let Some(tune) = tuned {
            self.tune(doc, &tops, tune, held);
        }
        if !held {
            self.tune_settled();
        }
        if let Some(boxing) = self.boxing.take_if(|_| !held) {
            if let Err(e) = self.core.commit(boxing.doc, if matches!(boxing.field, Field::X | Field::Y) { "Move" } else { "Scale" }) {
                let why = e.to_string();
                let said = self.core.doc(boxing.doc).map_or(why.clone(), |d| crate::edits::in_row_names(d, &why));
                self.toast(said);
            }
            return;
        }
        let (Some(field), Some(values), Some(joint)) = (changed, values, joint) else { return };
        let v = values[Field::ALL.iter().position(|f| *f == field).unwrap_or(0)];
        let nodes: Vec<NodeId> = boxed.iter().map(|(id, _)| *id).collect();
        // From the box as the drag began (or as it is, for one typed).
        let (was, nodes) = match &self.boxing {
            Some(b) if b.field == field && b.doc == doc => (b.was, b.nodes.clone()),
            _ => (joint, nodes),
        };
        let Some(by) = field.set(was, v) else { return };
        let command = match field {
            Field::X | Field::Y => Command::Transform { nodes: nodes.clone(), by },
            Field::W | Field::H if self.settings.scale_strokes => Command::Transform { nodes: nodes.clone(), by },
            Field::W | Field::H => Command::Resize { nodes: nodes.clone(), by },
        };
        if !held {
            // Typed in: done at once.
            self.edit(doc, &command, if matches!(field, Field::X | Field::Y) { "Move" } else { "Scale" });
            return;
        }
        if self.boxing.is_none() {
            if self.core.begin(doc, Actor::Alva).is_err() {
                return;
            }
            self.boxing = Some(Boxing { doc, field, was, nodes });
        }
        // What it comes to so far; a refusal waits for the drag's end
        // to be said.
        let _ = self.core.update(doc, &command);
        self.pointing.carried = Some((doc, by));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_set_is_a_move_or_a_scale_about_the_top_left() {
        let was = Rect::from_xywh(4.0, 6.0, 10.0, 5.0);
        let at = |f: Field, v: f64, p: Vec2| f.set(was, v).unwrap().apply(p);
        assert_eq!(at(Field::X, 7.0, was.min), Vec2::new(7.0, 6.0));
        assert_eq!(at(Field::Y, -1.0, was.max), Vec2::new(14.0, 4.0));
        assert_eq!((at(Field::W, 20.0, was.max), at(Field::W, 20.0, was.min)), (Vec2::new(24.0, 11.0), was.min));
        assert_eq!(at(Field::H, 2.5, was.max), Vec2::new(14.0, 8.5));
        // Nothing gives a flat box a size, and nothing is made flat.
        assert!(Field::W.set(Rect::from_xywh(0.0, 0.0, 0.0, 5.0), 3.0).is_none() && Field::H.set(was, 0.0).is_none());
        assert_eq!(Field::ALL.map(|f| f.of(was)), [4.0, 6.0, 10.0, 5.0]);
    }

    #[test]
    fn a_pixel_along_a_number_is_a_step_that_suits_the_page() {
        assert_eq!((step_for(24.0), step_for(48.0), step_for(512.0), step_for(2400.0)), (0.1, 0.1, 1.0, 10.0));
    }
}
