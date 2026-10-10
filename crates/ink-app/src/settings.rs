//! What Ink remembers between runs beside LUI2's own preferences, in
//! `~/.lantern/config/lantern-ink/` (LS3's `settings.rs`): the panel's
//! width, and the recent files (ten, one path a line). Written when
//! they change, never on a frame.

use std::path::{Path, PathBuf};

use lntrn_props::props;
use lntrn_ui::persist;

use crate::APP_ID;

const SETTINGS_FILE: &str = "ink.bin";
const RECENT_FILE: &str = "recent.txt";
/// How many recent files are kept.
pub const RECENT_MAX: usize = 10;

props! {
    /// Ink's own settings.
    pub struct Settings {
        /// The right panel's width, logical px.
        pub panel_width: f64 = 400.0 => { id: 1, hard: 260.0..=520.0 },
        /// A shape scaled by its handles has its stroke scale with it
        /// (off: the stroke keeps its width).
        pub scale_strokes: bool = false => { id: 2 },
        /// Object > Align lines things up against the page, not the
        /// box round what's selected.
        pub align_to_page: bool = false => { id: 3 },
        /// The paint section is folded away to its heading.
        pub paint_folded: bool = false => { id: 4 },
        /// View > Snapping: drags land on the grid and on lines, and
        /// show what they landed on.
        pub snapping: bool = true => { id: 5 },
        /// View > Pixel Grid: a line at every unit, zoomed in.
        pub pixel_grid: bool = true => { id: 6 },
        /// Preferences: what snapping lands things on. Whole and half
        /// units; other shapes and the page.
        pub snap_grid: bool = true => { id: 7 },
        pub snap_shapes: bool = true => { id: 8 },
    }
}

/// Ink's own config folder.
pub(crate) fn dir() -> Option<PathBuf> {
    // Tests keep their hands off the real settings.
    if cfg!(test) { None } else { persist::config_dir(APP_ID) }
}

/// Lantern Studio's palettes: what Ink's start as, the first time.
pub(crate) fn studio_palettes() -> Option<PathBuf> {
    if cfg!(test) { None } else { persist::config_dir("lantern-studio").map(|d| d.join("palettes.json")) }
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        if let Some(d) = dir() {
            // None yet is the first launch; one that can't be read is
            // worth a line (the defaults stand in).
            let file = d.join(SETTINGS_FILE);
            if !persist::load(&file, &mut s) && file.exists() {
                lntrn_core::log_error!("{} can't be read: starting from the defaults", file.display());
            }
        }
        s
    }

    pub fn save(&self) {
        if let Some(d) = dir()
            && let Err(e) = persist::save(&d.join(SETTINGS_FILE), self)
        {
            lntrn_core::log_error!("saving settings: {e}");
        }
    }
}

/// The recent files, newest first.
#[derive(Default)]
pub struct Recent {
    pub paths: Vec<PathBuf>,
    /// Whether each was there when the list last changed: the menu shows
    /// it without touching the disk on a frame.
    pub missing: Vec<bool>,
}

impl Recent {
    pub fn load() -> Recent {
        let text = dir().and_then(|d| persist::load_text(&d.join(RECENT_FILE))).unwrap_or_default();
        let mut r = Recent { paths: parse(&text), missing: Vec::new() };
        r.look();
        r
    }

    /// `path` was opened or saved: it goes first.
    pub fn add(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.to_owned());
        self.paths.truncate(RECENT_MAX);
        self.changed();
    }

    pub fn remove(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.changed();
    }

    pub fn clear(&mut self) {
        self.paths.clear();
        self.changed();
    }

    fn look(&mut self) {
        self.missing = self.paths.iter().map(|p| !p.exists()).collect();
    }

    fn changed(&mut self) {
        self.look();
        let text: String = self.paths.iter().map(|p| format!("{}\n", p.display())).collect();
        if let Some(d) = dir()
            && let Err(e) = persist::save_text(&d.join(RECENT_FILE), &text)
        {
            lntrn_core::log_error!("saving recent files: {e}");
        }
    }
}

fn parse(text: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = PathBuf::from(line);
        if p.is_absolute() && !paths.contains(&p) && paths.len() < RECENT_MAX {
            paths.push(p);
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_read_back_clean() {
        let text = "/a/one.svg\n\nrelative.svg\n/a/one.svg\n  /b/two.svg  \n";
        assert_eq!(parse(text), vec![PathBuf::from("/a/one.svg"), PathBuf::from("/b/two.svg")]);
        let many: String = (0..20).map(|i| format!("/p/{i}.svg\n")).collect();
        assert_eq!(parse(&many).len(), RECENT_MAX);
    }

    #[test]
    fn the_newest_goes_first_and_ten_are_kept() {
        let mut r = Recent::default();
        for i in 0..12 {
            r.add(Path::new(&format!("/p/{i}.svg")));
        }
        r.add(Path::new("/p/5.svg"));
        assert_eq!((r.paths.len(), r.paths[0].clone(), r.paths[1].clone()), (RECENT_MAX, PathBuf::from("/p/5.svg"), PathBuf::from("/p/11.svg")));
        assert_eq!(r.paths.iter().filter(|p| p.ends_with("5.svg")).count(), 1);
        assert!(r.missing.iter().all(|m| *m) && r.missing.len() == RECENT_MAX);
        r.remove(Path::new("/p/5.svg"));
        assert_eq!(r.paths.len(), RECENT_MAX - 1);
        r.clear();
        assert!(r.paths.is_empty() && r.missing.is_empty());
    }
}
