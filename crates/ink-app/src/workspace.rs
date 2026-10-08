//! One frame of the window's one area (LS3's `workspace.rs`): the
//! chrome, the panel's frame, and the canvas with the tool in hand.

use ink_core::DocId;
use ink_doc::Viewport;
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{AreaCx, CursorIcon, Ui};

use crate::camera::{CLICK_STEP, Camera};
use crate::canvas::{self, Hold};
use crate::chrome::panel::Resize;
use crate::chrome::status::{Click, Status};
use crate::chrome::tabs::{TabClick, TabLabel};
use crate::ink::{Ink, TOAST_SECONDS};
use crate::layout::Layout;
use crate::pointer::View;
use crate::tools::Tool;
use crate::{chrome, overlay, page, tree};

impl Ink {
    pub(crate) fn draw_workspace(&mut self, ui: &mut Ui, cx: &mut AreaCx<()>) {
        self.finished();
        // One of the shell's own was up (a menu, a dialog): the press
        // that closes it does nothing else here, on the toolbar, the
        // tabs or the canvas (the menu bar is the shell's, and has had
        // it).
        let popup = ui.state.shields().iter().any(|(_, layer)| *layer >= lntrn_ui::POPUP_LAYER);
        if popup && ui.state.pressed {
            ui.state.press_claimed = true;
        }
        let now = ui.now();
        if let Some(text) = self.pending_toast.take() {
            self.toast = Some((text, now + TOAST_SECONDS));
        }
        if let Some((_, until)) = &self.toast {
            if now >= *until {
                self.toast = None;
            } else {
                ui.state.request_redraw_after(until - now);
            }
        }
        let l = Layout::new(ui.clip(), ui.m.scale, self.panel_drag.unwrap_or(self.settings.panel_width));
        self.layout = l;
        self.scale = ui.m.scale;

        // First, so its flyout has the pointer before what it floats over.
        chrome::toolbar::draw(ui, l.toolbar, &mut self.tools, &self.icons, &mut self.toolbar);
        chrome::rainbow(ui, l.strip_v);
        chrome::rainbow(ui, l.strip_h);
        match chrome::panel::draw(ui, &l, self.panel_drag.unwrap_or(self.settings.panel_width), &mut self.grip) {
            Some(Resize::To(w)) => self.panel_drag = Some(w),
            Some(Resize::Settled(w)) => {
                self.panel_drag = None;
                if w != self.settings.panel_width {
                    self.settings.panel_width = w;
                    self.settings.save();
                }
            }
            None => {}
        }
        // The object tree, of the drawing as it looks: what it asks for
        // is done once it's drawn.
        let shown = self.tabs.active_mut().and_then(|tab| {
            let (drawing, _) = self.core.shown(tab.doc).ok()?;
            tab.selection.prune(drawing);
            Some((tab.doc, drawing, &mut tab.selection))
        });
        let doc = shown.as_ref().map(|(doc, ..)| *doc);
        let asked = tree::draw(ui, l.panel, &mut self.tree, shown.map(|(_, drawing, selection)| (drawing, selection)), &self.icons);
        if let Some(doc) = doc {
            for intent in asked {
                self.tree_asked(doc, intent);
            }
        }

        let labels: Vec<TabLabel> = self.tabs.iter().map(|t| TabLabel { name: self.label(t.doc), modified: self.is_modified(t.doc) && !self.untouched(t.doc) }).collect();
        match chrome::tabs::draw(ui, l.tabs, &labels, self.tabs.active_index()) {
            Some(TabClick::Select(i)) => self.tabs.select(i),
            Some(TabClick::Close(i)) => {
                let doc = self.tabs.iter().nth(i).map(|t| t.doc);
                if let Some(doc) = doc {
                    self.ask_close(doc, &mut cx.host());
                }
            }
            Some(TabClick::New) => self.new_document(),
            None => {}
        }

        let viewport = self.viewport();
        let name = self.tabs.active_doc().map(|d| self.label(d)).unwrap_or_default();
        let zoom = self.tabs.active().and_then(|t| t.camera).map(|c| c.zoom);
        let tool = self.tools.active().tooltip();
        // How many px of the screen a unit of the drawing is: how finely
        // the pointer's place is worth reading.
        let per_unit = viewport.zip(zoom).map_or(1.0, |(v, zoom)| zoom * v.to_page.linear(Vec2::X).length());
        let status = Status { name: &name, pointer: self.pointer, places: chrome::status::places(per_unit), page: viewport.map(|v| v.view), toast: self.toast.as_ref().map(|(t, _)| t.as_str()), zoom, tool: &tool };
        let click = chrome::status::draw(ui, l.status, &status);
        if matches!(click, Some(Click::Claude)) {
            self.toast("Claude comes into this window with the live bridge (M5)");
        }

        self.pointer = None;
        match (self.tabs.active_doc(), viewport) {
            (Some(doc), Some(viewport)) => self.canvas(ui, l.canvas, doc, &viewport, click, popup),
            // No drawing yet: the files it started with are on their way.
            _ => ui.draw.rect(l.canvas, crate::theme::GROUND),
        }
    }

    /// The canvas: the view moved as the pointer asks, the tool in
    /// hand, and the drawing as the camera shows it.
    fn canvas(&mut self, ui: &mut Ui, area: Rect, doc: DocId, viewport: &Viewport, click: Option<Click>, popup: bool) {
        let (tool, page, busy) = (self.tools.active(), viewport.size, self.pointing.busy());
        let Some(tab) = self.tabs.active_mut() else { return };
        // A tab first laid out is fitted.
        let cam = tab.camera.get_or_insert_with(|| Camera::fit(area, page));
        cam.follow(area, page);
        match click {
            Some(Click::In) => cam.step(area, 1),
            Some(Click::Out) => cam.step(area, -1),
            Some(Click::Fit) => *cam = Camera::fit(area, page),
            Some(Click::Claude) | None => {}
        }
        // The view holds still under a menu or a dialog.
        let input = canvas::input(ui, area, cam, Hold { locked: popup, busy, owns_alt: tool.owns_alt(), hand: tool == Tool::Hand });
        if tool == Tool::Zoom && input.clicked {
            let out = ui.state.mods.alt();
            cam.zoom_about(area, if out { 1.0 / CLICK_STEP } else { CLICK_STEP }, ui.state.pointer);
        }
        if input.over && tool == Tool::Hand && ui.state.cursor_icon == CursorIcon::Default {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        self.pointer = input.pointer.and_then(|p| Some(viewport.to_page.inverse()?.apply(p)));

        let cam = *cam;
        // The Pointer, and the selection's box whatever is in hand:
        // first, so that what a drag comes to this frame is what the
        // canvas is asked to show.
        let origin = cam.window_at(area, Vec2::ZERO);
        let unit = cam.window_at(area, Vec2::new(1.0, 1.0)) - origin;
        let to_window = viewport.to_page.then(&Affine::new(unit.x, 0.0, 0.0, unit.y, origin.x, origin.y));
        let Some(to_doc) = to_window.inverse() else { return };
        let view = View { to_window, to_doc, scale: ui.m.scale };
        let scene = self.pointer_tool(ui, &view, doc, &input, tool == Tool::Pointer && !popup);
        // As a gesture under way would leave it, else as it is.
        let Ok((drawing, look)) = self.core.shown(doc) else { return };
        self.tiles.want(doc, drawing, look, cam.zoom, Rect::from_min_size(Vec2::ZERO - cam.corner(), area.size()));
        page::draw(ui, area, &cam, page, self.icons.checker(), &self.tiles, doc);
        overlay::draw(ui, area, &scene);
    }
}
