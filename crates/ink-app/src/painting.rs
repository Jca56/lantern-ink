//! Paint, from the window (ARCHITECTURE §8): the paint section shows
//! what the selection is painted with (or, with nothing selected, what
//! the next shape drawn will be), and a paint it sets goes on every
//! shape and text of the selection as one step. A colour dragged in
//! the picker is a gesture in the core, like a drag on the canvas: the
//! drawing shows it as it goes, and it lands as one step when the
//! button comes up.

use ink_core::{Actor, Command, DocId, NodeId};
use lntrn_math::{Rect, Vec2};
use lntrn_props::Value;
use lntrn_ui::{Action, ContextMenu, Item, Ui};

use crate::colour::palettes::to_hex;
use crate::colour::section;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::menus::PALETTE_OP;
use crate::paint::{self, Paint, Paints, Which};

/// A paint being dragged to (in the picker, or pressed in the grid).
#[derive(Clone, Debug, PartialEq)]
pub struct Painting {
    doc: DocId,
    which: Which,
}

/// The rows of a palette swatch's menu.
const USE_FILL: &str = "fill";
const USE_STROKE: &str = "stroke";
const REPLACE: &str = "replace";
const REMOVE: &str = "remove";

impl Ink {
    /// The tab that shows, and the shapes and texts a paint would go
    /// on: those of its selection.
    fn paint_targets(&self) -> Option<(DocId, Vec<NodeId>)> {
        let tab = self.tabs.active()?;
        let drawing = self.core.doc(tab.doc).ok()?;
        Some((tab.doc, paint::painted(drawing, &tab.selection.tops(drawing))))
    }

    /// What the paint section shows: the paint of what's selected (of
    /// the one in hand, where several are), as the drawing shows it
    /// now; else what the next shape will be painted with.
    fn paints_shown(&self) -> Paints {
        let shown = self.tabs.active().and_then(|tab| {
            let (drawing, _) = self.core.shown(tab.doc).ok()?;
            let in_hand = tab.selection.active.map(|a| paint::painted(drawing, &[a])).unwrap_or_default();
            let first = in_hand.first().copied().or_else(|| paint::painted(drawing, &tab.selection.tops(drawing)).first().copied())?;
            Some(paint::read(drawing, first))
        });
        shown.unwrap_or_else(|| self.paints.clone())
    }

    /// Make `which` paint of the selection `paint`, and of the next
    /// shape drawn. `held`: the button is down on what's choosing it,
    /// so it's shown and not yet done ([`Ink::paint_settled`] does it).
    pub(crate) fn set_paint(&mut self, which: Which, paint: Paint, held: bool) {
        self.paints.set(which, paint.clone());
        let Some((doc, nodes)) = self.paint_targets().filter(|(_, nodes)| !nodes.is_empty()) else { return };
        let command = Command::SetStyle { nodes, set: paint::set(which, &paint) };
        if !held {
            self.edit(doc, &command, which.label());
            return;
        }
        if self.painting.is_none() {
            if self.core.begin(doc, Actor::Alva).is_err() {
                return;
            }
            self.painting = Some(Painting { doc, which });
        }
        // A refusal waits for the button to come up to be said.
        let _ = self.core.update(doc, &command);
    }

    /// The button came up: a paint being dragged to lands, as one step.
    fn paint_settled(&mut self) {
        let Some(painting) = self.painting.take() else { return };
        if let Err(e) = self.core.commit(painting.doc, painting.which.label()) {
            let why = e.to_string();
            let said = self.core.doc(painting.doc).map_or(why.clone(), |d| in_row_names(d, &why));
            self.toast(said);
        }
    }

    /// The paint section, in the top of `panel`: how much of the panel
    /// it took, and a palette swatch's menu to open.
    pub(crate) fn paint_section(&mut self, ui: &mut Ui, panel: Rect, window: Rect) -> (f64, Option<ContextMenu>) {
        self.picker.begin(window);
        let shown = self.paints_shown();
        let out = section::draw(ui, panel, &mut self.paint_panel, &shown, &mut self.palettes, &mut self.picker, &self.icons);
        if out.changed {
            self.palettes.save(crate::settings::dir().as_deref());
        }
        let held = ui.state.down;
        if !held {
            self.paint_settled();
        }
        if let Some((which, paint)) = out.set {
            self.set_paint(which, paint, held);
        }
        let menu = out.menu.and_then(|(index, at)| self.swatch_menu(index, at));
        (out.height, menu)
    }

    /// The menu for swatch `index` of the palette shown, at `at`.
    fn swatch_menu(&self, index: usize, at: Vec2) -> Option<ContextMenu> {
        let color = *self.palettes.shown().colors.get(index)?;
        let row = |label: &str, op: &str| Item::action(label, Action::new(PALETTE_OP).with("op", Value::Str(op.to_owned())).with("index", Value::I64(index as i64)));
        let mut items = vec![row("Use as Fill", USE_FILL), row("Use as Stroke", USE_STROKE)];
        if !self.palettes.is_locked(self.palettes.active) {
            items.extend([Item::Separator, row("Replace with Fill", REPLACE), Item::danger("Remove Swatch", Action::new(PALETTE_OP).with("op", Value::Str(REMOVE.to_owned())).with("index", Value::I64(index as i64)))]);
        }
        Some(ContextMenu::new(&to_hex(color).to_uppercase(), at).tab("", items))
    }

    /// Carry out a row of that menu.
    pub(crate) fn palette_op(&mut self, op: &str, index: usize) {
        let Some(&color) = self.palettes.shown().colors.get(index) else { return };
        match op {
            USE_FILL => self.set_paint(Which::Fill, Paint::Color(color), false),
            USE_STROKE => self.set_paint(Which::Stroke, Paint::Color(color), false),
            REPLACE => {
                if let Paint::Color(fill) = self.paints_shown().fill {
                    self.palettes.set_color(index, fill);
                }
            }
            REMOVE => self.palettes.remove_color(index),
            other => lntrn_core::log_error!("no such palette operation: {other}"),
        }
        if matches!(op, REPLACE | REMOVE) {
            self.palettes.save(crate::settings::dir().as_deref());
        }
    }
}
