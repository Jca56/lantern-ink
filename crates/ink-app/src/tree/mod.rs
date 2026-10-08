//! The object tree (ARCHITECTURE §8; LS3's layers panel): every element
//! of the drawing as a row, front to back, in the right panel. Groups
//! open and close; a row has its eye and its padlock; rows are picked,
//! renamed, and dragged to restack them or into and out of groups. It
//! draws from the document and never edits it: what it wants done
//! comes back as [`Intent`]s for the window to carry out through the
//! core.

mod drag;
mod rows;

use ink_doc::{Document, NodeId, Place};
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, FILL, Sense, Ui};

use crate::icons::Icons;
use crate::select::Selection;
use crate::theme::{self, ACCENT, BORDER, FONT_MD, TEXT_DIM};
use drag::Drag;

/// The panel's heading, logical px.
const HEAD: f64 = 40.0;

/// What the tree asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    /// Hide this node (`true`), or show it again.
    Hide(NodeId, bool),
    /// Lock it, or unlock it.
    Lock(NodeId, bool),
    /// Call it this (nothing: take its name off).
    Rename(NodeId, String),
    /// Put these, back to front, there.
    Move(Vec<NodeId>, Place),
}

/// What the tree keeps between frames.
#[derive(Default)]
pub struct Tree {
    /// The row pressed, whether it's being dragged, and whether letting
    /// go without a drag makes it the only one selected.
    drag: Option<(Drag, bool)>,
    /// A row to bring into sight when the list is next drawn.
    scroll_to: Option<NodeId>,
    /// Where each row was last drawn, the topmost first.
    #[cfg(test)]
    pub(crate) laid: Vec<(crate::select::Row, Rect)>,
}

impl Tree {
    /// Have `id`'s row in sight: something was picked on the canvas.
    pub fn show(&mut self, id: NodeId) {
        self.scroll_to = Some(id);
    }
}

/// Draw the tree in `r` (the right panel) for `shown`: the drawing as
/// it looks, and its tab's selection.
pub fn draw(ui: &mut Ui, r: Rect, st: &mut Tree, shown: Option<(&Document, &mut Selection)>, icons: &Icons) -> Vec<Intent> {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let mut out = Vec::new();
    let pad = px(10.0);
    let inner = Rect::new(Vec2::new(r.min.x + pad, r.min.y), Vec2::new(r.max.x - pad, r.max.y - px(6.0)));
    if inner.width() <= 0.0 || inner.height() <= 0.0 {
        return out;
    }
    ui.push_id("tree");
    let head = Rect::from_min_size(inner.min, Vec2::new(inner.width(), px(HEAD).min(inner.height())));
    ui.text_in_rect("Objects", &theme::text(ui, FONT_MD), head, TEXT_DIM);
    let rule = px(2.0).max(1.0);
    ui.draw.rect(Rect::from_min_size(Vec2::new(r.min.x, head.max.y), Vec2::new(r.width(), rule)), BORDER);
    let list = Rect::new(Vec2::new(inner.min.x, head.max.y + rule + px(6.0)), inner.max);
    let Some((document, sel)) = shown.filter(|_| list.height() > 0.0) else {
        st.drag = None;
        ui.pop_id();
        return out;
    };
    let rows = sel.rows(document);
    let dragging = st.drag.is_some_and(|(d, _)| d.live);
    let mut cx = rows::Cx { document, sel, icons, out: &mut out, pressed: None, dragging };
    let mut laid = Vec::with_capacity(rows.len());
    let layer = ui.layer();
    let scroll = ui.id("list");
    ui.child(list, layer, |ui| {
        ui.scroll_area("list", Some(list.height()), |ui| {
            let (row_h, view) = (px(rows::HEIGHT), ui.clip());
            for row in rows {
                let r = ui.alloc(Vec2::new(FILL, row_h));
                laid.push((row, r));
                // Out of sight: it keeps its place, and draws nothing.
                if r.max.y >= view.min.y && r.min.y <= view.max.y {
                    rows::draw(ui, r, row, &mut cx);
                }
            }
        });
        // What no row took: a press on the list's empty space lets go
        // of everything.
        if ui.interact(ui.id("empty"), list, Sense::CLICK).pressed {
            cx.sel.clear();
            ui.state.request_rebuild = true;
        }
    });
    let rows::Cx { sel, pressed, .. } = cx;
    // A row asked for that's out of sight: the list goes to it.
    if let Some((_, r)) = st.scroll_to.take().and_then(|id| laid.iter().find(|(row, _)| row.id == id)) {
        let by = if r.min.y < list.min.y {
            r.min.y - list.min.y
        } else if r.max.y > list.max.y {
            r.max.y - list.max.y
        } else {
            0.0
        };
        if by != 0.0 {
            let offset = &mut ui.state.scroll(scroll).offset.y;
            *offset = (*offset + by).max(0.0);
            ui.state.request_rebuild = true;
        }
    }
    #[cfg(test)]
    {
        st.laid.clone_from(&laid);
    }
    if let Some((id, settle)) = pressed {
        st.drag = Some((Drag { id, live: false }, settle));
    }
    // The pressed row, dragged (with every other selected one): a line
    // where they would land; let go, they move there.
    if let Some((d, settle)) = st.drag.as_mut() {
        let (pointer, held) = (ui.state.pointer, ui.state.down);
        d.live |= held && (pointer.y - ui.state.press_pos.y).abs() > px(drag::THRESHOLD);
        let dragged = sel.tops(document);
        let target = if d.live { drag::target(document, sel, &laid, &dragged, pointer, px(rows::INDENT)) } else { None };
        if d.live && held {
            ui.state.cursor_icon = CursorIcon::Grabbing;
            if let Some(t) = target {
                ui.draw.push_clip(list);
                ui.draw.line(t.line.0, t.line.1, px(3.0).max(1.0), ACCENT);
                ui.draw.pop_clip();
            }
            // Near the list's top or bottom, it scrolls along.
            let edge = px(28.0);
            let nudge = if pointer.y < list.min.y + edge {
                -1.0
            } else if pointer.y > list.max.y - edge {
                1.0
            } else {
                0.0
            };
            if nudge != 0.0 {
                let offset = &mut ui.state.scroll(scroll).offset.y;
                *offset = (*offset + nudge * px(12.0)).max(0.0);
                ui.state.request_redraw_after(1.0 / 60.0);
            }
        }
        if !held {
            match (d.live, target) {
                (true, Some(t)) => out.push(Intent::Move(dragged, t.place)),
                // Pressed among several and let go where it was: it's
                // the one selected now.
                (false, _) if *settle => sel.select_only(d.id),
                _ => {}
            }
            st.drag = None;
        }
    }
    ui.pop_id();
    out
}
