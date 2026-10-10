//! The rulers and the guides: a guide is dragged out of a ruler, kept
//! in the file, moved with the Pointer by where it crosses bare
//! canvas, and dragged back onto a ruler to be rid of it. Things land
//! on the guides that show.

use ink_core::NodeId;

use super::*;
use crate::menus;
use crate::snap::Landed;

/// A blue rect on the grid, a red one off it, and a line.
const SCENE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"8\" height=\"6\" fill=\"#08f\"/>\n  <rect id=\"odd\" x=\"20.3\" y=\"30.7\" width=\"10\" height=\"5\" fill=\"#c00\"/>\n  <path id=\"hill\" d=\"M30.3 8.4 L40 8 L36 16\" fill=\"none\" stroke=\"#000\"/>\n</svg>\n";
const ROOT: NodeId = NodeId(1);
const ODD: NodeId = NodeId(3);

fn scene(name: &str) -> Running {
    let path = scratch(name).join("scene.svg");
    std::fs::write(&path, SCENE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    /// The guides as the file says them.
    fn guides(&self) -> Option<String> {
        self.says(ROOT, "ink:guides")
    }

    /// A spot on the top ruler, and one on the left one.
    fn on_top(&self) -> Vec2 {
        self.ink.layout.ruler_top.center()
    }

    fn on_left(&self) -> Vec2 {
        self.ink.layout.ruler_left.center()
    }
}

#[test]
fn the_rulers_stand_round_the_canvas_and_can_be_hidden() {
    let mut r = scene("guides-rulers");
    let l = r.ink.layout;
    assert_eq!((l.ruler_top.height(), l.ruler_left.width(), l.ruler_top.max.y, l.ruler_left.max.x), (30.0, 30.0, l.canvas.min.y, l.canvas.min.x));
    assert!(r.ink.menu_state().view.rulers && r.ink.menu_state().view.guides && !r.ink.menu_state().view.any_guides);
    r.run(menus::RULERS);
    let hidden = r.ink.layout;
    assert!(hidden.ruler_top.is_empty() && hidden.canvas.min == l.ruler_corner.min && !r.ink.settings.rulers);
    r.run(menus::RULERS);
    assert_eq!(r.ink.layout, l);
}

#[test]
fn a_guide_is_dragged_out_of_a_ruler_and_kept_in_the_file() {
    let mut r = scene("guides-make");
    let doc = r.doc();
    // Out of the top ruler: a line across, landing on the grid. Nothing
    // is in the drawing till it's let go.
    r.drag_to(r.on_top(), r.spot(10.0, 12.2));
    assert!(r.ink.guiding.busy() && r.guides().is_none() && r.steps().is_empty() && !r.ink.core.gesturing(doc));
    r.let_go();
    assert_eq!((r.guides().as_deref(), r.steps(), r.ink.guiding.busy()), (Some("y12"), vec!["Add Guide".to_owned()], false));
    assert!(r.svg().starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\" xmlns:ink=\"urn:lantern:ink\" ink:guides=\"y12\">"));
    // Out of the left one: a line down, landing on the red one's edge,
    // which is off the grid; the line it landed on shows as it goes.
    r.drag_to(r.on_left(), r.spot(20.25, 30.0));
    assert_eq!(r.ink.landed, Landed { x: Some(20.3), y: None });
    r.let_go();
    assert_eq!(r.guides().as_deref(), Some("y12 x20.3"));
    assert!(r.ink.menu_state().view.any_guides);
    // Let go on the ruler again, it never was; so with Escape.
    r.drag(r.on_top(), r.on_top() + Vec2::new(40.0, 0.0));
    r.drag_to(r.on_top(), r.spot(10.0, 30.0));
    r.key(Key::Escape, Modifiers::NONE);
    r.let_go();
    assert_eq!((r.guides().as_deref(), r.steps().len()), (Some("y12 x20.3"), 2));
    // Each is a step to undo.
    r.undo(1);
    assert_eq!(r.guides().as_deref(), Some("y12"));
    r.undo(1);
    assert_eq!((r.guides(), r.svg().as_str()), (None, SCENE));
}

#[test]
fn the_pointer_moves_a_guide_by_where_it_crosses_bare_canvas() {
    let mut r = scene("guides-move");
    r.drag(r.on_top(), r.spot(10.0, 12.0));
    r.drag(r.on_left(), r.spot(20.3, 30.0));
    assert_eq!(r.guides().as_deref(), Some("y12 x20.3"));
    // Dragged where nothing's drawn: moved, landing on the grid.
    r.drag(r.spot(44.0, 12.0), r.spot(44.0, 15.3));
    assert_eq!((r.guides().as_deref(), r.steps().last().map(String::as_str), r.selected()), (Some("y15.5 x20.3"), Some("Move Guide"), vec![]));
    // Where it crosses a shape, a press is the shape's.
    r.drag(r.spot(20.4, 33.0), r.spot(24.4, 33.0));
    assert_eq!((r.guides().as_deref(), r.steps().last().map(String::as_str), r.selected()), (Some("y15.5 x20.3"), Some("Move"), vec![ODD]));
    r.key(Key::Escape, Modifiers::NONE);
    // With another tool in hand, a guide isn't taken hold of.
    r.ink.tools.select(Tool::Node);
    r.drag(r.spot(44.0, 15.5), r.spot(44.0, 20.0));
    assert_eq!(r.guides().as_deref(), Some("y15.5 x20.3"));
    r.ink.tools.select(Tool::Pointer);
    // Dragged back onto a ruler, it's gone.
    r.drag(r.spot(44.0, 15.5), r.on_top());
    assert_eq!((r.guides().as_deref(), r.steps().last().map(String::as_str)), (Some("x20.3"), Some("Remove Guide")));
    // View > Clear Guides takes the rest, as a step.
    r.run(menus::CLEAR_GUIDES);
    assert_eq!((r.guides(), r.steps().last().map(String::as_str), r.ink.menu_state().view.any_guides), (None, Some("Clear Guides"), false));
}

#[test]
fn things_land_on_the_guides_that_show() {
    let mut r = scene("guides-land");
    // A guide put where no grid line is (Ctrl holds the landing off).
    r.h.set_mods(Modifiers::CTRL);
    r.drag(r.on_left(), r.spot(10.37, 20.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.guides().as_deref(), Some("x10.37"));
    // A new shape's corner lands on it, and it shows as landed on.
    let newest = |r: &Running| {
        let doc = r.ink.core.doc(r.doc()).unwrap();
        doc.markup(doc.node(doc.root()).unwrap().elements().last().unwrap()).unwrap()
    };
    r.ink.tools.select(Tool::Rect);
    r.drag_to(r.spot(14.0, 40.0), r.spot(10.3, 44.0));
    assert_eq!(r.ink.landed, Landed { x: Some(10.37), y: None });
    r.let_go();
    assert_eq!(newest(&r), "<rect x=\"10.37\" y=\"40\" width=\"3.63\" height=\"4\" fill=\"#f3b700\"/>");
    // Hidden, nothing lands on them (with that shape gone again: its
    // own edge is a line there too).
    r.undo(1);
    r.run(menus::GUIDES);
    assert!(!r.ink.settings.guides);
    r.drag(r.spot(14.0, 20.0), r.spot(10.3, 24.0));
    assert_eq!(newest(&r), "<rect x=\"10.5\" y=\"20\" width=\"3.5\" height=\"4\" fill=\"#f3b700\"/>");
    // A new one dragged out shows them again.
    r.drag(r.on_top(), r.spot(30.0, 44.0));
    assert!(r.ink.settings.guides);
    assert_eq!(r.guides().as_deref(), Some("x10.37 y44"));
}
