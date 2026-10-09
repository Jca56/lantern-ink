//! Lantern Ink, the window (ARCHITECTURE §8): an `ink-core` drawn with
//! Lantern UI 2, in LS3's look. Paths on the command line open as tabs.
//! What it has to do to be done is `docs/M4.md`.

mod actions;
mod anchors;
mod boxes;
mod camera;
mod canvas;
mod chrome;
mod colour;
mod controls;
mod cursors;
mod docs;
mod edits;
mod files;
mod handles;
mod host;
mod icons;
mod ink;
#[cfg(test)]
mod ink_tests;
mod layout;
mod lifecycle;
mod log;
mod menus;
mod nodes;
mod noding;
mod ops;
mod overlay;
mod page;
mod paint;
mod painting;
mod picker;
mod picking;
mod pointer;
mod polygons;
mod rounding;
mod select;
mod settings;
mod shapebox;
mod shapes;
mod shaping;
mod theme;
mod tiles;
mod toolbox;
mod tools;
mod tree;
mod workspace;

use std::path::PathBuf;

use lntrn_app::{AppConfig, run};
use lntrn_ui::Shell;

use ink::{Editor, Ink};

/// The window's app id, and its config folder's name.
pub const APP_ID: &str = "lantern-ink";

fn main() {
    log::start();
    // Text that asks for `sans-serif` is set in the desktop's own font.
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Some(font) = ink_core::desktop::use_font(&home)
    {
        lntrn_core::log_info!("sans-serif is {font} (lantern.toml)");
    }
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let config = AppConfig { title: "Lantern Ink".into(), app_id: APP_ID.into(), size: (1280.0, 800.0), min_size: (800.0, 600.0), maximized: true, ..AppConfig::default() };
    run(config, Ink::new(paths), Shell::new(Editor::Workspace));
}
