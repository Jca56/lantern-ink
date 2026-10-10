//! The rulers (`docs/M4.md`, slice f): one along the canvas's top and
//! one down its left, numbered in the drawing's own units, each with a
//! gold mark where the pointer is. A guide is dragged out of one
//! (`guiding.rs`). View > Rulers hides them.
//!
//! The numbers stand at round steps (1, 2, 5, 10, …), far enough apart
//! to be read at 18 px. Down the left ruler a number's digits are
//! stacked, so that ruler is no wider than the other is tall (LUI2
//! turns no text on its side).

use lntrn_math::{Rect, Vec2};
use lntrn_ui::Ui;

use crate::layout::Layout;
use crate::pointer::View;
use crate::theme::{self, ACCENT, BORDER, FONT_SM, PANEL, TEXT, TEXT_DIM};

/// The least between two numbers, and between two small ticks,
/// logical px.
pub const LABEL_GAP: f64 = 70.0;
const TICK_GAP: f64 = 7.0;

/// How far apart the numbers are, in the drawing's units, when a unit
/// shows `per_unit` window px and numbers want `gap` px between them:
/// the least of 1, 2, 5 and their tens that's enough.
pub fn step(per_unit: f64, gap: f64) -> f64 {
    let least = gap / per_unit.max(1e-12);
    let tens = 10f64.powf(least.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * tens).find(|s| *s >= least * (1.0 - 1e-9)).unwrap_or(tens * 10.0)
}

/// How many parts the small ticks cut a `step` into: tenths, fifths,
/// quarters or halves of it, the finest that stay `gap` px apart.
pub fn parts(step: f64, per_unit: f64, gap: f64) -> usize {
    let lead = (step / 10f64.powf(step.log10().floor())).round() as usize;
    let ways: &[usize] = match lead {
        2 => &[4, 2],
        5 => &[5],
        _ => &[10, 5, 2],
    };
    ways.iter().copied().find(|n| step / *n as f64 * per_unit >= gap).unwrap_or(1)
}

/// `v` as a ruler says it, where numbers are `step` apart: as many
/// decimals as the step has, and none it doesn't need.
pub fn label(v: f64, step: f64) -> String {
    let places = if step >= 1.0 { 0 } else { (-step.log10().floor()) as usize };
    let mut said = format!("{v:.places$}");
    if said.contains('.') {
        said = said.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if said == "-0" { "0".to_owned() } else { said }
}

/// A ruler's mark: how far along it (the drawing's units), and its
/// number, where it has one.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub at: f64,
    pub label: Option<String>,
}

/// The ticks of a ruler that shows the drawing from `lo` to `hi`, at
/// `per_unit` window px a unit and the display's `scale`.
pub fn ticks(lo: f64, hi: f64, per_unit: f64, scale: f64) -> Vec<Tick> {
    let step = step(per_unit, LABEL_GAP * scale);
    let parts = parts(step, per_unit, TICK_GAP * scale);
    let small = step / parts as f64;
    let (first, last) = ((lo.min(hi) / small - 1e-6).ceil() as i64, (lo.max(hi) / small + 1e-6).floor() as i64);
    // A small tick is a whole number of units, or a whole part of one:
    // counted so, the thirtieth tenth is 3, not a sum of thirty tenths.
    let at = |k: i64| if small < 1.0 { k as f64 / (1.0 / small).round() } else { k as f64 * small.round() };
    (first..=last).map(|k| Tick { at: at(k), label: (k.rem_euclid(parts as i64) == 0).then(|| label(at(k), step)) }).collect()
}

/// Both rulers and the square where they meet. `pointer`: where it is,
/// window px, while it's somewhere the rulers measure.
pub fn draw(ui: &mut Ui, l: &Layout, view: &View, pointer: Option<Vec2>) {
    if l.ruler_top.is_empty() || l.ruler_left.is_empty() {
        return;
    }
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round().max(1.0);
    let style = theme::text(ui, FONT_SM);
    let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);
    let edge = px(2.0);
    for r in [l.ruler_top, l.ruler_left, l.ruler_corner] {
        ui.draw.rect(r, PANEL);
    }

    // Along the top: a number stands right of its tick.
    let top = l.ruler_top;
    let (lo, hi) = (view.to_doc.apply(top.min).x, view.to_doc.apply(top.max).x);
    ui.draw.push_clip(top);
    for tick in ticks(lo, hi, per_unit, s) {
        let x = view.to_window.apply(Vec2::new(tick.at, 0.0)).x.round();
        match &tick.label {
            Some(label) => {
                ui.draw.rect(Rect::from_xywh(x, top.min.y, px(1.0), top.height()), TEXT_DIM);
                let w = ui.measure(label, &style);
                ui.text_in_rect(label, &style, Rect::from_xywh(x + px(5.0), top.min.y, w, top.height() - edge), TEXT);
            }
            None => ui.draw.rect(Rect::from_xywh(x, top.max.y - px(8.0), px(1.0), px(8.0)), TEXT_DIM),
        }
    }
    if let Some(p) = pointer.filter(|p| p.x >= top.min.x && p.x <= top.max.x) {
        ui.draw.rect(Rect::from_xywh(p.x.round() - edge / 2.0, top.min.y, edge, top.height()), ACCENT);
    }
    ui.draw.pop_clip();

    // Down the left: a number's digits stand under its tick, one
    // under the other.
    let left = l.ruler_left;
    let (lo, hi) = (view.to_doc.apply(left.min).y, view.to_doc.apply(left.max).y);
    let line = px(FONT_SM);
    ui.draw.push_clip(left);
    for tick in ticks(lo, hi, per_unit, s) {
        let y = view.to_window.apply(Vec2::new(0.0, tick.at)).y.round();
        match &tick.label {
            Some(label) => {
                ui.draw.rect(Rect::from_xywh(left.min.x, y, left.width(), px(1.0)), TEXT_DIM);
                for (i, c) in label.chars().enumerate() {
                    let cell = Rect::from_xywh(left.min.x, y + px(3.0) + line * i as f64, left.width() - edge, line);
                    ui.text_centered(c.encode_utf8(&mut [0; 4]), &style, cell, TEXT);
                }
            }
            None => ui.draw.rect(Rect::from_xywh(left.max.x - px(8.0), y, px(8.0), px(1.0)), TEXT_DIM),
        }
    }
    if let Some(p) = pointer.filter(|p| p.y >= left.min.y && p.y <= left.max.y) {
        ui.draw.rect(Rect::from_xywh(left.min.x, p.y.round() - edge / 2.0, left.width(), edge), ACCENT);
    }
    ui.draw.pop_clip();

    // A 2 px line between each ruler and the canvas, and round the
    // square where they meet.
    ui.draw.rect(Rect::from_xywh(top.min.x, top.max.y - edge, top.width(), edge), BORDER);
    ui.draw.rect(Rect::from_xywh(left.max.x - edge, left.min.y, edge, left.height()), BORDER);
    ui.draw.stroke_rect(l.ruler_corner, edge, 0.0, BORDER);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_stand_at_round_steps_far_enough_apart() {
        // An icon's page filling a screen: every two units. A big page
        // fitted: every fifty. Zoomed right in: halves, fifths.
        assert_eq!((step(37.0, 70.0), step(70.0, 70.0), step(1.7, 70.0), step(8.0, 70.0), step(200.0, 70.0), step(500.0, 70.0)), (2.0, 1.0, 50.0, 10.0, 0.5, 0.2));
        assert_eq!((step(7.0, 70.0), step(0.02, 70.0)), (10.0, 5000.0));
        // The small ticks: tenths where there's room, else fewer.
        assert_eq!((parts(2.0, 37.0, 7.0), parts(1.0, 70.0, 7.0), parts(50.0, 1.7, 7.0), parts(10.0, 8.0, 7.0), parts(0.5, 200.0, 7.0), parts(1.0, 20.0, 7.0), parts(5.0, 1.0, 7.0)), (4, 10, 5, 10, 5, 2, 1));
        assert_eq!((label(12.0, 2.0), label(-4.0, 2.0), label(0.5, 0.5), label(-0.0, 1.0), label(1.25, 0.05), label(-0.04, 0.5), label(3.0, 0.5)), ("12".to_owned(), "-4".to_owned(), "0.5".to_owned(), "0".to_owned(), "1.25".to_owned(), "0".to_owned(), "3".to_owned()));
    }

    #[test]
    fn a_ruler_marks_what_shows() {
        // From -3 to 9 units at 37 px a unit: a number every two, and a
        // small tick every half between.
        let all = ticks(-3.0, 9.0, 37.0, 1.0);
        let numbers: Vec<(f64, &str)> = all.iter().filter_map(|t| Some((t.at, t.label.as_deref()?))).collect();
        assert_eq!(numbers, [(-2.0, "-2"), (0.0, "0"), (2.0, "2"), (4.0, "4"), (6.0, "6"), (8.0, "8")]);
        assert_eq!((all.len(), all[0].at, all[1].at, all.last().unwrap().at), (25, -3.0, -2.5, 9.0));
        // At a bigger display scale the numbers are further apart.
        assert_eq!(ticks(0.0, 10.0, 37.0, 1.4).iter().filter(|t| t.label.is_some()).count(), 3);
        // Parts of a unit don't drift: 3 is 3, not fifteen fifths added
        // up.
        assert!(ticks(0.0, 4.0, 500.0, 1.0).iter().any(|t| t.at == 3.0 && t.label.as_deref() == Some("3")));
    }
}
