//! Snapping in the window: every drag lands on the grid (whole units,
//! and half ones this far in) or on a line of what's there, and shows
//! the line it landed on. Ctrl holds it off; View > Snapping turns it
//! off.

use ink_core::NodeId;
use lntrn_ui::{Action, HostCx};

use super::*;
use crate::menus;
use crate::snap::Landed;

/// A blue rect on the grid, a red one off it, and a line off it too.
const SCENE: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"8\" height=\"6\" fill=\"#08f\"/>\n  <rect id=\"odd\" x=\"20.3\" y=\"30.7\" width=\"10\" height=\"5\" fill=\"#c00\"/>\n  <path id=\"hill\" d=\"M30.3 8.4 L40 8 L36 16\" fill=\"none\" stroke=\"#000\"/>\n</svg>\n";
const A: NodeId = NodeId(2);
const HILL: NodeId = NodeId(4);

fn scene(name: &str) -> Running {
    let path = scratch(name).join("scene.svg");
    std::fs::write(&path, SCENE).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    fn run(&mut self, id: &str) {
        let mut requests = Vec::new();
        self.ink.act(&Action::new(id), &mut HostCx { pointer: Vec2::ZERO, requests: &mut requests });
        self.frames(2);
    }

    /// What `A` says of where it is and how big.
    fn a(&self) -> [String; 4] {
        ["x", "y", "width", "height"].map(|name| self.says(A, name).unwrap())
    }
}

#[test]
fn a_moved_shape_lands_on_the_grid_or_on_another_shapes_line() {
    let mut r = scene("snaps-move");
    let doc = r.doc();
    r.click(r.spot(8.0, 7.0));
    assert_eq!(r.selected(), [A]);
    // Dragged a bit over three units: its edges land on whole ones.
    r.drag(r.spot(8.0, 7.0), r.spot(11.2, 7.1));
    assert_eq!(r.a(), ["7", "4", "8", "6"]);
    // Nearer a half, a half.
    r.drag(r.spot(11.0, 7.0), r.spot(11.6, 7.0));
    assert_eq!(r.a()[0], "7.5");
    r.undo(1);
    // Its left near the red one's, which is off the grid: that's the
    // nearer, and the line shows while it's held.
    r.drag_to(r.spot(11.0, 7.0), r.spot(24.25, 7.0));
    assert_eq!(r.ink.landed, Landed { x: Some(20.3), y: None });
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("<rect id=\"a\" x=\"20.3\" y=\"4\""));
    r.let_go();
    assert_eq!((r.a()[0].as_str(), r.ink.landed, r.steps().len()), ("20.3", Landed::default(), 2));
    // With Shift it goes one way, and only lands that way.
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.spot(24.0, 7.0), r.spot(24.2, 20.2));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.a(), ["20.3", "17", "8", "6"]);
    // Ctrl holds it off: where the pointer takes it.
    r.h.set_mods(Modifiers::CTRL);
    r.drag(r.spot(24.0, 20.0), r.spot(24.33, 20.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.a()[0], "20.63");
    // View > Snapping turns it off, and on again.
    r.run(menus::SNAPPING);
    assert!(!r.ink.settings.snapping);
    r.drag(r.spot(24.0, 20.0), r.spot(24.41, 20.0));
    assert_eq!(r.a()[0], "21.04");
    r.run(menus::SNAPPING);
    r.drag(r.spot(24.0, 20.0), r.spot(23.7, 20.0));
    assert_eq!(r.a()[0], "20.5");
}

#[test]
fn the_boxs_handles_land_what_they_move() {
    let mut r = scene("snaps-scale");
    r.click(r.spot(8.0, 7.0));
    // The right side: across to the grid, and nothing else moves.
    r.drag(r.spot(12.0, 7.0), r.spot(15.2, 7.3));
    assert_eq!(r.a(), ["4", "4", "11", "6"]);
    // The bottom right corner: each way by itself.
    r.drag(r.spot(15.0, 10.0), r.spot(15.8, 11.3));
    assert_eq!(r.a(), ["4", "4", "12", "7.5"]);
    // With Shift the shape is kept (8 to 5), and one edge lands.
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.spot(16.0, 11.5), r.spot(19.9, 13.9));
    r.h.set_mods(Modifiers::NONE);
    let (w, h): (f64, f64) = (r.a()[2].parse().unwrap(), r.a()[3].parse().unwrap());
    assert!((w / h - 1.6).abs() < 1e-3 && (w == 16.0 || (h * 2.0).fract() == 0.0), "{w} × {h}");
}

#[test]
fn zoomed_out_things_land_on_whole_units_only() {
    let mut r = scene("snaps-far");
    let area = r.ink.layout.canvas;
    r.ink.tabs.active_mut().unwrap().camera.as_mut().unwrap().zoom_about(area, 0.4, area.center());
    r.frames(2);
    assert!(r.unit() < 12.0, "{}", r.unit());
    r.click(r.spot(8.0, 7.0));
    r.drag(r.spot(8.0, 7.0), r.spot(11.4, 7.0));
    assert_eq!(r.a()[0], "7");
}

#[test]
fn points_land_level_with_what_is_there() {
    let mut r = scene("snaps-points");
    // A new shape's corners: on the red one's left and right, which are
    // off the grid, and on the grid down the page.
    r.ink.tools.select(Tool::Rect);
    r.drag_to(r.spot(20.35, 40.2), r.spot(30.25, 44.1));
    assert_eq!(r.ink.landed, Landed { x: Some(30.3), y: None });
    r.let_go();
    let doc = r.ink.core.doc(r.doc()).unwrap();
    let last = doc.node(doc.root()).unwrap().elements().last().unwrap();
    assert_eq!(doc.markup(last).unwrap(), "<rect x=\"20.3\" y=\"40\" width=\"10\" height=\"4\" fill=\"#f3b700\"/>");
    // An anchor dragged: straight under the path's first one, which
    // stays where it is. (Its own box is no line: the anchor changes it.)
    r.ink.tools.select(Tool::Node);
    r.click(r.spot(35.0, 8.2));
    assert_eq!(r.selected(), [HILL]);
    r.drag_to(r.spot(36.0, 16.0), r.spot(30.35, 15.9));
    assert_eq!(r.ink.landed, Landed { x: Some(30.3), y: None });
    r.let_go();
    assert_eq!(r.says(HILL, "d").as_deref(), Some("M30.3 8.4 L40 8 L30.3 16"));
    // The Pen: where a point would land shows before the press.
    r.key(Key::Escape, Modifiers::NONE);
    r.key(Key::Escape, Modifiers::NONE);
    r.ink.tools.select(Tool::Pen);
    r.click(r.spot(10.2, 20.2));
    r.h.move_to(r.spot(20.33, 26.1));
    r.frames(2);
    assert_eq!(r.ink.landed, Landed { x: Some(20.3), y: None });
    r.click(r.spot(20.33, 26.1));
    let doc = r.ink.core.doc(r.doc()).unwrap();
    let last = doc.node(doc.root()).unwrap().elements().last().unwrap();
    assert!(doc.markup(last).unwrap().starts_with("<path d=\"M10 20 L20.3 26\""), "{}", doc.markup(last).unwrap());
}

#[test]
fn the_pixel_grid_is_a_switch_of_the_view_menu() {
    let mut r = scene("snaps-grid");
    assert!(r.ink.settings.pixel_grid && r.ink.menu_state().view.pixel_grid && r.ink.menu_state().view.snapping);
    r.run(menus::PIXEL_GRID);
    assert!(!r.ink.settings.pixel_grid && !r.ink.menu_state().view.pixel_grid);
    r.run(menus::PIXEL_GRID);
    assert!(r.ink.settings.pixel_grid);
}
