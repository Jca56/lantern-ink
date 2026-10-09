//! A row of the object tree (LS3's layer rows): a card with, left to
//! right, what opens it (when it holds anything), its kind's picture
//! and its name, then its padlock and its eye. A press selects (Ctrl
//! adds or takes away, Shift takes the run between), a second renames
//! in place.

use ink_doc::{Document, Kind, Node, NodeId};
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui};

use super::Intent;
use crate::icons::{Glyph, Icons};
use crate::select::{self, Click, Row, Selection};
use crate::theme::{self, ACCENT, FONT_PANEL, INPUT_BG, LAYER_ROW, LAYER_ROW_BORDER, LAYER_ROW_HOVER, TAB_ACTIVE, TEXT, TEXT_DIM};

/// A row's height, how far what a row holds stands in from it, and
/// its buttons, logical px.
pub const HEIGHT: f64 = 52.0;
pub const INDENT: f64 = 16.0;
const DISCLOSURE: f64 = 28.0;
const BUTTON: f64 = 40.0;

/// What drawing rows needs.
pub struct Cx<'a> {
    pub document: &'a Document,
    pub sel: &'a mut Selection,
    pub icons: &'a Icons,
    pub out: &'a mut Vec<Intent>,
    /// The row a plain press landed on this frame (it may become a
    /// drag), and whether the press left the selection for the release
    /// to settle (a row among several selected: all of them may be
    /// about to be dragged).
    pub pressed: Option<(NodeId, bool)>,
    /// A right press landed on a row this frame.
    pub menu: bool,
    /// Rows are being dragged: none lights up under the pointer.
    pub dragging: bool,
}

/// The picture of what kind of thing a row is.
fn kind_glyph(node: &Node) -> Option<Glyph> {
    Some(match node.kind {
        Kind::G | Kind::Svg | Kind::A | Kind::Switch => Glyph::Group,
        Kind::Rect => Glyph::Rect,
        Kind::Circle | Kind::Ellipse => Glyph::Ellipse,
        Kind::Line => Glyph::Line,
        Kind::Polygon | Kind::Polyline => Glyph::Polygon,
        Kind::Path => Glyph::Path,
        Kind::Text => Glyph::Text,
        Kind::LinearGradient | Kind::RadialGradient => Glyph::Gradient,
        _ => return None,
    })
}

/// `which` in the middle of `r`, pixel for pixel.
fn glyph(ui: &mut Ui, r: Rect, icons: &Icons, which: Glyph, tint: Color) {
    let Some(image) = icons.glyph(which) else { return };
    let size = Vec2::new(image.width as f64, image.height as f64);
    let at = Vec2::new((r.center().x - size.x / 2.0).round(), (r.center().y - size.y / 2.0).round());
    ui.draw.image(Rect::from_min_size(at, size), image, 0.0, tint);
}

/// A small flat button of a row: its hover ground, the pointer.
fn button(ui: &mut Ui, id: &str, r: Rect) -> lntrn_ui::Response {
    let resp = ui.interact(ui.id(id), r, Sense::CLICK);
    if resp.hovered {
        ui.draw.rounded_rect(r, (4.0 * ui.m.scale).round(), Color::rgba(1.0, 1.0, 1.0, 0.10));
        ui.state.cursor_icon = CursorIcon::Pointer;
    }
    resp
}

/// A padlock in `r`: shut and gold when the row is locked, gold in
/// outline when a row it's inside holds it, a faint open one otherwise.
fn padlock(ui: &mut Ui, r: Rect, own: bool, held: bool) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let (c, w) = (r.center(), px(2.5).max(1.0));
    let body = Rect::from_xywh(c.x - px(8.0), c.y - px(2.0), px(16.0), px(12.0));
    let ink = if own || held { ACCENT } else { TEXT_DIM.with_alpha(0.55) };
    // Its shackle: down in the body when shut, lifted when open.
    let lift = if own || held { 0.0 } else { px(3.0) };
    ui.draw.arc(Vec2::new(c.x, c.y - px(2.0) - lift), px(5.5), std::f64::consts::PI, std::f64::consts::TAU, w, ink);
    if own {
        ui.draw.rounded_rect(body, px(2.0), ink);
    } else {
        ui.draw.stroke_rect(body, px(2.0).max(1.0), px(2.0), ink);
    }
}

/// One row in `r`.
pub fn draw(ui: &mut Ui, r: Rect, row: Row, cx: &mut Cx) {
    let document = cx.document;
    let Some(node) = document.get(row.id) else { return };
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let card = Rect::new(Vec2::new(r.min.x + px(INDENT) * row.depth as f64, r.min.y), r.max);
    if card.width() < px(80.0) {
        return;
    }
    let (active, selected) = (cx.sel.active == Some(row.id), cx.sel.is_selected(row.id));
    let renaming = cx.sel.renaming.as_ref().is_some_and(|(id, _)| *id == row.id);
    let (name, _) = select::name_of(document, row.id);
    let hidden = select::is_hidden(document, row.id) || document.ancestors(row.id).any(|n| select::is_hidden(document, n.id));
    ui.push_id("row");
    ui.push_index(row.id.0 as usize);

    // Where everything is: what opens it and its kind from the left,
    // the eye and the padlock from the right, the name between.
    let inner = Rect::new(Vec2::new(card.min.x + px(6.0), card.min.y + px(3.0)), Vec2::new(card.max.x - px(6.0), card.max.y - px(3.0)));
    let disclosure = Rect::from_xywh(inner.min.x, inner.min.y, px(DISCLOSURE), inner.height());
    let mut x = disclosure.max.x + px(2.0);
    let mut right = inner.max.x;
    let mut side_button = |on: bool| {
        on.then(|| {
            let r = Rect::from_xywh(right - px(BUTTON), inner.min.y, px(BUTTON), inner.height());
            right = r.min.x - px(2.0);
            r
        })
    };
    let eye = side_button(select::is_drawn(document, row.id));
    let lock = side_button(true);

    // The card first, so its buttons draw over it; they take their
    // presses before it does, below.
    let over = !cx.dragging && ui.state.pointer_in_window && !ui.state.shielded(ui.layer(), ui.state.pointer) && card.intersection(&ui.clip()).contains(ui.state.pointer);
    let (fill, edge, width) = match (active, selected, over) {
        (true, _, _) => (TAB_ACTIVE, ACCENT, 3.0),
        (false, true, _) => (LAYER_ROW_HOVER, ACCENT, 2.0),
        (false, false, true) => (LAYER_ROW_HOVER, LAYER_ROW_BORDER, 2.0),
        (false, false, false) => (LAYER_ROW, LAYER_ROW_BORDER, 2.0),
    };
    let radius = px(5.0);
    ui.draw.rounded_rect(card, radius, fill);
    ui.draw.stroke_rect(card, px(width).max(1.0), radius, edge);

    if row.holds {
        if button(ui, "disclosure", disclosure).clicked {
            cx.sel.toggle_open(row.id);
            ui.state.request_rebuild = true;
        }
        let (c, a) = (disclosure.center(), px(7.0));
        if row.open {
            ui.draw.triangle(Vec2::new(c.x - a, c.y - a * 0.6), Vec2::new(c.x + a, c.y - a * 0.6), Vec2::new(c.x, c.y + a * 0.8), ACCENT);
        } else {
            ui.draw.triangle(Vec2::new(c.x - a * 0.6, c.y - a), Vec2::new(c.x - a * 0.6, c.y + a), Vec2::new(c.x + a * 0.8, c.y), ACCENT);
        }
    }
    if let Some(e) = eye {
        let shut = select::is_hidden(document, row.id);
        if button(ui, "eye", e).clicked {
            cx.out.push(Intent::Hide(row.id, !shut));
        }
        glyph(ui, e, cx.icons, if shut { Glyph::Invisible } else { Glyph::Visible }, Color::WHITE);
    }
    if let Some(l) = lock {
        let own = document.is_locked(row.id);
        if button(ui, "lock", l).clicked {
            cx.out.push(Intent::Lock(row.id, !own));
        }
        padlock(ui, l, own, !own && document.lock_over(row.id).is_some());
    }

    // The name: its kind's picture, then the words; or the field,
    // renaming.
    if let Some(g) = kind_glyph(node) {
        let side = px(g.side());
        if right - x > side * 2.0 {
            glyph(ui, Rect::from_xywh(x, inner.center().y - side / 2.0, side, side), cx.icons, g, if hidden { Color::WHITE.with_alpha(0.5) } else { Color::WHITE });
            x += side + px(8.0);
        }
    }
    let words = Rect::new(Vec2::new(x, inner.min.y), Vec2::new(right.max(x), inner.max.y));
    if renaming {
        rename(ui, words, row, &name, cx);
    } else {
        ui.draw.push_clip(words);
        ui.text_in_rect(&name, &theme::text(ui, FONT_PANEL), words, if hidden { TEXT_DIM } else { TEXT });
        ui.draw.pop_clip();
    }

    // The row itself, last: whatever its buttons didn't take.
    let resp = ui.interact(ui.id("card"), card, Sense::CLICK);
    if resp.pressed && !renaming {
        let mods = ui.state.mods;
        let how = if mods.shift() {
            Click::Range
        } else if mods.ctrl() {
            Click::Toggle
        } else {
            Click::Plain
        };
        if resp.double_clicked && how == Click::Plain {
            cx.sel.rename(row.id, &name);
        } else if how == Click::Plain && selected && cx.sel.nodes.len() > 1 {
            // One of several: they may all be about to be dragged. If
            // not, letting go makes it the only one.
            cx.sel.active = Some(row.id);
            cx.pressed = Some((row.id, true));
        } else {
            cx.sel.click(document, row.id, how);
            if how == Click::Plain {
                cx.pressed = Some((row.id, false));
            }
        }
        ui.state.request_rebuild = true;
    }
    // A right press: the row's menu, for the selection it's in (or it
    // alone, if it wasn't in one).
    if !renaming && ui.state.right_pressed && !ui.state.shielded(ui.layer(), ui.state.right_press_pos) && card.intersection(&ui.clip()).contains(ui.state.right_press_pos) {
        if !cx.sel.is_selected(row.id) {
            cx.sel.click(document, row.id, Click::Plain);
        } else {
            cx.sel.active = Some(row.id);
        }
        cx.menu = true;
        ui.state.request_rebuild = true;
    }
    ui.pop_id();
    ui.pop_id();
}

/// The name as a field: Enter or a press anywhere else keeps it,
/// Escape drops it.
fn rename(ui: &mut Ui, words: Rect, row: Row, was: &str, cx: &mut Cx) {
    let id = ui.id("rename");
    if std::mem::take(&mut cx.sel.rename_fresh) {
        // The field is this row's: it takes the keys from now, with the
        // whole name selected.
        ui.state.focus = Some(id);
        let len = cx.sel.renaming.as_ref().map_or(0, |(_, t)| t.len());
        let edit = ui.state.text_edit(id);
        (edit.anchor, edit.cursor) = (0, len);
    }
    let Some((_, text)) = cx.sel.renaming.as_mut() else { return };
    let h = ui.m.widget_h.min(words.height());
    let field = Rect::from_xywh(words.min.x, words.center().y - h / 2.0, words.width(), h);
    ui.draw.rect(field, INPUT_BG);
    let resp = ui.text_edit_core(id, field, text);
    ui.draw.stroke_rect(field, (2.0 * ui.m.scale).round().max(1.0), 0.0, ACCENT);
    if resp.cancelled {
        cx.sel.renaming = None;
    } else if resp.committed || !resp.focused {
        if let Some((_, text)) = cx.sel.renaming.take()
            && text.trim() != was
        {
            cx.out.push(Intent::Rename(row.id, text.trim().to_owned()));
        }
        if ui.state.focus == Some(id) {
            ui.state.focus = None;
        }
    }
}
