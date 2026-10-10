//! Paint, from the window (ARCHITECTURE §8): the paint section shows
//! what the selection is painted with (or, with nothing selected, what
//! the next shape drawn will be), and a paint it sets goes on every
//! shape and text of the selection as one step. A colour dragged in
//! the picker is a gesture in the core, like a drag on the canvas: the
//! drawing shows it as it goes, and it lands as one step when the
//! button comes up.

use ink_core::{Actor, Command, DocId, Document};
use ink_doc::Kind;
use ink_doc::gradient::Gradient;
use ink_doc::refs::Ids;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_props::Value;
use lntrn_ui::{Action, ContextMenu, Item, Ui};

use crate::colour::palettes::to_hex;
use crate::colour::section::{self, Shown};
use crate::controls::written;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::menus::PALETTE_OP;
use crate::paint::{self, Paint, Paints, Set, Which};

/// Something being dragged to (a colour in the picker, a swatch held,
/// a width, an opacity): a gesture, and what its step will be called.
#[derive(Clone, Debug, PartialEq)]
pub struct Painting {
    doc: DocId,
    label: &'static str,
}

/// The colours along the gradient called `id`, if the drawing has one.
fn stops_of(drawing: &Document, ids: &Ids, id: &str) -> Vec<(f64, Color)> {
    let gradient = ids.get(id).and_then(|node| drawing.get(node)).and_then(|node| Gradient::of(drawing, ids, node));
    gradient.map(|g| g.stops.iter().map(|stop| (stop.offset, stop.color)).collect()).unwrap_or_default()
}

/// What of the right panel the object tree keeps under the paint
/// section, at least: a share of it, and no less than this many
/// logical px.
const TREE_SHARE: f64 = 0.38;
const TREE_LEAST: f64 = 220.0;

/// The rows of a palette swatch's menu.
const USE_FILL: &str = "fill";
const USE_STROKE: &str = "stroke";
const REPLACE: &str = "replace";
const REMOVE: &str = "remove";

impl Ink {
    /// What the paint section shows: the paint of what's selected (of
    /// the one in hand, where several are), as the drawing shows it
    /// now; else what the next shape will be painted with. And each
    /// paint's colours, where it's a gradient.
    fn paints_shown(&self) -> (Paints, [Vec<(f64, Color)>; 2], f64) {
        let Some((tab, (drawing, _))) = self.tabs.active().and_then(|tab| Some((tab, self.core.shown(tab.doc).ok()?))) else { return (self.paints.clone(), Default::default(), 1.0) };
        let page = ink_doc::arrange::page_box(drawing);
        let step = crate::boxes::step_for(page.width().max(page.height()));
        let tops = tab.selection.tops(drawing);
        let in_hand = tab.selection.active.map(|a| paint::painted(drawing, &[a])).unwrap_or_default();
        let Some(first) = in_hand.first().copied().or_else(|| paint::painted(drawing, &tops).first().copied()) else { return (self.paints.clone(), Default::default(), step) };
        let mut shown = paint::read(drawing, first);
        // The opacity is the selected thing's own, group or shape.
        let faded = paint::faded(drawing, &tops);
        if let Some(top) = tab.selection.active.filter(|a| faded.contains(a)).or(faded.first().copied()) {
            shown.opacity = paint::opacity_of(drawing, top);
        }
        let ids = Ids::of(drawing);
        let stops = Which::BOTH.map(|which| match shown.get(which) {
            Paint::Server(id) => stops_of(drawing, &ids, id),
            _ => Vec::new(),
        });
        (shown, stops, step)
    }

    /// Set `set` on the selection, and as what the next shape drawn
    /// gets. `held`: the button is down on what's choosing it, so it's
    /// shown and not yet done ([`Ink::paint_settled`] does it).
    pub(crate) fn set_paint(&mut self, set: Set, held: bool) {
        self.paints.take(&set);
        let Some(tab) = self.tabs.active() else { return };
        let doc = tab.doc;
        let Ok(drawing) = self.core.doc(doc) else { return };
        let tops = tab.selection.tops(drawing);
        // A paint and a line go on each shape and text; an opacity on
        // the selected things themselves.
        let nodes = match &set {
            Set::Opacity(_) => paint::faded(drawing, &tops),
            _ => paint::painted(drawing, &tops),
        };
        if nodes.is_empty() {
            return;
        }
        let command = match &set {
            Set::Gradient(which) => {
                let Some(command) = self.gradient_for(drawing, *which, nodes) else { return };
                command
            }
            _ => Command::SetStyle { nodes, set: set.properties() },
        };
        if !held {
            self.edit(doc, &command, set.label());
            return;
        }
        if self.painting.is_none() {
            if self.core.begin(doc, Actor::Alva).is_err() {
                return;
            }
            self.painting = Some(Painting { doc, label: set.label() });
        }
        // A refusal waits for the button to come up to be said.
        let _ = self.core.update(doc, &command);
    }

    /// What makes `which` paint of `nodes` a gradient: the one it last
    /// was, while the drawing still has it; else a new one, from the
    /// colour it last was to that colour darker, down the shape.
    fn gradient_for(&self, drawing: &Document, which: Which, nodes: Vec<ink_core::NodeId>) -> Option<Command> {
        let k = Which::BOTH.iter().position(|w| *w == which)?;
        let ids = Ids::of(drawing);
        let is_gradient = |id: &String| ids.get(id).and_then(|node| drawing.get(node)).is_some_and(|node| matches!(node.kind, Kind::LinearGradient | Kind::RadialGradient));
        if let Some(id) = self.paint_panel.last_gradient[k].clone().filter(is_gradient) {
            return Some(Command::SetStyle { nodes, set: paint::set(which, &Paint::Server(id)) });
        }
        let id = (1..).map(|n| format!("gradient-{n}")).find(|id| ids.get(id).is_none())?;
        let stop = |offset: u8, c: Color| format!("<stop offset=\"{offset}\" stop-color=\"{}\"{}/>", to_hex(c), if c.a < 1.0 - 1e-9 { format!(" stop-opacity=\"{}\"", written(c.a, 3)) } else { String::new() });
        let [top, bottom] = section::gradient_of(self.paint_panel.last[k]);
        // Across the box of whatever it paints, top to bottom.
        let markup = format!("<linearGradient id=\"{id}\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">{}{}</linearGradient>", stop(0, top), stop(1, bottom));
        let elements = ink_doc::elements(&markup).ok()?;
        Some(Command::Batch(vec![Command::Define { elements }, Command::SetStyle { nodes, set: paint::set(which, &Paint::Server(id)) }]))
    }

    /// The button came up: what was being dragged to lands, as one
    /// step.
    fn paint_settled(&mut self) {
        let Some(painting) = self.painting.take() else { return };
        if let Err(e) = self.core.commit(painting.doc, painting.label) {
            let why = e.to_string();
            let said = self.core.doc(painting.doc).map_or(why.clone(), |d| in_row_names(d, &why));
            self.toast(said);
        }
    }

    /// The paint section, in the top of `panel`: how much of the panel
    /// it took, and a palette swatch's menu to open.
    pub(crate) fn paint_section(&mut self, ui: &mut Ui, whole: Rect, foot: f64, window: Rect) -> (f64, Option<ContextMenu>) {
        // The panel but for its foot (the preview strip's).
        let panel = Rect::new(whole.min, Vec2::new(whole.max.x, (whole.max.y - foot).max(whole.min.y)));
        self.picker.begin(window);
        let (paints, stops, step) = self.paints_shown();
        let shown = Shown { paints: &paints, folded: self.settings.paint_folded, stops, step };
        // The object tree keeps its room under it: where the panel is
        // too short for all of the section, the section scrolls.
        // (Its share of the whole panel: what stands at the panel's foot
        // comes out of this section's room, which scrolls, not the
        // tree's.)
        let room = (panel.height() - (whole.height() * TREE_SHARE).max(TREE_LEAST * ui.m.scale)).max(0.0);
        let tall = self.paint_panel.tall;
        let out = if tall > room && room > 0.0 {
            let mut out = section::Out::default();
            let (layer, view) = (ui.layer(), Rect::from_min_size(panel.min, Vec2::new(panel.width(), room)));
            let Ink { paint_panel, palettes, picker, icons, .. } = self;
            ui.child(view, layer, |ui| {
                ui.scroll_area("paint-scroll", Some(room), |ui| {
                    let all = ui.alloc(Vec2::new(lntrn_ui::FILL, tall));
                    out = section::draw(ui, Rect::new(all.min, Vec2::new(all.max.x, all.min.y + tall.max(panel.height()))), paint_panel, &shown, palettes, picker, icons);
                });
            });
            self.paint_panel.tall = out.height;
            section::Out { height: room, ..out }
        } else {
            let out = section::draw(ui, panel, &mut self.paint_panel, &shown, &mut self.palettes, &mut self.picker, &self.icons);
            self.paint_panel.tall = out.height;
            section::Out { height: out.height.min(panel.height()), ..out }
        };
        if out.fold {
            self.settings.paint_folded = !self.settings.paint_folded;
            self.settings.save();
            ui.state.request_rebuild = true;
        }
        if out.changed {
            self.palettes.save(crate::settings::dir().as_deref());
        }
        let held = ui.state.down;
        if !held {
            self.paint_settled();
        }
        if let Some(set) = out.set {
            // A choice made by letting go (a kind, a cap) is done at
            // once; so is a gradient, which is made, not dragged to.
            let held = held && !matches!(set, Set::Gradient(_) | Set::Cap(_) | Set::Join(_) | Set::Dashes(_));
            self.set_paint(set, held);
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
            USE_FILL => self.set_paint(Set::Paint(Which::Fill, Paint::Color(color)), false),
            USE_STROKE => self.set_paint(Set::Paint(Which::Stroke, Paint::Color(color)), false),
            REPLACE => {
                if let Paint::Color(fill) = self.paints_shown().0.fill {
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
