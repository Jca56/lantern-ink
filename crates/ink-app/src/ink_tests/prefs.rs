//! Edit > Preferences: what things dragged land on, how finely a new
//! drawing is written, and the look; and the dialogs' own rows, drawn
//! in real frames.

use super::*;
use crate::menus;
use crate::prefs::Ground;

/// A blue rect on the grid and a red one off it.
const SCENE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"8\" height=\"6\" fill=\"#08f\"/>\n  <rect id=\"odd\" x=\"20.3\" y=\"30.7\" width=\"10\" height=\"5\" fill=\"#c00\"/>\n</svg>\n";

fn scene(name: &str) -> Running {
    let path = scratch(name).join("scene.svg");
    std::fs::write(&path, SCENE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    /// Draw a rectangle from one spot of the drawing to another, and
    /// say what it's written as.
    fn drawn_rect(&mut self, from: (f64, f64), to: (f64, f64)) -> String {
        self.ink.tools.select(Tool::Rect);
        self.drag(self.spot(from.0, from.1), self.spot(to.0, to.1));
        let doc = self.ink.core.doc(self.doc()).unwrap();
        let markup = doc.markup(doc.node(doc.root()).unwrap().elements().last().unwrap()).unwrap();
        self.undo(1);
        markup
    }
}

#[test]
fn preferences_say_what_things_land_on() {
    let mut r = scene("prefs-snap");
    // All three: the left on the red one's edge (off the grid), the
    // rest on the grid.
    assert_eq!(r.drawn_rect((20.35, 40.2), (30.9, 44.1)), "<rect x=\"20.3\" y=\"40\" width=\"10.7\" height=\"4\" fill=\"#f3b700\"/>");
    // Not on shapes: the grid alone.
    r.ink.settings.snap_shapes = false;
    r.ink.snaps = None;
    assert_eq!(r.drawn_rect((20.35, 40.2), (30.9, 44.1)), "<rect x=\"20.5\" y=\"40\" width=\"10.5\" height=\"4\" fill=\"#f3b700\"/>");
    // Not on the grid: shapes' lines alone, and where there's none
    // near, where the pointer is.
    (r.ink.settings.snap_shapes, r.ink.settings.snap_grid) = (true, false);
    r.ink.snaps = None;
    assert_eq!(r.drawn_rect((20.35, 40.25), (33.875, 44.125)), "<rect x=\"20.3\" y=\"40.25\" width=\"13.575\" height=\"3.875\" fill=\"#f3b700\"/>");
}

#[test]
fn a_new_drawing_is_written_as_finely_as_preferences_say() {
    let mut r = Running::start(1280.0, 800.0, 1.0);
    r.ink.settings.new_decimals = 1;
    r.ink.new_document();
    r.frames(2);
    // Said on its root from the start: nothing was done to it.
    assert!(r.svg().contains("viewBox=\"0 0 24 24\" ink:decimals=\"1\">") && r.ink.untouched(r.doc()), "{}", r.svg());
    r.h.set_mods(Modifiers::CTRL);
    r.ink.tools.select(Tool::Rect);
    r.drag(r.spot(4.26, 4.0), r.spot(12.0, 10.52));
    r.h.set_mods(Modifiers::NONE);
    assert!(r.svg().contains("<rect x=\"4.3\" y=\"4\" width=\"7.7\" height=\"6.5\""), "{}", r.svg());
    // Three is Ink's own, and isn't said.
    r.ink.settings.new_decimals = 3;
    r.ink.new_document();
    r.frames(2);
    assert!(!r.svg().contains("decimals"));
}

#[test]
fn the_dialogs_draw_their_rows_in_real_frames() {
    let mut r = scene("prefs-dialogs");
    let settings = (r.ink.settings.snap_grid, r.ink.settings.snap_shapes, r.ink.settings.snap_guides, r.ink.settings.ground(), r.ink.settings.decimals());
    // Preferences (Ctrl+,), Page… and Export… (Ctrl+E): each is up for
    // a few frames with its rows drawn, and Escape puts it away with
    // nothing done.
    r.key(Key::Char(','), Modifiers::CTRL);
    r.frames(3);
    // The drawing under a dialog isn't the pointer's: a click there
    // picks nothing.
    r.click(r.spot(8.0, 7.0));
    assert!(r.selected().is_empty());
    r.key(Key::Escape, Modifiers::NONE);
    r.ask(menus::PAGE);
    assert_eq!(r.ink.paging.width, "48");
    r.key(Key::Escape, Modifiers::NONE);
    r.key(Key::Char('e'), Modifiers::CTRL);
    r.frames(3);
    assert!(r.ink.exporting.ready());
    r.key(Key::Escape, Modifiers::NONE);
    assert!(r.steps().is_empty() && r.svg() == SCENE);
    assert_eq!(settings, (true, true, true, Ground::Checks, 3));
    // With them away, the canvas is the pointer's again.
    r.click(r.spot(8.0, 7.0));
    assert_eq!(r.selected().len(), 1);
}
