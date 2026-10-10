//! The shape tools: a drag draws a new shape, shown as it's dragged
//! out and landed as one step, painted as the last one was, on top of
//! the level the Pointer is in. The Box holds the tools' own settings.

use ink_core::NodeId;
use lntrn_math::Color;

use super::*;
use crate::paint::{Paint, Set, Which};

/// An empty page of 48 units, and a moved group to draw into.
const EMPTY: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <g id=\"g\" transform=\"translate(20 10)\">\n    <rect id=\"in\" width=\"4\" height=\"4\"/>\n  </g>\n</svg>\n";
const G: NodeId = NodeId(2);

fn empty(name: &str) -> Running {
    let path = scratch(name).join("empty.svg");
    std::fs::write(&path, EMPTY).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    /// Where the drawing's point (`x`, `y`) shows in the window.
    fn on_page(&self, x: f64, y: f64) -> Vec2 {
        let page = self.ink.viewport().unwrap().to_page.apply(Vec2::new(x, y));
        self.camera().window_at(self.ink.layout.canvas, page)
    }

    /// The last thing in the drawing's top level, as markup.
    fn newest(&self) -> String {
        let doc = self.ink.core.doc(self.doc()).unwrap();
        let last = doc.node(doc.root()).unwrap().elements().last().unwrap();
        doc.markup(last).unwrap()
    }
}

#[test]
fn a_drag_with_a_shape_tool_draws_a_shape() {
    let mut r = empty("shapes-draw");
    let doc = r.doc();
    r.key(Key::Char('r'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Rect);
    // Dragged out: shown as it goes, with the drawing untouched.
    let (from, to) = (r.on_page(4.0, 30.0), r.on_page(14.0, 36.0));
    r.h.advance(1.0);
    r.h.move_to(from);
    r.frames(1);
    r.h.press();
    r.frames(1);
    r.h.move_to(from + (to - from) * 0.5);
    r.frames(2);
    assert!(r.ink.core.gesturing(doc) && r.steps().is_empty() && r.svg() == EMPTY);
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("<rect x=\"4\" y=\"30\" width=\"5\" height=\"3\""));
    r.h.move_to(to);
    r.frames(1);
    r.h.release();
    r.frames(2);
    // Landed: one step, on top, selected, in Lantern's gold; and the
    // tool is still in hand.
    assert_eq!((r.newest(), r.steps()), ("<rect x=\"4\" y=\"30\" width=\"10\" height=\"6\" fill=\"#f3b700\"/>".to_owned(), vec!["Rectangle".to_owned()]));
    let made = r.selected();
    assert_eq!((made.len(), r.ink.tools.active(), r.ink.core.gesturing(doc)), (1, Tool::Rect, false));
    // The next one: with Shift a square; an ellipse's is a circle.
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.on_page(30.0, 30.0), r.on_page(40.0, 34.0));
    assert_eq!(r.newest(), "<rect x=\"30\" y=\"30\" width=\"10\" height=\"10\" fill=\"#f3b700\"/>");
    r.h.set_mods(Modifiers::NONE);
    r.key(Key::Char('r'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Ellipse);
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.on_page(4.0, 4.0), r.on_page(12.0, 10.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!((r.newest(), r.steps().len()), ("<circle cx=\"8\" cy=\"8\" r=\"4\" fill=\"#f3b700\"/>".to_owned(), 3));
    // A press and no drag draws nothing; nor does a drag given up.
    r.click(r.on_page(44.0, 44.0));
    let (from, to) = (r.on_page(40.0, 40.0), r.on_page(46.0, 46.0));
    r.h.advance(1.0);
    r.h.move_to(from);
    r.frames(1);
    r.h.press();
    r.frames(1);
    r.h.move_to(to);
    r.frames(2);
    r.key(Key::Escape, Modifiers::NONE);
    r.h.release();
    r.frames(2);
    assert_eq!((r.steps().len(), r.ink.core.gesturing(doc)), (3, false));
    // Undone, each shape is one step.
    for _ in 0..3 {
        r.key(Key::Char('z'), Modifiers::CTRL);
    }
    assert_eq!(r.svg(), EMPTY);
}

#[test]
fn a_new_shape_is_painted_as_the_last_one_was() {
    let mut r = empty("shapes-paint");
    // What the paint section was last told, with nothing selected.
    r.ink.set_paint(Set::Paint(Which::Fill, Paint::None), false);
    r.ink.set_paint(Set::Paint(Which::Stroke, Paint::Color(Color::hex(0x102030))), false);
    r.ink.set_paint(Set::Width(2.0), false);
    r.ink.tools.select(Tool::Line);
    r.drag(r.on_page(4.0, 40.0), r.on_page(14.0, 44.0));
    assert_eq!(r.newest(), "<line x1=\"4\" y1=\"40\" x2=\"14\" y2=\"44\" stroke=\"#102030\" stroke-width=\"2\"/>");
    r.ink.tools.select(Tool::Polygon);
    r.drag(r.on_page(30.0, 30.0), r.on_page(40.0, 40.0));
    assert!(r.newest().starts_with("<polygon points=\"35,30 40,33.82 ") && r.newest().ends_with("fill=\"none\" stroke=\"#102030\" stroke-width=\"2\"/>"), "{}", r.newest());
    assert_eq!(r.steps(), ["Line", "Polygon"]);
}

#[test]
fn a_shape_goes_into_the_group_the_pointer_is_in() {
    let mut r = empty("shapes-group");
    // In the moved group: a drag at (30, 20) to (36, 24) on the page is
    // (10, 10) to (16, 14) in the group's own coordinates.
    r.ink.tabs.active_mut().unwrap().selection.within = Some(G);
    r.ink.tools.select(Tool::Rect);
    r.drag(r.on_page(30.0, 20.0), r.on_page(36.0, 24.0));
    let doc = r.ink.core.doc(r.doc()).unwrap();
    let made = r.selected()[0];
    assert_eq!((doc.node(made).unwrap().parent, doc.markup(made).unwrap()), (Some(G), "<rect x=\"10\" y=\"10\" width=\"6\" height=\"4\" fill=\"#f3b700\"/>".to_owned()));
}

#[test]
fn the_box_holds_a_shape_tools_own_settings() {
    let mut r = empty("shapes-box");
    // The Ellipse and the Line have none: no Box.
    r.ink.tools.select(Tool::Ellipse);
    r.frames(2);
    assert!(r.ink.toolbox.rect().is_none());
    r.ink.tools.select(Tool::Rect);
    r.frames(2);
    let square = r.ink.toolbox.rect().expect("the Rectangle's Box");
    r.click(square.center());
    assert_eq!(r.ink.toolbox.laid.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["Corners"]);
    // Its corners' rounding, typed: the next rectangle has it.
    let corners = r.ink.toolbox.laid[0].1.center();
    r.click(corners);
    r.h.type_text("2");
    r.frames(1);
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.ink.shape_settings.radius, 2.0);
    r.drag(r.on_page(4.0, 30.0), r.on_page(14.0, 40.0));
    assert_eq!(r.newest(), "<rect x=\"4\" y=\"30\" width=\"10\" height=\"10\" rx=\"2\" fill=\"#f3b700\"/>");
    // The Polygon's: its sides, and a star with its depth.
    r.ink.tools.select(Tool::Polygon);
    r.frames(2);
    assert_eq!(r.ink.toolbox.laid.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["Sides", "Star"]);
    let star = r.ink.toolbox.laid[1].1;
    r.click(Vec2::new(star.min.x + 27.0, star.center().y));
    assert!(r.ink.shape_settings.star);
    assert_eq!(r.ink.toolbox.laid.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["Sides", "Star", "Depth"]);
    r.drag(r.on_page(30.0, 30.0), r.on_page(40.0, 40.0));
    // Five points and five between: ten corners.
    assert_eq!(r.newest().matches(',').count(), 10, "{}", r.newest());
    assert_eq!(r.steps(), ["Rectangle", "Polygon"]);
}

#[test]
fn a_shape_lands_on_the_grid_unless_ctrl_frees_it() {
    let mut r = empty("shapes-grid");
    r.ink.tools.select(Tool::Rect);
    // Whole units, and this far in, half ones.
    r.drag(r.on_page(4.2, 29.9), r.on_page(13.8, 36.4));
    assert_eq!(r.newest(), "<rect x=\"4\" y=\"30\" width=\"10\" height=\"6.5\" fill=\"#f3b700\"/>");
    // Out from the middle, the middle is on the grid too.
    r.h.set_mods(Modifiers::ALT);
    r.drag(r.on_page(30.4, 20.2), r.on_page(33.3, 22.1));
    assert_eq!(r.newest(), "<rect x=\"27.5\" y=\"18\" width=\"6\" height=\"4\" fill=\"#f3b700\"/>");
    // With Ctrl, where the pointer is, as finely as the file says.
    r.h.set_mods(Modifiers::CTRL);
    r.drag(r.on_page(4.25, 40.5), r.on_page(10.75, 44.125));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!(r.newest(), "<rect x=\"4.25\" y=\"40.5\" width=\"6.5\" height=\"3.625\" fill=\"#f3b700\"/>");
}
