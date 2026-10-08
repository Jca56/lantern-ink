//! Named colour palettes for the paint section (LS3's `palettes.rs`,
//! its rules kept): the built-in Default is always first and can't be
//! edited, renamed or deleted; a palette holds at most 64 colours and
//! never the same one twice.
//!
//! Kept in `~/.lantern/config/lantern-ink/palettes.json` as
//! `{active, palettes: [{name, colors: ["#rrggbb"], builtin}]}`, written
//! after every change, so it can be read and edited by hand. The first
//! time there's no such file, Lantern Studio's
//! (`~/.lantern/config/lantern-studio/palettes.json`) is read in its
//! place (Alva's choice, 2026-10-08): Ink starts with her palettes and
//! never writes Studio's file.

use std::path::{Path, PathBuf};

use lntrn_data::{Doc, Map, json};
use lntrn_math::Color;

/// The most colours in one palette: eight rows of the grid.
pub const MAX_COLORS: usize = 64;
const FILE: &str = "palettes.json";

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<Color>,
    /// The one that ships with the app.
    pub builtin: bool,
}

/// Every palette, and the one the grid shows. The built-in is there,
/// first, the only one flagged so, and `active` is in range:
/// `normalize` sees to it after a load or a change.
#[derive(Clone, Debug, PartialEq)]
pub struct Library {
    pub palettes: Vec<Palette>,
    pub active: usize,
}

const fn hex(rgb: u32) -> Color {
    Color::hex(rgb)
}

fn rgb8(c: Color) -> [u8; 3] {
    let ch = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [ch(c.r), ch(c.g), ch(c.b)]
}

pub fn to_hex(c: Color) -> String {
    let [r, g, b] = rgb8(c);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// `#rrggbb` or `rrggbb`; anything longer is read by its first six
/// digits.
pub fn from_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    let digits = s.get(..6)?;
    u32::from_str_radix(digits, 16).ok().map(Color::hex)
}

/// LS3's shipped palette: light, medium, dark, then neutrals and the
/// two Lantern golds.
fn builtin() -> Palette {
    const COLORS: [u32; 32] = [
        0xff8080, 0xffb366, 0xffff80, 0x80ff80, 0x80ffe0, 0x80b3ff, 0xcc80ff, 0xff80cc, 0xff0000, 0xff8000, 0xffff00, 0x00cc00, 0x00ccaa, 0x0066ff, 0x8800cc, 0xff0088, 0x990000, 0x994c00, 0x999900, 0x006600, 0x006655, 0x003399,
        0x550080, 0x880044, 0xffffff, 0xcccccc, 0x888888, 0x555555, 0x2a2a2a, 0x000000, 0xf3b700, 0x8b6c42,
    ];
    Palette { name: "Default".into(), builtin: true, colors: COLORS.iter().map(|&c| hex(c)).collect() }
}

impl Default for Library {
    fn default() -> Library {
        Library { palettes: vec![builtin()], active: 0 }
    }
}

impl Library {
    /// From the file's text; whatever it holds, a library that keeps its
    /// rules. `None` when it isn't a palette file at all.
    pub fn parse(text: &str) -> Option<Library> {
        let doc = json::parse(text).ok()?;
        let palettes = doc
            .get("palettes")?
            .as_list()?
            .iter()
            .filter_map(|p| {
                let name = p.get("name")?.as_str()?.to_owned();
                let colors = p.get("colors")?.as_list()?.iter().filter_map(|c| from_hex(c.as_str()?)).take(MAX_COLORS).collect();
                Some(Palette { name, colors, builtin: p.get("builtin").and_then(Doc::as_bool).unwrap_or(false) })
            })
            .collect();
        let active = doc.get("active").and_then(Doc::as_i64).and_then(|a| usize::try_from(a).ok()).unwrap_or(0);
        let mut lib = Library { palettes, active };
        // Files from before the built-in was locked hold it as a plain
        // palette named Default: that one is it.
        if !lib.palettes.iter().any(|p| p.builtin)
            && let Some(p) = lib.palettes.iter_mut().find(|p| p.name == "Default")
        {
            p.builtin = true;
        }
        lib.normalize();
        Some(lib)
    }

    pub fn to_json(&self) -> String {
        let palettes = self
            .palettes
            .iter()
            .map(|p| {
                let mut m = Map::new();
                m.insert("name", Doc::Str(p.name.clone()));
                m.insert("colors", Doc::List(p.colors.iter().map(|&c| Doc::Str(to_hex(c))).collect()));
                m.insert("builtin", Doc::Bool(p.builtin));
                Doc::Map(m)
            })
            .collect();
        let mut m = Map::new();
        m.insert("active", Doc::Int(self.active as i64));
        m.insert("palettes", Doc::List(palettes));
        json::write_pretty(&Doc::Map(m))
    }

    /// The library in `dir` (Ink's config); with none there yet, the
    /// one Studio keeps at `old`; with neither, the built-in alone.
    pub fn load(dir: Option<&Path>, old: Option<&Path>) -> Library {
        let read = |path: PathBuf| std::fs::read_to_string(&path).ok().and_then(|text| Library::parse(&text).or_else(|| {
            lntrn_core::log_error!("{} isn't a palette file: starting from the built-in", path.display());
            None
        }));
        if let Some(lib) = dir.and_then(|d| read(d.join(FILE))) {
            return lib;
        }
        let own = dir.is_some_and(|d| d.join(FILE).exists());
        // An unreadable file of our own is left alone, not replaced by
        // Studio's.
        match old.filter(|_| !own).and_then(|o| read(o.to_owned())) {
            Some(lib) => {
                lib.save(dir);
                lib
            }
            None => Library::default(),
        }
    }

    pub fn save(&self, dir: Option<&Path>) {
        let Some(dir) = dir else { return };
        if let Err(e) = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(dir.join(FILE), self.to_json())) {
            lntrn_core::log_error!("saving palettes: {e}");
        }
    }

    /// One built-in, first, with the shipped colours; `active` in range.
    fn normalize(&mut self) {
        // The first one flagged is it; any other is a plain palette.
        let mut seen = false;
        for p in &mut self.palettes {
            if p.builtin && seen {
                p.builtin = false;
            }
            seen |= p.builtin;
        }
        match self.palettes.iter().position(|p| p.builtin) {
            Some(0) => {}
            Some(i) => {
                let p = self.palettes.remove(i);
                self.palettes.insert(0, p);
                // `active` still names the same palette.
                if self.active == i {
                    self.active = 0;
                } else if self.active < i {
                    self.active += 1;
                }
            }
            None => {
                if !self.palettes.is_empty() {
                    self.active += 1;
                }
                self.palettes.insert(0, builtin());
            }
        }
        self.palettes[0].colors = builtin().colors;
        self.active = self.active.min(self.palettes.len() - 1);
    }

    pub fn shown(&self) -> &Palette {
        &self.palettes[self.active]
    }

    /// The built-in refuses every edit.
    pub fn is_locked(&self, i: usize) -> bool {
        self.palettes.get(i).is_some_and(|p| p.builtin)
    }

    fn editable(&mut self) -> Option<&mut Palette> {
        let i = self.active;
        self.palettes.get_mut(i).filter(|p| !p.builtin)
    }

    /// "Greys", then "Greys 2", "Greys 3".
    fn unique_name(&self, base: &str) -> String {
        let taken = |name: &str| self.palettes.iter().any(|p| p.name == name);
        if !taken(base) {
            return base.to_owned();
        }
        (2..).map(|n| format!("{base} {n}")).find(|c| !taken(c)).unwrap_or_else(|| base.to_owned())
    }

    /// An empty palette after the one shown, shown itself.
    pub fn add_empty(&mut self) {
        let name = self.unique_name("Palette");
        self.active += 1;
        self.palettes.insert(self.active, Palette { name, colors: Vec::new(), builtin: false });
    }

    /// A copy of the one shown after it, shown itself: how the built-in
    /// becomes a start.
    pub fn duplicate(&mut self) {
        let src = self.shown();
        let copy = Palette { name: self.unique_name(&format!("{} copy", src.name)), colors: src.colors.clone(), builtin: false };
        self.active += 1;
        self.palettes.insert(self.active, copy);
    }

    pub fn remove(&mut self, i: usize) {
        if i >= self.palettes.len() || self.is_locked(i) {
            return;
        }
        self.palettes.remove(i);
        if self.active > i {
            self.active -= 1;
        }
        self.normalize();
    }

    /// A blank name keeps the old one.
    pub fn rename(&mut self, i: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() || self.is_locked(i) {
            return;
        }
        if let Some(p) = self.palettes.get_mut(i) {
            p.name = name.to_owned();
        }
    }

    pub fn select(&mut self, i: usize) {
        if i < self.palettes.len() {
            self.active = i;
        }
    }

    /// Whether the palette shown has room for another colour.
    pub fn can_add(&self) -> bool {
        !self.is_locked(self.active) && self.shown().colors.len() < MAX_COLORS
    }

    /// `c` at the end of the palette shown; nothing at the cap, or when
    /// it's there already (as 8-bit colours).
    pub fn add_color(&mut self, c: Color) {
        let c = c.with_alpha(1.0);
        if let Some(p) = self.editable()
            && p.colors.len() < MAX_COLORS
            && !p.colors.iter().any(|&e| rgb8(e) == rgb8(c))
        {
            p.colors.push(c);
        }
    }

    pub fn set_color(&mut self, i: usize, c: Color) {
        if let Some(slot) = self.editable().and_then(|p| p.colors.get_mut(i)) {
            *slot = c.with_alpha(1.0);
        }
    }

    pub fn remove_color(&mut self, i: usize) {
        if let Some(p) = self.editable().filter(|p| i < p.colors.len()) {
            p.colors.remove(i);
        }
    }

    /// The colour at `from` to place `to`, the others making room.
    pub fn move_color(&mut self, from: usize, to: usize) {
        if let Some(p) = self.editable().filter(|p| from != to && from < p.colors.len() && to < p.colors.len()) {
            let c = p.colors.remove(from);
            p.colors.insert(to, c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(lib: &Library) -> Vec<&str> {
        lib.palettes.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn the_builtin_is_always_first_and_as_shipped() {
        let lib = Library::default();
        assert_eq!((names(&lib), lib.shown().colors.len(), lib.is_locked(0)), (vec!["Default"], 32, true));
        assert_eq!((to_hex(lib.shown().colors[30]), to_hex(lib.shown().colors[0])), ("#f3b700".to_owned(), "#ff8080".to_owned()));
        // A hand-edited file: no Default, `active` out of range.
        let lib = Library::parse(r##"{"active": 9, "palettes": [{"name": "Mine", "colors": ["#102030", "nope", "#fff"]}]}"##).unwrap();
        assert_eq!((names(&lib), lib.active, lib.palettes[1].colors.clone()), (vec!["Default", "Mine"], 1, vec![Color::hex(0x102030)]));
        // Two flagged built-in, the first not first, its colours edited:
        // one built-in, first, as shipped; `active` names the same palette.
        let lib = Library::parse(r##"{"active": 0, "palettes": [{"name": "A", "colors": []}, {"name": "Default", "colors": ["#000000"], "builtin": true}, {"name": "B", "colors": [], "builtin": true}]}"##).unwrap();
        assert_eq!((names(&lib), lib.active, lib.palettes[0].colors.len(), lib.is_locked(2)), (vec!["Default", "A", "B"], 1, 32, false));
        // An old file's plain "Default" is the built-in.
        let lib = Library::parse(r##"{"active": 1, "palettes": [{"name": "Default", "colors": []}, {"name": "X", "colors": []}]}"##).unwrap();
        assert_eq!((names(&lib), lib.active, lib.is_locked(0)), (vec!["Default", "X"], 1, true));
        assert!(Library::parse("not json").is_none() && Library::parse("{}").is_none());
    }

    #[test]
    fn it_reads_back_what_it_wrote() {
        let mut lib = Library::default();
        lib.duplicate();
        lib.add_empty();
        lib.add_color(Color::hex(0x123456));
        assert_eq!(Library::parse(&lib.to_json()), Some(lib.clone()));
        assert_eq!((names(&lib), lib.active), (vec!["Default", "Default copy", "Palette"], 2));
        assert_eq!((from_hex("#A0b1C2"), from_hex("a0b1c2ff"), from_hex("#abc")), (Some(Color::hex(0xa0b1c2)), Some(Color::hex(0xa0b1c2)), None));
    }

    #[test]
    fn the_builtin_takes_no_edits_and_the_others_keep_their_rules() {
        let mut lib = Library::default();
        lib.add_color(Color::hex(0x010203));
        lib.remove_color(0);
        lib.rename(0, "Mine");
        lib.remove(0);
        assert_eq!((lib.clone(), lib.can_add()), (Library::default(), false));
        lib.add_empty();
        assert!(lib.can_add());
        // No colour twice (as 8-bit colours), alpha left behind.
        lib.add_color(Color::rgba(0.5, 0.25, 0.0, 0.3));
        lib.add_color(Color::rgba(0.501, 0.25, 0.0, 1.0));
        lib.add_color(Color::hex(0x0000ff));
        assert_eq!(lib.shown().colors, [Color::rgba(0.5, 0.25, 0.0, 1.0), Color::hex(0x0000ff)]);
        lib.move_color(1, 0);
        lib.set_color(1, Color::hex(0xff0000));
        assert_eq!(lib.shown().colors, [Color::hex(0x0000ff), Color::hex(0xff0000)]);
        // 64 at most.
        for i in 0..100u32 {
            lib.add_color(Color::hex(0x100000 + i * 3));
        }
        assert_eq!((lib.shown().colors.len(), lib.can_add()), (MAX_COLORS, false));
        // Names: unique, blank keeps the old one; removing the one shown
        // shows the one before.
        lib.add_empty();
        lib.rename(2, "   ");
        assert_eq!(names(&lib), ["Default", "Palette", "Palette 2"]);
        lib.rename(2, " Skin ");
        lib.remove(2);
        assert_eq!((names(&lib), lib.active), (vec!["Default", "Palette"], 1));
    }

    #[test]
    fn the_first_run_takes_2xs_palettes_and_never_writes_them() {
        let root = std::env::temp_dir().join(format!("ls3-palettes-{}", std::process::id()));
        let (ours, theirs) = (root.join("lantern-studio"), root.join("palettes.json"));
        std::fs::create_dir_all(&root).unwrap();
        let old = r##"{"active": 1, "palettes": [{"name": "Default", "colors": [], "builtin": true}, {"name": "Sunset", "colors": ["#ff8800"], "builtin": false}]}"##;
        std::fs::write(&theirs, old).unwrap();
        // No file of our own: the other app's, copied into ours.
        let lib = Library::load(Some(&ours), Some(&theirs));
        assert_eq!((names(&lib), lib.active), (vec!["Default", "Sunset"], 1));
        assert!(ours.join(FILE).exists());
        assert_eq!(std::fs::read_to_string(&theirs).unwrap(), old, "the other file is as it was");
        // From then on ours is the one read, whatever the other says.
        let mut lib = lib;
        lib.add_empty();
        lib.save(Some(&ours));
        std::fs::write(&theirs, r##"{"active": 0, "palettes": []}"##).unwrap();
        assert_eq!(names(&Library::load(Some(&ours), Some(&theirs))), ["Default", "Sunset", "Palette"]);
        // Neither: the built-in alone, and nothing written.
        let empty = root.join("empty");
        assert_eq!(Library::load(Some(&empty), Some(&root.join("none.json"))), Library::default());
        assert!(!empty.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
