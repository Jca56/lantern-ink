//! The Gradient tool on the canvas, and its Box (ARCHITECTURE §8;
//! `grads.rs` has what a gradient is and the Commands that change it).
//!
//! A drag across a shape paints it with a gradient along the drag: a
//! linear one from where the drag began to where it ended; a radial one
//! (the Box's "Radial") about where it began, out to where it ended. A
//! shape that has a gradient already has its line put there; one with a
//! plain colour gets a new gradient, from that colour to a darker one.
//! The line's two ends stay on the canvas as handles to drag. A press
//! on another shape takes that one up. Shift holds a line to 45°.
//!
//! The Box has the gradient's stops on a bar: a press on the bar puts
//! one there, a stop is dragged along it, and the one picked has its
//! colour and its place beside a button that takes it off. It's the
//! fill's gradient, or with "Stroke" ticked the stroke's.
//!
//! Every drag is a gesture in the core: the drawing shows it as it
//! goes, and it lands as one step.

use ink_core::{Actor, Command, DocId, Document, NodeId};
use ink_doc::gradient::Stop;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, FILL, Sense, Ui};

use crate::canvas::CanvasInput;
use crate::colour::picker::swatch_face;
use crate::colour::section::{gradient_face, gradient_of};
use crate::controls;
use crate::edits::in_row_names;
use crate::grads::{self, Change, Held};
use crate::ink::Ink;
use crate::overlay::Scene;
use crate::paint::{self, Paint, Which};
use crate::picking::top_at;
use crate::pointer::View;
use crate::shapebox::Laid;
use crate::theme::ACCENT;
use crate::toolbox;
use crate::tools::Tool;

/// How big an end of the line is for the pointer, how short a drag
/// draws nothing, how far from the pointer a press still finds a shape,
/// and how near a stop on the bar a press takes it, logical px.
const END: f64 = 28.0;
const DRAW_MIN: f64 = 4.0;
const REACH: f64 = 3.0;
const NEAR: f64 = 12.0;

/// A gradient's line over the canvas, window px.
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub from: Vec2,
    pub to: Vec2,
    /// Each stop: how far along, and its colour.
    pub stops: Vec<(f64, Color)>,
}

enum Drag {
    /// A new line, from here (the drawing's coordinates) across `shape`.
    Draw { shape: NodeId, from: Vec2 },
    /// An end of the line there is: where it starts (`true`), or ends.
    End { shape: NodeId, start: bool },
}

/// A stop being dragged along the Box's bar: which one, and the colour
/// of one this press put there.
#[derive(Clone, Copy)]
struct OnBar {
    index: usize,
    added: Option<Color>,
}

/// What the Gradient tool keeps between frames.
#[derive(Default)]
pub struct Grading {
    /// It's the stroke's gradient, not the fill's.
    pub stroke: bool,
    /// A new one is radial.
    pub radial: bool,
    /// The stop picked on the bar.
    stop: usize,
    drag: Option<(DocId, Drag)>,
    bar: Option<OnBar>,
    said: Option<String>,
}

impl Grading {
    /// The button is down on the canvas for the Gradient tool.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }

    fn which(&self) -> Which {
        if self.stroke { Which::Stroke } else { Which::Fill }
    }
}

/// `v` turned to the nearest multiple of 45°, as long as it was.
fn octant(v: Vec2) -> Vec2 {
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (v.y.atan2(v.x) / step).round() * step;
    Vec2::new(angle.cos(), angle.sin()) * v.length()
}

/// The shape the tool works on: of the selection, the one in hand (or
/// the first shape it holds); a shape, that may be changed.
fn target(drawing: &Document, active: Option<NodeId>, tops: &[NodeId]) -> Option<NodeId> {
    let in_hand: Vec<NodeId> = active.filter(|a| tops.contains(a)).into_iter().chain(tops.iter().rev().copied()).collect();
    in_hand.into_iter().flat_map(|top| paint::painted(drawing, &[top])).find(|&id| drawing.get(id).is_some_and(|n| n.kind.is_shape()) && drawing.lock_over(id).is_none())
}

impl Ink {
    /// The shape the Gradient tool works on, in `doc`.
    fn grade_target(&self, doc: DocId) -> Option<NodeId> {
        let drawing = self.core.doc(doc).ok()?;
        let tab = self.tabs.iter().find(|t| t.doc == doc)?;
        target(drawing, tab.selection.active, &tab.selection.tops(drawing))
    }

    /// Give up a drag of the Gradient tool's: as if it never began.
    pub(crate) fn drop_grade(&mut self) {
        if let Some((doc, _)) = self.grading.drag.take() {
            self.core.cancel(doc);
            self.grading.said = None;
        }
    }

    /// The Command a line from `from` to `to` across `shape` comes to:
    /// its gradient's line put there, or a new gradient of its colour.
    fn graded(&self, drawing: &Document, shape: NodeId, line: (Vec2, Vec2)) -> Option<Command> {
        let which = self.grading.which();
        if let Some(held) = Held::of(drawing, shape, which) {
            return Some(held.set(drawing, &Change::Line(line.0, line.1)));
        }
        // From the colour it is (the next shape's, where it has none)
        // to that colour darker.
        let colour = match paint::read(drawing, shape).get(which) {
            Paint::Color(c) => *c,
            _ => match self.paints.get(which) {
                Paint::Color(c) => *c,
                _ => Color::hex(0xF3B700),
            },
        };
        let [first, last] = gradient_of(colour);
        grads::fresh(drawing, shape, which, self.grading.radial, line, &[Stop { offset: 0.0, color: first }, Stop { offset: 1.0, color: last }])
    }

    /// One frame of the Gradient tool on `doc`: what it draws over the
    /// canvas goes into `scene`.
    pub(crate) fn grade_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, active: bool, scene: &mut Scene) {
        if self.grading.drag.as_ref().is_some_and(|(on, _)| *on != doc || !active) {
            self.drop_grade();
        }
        if !active {
            return;
        }
        let (s, pointer, shift) = (view.scale, ui.state.pointer, ui.state.mods.shift());
        let at = view.to_doc.apply(pointer);
        let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);
        let which = self.grading.which();
        let Ok(drawing) = self.core.doc(doc) else { return };
        let Ok((looks, _)) = self.core.shown(doc) else { return };
        let shape = self.grade_target(doc);
        // The line as the drawing shows it, with its ends to take.
        let shown = shape.and_then(|shape| Held::of(looks, shape, which));
        let ends = shown.as_ref().map(|h| h.ends()).map(|(a, b)| (view.to_window.apply(a), view.to_window.apply(b)));
        let over = ends.filter(|_| input.over && self.grading.drag.is_none()).and_then(|(a, b)| {
            let near = |p: Vec2| p.distance(pointer) <= END * s / 2.0;
            // The end it finishes at first: on a line of no length,
            // that's the one to pull out.
            if near(b) { Some(false) } else { near(a).then_some(true) }
        });

        let (mut begin, mut update, mut done, mut picked) = (false, None, None, None);
        let released = input.released || !input.held;
        let drag = match self.grading.drag.take().map(|(_, drag)| drag) {
            None if input.pressed => match (over, shape) {
                (Some(start), Some(shape)) => {
                    begin = true;
                    Some(Drag::End { shape, start })
                }
                _ => {
                    // On another shape: that one is taken up. Anywhere
                    // else, the line is the one in hand's.
                    let under = top_at(drawing, at, REACH * s / per_unit).filter(|&id| drawing.get(id).is_some_and(|n| n.kind.is_shape()));
                    let onto = match under {
                        Some(other) if Some(other) != shape => {
                            picked = Some(other);
                            Some(other)
                        }
                        _ => shape,
                    };
                    onto.map(|shape| {
                        begin = true;
                        Drag::Draw { shape, from: at }
                    })
                }
            },
            None => None,
            Some(Drag::Draw { shape, from }) => {
                let far = view.to_window.apply(from).distance(pointer) >= DRAW_MIN * s;
                let to = if shift { from + octant(at - from) } else { at };
                let command = if far { self.graded(drawing, shape, (from, to)) } else { None };
                ui.state.cursor_icon = CursorIcon::Grabbing;
                if released {
                    done = Some(command);
                    None
                } else {
                    update = Some(command.unwrap_or(Command::Batch(Vec::new())));
                    Some(Drag::Draw { shape, from })
                }
            }
            Some(Drag::End { shape, start }) => {
                let command = Held::of(drawing, shape, which).map(|held| {
                    let (a, b) = held.ends();
                    let line = match (start, shift) {
                        (true, false) => (at, b),
                        (true, true) => (b + octant(at - b), b),
                        (false, false) => (a, at),
                        (false, true) => (a, a + octant(at - a)),
                    };
                    held.set(drawing, &Change::Line(line.0, line.1))
                });
                ui.state.cursor_icon = CursorIcon::Grabbing;
                if released {
                    done = Some(command);
                    None
                } else {
                    update = Some(command.unwrap_or(Command::Batch(Vec::new())));
                    Some(Drag::End { shape, start })
                }
            }
        };
        if drag.is_none() && over.is_some() {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        scene.axis = shown.zip(ends).map(|(held, (from, to))| Axis { from, to, stops: held.said.stops.iter().map(|stop| (stop.offset, stop.color)).collect() });

        // The drawing is read; now the selection, and what the drag asks
        // of the core.
        if let Some(shape) = picked
            && let (Ok(drawing), Some(tab)) = (self.core.doc(doc), self.tabs.iter_mut().find(|t| t.doc == doc))
        {
            tab.selection.select_only(shape);
            tab.selection.within = drawing.get(shape).and_then(|n| n.parent).filter(|&up| up != drawing.root() && drawing.get(up).is_some_and(|n| n.kind.is_group()));
            tab.selection.reveal(drawing, shape);
            self.tree.show(shape);
            self.grading.stop = 0;
        }
        let mut say = None;
        if begin && let Err(e) = self.core.begin(doc, Actor::Alva) {
            say = Some(e.to_string());
        }
        if let Some(command) = update
            && let Err(e) = self.core.update(doc, &command)
        {
            say = Some(e.to_string());
        }
        match done {
            // Too short to be a line: nothing was done.
            Some(None) => self.core.cancel(doc),
            Some(Some(command)) => {
                if let Err(e) = self.core.update(doc, &command).and_then(|_| self.core.commit(doc, "Gradient")) {
                    say = Some(e.to_string());
                }
            }
            None => {}
        }
        match say {
            Some(why) if self.grading.said.as_ref() != Some(&why) => {
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
                self.grading.said = Some(why);
            }
            _ => {}
        }
        if drag.is_none() {
            self.grading.said = None;
        }
        self.grading.drag = drag.map(|drag| (doc, drag));
    }

    /// Make `change` to the gradient of the shape in hand. `held`: the
    /// button is down on what chose it, so it's shown and not yet done.
    fn grade(&mut self, doc: DocId, change: &Change, held: bool) {
        let which = self.grading.which();
        let command = self.grade_target(doc).and_then(|shape| {
            let drawing = self.core.doc(doc).ok()?;
            Some(Held::of(drawing, shape, which)?.set(drawing, change))
        });
        if let Some(command) = command {
            self.box_set(doc, &command, "Gradient", held);
        }
    }

    /// The Box under the Gradient tool: which paint, which kind, and the
    /// stops of the gradient in hand on a bar.
    pub(crate) fn grade_box(&mut self, ui: &mut Ui, canvas: Rect) {
        let Some(doc) = self.tabs.active_doc() else { return self.toolbox.gone() };
        let which = self.grading.which();
        // As the drawing shows it, to show; as it is, to change.
        let held = |ink: &Ink, live: bool| {
            let drawing = if live { ink.core.shown(doc).ok()?.0 } else { ink.core.doc(doc).ok()? };
            Held::of(drawing, ink.grade_target(doc)?, which)
        };
        let (shown, base) = (held(self, true), held(self, false));
        let stops: Vec<Stop> = shown.as_ref().map(|h| h.said.stops.clone()).unwrap_or_default();
        self.grading.stop = self.grading.stop.min(stops.len().saturating_sub(1));
        let picked = self.grading.stop;
        let (mut radial, mut stroke) = (shown.as_ref().map_or(self.grading.radial, |h| h.said.radial), self.grading.stroke);
        let colour = ui.id("stop-colour");
        let (icons, picking) = (&self.icons, self.picker.is_open_for(colour));
        // What the rows asked for.
        let (mut bar_at, mut pressed, mut swatch, mut remove, mut place) = (None, false, None, false, stops.get(picked).map(|stop| (stop.offset * 100.0).round()));
        let was_place = place;
        let mut laid = Laid::new();
        toolbox::draw_with(ui, canvas, &mut self.toolbox, Tool::Gradient.label(), |ui| {
            laid.push(("Radial", Rect::from_min_size(ui.cursor(), Vec2::new(ui.avail_width(), ui.m.widget_h))));
            controls::toggle(ui, "Radial", &mut radial);
            laid.push(("Stroke", Rect::from_min_size(ui.cursor(), Vec2::new(ui.avail_width(), ui.m.widget_h))));
            controls::toggle(ui, "Stroke", &mut stroke);
            if stops.is_empty() {
                return;
            }
            // The bar: its colours along it, and a marker at each stop.
            let scale = ui.m.scale;
            let px = |v: f64| (v * scale).round().max(1.0);
            let bar = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
            laid.push(("Stops", bar));
            let resp = ui.interact(ui.id("stops"), bar, Sense::DRAG);
            let face = bar.shrink(px(2.0));
            if let Some(checks) = icons.checker() {
                let uv = Rect::from_xywh(0.0, 0.0, (face.width() / checks.width as f64).min(1.0), (face.height() / checks.height as f64).min(1.0));
                ui.draw.image_uv(face, checks, uv, 0.0, Color::WHITE);
            }
            gradient_face(ui, face, &stops.iter().map(|stop| (stop.offset, stop.color)).collect::<Vec<_>>());
            for (i, stop) in stops.iter().enumerate() {
                let x = (face.min.x + face.width() * stop.offset).round();
                let marker = Rect::from_xywh(x - px(5.0), bar.min.y, px(10.0), bar.height());
                ui.draw.stroke_rect(marker.expand(px(2.0)), px(2.0), px(3.0), Color::rgba(0.0, 0.0, 0.0, 0.7));
                ui.draw.stroke_rect(marker, px(2.0), px(3.0), if i == picked { ACCENT } else { Color::WHITE });
            }
            if resp.hovered || resp.held {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            if resp.pressed || resp.held {
                bar_at = Some(((ui.state.pointer.x - face.min.x) / face.width().max(1.0), px(NEAR) / face.width().max(1.0)));
                pressed = resp.pressed;
            }
            // The stop picked: its colour, and where along it is.
            let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
            let (gap, half) = (ui.m.gap * 2.0, ((row.width() - ui.m.gap * 2.0) / 2.0).floor());
            let well = Rect::from_xywh(row.min.x, row.min.y, half, row.height());
            laid.push(("Stop Colour", well));
            let resp = ui.interact(ui.id("Stop Colour"), well, Sense::CLICK);
            if let Some(stop) = stops.get(picked) {
                swatch_face(ui, well, stop.color, picking || resp.hovered, icons);
            }
            swatch = Some((well, resp.clicked));
            if let Some(place) = place.as_mut() {
                let field = Rect::from_xywh(row.min.x + half + gap, row.min.y, half, row.height());
                laid.push(("At", field));
                controls::number_in(ui, ui.id("At"), field, "At %", place, 1.0, Some((0.0, 100.0)), 0);
            }
            let (clicked, rect) = controls::button_if(ui, "Remove Stop", stops.len() > 2);
            laid.push(("Remove Stop", rect));
            remove = clicked;
        });
        #[cfg(test)]
        {
            self.toolbox.laid = laid;
        }
        let held_down = ui.state.down;
        self.grading.stroke = stroke;
        // The kind: the gradient in hand made the other; else the next
        // one's.
        if radial != shown.as_ref().map_or(self.grading.radial, |h| h.said.radial) {
            self.grading.radial = radial;
            if shown.is_some() {
                self.grade(doc, &Change::Kind(radial), false);
            }
        }
        let Some(base) = base.map(|h| h.said.stops) else {
            if !held_down {
                self.tune_settled();
            }
            return;
        };
        // The bar: a press takes the stop near it, or puts one there;
        // held, that stop goes along with the pointer, between its
        // neighbours.
        let mut now: Option<Vec<Stop>> = None;
        if let Some((t, near)) = bar_at {
            let t = (t.clamp(0.0, 1.0) * 100.0).round() / 100.0;
            if pressed {
                let nearest = base.iter().enumerate().map(|(i, stop)| (i, (stop.offset - t).abs())).filter(|(_, off)| *off <= near).min_by(|a, b| a.1.total_cmp(&b.1));
                self.grading.bar = Some(match nearest {
                    Some((index, _)) => OnBar { index, added: None },
                    None => OnBar { index: base.iter().position(|stop| stop.offset > t).unwrap_or(base.len()), added: Some(Held::colour_at(&base, t)) },
                });
            }
            if let Some(OnBar { index, added }) = self.grading.bar {
                let mut stops = base.clone();
                if let Some(color) = added {
                    stops.insert(index.min(stops.len()), Stop { offset: t, color });
                }
                if index < stops.len() {
                    let (lo, hi) = (if index > 0 { stops[index - 1].offset } else { 0.0 }, stops.get(index + 1).map_or(1.0, |next| next.offset));
                    stops[index].offset = t.clamp(lo, hi.max(lo));
                    self.grading.stop = index;
                    now = Some(stops);
                }
            }
        } else if !held_down {
            self.grading.bar = None;
        }
        // Its colour, in the picker its swatch opens; its place, typed
        // or dragged along; and the button that takes it off.
        let picked = self.grading.stop.min(base.len().saturating_sub(1));
        if let Some((well, clicked)) = swatch {
            if clicked {
                self.picker.toggle(colour);
            }
            if let Some(stop) = base.get(picked).filter(|_| self.picker.is_open_for(colour)) {
                let mut color = stops.get(picked).map_or(stop.color, |shown| shown.color);
                if self.picker.popup(ui, colour, well, "Stop", &mut color, &self.icons) {
                    let mut stops = base.clone();
                    stops[picked].color = color;
                    now = Some(stops);
                }
            }
        }
        if let (Some(place), true, Some(_)) = (place, place != was_place, base.get(picked)) {
            let mut stops = base.clone();
            let (lo, hi) = (if picked > 0 { stops[picked - 1].offset } else { 0.0 }, stops.get(picked + 1).map_or(1.0, |next| next.offset));
            stops[picked].offset = (place / 100.0).clamp(lo, hi.max(lo));
            now = Some(stops);
        }
        if remove && base.len() > 2 {
            let mut stops = base.clone();
            stops.remove(picked);
            self.grading.stop = picked.saturating_sub(1);
            self.grade(doc, &Change::Stops(stops), false);
        } else if let Some(stops) = now.filter(|stops| *stops != base) {
            self.grade(doc, &Change::Stops(stops), held_down);
        }
        if !held_down {
            self.tune_settled();
        }
    }
}
