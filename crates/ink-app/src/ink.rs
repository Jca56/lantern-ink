//! The window's state: the core and its open drawings, the tabs, the
//! tool in hand, and what the chrome keeps between frames. A frame of
//! the one area Ink draws is `workspace.rs`; its seams with LUI2 are
//! in `host.rs`.

use std::path::PathBuf;

use ink_core::{Core, DocId};
use ink_doc::Viewport;
use lntrn_math::Vec2;
use lntrn_ui::KeyConfig;

use crate::chrome::panel::Grip;
use crate::chrome::toolbar::Toolbar;
use crate::docs::Tabs;
use crate::files::Files;
use crate::icons::Icons;
use crate::layout::Layout;
use crate::menus::{self, MenuState};
use crate::pointer::Pointer;
use crate::settings::{Recent, Settings};
use crate::tiles::Tiles;
use crate::tools::Tools;
use crate::tree::Tree;

/// How long a result stays in the status bar, seconds.
pub(crate) const TOAST_SECONDS: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    /// Everything: the one area Ink draws.
    Workspace,
}

pub struct Ink {
    /// The window's own core: its drawings are `w1`, `w2`, …
    pub core: Core,
    pub tabs: Tabs,
    pub files: Files,
    /// The canvas's picture, tile by tile.
    pub tiles: Tiles,
    pub settings: Settings,
    pub recent: Recent,
    pub(crate) keys: KeyConfig,
    /// The tool in hand.
    pub tools: Tools,
    pub(crate) toolbar: Toolbar,
    pub icons: Icons,
    /// The scale the last frame was laid out at: what the icons are
    /// made for.
    pub(crate) scale: f64,
    pub(crate) grip: Grip,
    /// The object tree, in the right panel.
    pub(crate) tree: Tree,
    /// The Pointer tool, and what it's in the middle of.
    pub(crate) pointing: Pointer,
    /// The Node tool: the anchors picked, and what it's in the middle of.
    pub(crate) noding: crate::noding::Noding,
    /// The Pen: a path being drawn, point by point.
    pub(crate) penning: crate::penning::Penning,
    /// The desktop's resize and turn cursors.
    pub(crate) cursor_theme: crate::cursors::Themed,
    /// The Box over the canvas, and a number of it being dragged along.
    pub(crate) toolbox: crate::toolbox::ToolBox,
    pub(crate) boxing: Option<crate::boxes::Boxing>,
    /// A row of the selected shape's own (its corners, its sides) being
    /// dragged along in the Box.
    pub(crate) tuning: Option<crate::shapebox::Tuning>,
    /// What was copied, on its way to the clipboard with the next
    /// frame; and a paste waiting for the clipboard's text.
    pub(crate) clip_out: Option<String>,
    pub(crate) pasting: Option<crate::ops::Pasting>,
    /// The paint section: its face, the one colour picker, the
    /// palettes, what the next shape drawn is painted with, and a
    /// paint being dragged to.
    pub(crate) paint_panel: crate::colour::section::Section,
    pub(crate) picker: crate::colour::picker::Picker,
    pub(crate) palettes: crate::colour::palettes::Library,
    pub(crate) paints: crate::paint::Paints,
    pub(crate) painting: Option<crate::painting::Painting>,
    /// The shape tools' own settings, and a shape being dragged out.
    pub(crate) shape_settings: crate::shapes::Settings,
    pub(crate) shaping: Option<crate::shaping::Shaping>,
    /// The panel's width while its grip is dragged, logical px.
    pub(crate) panel_drag: Option<f64>,
    /// Last frame's regions.
    pub(crate) layout: Layout,
    /// A result for the status bar, and until when it shows.
    pub(crate) toast: Option<(String, f64)>,
    /// A toast asked for away from a frame: it starts showing next frame.
    pub(crate) pending_toast: Option<String>,
    /// Where the pointer is on the canvas, in the drawing's own units.
    pub(crate) pointer: Option<Vec2>,
    pub(crate) themed: bool,
    /// Paths from the command line, opened once the loop can be woken
    /// by what's read.
    pub(crate) startup: Vec<PathBuf>,
    /// "Quit anyway" was chosen: the next close goes through.
    pub quitting: bool,
}

impl Ink {
    /// A window with a new drawing in it, or with `startup` (paths from
    /// the command line) to open instead.
    pub fn new(startup: Vec<PathBuf>) -> Ink {
        let mut ink = Ink {
            core: Core::window(1),
            tabs: Tabs::default(),
            files: Files::default(),
            tiles: Tiles::default(),
            settings: Settings::load(),
            recent: Recent::load(),
            keys: menus::keys(),
            tools: Tools::default(),
            toolbar: Toolbar::default(),
            icons: Icons::default(),
            scale: 0.0,
            grip: Grip::default(),
            tree: Tree::default(),
            pointing: Pointer::default(),
            noding: crate::noding::Noding::default(),
            penning: crate::penning::Penning::default(),
            cursor_theme: if cfg!(test) { Default::default() } else { crate::cursors::themed() },
            toolbox: crate::toolbox::ToolBox::default(),
            boxing: None,
            tuning: None,
            clip_out: None,
            pasting: None,
            paint_panel: crate::colour::section::Section::default(),
            picker: crate::colour::picker::Picker::default(),
            palettes: crate::colour::palettes::Library::load(crate::settings::dir().as_deref(), crate::settings::studio_palettes().as_deref()),
            paints: crate::paint::Paints::default(),
            painting: None,
            shape_settings: crate::shapes::Settings::default(),
            shaping: None,
            panel_drag: None,
            layout: Layout::default(),
            toast: None,
            pending_toast: None,
            pointer: None,
            themed: false,
            startup,
            quitting: false,
        };
        // With files to open, the first of them is the first tab; a
        // new drawing takes their place only if none opens.
        if ink.startup.is_empty() {
            ink.new_document();
        }
        ink
    }

    /// Show `text` in the status bar for a few seconds.
    pub fn toast(&mut self, text: impl Into<String>) {
        self.pending_toast = Some(text.into());
    }

    /// What the status bar says, or is about to.
    #[cfg(test)]
    pub(crate) fn toast_text(&self) -> Option<&str> {
        self.pending_toast.as_deref().or(self.toast.as_ref().map(|(text, _)| text.as_str()))
    }

    /// The tab's name: its file's, else the one it was given as it was
    /// made.
    pub fn label(&self, doc: DocId) -> String {
        if let Ok(Some(path)) = self.core.path(doc)
            && let Some(stem) = path.file_stem()
        {
            return stem.to_string_lossy().into_owned();
        }
        self.tabs.iter().find(|t| t.doc == doc).map(|t| t.name.clone()).unwrap_or_default()
    }

    pub fn is_modified(&self, doc: DocId) -> bool {
        self.core.is_modified(doc).unwrap_or(false)
    }

    /// How the showing drawing sits on its page.
    pub fn viewport(&self) -> Option<Viewport> {
        self.core.viewport(self.tabs.active_doc()?).ok()
    }

    pub(crate) fn menu_state(&self) -> MenuState<'_> {
        let history = self.tabs.active_doc().and_then(|d| self.core.history(d).ok());
        MenuState { has_doc: self.tabs.active_doc().is_some(), undo: history.and_then(|h| h.undoable().next_back()), redo: history.and_then(|h| h.redoable().next()), recent: &self.recent, picked: self.picked(), align_to_page: self.settings.align_to_page }
    }
}
