//! The slider (LS3's `controls/slider.rs`): a rail with a knob, and the
//! value in a box of its own. Ink's panels are laid out by hand, so
//! this one runs in a row it's given, its name the caller's to draw.
//!
//! - Press or drag anywhere along the rail; the arrow keys nudge it once
//!   it has the keyboard (Home and End go to the ends, Shift goes fine).
//! - Press the box and type a number; Enter (or a press elsewhere) takes
//!   it, Escape leaves the value alone. What's typed isn't held to the
//!   drag's stops: a slider that stops every 5 takes a typed 23.
//! - A right press anywhere on it puts it back to rest, where it has
//!   one.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, KeyStep, Sense, Ui, WidgetId};

use super::field::typing;
use super::{EDGE, GOLD_EDGE, px, right_pressed, snapped, well, written};
use crate::theme::{ACCENT, BUTTON, SLIDER_FILL, TEXT};
use lntrn_math::Color;

/// The rail, the knob's radius and the gap between rail and box:
/// logical px.
const RAIL_H: f64 = 10.0;
const KNOB: f64 = 11.0;
const GAP: f64 = 12.0;
/// The least a value's box is wide.
const BOX_MIN: f64 = 76.0;

/// The rail where nothing's taken (LS3's, verbatim).
const EMPTY_TOP: Color = Color::rgb(0.22, 0.18, 0.12);
const EMPTY_BOTTOM: Color = Color::rgb(0.14, 0.11, 0.08);
/// The knob at rest (LS3's); in hand it's the accent.
const KNOB_REST: Color = Color::rgb(0.85, 0.68, 0.0);

/// A slider's range and manners.
#[derive(Clone, Copy, Debug)]
pub struct Slider<'a> {
    pub min: f64,
    pub max: f64,
    /// The stops a drag lands on (0: anywhere).
    pub step: f64,
    /// Written after the value: `%`, `px`.
    pub unit: &'a str,
    /// Where a right press puts it back to.
    pub rest: Option<f64>,
}

impl<'a> Slider<'a> {
    pub fn new(min: f64, max: f64, step: f64) -> Slider<'a> {
        Slider { min, max, step, unit: "", rest: None }
    }

    pub fn unit(self, unit: &'a str) -> Slider<'a> {
        Slider { unit, ..self }
    }

    pub fn rest(self, rest: f64) -> Slider<'a> {
        Slider { rest: Some(rest), ..self }
    }

    /// What the box says for `value`.
    pub(crate) fn shown(&self, value: f64) -> String {
        let n = written(value, 3);
        if self.unit.is_empty() { n } else { format!("{n} {}", self.unit) }
    }

    /// How far along the rail `value` is, 0 to 1.
    fn along(&self, value: f64) -> f64 {
        if self.max > self.min { ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Run the slider in `row`: its rail, and its value's box at the
    /// right. Whether `value` changed.
    pub fn in_row(&self, ui: &mut Ui, id: WidgetId, row: Rect, value: &mut f64) -> bool {
        let style = ui.text_style();
        let rect = row;

        // The box is as wide as the longest thing it may say.
        let widest = ui.measure(&self.shown(self.min), &style).max(ui.measure(&self.shown(self.max), &style));
        let box_w = (widest + ui.m.pad * 2.0).max(px(ui, BOX_MIN)).min(row.width() * 0.5);
        let inset = px(ui, 3.0);
        let boxed = Rect::new(Vec2::new(row.max.x - box_w, row.min.y + inset), Vec2::new(row.max.x, row.max.y - inset));
        let knob = px(ui, KNOB);
        // The knob's middle runs the whole of this, so a press a third of
        // the way along it is a third of the range.
        let track = Rect::new(Vec2::new(row.min.x + knob, row.min.y), Vec2::new((boxed.min.x - px(ui, GAP) - knob).max(row.min.x + knob + 1.0), row.max.y));

        let mut changed = false;
        let typed = typing(ui, id.with("value"), boxed, value, (self.min, self.max), if self.step >= 1.0 { 1.0 } else { 0.0 }, &self.shown(*value));
        changed |= typed.changed;

        let r = ui.interact(id, track, Sense::DRAG);
        let focused = ui.focusable(id, track);
        let at = Vec2::new(track.min.x + track.width() * self.along(*value), track.center().y);
        // The knob hangs over the rail's ends: a press on that half is
        // the slider's too.
        let grip = ui.interact(id.with("knob"), Rect::from_center_size(at, Vec2::splat(knob * 2.0 + px(ui, 8.0))), Sense::DRAG);
        let in_hand = r.held || grip.held;
        if r.hovered || grip.hovered || in_hand {
            ui.state.cursor_icon = CursorIcon::EwResize;
        }
        if self.max > self.min {
            if r.dragging || grip.dragging {
                let t = ((ui.state.pointer.x - track.min.x) / track.width()).clamp(0.0, 1.0);
                let v = snapped(self.min + t * (self.max - self.min), self.min, self.max, self.step);
                changed |= v != *value;
                *value = v;
            }
            if focused {
                let fine = if ui.state.mods.shift() { 0.1 } else { 1.0 };
                let by = ((self.max - self.min) * 0.01 * fine).max(self.step);
                let v = match ui.key_step(id) {
                    KeyStep::By(n) => snapped(*value + by * f64::from(n), self.min, self.max, self.step),
                    KeyStep::Min => self.min,
                    KeyStep::Max => self.max,
                    KeyStep::None => *value,
                };
                changed |= v != *value;
                *value = v;
            }
        }
        if let Some(rest) = self.rest
            && right_pressed(ui, rect)
            && *value != rest
        {
            *value = rest;
            changed = true;
        }

        // The rail: gold as far as the value has come, dark beyond.
        let rail_h = px(ui, RAIL_H);
        let rail = Rect::new(Vec2::new(track.min.x, track.center().y - rail_h / 2.0), Vec2::new(track.max.x, track.center().y + rail_h / 2.0));
        let small = px(ui, 3.0);
        ui.draw.rounded_rect_gradient(rail, small, EMPTY_TOP, EMPTY_BOTTOM);
        let t = self.along(*value);
        let taken = Rect::new(rail.min, Vec2::new(rail.min.x + rail.width() * t, rail.max.y));
        if taken.width() >= 1.0 {
            ui.draw.push_clip(taken);
            ui.draw.rounded_rect_gradient(rail, small, SLIDER_FILL.top, SLIDER_FILL.bottom);
            ui.draw.pop_clip();
        }
        ui.draw.stroke_rect(rail, px(ui, 2.0), small, BUTTON);
        // The knob, where the value is now.
        let at = Vec2::new(track.min.x + track.width() * t, track.center().y);
        ui.draw.circle(at, knob, GOLD_EDGE);
        ui.draw.circle(at, knob - px(ui, 2.0), if in_hand || r.hovered || grip.hovered { ACCENT } else { KNOB_REST });
        ui.focus_ring(id, rail.expand(px(ui, 4.0)));

        // The value's box, when it isn't being typed into.
        if !typed.open {
            let vid = id.with("value");
            let b = ui.interact(vid, boxed, Sense::CLICK);
            ui.focusable(vid, boxed);
            if b.hovered {
                ui.state.cursor_icon = CursorIcon::Text;
            }
            if b.pressed {
                ui.state.request_rebuild = true;
            }
            well(ui, boxed, if b.hovered { ACCENT } else { EDGE });
            ui.text_centered(&self.shown(*value), &style, boxed, TEXT);
        }
        changed
    }
}
