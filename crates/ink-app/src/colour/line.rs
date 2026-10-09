//! The rows under Stroke in the paint section: the line's width, its
//! ends and its corners, its dashes; and under those, how see-through
//! the whole thing is. The line's rows wait, dim, while there's no
//! stroke to draw with them.

use ink_geom::{Cap, Join};
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui};

use crate::controls::{self, Slider, written};
use crate::paint::{Line, Set};
use crate::theme::{self, ACCENT, BORDER, FONT_PANEL, INPUT_BG, LAYER_ROW_BORDER, TEXT, TEXT_DIM};

/// A row, a button of a three-way choice, and the least the section is
/// wide for two things to share a row: logical px.
const ROW: f64 = 50.0;
const BUTTON: (f64, f64) = (42.0, 38.0);
const PAIR_FROM: f64 = 376.0;

/// What these rows keep between frames.
#[derive(Default)]
pub struct Rows {
    /// The dashes as they're being typed.
    typing: Option<String>,
    /// Where each was last drawn, by name.
    #[cfg(test)]
    pub(crate) laid: Vec<(&'static str, Rect)>,
}

/// The dashes a person typed: lengths split by spaces or commas. None
/// for what isn't that; no lengths at all for a solid line.
pub fn dashes_typed(text: &str) -> Option<Vec<f64>> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("none") || text.eq_ignore_ascii_case("solid") {
        return Some(Vec::new());
    }
    let lengths: Option<Vec<f64>> = text.split(|c: char| c == ',' || c.is_whitespace()).filter(|part| !part.is_empty()).map(|part| part.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)).collect();
    // All gaps and no line draws nothing: not what anyone means.
    lengths.filter(|l| l.iter().any(|v| *v > 0.0))
}

/// The dashes as the row says them.
pub fn dashes_said(dashes: &[f64]) -> String {
    if dashes.is_empty() { "Solid".to_owned() } else { dashes.iter().map(|d| written(*d, 3)).collect::<Vec<_>>().join(" ") }
}

/// How an end of a line looks: the line coming in from the left to
/// where its path ends (the gold dot), flat there, rounded past it, or
/// squared off past it.
fn cap_glyph(ui: &mut Ui, r: Rect, cap: Cap, ink: Color) {
    let s = ui.m.scale;
    let t = (10.0 * s).round();
    let (end, cy) = (r.center().x.round(), r.center().y.round());
    ui.draw.rect(Rect::new(Vec2::new(r.min.x + 5.0 * s, cy - t / 2.0), Vec2::new(end, cy + t / 2.0)), ink);
    match cap {
        Cap::Butt => {}
        Cap::Round => ui.draw.circle(Vec2::new(end, cy), t / 2.0, ink),
        Cap::Square => ui.draw.rect(Rect::new(Vec2::new(end, cy - t / 2.0), Vec2::new(end + t / 2.0, cy + t / 2.0)), ink),
    }
    ui.draw.circle(Vec2::new(end, cy), (2.5 * s).max(1.0), ACCENT);
}

/// How a corner looks: a line turning at the top left, pointed there,
/// rounded, or cut off.
fn join_glyph(ui: &mut Ui, r: Rect, join: Join, ink: Color) {
    let s = ui.m.scale;
    let t = (9.0 * s).round();
    let g = Rect::from_center_size(r.center(), Vec2::splat((22.0 * s).round()));
    let (x0, y0) = (g.min.x.round(), g.min.y.round());
    let h = t / 2.0;
    // Its two arms, from the middle of the corner on.
    ui.draw.rect(Rect::new(Vec2::new(x0 + h, y0), Vec2::new(g.max.x, y0 + t)), ink);
    ui.draw.rect(Rect::new(Vec2::new(x0, y0 + h), Vec2::new(x0 + t, g.max.y)), ink);
    match join {
        Join::Miter => ui.draw.rect(Rect::new(Vec2::new(x0, y0), Vec2::new(x0 + h, y0 + h)), ink),
        Join::Round => ui.draw.circle(Vec2::new(x0 + h, y0 + h), h, ink),
        Join::Bevel => ui.draw.triangle(Vec2::new(x0 + h, y0), Vec2::new(x0, y0 + h), Vec2::new(x0 + h, y0 + h), ink),
    }
}

/// One of a three-way choice: lit when it's the one chosen.
fn choice(ui: &mut Ui, id: &str, r: Rect, on: bool, live: bool, glyph: impl FnOnce(&mut Ui, Rect, Color)) -> bool {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round().max(1.0);
    let resp = live.then(|| ui.interact(ui.id(id), r, Sense::CLICK));
    let hovered = resp.as_ref().is_some_and(|r| r.hovered);
    ui.draw.rounded_rect(r, px(4.0), if hovered { theme::BUTTON_HOVER } else { theme::BUTTON });
    let dim = if live { 1.0 } else { 0.35 };
    ui.draw.stroke_rect(r, px(if on { 3.0 } else { 2.0 }), px(4.0), if on { ACCENT.with_alpha(dim) } else if hovered { ACCENT } else { BORDER });
    glyph(ui, r, TEXT.with_alpha(dim));
    if hovered {
        ui.state.cursor_icon = CursorIcon::Pointer;
    }
    resp.is_some_and(|r| r.clicked && !on)
}

/// The rows, from `y` down in `inner`, for a line that's `line` (and
/// `stroked`: there's a stroke for it to be the line of) on something
/// `opacity` see-through. `step`: what a pixel along Width changes it
/// by. Where they end, and what was set.
#[allow(clippy::too_many_arguments)]
pub fn draw(ui: &mut Ui, inner: Rect, mut y: f64, st: &mut Rows, line: &Line, stroked: bool, opacity: f64, step: f64) -> (f64, Option<Set>) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let mut set = None;
    let style = theme::text(ui, FONT_PANEL);
    let dim = if stroked { 1.0 } else { 0.35 };
    #[cfg(test)]
    st.laid.clear();
    ui.push_id("line");
    let (bw, bh, gap) = (px(BUTTON.0), px(BUTTON.1), px(4.0));
    let indent = px(14.0);
    let left = inner.min.x + indent;
    let pair = inner.width() >= px(PAIR_FROM);
    // What each of the four takes across: a number's well, or a name
    // and three buttons.
    let three = |ui: &mut Ui, name: &str| ui.measure(name, &style).ceil() + px(8.0) + bw * 3.0 + gap * 2.0;
    let (caps_w, joins_w) = (three(ui, "Caps"), three(ui, "Joins"));
    let rows: [Rect; 4] = if pair {
        let (a, b) = (Rect::from_xywh(left, y, inner.max.x - left, px(ROW)), Rect::from_xywh(left, y + px(ROW) + px(4.0), inner.max.x - left, px(ROW)));
        let between = px(12.0);
        [
            Rect::new(a.min, Vec2::new(a.max.x - caps_w - between, a.max.y)),
            Rect::new(Vec2::new(a.max.x - caps_w, a.min.y), a.max),
            Rect::new(b.min, Vec2::new(b.min.x + joins_w, b.max.y)),
            Rect::new(Vec2::new(b.min.x + joins_w + between, b.min.y), b.max),
        ]
    } else {
        [0, 1, 2, 3].map(|i| Rect::from_xywh(left, y + (px(ROW) + px(4.0)) * i as f64, inner.max.x - left, px(ROW)))
    };
    y = rows[3].max.y + px(4.0);
    let well_in = |row: Rect| Rect::from_xywh(row.min.x, (row.center().y - bh / 2.0).round(), row.width(), bh);

    // Width: dragged along or typed, in the drawing's units.
    let width_r = well_in(rows[0]);
    #[cfg(test)]
    st.laid.push(("Width", width_r));
    if stroked {
        let mut w = line.width;
        if controls::number_in(ui, ui.id("Width"), width_r, "Width", &mut w, step, Some((0.0, f64::INFINITY)), 3) {
            set = Some(Set::Width(w));
        }
    } else {
        ui.draw.rounded_rect(width_r, px(5.0), INPUT_BG);
        ui.draw.stroke_rect(width_r, px(2.0).max(1.0), px(5.0), LAYER_ROW_BORDER.with_alpha(0.5));
        ui.text_in_rect("Width", &style, Rect::new(Vec2::new(width_r.min.x + px(8.0), width_r.min.y), width_r.max), TEXT_DIM.with_alpha(0.5));
    }

    // Caps and joins: three ways each.
    let buttons = |row: Rect, i: usize| Rect::from_xywh(row.max.x - bw * (3 - i) as f64 - gap * (2 - i) as f64, (row.center().y - bh / 2.0).round(), bw, bh);
    ui.text_in_rect("Caps", &style, rows[1], TEXT.with_alpha(dim));
    for (i, (name, cap)) in [("butt", Cap::Butt), ("round", Cap::Round), ("square", Cap::Square)].into_iter().enumerate() {
        let r = buttons(rows[1], i);
        #[cfg(test)]
        st.laid.push((["Caps butt", "Caps round", "Caps square"][i], r));
        if choice(ui, &format!("cap-{name}"), r, stroked && line.cap == cap, stroked, |ui, r, ink| cap_glyph(ui, r, cap, ink)) {
            set = Some(Set::Cap(cap));
        }
    }
    ui.text_in_rect("Joins", &style, rows[2], TEXT.with_alpha(dim));
    for (i, (name, join)) in [("miter", Join::Miter), ("round", Join::Round), ("bevel", Join::Bevel)].into_iter().enumerate() {
        let r = Rect::from_xywh(rows[2].min.x + joins_w - bw * (3 - i) as f64 - gap * (2 - i) as f64, (rows[2].center().y - bh / 2.0).round(), bw, bh);
        #[cfg(test)]
        st.laid.push((["Joins miter", "Joins round", "Joins bevel"][i], r));
        if choice(ui, &format!("join-{name}"), r, stroked && line.join == join, stroked, |ui, r, ink| join_glyph(ui, r, join, ink)) {
            set = Some(Set::Join(join));
        }
    }

    // Dashes: typed as lengths, on then off ("4 2"); nothing is solid.
    let dash_r = well_in(rows[3]);
    #[cfg(test)]
    st.laid.push(("Dashes", dash_r));
    let id = ui.id("Dashes");
    let name_w = ui.measure("Dashes", &style).ceil() + px(16.0);
    let typed_r = Rect::new(Vec2::new((dash_r.min.x + name_w).min(dash_r.max.x), dash_r.min.y + px(3.0)), Vec2::new(dash_r.max.x - px(4.0), dash_r.max.y - px(3.0)));
    let editing = stroked && ui.state.has_focus(id);
    // Typed, and then the keyboard went elsewhere: it's entered.
    if !editing
        && let Some(lengths) = st.typing.take().as_deref().and_then(dashes_typed).filter(|l| *l != line.dashes && stroked)
    {
        set = Some(Set::Dashes(lengths));
    }
    ui.draw.rounded_rect(dash_r, px(5.0), INPUT_BG);
    if stroked {
        let resp = ui.interact(ui.id("dashes-well"), dash_r, Sense::CLICK);
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Text;
        }
        if resp.clicked && !editing {
            // Taken up: what it says now, all of it selected.
            let text = if line.dashes.is_empty() { String::new() } else { dashes_said(&line.dashes) };
            ui.state.focus = Some(id);
            let edit = ui.state.text_edit(id);
            (edit.anchor, edit.cursor) = (0, text.len());
            st.typing = Some(text);
            ui.state.request_rebuild = true;
        }
        ui.draw.stroke_rect(dash_r, px(2.0).max(1.0), px(5.0), if editing || resp.hovered { ACCENT } else { LAYER_ROW_BORDER });
    } else {
        ui.draw.stroke_rect(dash_r, px(2.0).max(1.0), px(5.0), LAYER_ROW_BORDER.with_alpha(0.5));
    }
    ui.text_in_rect("Dashes", &style, Rect::new(Vec2::new(dash_r.min.x + px(8.0), dash_r.min.y), dash_r.max), TEXT_DIM.with_alpha(dim));
    if editing {
        let mut text = st.typing.take().unwrap_or_default();
        let edit = ui.text_edit_core(id, typed_r, &mut text);
        if edit.committed {
            if let Some(lengths) = dashes_typed(&text).filter(|l| *l != line.dashes) {
                set = Some(Set::Dashes(lengths));
            }
            ui.state.focus = None;
            ui.state.request_rebuild = true;
        } else if edit.cancelled {
            ui.state.focus = None;
            ui.state.request_rebuild = true;
        } else {
            st.typing = Some(text);
        }
    } else {
        ui.text_right(&dashes_said(&line.dashes), &style, Rect::new(typed_r.min, Vec2::new(typed_r.max.x - px(4.0), typed_r.max.y)), TEXT.with_alpha(dim));
    }
    ui.pop_id();

    // Opacity: of the whole thing, line and fill and what it holds.
    y += px(4.0);
    let row = Rect::from_xywh(inner.min.x, y, inner.width(), px(ROW));
    let label_w = ui.measure("Opacity", &style).ceil() + px(4.0);
    ui.text_in_rect("Opacity", &style, Rect::from_min_size(row.min, Vec2::new(label_w, row.height())), TEXT);
    let rail = Rect::new(Vec2::new(row.min.x + label_w + px(8.0), row.min.y + px(4.0)), Vec2::new(row.max.x, row.max.y - px(4.0)));
    #[cfg(test)]
    st.laid.push(("Opacity", rail));
    let mut percent = (opacity * 100.0).round();
    if Slider::new(0.0, 100.0, 1.0).unit("%").rest(100.0).in_row(ui, ui.id("Opacity"), rail, &mut percent) {
        set = Some(Set::Opacity(percent / 100.0));
    }
    (row.max.y + px(8.0), set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_are_typed_as_lengths_and_said_back_the_same() {
        assert_eq!(dashes_typed("4 2"), Some(vec![4.0, 2.0]));
        assert_eq!(dashes_typed(" 1.5,0.5 , 3 "), Some(vec![1.5, 0.5, 3.0]));
        assert_eq!((dashes_typed(""), dashes_typed("none"), dashes_typed("Solid")), (Some(vec![]), Some(vec![]), Some(vec![])));
        // What isn't lengths, a length under nothing, all gaps: no.
        assert_eq!((dashes_typed("4 two"), dashes_typed("4 -2"), dashes_typed("0 0")), (None, None, None));
        assert_eq!((dashes_said(&[4.0, 2.0]), dashes_said(&[1.25, 0.5]), dashes_said(&[])), ("4 2".to_owned(), "1.25 0.5".to_owned(), "Solid".to_owned()));
    }
}
