//! The window from the start, without a window or a GPU: real shell
//! frames (the title bar, the menus, the whole area Ink draws) at a
//! screen's scale, each followed by what the window does once a frame
//! is laid out. The tiles' pictures go to a stand-in for the GPU's
//! images, which counts them: none is kept twice or left behind.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ink_core::DocId;
use lntrn_app::lntrn_render::ImageId;
use lntrn_image::Image;
use lntrn_math::Vec2;
use lntrn_ui::testing::Harness;
use lntrn_ui::{Event, ImageHandle, Key, Modifiers, Shell};

use crate::camera::Camera;
use crate::files::Then;
use crate::ink::{Editor, Ink};
use crate::tiles::Store;
use crate::tools::Tool;

mod canvas;

/// Where the tiles' pictures are kept in a test.
#[derive(Default)]
struct Kept {
    next: u32,
    live: HashSet<u32>,
}

impl Store for Kept {
    fn add(&mut self, image: &Image) -> ImageHandle {
        self.next += 1;
        self.live.insert(self.next);
        ImageHandle { id: ImageId(self.next), width: image.width, height: image.height }
    }

    fn remove(&mut self, id: ImageId) {
        assert!(self.live.remove(&id.0), "{id:?} freed twice, or never kept");
    }
}

/// A started Ink in a `width` × `height` px window at `scale`.
struct Running {
    h: Harness,
    ink: Ink,
    shell: Shell<Ink>,
    kept: Kept,
}

fn corpus(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(name)
}

/// A folder of this test's own, empty, where cargo keeps its tests'
/// (`target/tmp`).
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/ink-app").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A file of the corpus with nothing of another editor's in it: one
/// that saves as the bytes it was.
const PLAIN: &str = "de-cursors--lntrn-cursor-ew.svg";
const ICON: &str = "de-apps--lntrn-calculator.svg";

impl Running {
    fn start(width: f64, height: f64, scale: f64) -> Running {
        let mut h = Harness::new(width, height);
        h.scale = scale;
        let mut r = Running { h, ink: Ink::new(Vec::new()), shell: Shell::new(Editor::Workspace), kept: Kept::default() };
        r.frames(4);
        r
    }

    /// Rebuilds as the window makes them: the shell's frame, then what
    /// the pool finished.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.h.shell_frame(&mut self.shell, &mut self.ink);
            self.ink.tiles.finished(&mut self.kept);
        }
    }

    /// Frames until `done`, giving the pool a moment between them.
    fn until(&mut self, what: &str, done: impl Fn(&Running) -> bool) {
        for _ in 0..2000 {
            if done(self) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
            self.frames(1);
        }
        panic!("never happened: {what}");
    }

    fn doc(&self) -> DocId {
        self.ink.tabs.active_doc().expect("a drawing showing")
    }

    fn camera(&self) -> Camera {
        self.ink.tabs.active().and_then(|t| t.camera).expect("a view of the drawing")
    }

    /// The middle of the canvas, window px.
    fn middle(&self) -> Vec2 {
        self.ink.layout.canvas.center()
    }

    /// Open `path` and wait for its tab.
    fn open(&mut self, path: &Path) {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        self.ink.open(path.to_owned());
        self.until("the file's tab", |r| r.ink.label(r.doc()) == stem);
    }

    /// Wait for the picture of what shows, at the zoom it shows at.
    fn drawn(&mut self) -> usize {
        self.until("the tiles for this view", |r| r.ink.tiles.showing(r.doc()).is_some_and(|(zoom, _)| zoom == r.camera().zoom));
        self.frames(2);
        self.ink.tiles.showing(self.doc()).unwrap().1
    }

    fn click(&mut self, at: Vec2) {
        self.h.advance(1.0);
        self.h.move_to(at);
        self.frames(1);
        self.h.press();
        self.frames(1);
        self.h.release();
        self.frames(2);
    }

    /// Press at `from`, drag to `to` in a few steps, let go.
    fn drag(&mut self, from: Vec2, to: Vec2) {
        self.h.advance(1.0);
        self.h.move_to(from);
        self.frames(1);
        self.h.press();
        self.frames(1);
        for step in 1..=4 {
            self.h.move_to(from + (to - from) * (f64::from(step) / 4.0));
            self.frames(1);
        }
        self.h.release();
        self.frames(2);
    }

    fn key(&mut self, key: Key, mods: Modifiers) {
        self.h.key_with(key, mods);
        self.frames(2);
    }
}

#[test]
fn the_window_starts_with_a_drawing_fitted_in_it() {
    for (width, height, scale) in [(1792.0, 1120.0, 1.4), (1920.0, 1080.0, 1.0), (800.0, 600.0, 1.0)] {
        let mut r = Running::start(width, height, scale);
        assert_eq!((r.ink.tabs.len(), r.ink.label(r.doc()).as_str()), (1, "untitled"), "{width} × {height}");
        let (canvas, cam) = (r.ink.layout.canvas, r.camera());
        assert!(canvas.width() > 100.0 && canvas.height() > 100.0 && cam.fitted, "{canvas:?}");
        // An icon's grid, at nine tenths of the room.
        assert_eq!(r.ink.viewport().unwrap().size, Vec2::new(24.0, 24.0));
        assert!((cam.zoom - canvas.width().min(canvas.height()) * 0.9 / 24.0).abs() < 1e-9);
        // Nothing in it, so nothing to lose and nothing to draw.
        assert!(r.ink.untouched(r.doc()));
        assert_eq!((r.drawn(), r.kept.live.len()), (0, 0));
        assert_eq!(r.ink.toast_text(), None);
    }
}

#[test]
fn a_file_opens_in_the_blank_tabs_place_and_is_drawn() {
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    let blank = r.doc();
    r.open(&corpus(ICON));
    assert_eq!(r.ink.tabs.len(), 1, "the untouched drawing gave way");
    assert!(r.ink.core.doc(blank).is_err() && !r.ink.is_modified(r.doc()));
    // It was Boxy's: that its marks will go is said once, as it opens.
    assert_eq!(r.ink.toast_text(), Some("Opened de-apps--lntrn-calculator.svg: Boxy SVG\u{2019}s own marks come out when it\u{2019}s saved"));
    assert_eq!(r.ink.recent.paths, [corpus(ICON)]);
    let tiles = r.drawn();
    assert!(tiles > 0 && r.kept.live.len() == tiles, "{tiles} tiles drawn, {} kept", r.kept.live.len());
    // Asked for again, it's the tab it has.
    r.ink.open(corpus(ICON));
    r.frames(3);
    std::thread::sleep(std::time::Duration::from_millis(50));
    r.frames(3);
    assert_eq!(r.ink.tabs.len(), 1);
    // Another opens beside it, and shows.
    r.open(&corpus(PLAIN));
    assert_eq!((r.ink.tabs.len(), r.ink.tabs.active_index()), (2, 1));
    assert!(r.drawn() > 0);
    // What can't be read says why, and opens nothing.
    r.ink.open(corpus("no-such-file.svg"));
    r.until("word of the failure", |r| r.ink.toast_text().is_some_and(|said| said.starts_with("Couldn't open no-such-file.svg: ")));
    assert_eq!(r.ink.tabs.len(), 2);
}

#[test]
fn zooming_keeps_the_old_picture_until_the_new_one_lands() {
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&corpus(ICON));
    let fitted = r.camera().zoom;
    assert!(r.drawn() > 0);
    // Ctrl+wheel, toward the pointer.
    r.h.move_to(r.middle());
    r.h.set_mods(Modifiers::CTRL);
    r.h.wheel(2.0);
    r.h.shell_frame(&mut r.shell, &mut r.ink);
    r.h.set_mods(Modifiers::NONE);
    let zoomed = r.camera().zoom;
    assert!(zoomed > fitted * 1.4 && !r.camera().fitted, "{fitted} to {zoomed}");
    assert_eq!(r.ink.tiles.showing(r.doc()).map(|(zoom, _)| zoom), Some(fitted), "the fitted picture still shows, stretched");
    // Its own tiles take over, and the old ones are freed.
    let tiles = r.drawn();
    assert!(tiles > 0 && r.kept.live.len() == tiles, "{tiles} tiles show, {} kept", r.kept.live.len());
    // A pinch zooms too; and Fit puts it back.
    r.h.pinch(0.8);
    r.frames(1);
    assert!(r.camera().zoom < zoomed);
    r.key(Key::Char('0'), Modifiers::CTRL);
    assert!(r.camera().fitted && r.camera().zoom == fitted);
    r.key(Key::Char('1'), Modifiers::CTRL);
    assert_eq!(r.camera().zoom, 1.0, "its actual size");
    let tiles = r.drawn();
    assert_eq!(r.kept.live.len(), tiles);
    // Closed, a tab takes its tiles with it.
    r.key(Key::Char('w'), Modifiers::CTRL);
    r.frames(2);
    assert_eq!((r.ink.label(r.doc()).as_str(), r.kept.live.len()), ("untitled 2", 0), "a drawing takes the closed one's place");
}

#[test]
fn the_view_moves_by_the_hand_the_wheel_and_alt() {
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&corpus(ICON));
    let (from, by) = (r.middle(), Vec2::new(40.0, -25.0));
    // The Pointer's drag isn't the view's.
    assert_eq!(r.ink.tools.active(), Tool::Pointer);
    let at = r.camera().corner();
    r.drag(from, from + by);
    assert_eq!(r.camera().corner(), at);
    // The Hand's is.
    r.key(Key::Char('h'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Hand);
    r.drag(from, from + by);
    assert_eq!(r.camera().corner(), at + by);
    // So is anyone's with Alt held, or Space.
    r.key(Key::Char('v'), Modifiers::NONE);
    r.h.set_mods(Modifiers::ALT);
    r.drag(from, from - by);
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.camera().corner(), at);
    r.h.event(Event::Key { key: Key::Space, pressed: true, repeat: false, mods: Modifiers::NONE });
    r.drag(from, from + by);
    r.h.event(Event::Key { key: Key::Space, pressed: false, repeat: false, mods: Modifiers::NONE });
    r.frames(1);
    assert_eq!(r.camera().corner(), at + by);
    r.drag(from, from + by);
    assert_eq!(r.camera().corner(), at + by, "Space let go, the Pointer has its drag back");
    // The wheel slides it: a notch is 30 px.
    r.h.move_to(from);
    r.h.wheel(-1.0);
    r.frames(1);
    assert_eq!(r.camera().corner(), at + by + Vec2::new(0.0, -30.0));
    // The status bar reads where the pointer is, in the drawing's units.
    let (area, cam, to_page) = (r.ink.layout.canvas, r.camera(), r.ink.viewport().unwrap().to_page);
    let there = to_page.inverse().unwrap().apply(cam.page_at(area, from));
    assert!((r.ink.pointer.unwrap() - there).length() < 1e-9);
}

#[test]
fn the_zoom_tool_doubles_and_halves_about_its_click() {
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&corpus(ICON));
    r.key(Key::Char('z'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Zoom);
    let (area, at) = (r.ink.layout.canvas, r.middle() + Vec2::new(120.0, 60.0));
    let (zoom, under) = (r.camera().zoom, r.camera().page_at(area, at));
    r.click(at);
    assert!((r.camera().zoom - zoom * 2.0).abs() < 1e-9);
    // To within the whole pixel each view's corner is shown on.
    assert!((r.camera().page_at(area, at) - under).length() * r.camera().zoom < 1.5, "what was under the click still is");
    r.h.set_mods(Modifiers::ALT);
    r.click(at);
    r.h.set_mods(Modifiers::NONE);
    assert!((r.camera().zoom - zoom).abs() < 1e-9);
    // A drag isn't a click.
    r.drag(at, at + Vec2::new(60.0, 0.0));
    assert!((r.camera().zoom - zoom).abs() < 1e-9);
}

#[test]
fn a_file_saved_untouched_is_the_bytes_it_was() {
    let dir = scratch("save");
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&corpus(PLAIN));
    let (doc, copy) = (r.doc(), dir.join("copy.svg"));
    r.ink.write(doc, copy.clone(), Then::Nothing);
    r.until("the save", |r| r.ink.toast_text().is_some());
    assert_eq!(r.ink.toast_text(), Some("Saved copy.svg"));
    assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(corpus(PLAIN)).unwrap());
    // The copy is its file now.
    assert_eq!((r.ink.label(doc).as_str(), r.ink.is_modified(doc), r.ink.recent.paths[0].clone()), ("copy", false, copy.clone()));
    // Another drawing can't be saved over a file that's open.
    r.key(Key::Char('n'), Modifiers::CTRL);
    let other = r.doc();
    assert!(other != doc && r.ink.tabs.len() == 2);
    r.ink.pending_toast = None;
    r.ink.toast = None;
    r.ink.write(other, copy.clone(), Then::Nothing);
    assert_eq!(r.ink.toast_text(), Some("Couldn't save: copy.svg is open in the tab \u{201c}copy\u{201d}"));
    // "Save first?" answered Save: written, then closed.
    let second = dir.join("second.svg");
    r.ink.pending_toast = None;
    r.ink.write(other, second.clone(), Then::Close);
    r.until("the save", |r| r.ink.tabs.len() == 1);
    assert!(second.exists() && r.doc() == doc && r.ink.core.doc(other).is_err());
}

#[test]
fn tabs_come_and_go_and_only_changed_work_is_asked_about() {
    let mut r = Running::start(1280.0, 800.0, 1.0);
    let first = r.doc();
    r.key(Key::Char('n'), Modifiers::CTRL);
    assert_eq!((r.ink.tabs.len(), r.ink.label(r.doc()).as_str()), (2, "untitled 2"));
    // Untouched, it closes without a word; the first shows again.
    r.key(Key::Char('w'), Modifiers::CTRL);
    assert_eq!((r.ink.tabs.len(), r.doc()), (1, first));
    // Tool keys pick tools, and the shape tools take turns under R.
    for (key, tool) in [('a', Tool::Node), ('p', Tool::Pen), ('r', Tool::Rect), ('r', Tool::Ellipse), ('t', Tool::Text), ('g', Tool::Gradient), ('i', Tool::Eyedrop), ('z', Tool::Zoom), ('v', Tool::Pointer)] {
        r.key(Key::Char(key), Modifiers::NONE);
        assert_eq!(r.ink.tools.active(), tool, "{key}");
    }
    // A key with Ctrl is no tool's.
    r.key(Key::Char('h'), Modifiers::CTRL);
    assert_eq!(r.ink.tools.active(), Tool::Pointer);
}
