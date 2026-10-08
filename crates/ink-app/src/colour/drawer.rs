//! The palettes (LS3's `colour/drawer.rs`): under the palette grid, a
//! list of them with New, Duplicate, Rename and Delete above it. A
//! press on a row shows that palette in the grid. The built-in Default
//! is first and takes no rename and no delete. A new or duplicated one
//! is named at once, in its row.

use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui};

use super::palettes::Library;
use crate::chrome::{Look, text_button};
use crate::theme::{self, ACCENT, BORDER, FONT_BASE, FONT_MD, FONT_SM, INPUT_BG, LAYER_ROW, LAYER_ROW_BORDER, LAYER_ROW_HOVER, TAB_ACTIVE, TEXT, TEXT_DIM};

/// A palette's row, and its preview's cells, logical px (LS3's).
const ROW: f64 = 44.0;
const CELL: (f64, f64) = (12.0, 26.0);
const CELLS: usize = 14;

#[derive(Default)]
pub struct Drawer {
    pub open: bool,
    /// The palette being renamed, and its name so far.
    pub renaming: Option<(usize, String)>,
    /// The rename has only just begun: its field takes the keys.
    fresh: bool,
}

#[derive(Default)]
pub struct Out {
    pub height: f64,
    pub changed: bool,
}

impl Drawer {
    /// Begin renaming palette `i`.
    pub fn rename(&mut self, i: usize, lib: &Library) {
        if let Some(p) = lib.palettes.get(i).filter(|_| !lib.is_locked(i)) {
            self.renaming = Some((i, p.name.clone()));
            self.fresh = true;
        }
    }
}

/// The drawer in the top of `r`, as tall as it comes out.
pub fn draw(ui: &mut Ui, r: Rect, st: &mut Drawer, lib: &mut Library) -> Out {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let mut out = Out::default();
    ui.push_id("palettes-drawer");
    ui.draw.rect(Rect::from_min_size(r.min, Vec2::new(r.width(), px(2.0).max(1.0))), BORDER);
    let mut y = r.min.y + px(10.0);
    let title = Rect::from_xywh(r.min.x, y, r.width(), px(30.0));
    ui.text_in_rect("P A L E T T E S", &theme::text(ui, FONT_MD).bold(), title, ACCENT);
    y = title.max.y + px(6.0);

    // New, Duplicate, Rename, Delete: the last two not for the built-in.
    let locked = lib.is_locked(lib.active);
    let style = theme::text(ui, FONT_SM);
    let (button_h, mut x) = (px(34.0), r.min.x);
    for (id, label, on) in [("new", "New", true), ("duplicate", "Duplicate", true), ("rename", "Rename", !locked), ("delete", "Delete", !locked)] {
        let w = ui.measure(label, &style).ceil() + px(18.0);
        let b = Rect::from_xywh(x, y, w, button_h);
        x += w + px(6.0);
        ui.draw.rounded_rect(b, px(4.0), theme::BUTTON);
        if !on {
            ui.text_centered(label, &style, b, TEXT_DIM.with_alpha(0.5));
            continue;
        }
        let look = Look { size: FONT_SM, ink: TEXT, hover_ink: TEXT, hover: theme::BUTTON_HOVER, radius: 4.0 };
        if !text_button(ui, id, b, label, &look).clicked {
            continue;
        }
        match id {
            "new" => {
                lib.add_empty();
                st.rename(lib.active, lib);
            }
            "duplicate" => {
                lib.duplicate();
                st.rename(lib.active, lib);
            }
            "rename" => st.rename(lib.active, lib),
            _ => {
                lib.remove(lib.active);
                st.renaming = None;
            }
        }
        out.changed = true;
        ui.state.request_rebuild = true;
    }
    y += button_h + px(8.0);

    // The list.
    let name_style = theme::text(ui, FONT_BASE);
    for i in 0..lib.palettes.len() {
        let row = Rect::from_xywh(r.min.x, y, r.width(), px(ROW));
        y += px(ROW) + px(6.0);
        ui.push_index(i);
        let renaming = st.renaming.as_ref().is_some_and(|(at, _)| *at == i);
        let active = i == lib.active;
        let over = ui.state.pointer_in_window && row.contains(ui.state.pointer) && !ui.state.shielded(ui.layer(), ui.state.pointer);
        // A row's sheen; the palette shown is lit as a tab is.
        let ground = theme::sheen(if active { TAB_ACTIVE } else if over { LAYER_ROW_HOVER } else { LAYER_ROW });
        ui.draw.rounded_rect_gradient(row, px(5.0), ground.top, ground.bottom);
        ui.draw.stroke_rect(row, px(2.0).max(1.0), px(5.0), if active { ACCENT } else { LAYER_ROW_BORDER });
        let inner = row.shrink(px(8.0));
        // The dot of the one the grid shows.
        if active {
            ui.draw.circle(Vec2::new(inner.min.x + px(5.0), inner.center().y), px(5.0), ACCENT);
        }
        // A strip of its first colours, at the right.
        let palette = &lib.palettes[i];
        let (cw, ch) = (px(CELL.0), px(CELL.1));
        let shown = palette.colors.len().min(CELLS);
        let strip_w = cw * CELLS as f64;
        let strip_x = (inner.max.x - strip_w).max(inner.min.x + px(60.0));
        ui.draw.push_clip(Rect::new(Vec2::new(strip_x, row.min.y), row.max));
        for (k, &color) in palette.colors.iter().take(shown).enumerate() {
            ui.draw.rect(Rect::from_xywh(strip_x + cw * k as f64, inner.center().y - ch / 2.0, cw, ch), color);
        }
        ui.draw.pop_clip();
        let words = Rect::new(Vec2::new(inner.min.x + px(18.0), inner.min.y), Vec2::new(strip_x - px(8.0), inner.max.y));
        if renaming {
            let id = ui.id("name");
            if std::mem::take(&mut st.fresh) {
                ui.state.focus = Some(id);
                let len = st.renaming.as_ref().map_or(0, |(_, t)| t.len());
                let edit = ui.state.text_edit(id);
                (edit.anchor, edit.cursor) = (0, len);
            }
            if let Some((_, text)) = st.renaming.as_mut() {
                ui.draw.rect(words, INPUT_BG);
                let resp = ui.text_edit_core(id, words, text);
                ui.draw.stroke_rect(words, px(2.0).max(1.0), 0.0, ACCENT);
                if resp.cancelled {
                    st.renaming = None;
                } else if resp.committed || !resp.focused {
                    if let Some((at, text)) = st.renaming.take() {
                        lib.rename(at, &text);
                        out.changed = true;
                    }
                    if ui.state.focus == Some(id) {
                        ui.state.focus = None;
                    }
                }
            }
        } else {
            ui.draw.push_clip(words);
            ui.text_in_rect(&palette.name, &name_style, words, TEXT);
            ui.draw.pop_clip();
            let resp = ui.interact(ui.id("row"), row, Sense::CLICK);
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            if resp.pressed && !active {
                lib.select(i);
                out.changed = true;
                ui.state.request_rebuild = true;
            }
        }
        ui.pop_id();
    }
    ui.pop_id();
    out.height = y - r.min.y;
    out
}
