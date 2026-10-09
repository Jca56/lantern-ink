//! Ink's two seams with LUI2: `Host` (the menus, the keys, drawing the
//! one area) and `AppHost` (the GPU, which Ink needs only to show
//! pictures its own renderer made: the icons, and the canvas's tiles
//! as they come off the pool).

use std::path::PathBuf;

use lntrn_app::lntrn_render::{Gpu, Images};
use lntrn_app::{AppHost, Waker};
use lntrn_math::Rect;
use lntrn_ui::{Action, AreaCx, Host, HostCx, KeyItem, KeyPress, Menu, Shell, Ui, keymap::CTX_WINDOW};

use crate::ink::{Editor, Ink};
use crate::tiles::OnGpu;
use crate::{chrome, menus, theme};

impl Host for Ink {
    type Editor = Editor;
    type AreaState = ();

    fn editors(&self) -> &[Editor] {
        &[Editor::Workspace]
    }

    fn editor_label(&self, _: Editor) -> &str {
        "Workspace"
    }

    fn title(&self) -> String {
        // The logo is the title.
        String::new()
    }

    fn title_menus(&self) -> &[(&str, &str)] {
        &menus::TITLE_MENUS
    }

    fn draw_title_bar(&mut self, ui: &mut Ui, space: Rect) -> f64 {
        chrome::logo(ui, space, self.icons.app())
    }

    fn menu(&self, name: &str) -> Option<Menu> {
        menus::menu(name, &self.menu_state())
    }

    fn key_hint(&self, action: &Action) -> Option<String> {
        self.keys.hint_for(action)
    }

    fn key(&self, press: KeyPress, _: Option<Editor>) -> Option<Action> {
        // In the middle of a drag no key does anything (it would end
        // under another tool, or another state of the drawing) but
        // Escape, which gives the drag up.
        if self.pointing.busy() || self.boxing.is_some() || self.painting.is_some() || self.shaping.is_some() {
            return (press.key == lntrn_ui::Key::Escape).then(|| Action::new(menus::ESCAPE));
        }
        self.keys.resolve(&[CTX_WINDOW], &press.to_event(), |_| true).map(KeyItem::action).or_else(|| menus::tool_key(press)).or_else(|| menus::canvas_key(press))
    }

    fn paints_body(&self, _: Editor) -> bool {
        true
    }

    fn shows_header(&self, _: Editor) -> bool {
        false
    }

    fn draw_body(&mut self, _: Editor, ui: &mut Ui, cx: &mut AreaCx<()>) -> bool {
        self.draw_workspace(ui, cx);
        false
    }

    fn run(&mut self, action: &Action, cx: &mut HostCx) {
        self.act(action, cx);
        cx.rebuild();
    }

    fn close_requested(&mut self, _main: bool, cx: &mut HostCx) -> bool {
        self.may_quit(cx)
    }

    fn dropped(&mut self, paths: &[PathBuf], _: Option<lntrn_ui::AreaId>, _: Option<Editor>, _: &mut HostCx) {
        for p in paths {
            self.open(p.clone());
        }
    }
}

impl AppHost for Ink {
    fn cursors(&self, scale: f64) -> Vec<lntrn_app::CursorImage> {
        crate::cursors::images(scale, &self.cursor_theme)
    }

    fn cursor(&self, wanted: lntrn_ui::CursorIcon) -> lntrn_ui::CursorIcon {
        crate::cursors::shown(wanted, &self.cursor_theme)
    }

    fn waker(&mut self, waker: Waker) {
        self.files.set_waker(waker.clone());
        self.tiles.set_waker(waker);
        for path in std::mem::take(&mut self.startup) {
            self.open(path);
        }
    }

    fn after_rebuild(&mut self, gpu: &Gpu, images: &mut Images, shell: &mut Shell<Self>) -> bool {
        let mut again = self.icons.ensure(gpu, images, self.scale);
        if !self.themed {
            // Ink's colours over whatever the saved preferences held.
            shell.prefs.theme = theme::lui2();
            self.themed = true;
            again = true;
        }
        // Tiles the pool has drawn since the last frame.
        again |= self.tiles.finished(&mut OnGpu(gpu, images));
        again
    }
}
