//! A drawn shape's own handles: a rectangle's corner dot on the canvas,
//! and the selected shape's own rows in the Box (a rectangle's corners;
//! a polygon's sides, whether it's a star, how deep its points go).

use ink_core::NodeId;

use super::*;
use crate::shapebox;

/// A card, a pentagon as the tool draws one (10 from its middle, about
/// 36,32), a bar too thin for a dot, and a locked square.
const SHAPES: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" viewBox=\"0 0 48 48\">\n  <rect id=\"card\" x=\"4\" y=\"4\" width=\"20\" height=\"12\" fill=\"#08f\"/>\n  <polygon id=\"five\" points=\"36,22 45.511,28.91 41.878,40.09 30.122,40.09 26.489,28.91\" fill=\"#fc0\"/>\n  <rect id=\"thin\" x=\"4\" y=\"40\" width=\"20\" height=\"2\" fill=\"#333\"/>\n  <rect id=\"held\" x=\"30\" y=\"4\" width=\"12\" height=\"12\" fill=\"#c00\" ink:locked=\"true\"/>\n</svg>\n";
const CARD: NodeId = NodeId(2);
const FIVE: NodeId = NodeId(3);
const THIN: NodeId = NodeId(4);
const HELD: NodeId = NodeId(5);

fn shapes(name: &str) -> Running {
    let path = scratch(name).join("shapes.svg");
    std::fs::write(&path, SHAPES).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    assert_eq!(r.ink.tools.active(), Tool::Pointer);
    r
}

impl Running {
    /// The number `node` says its `name` is.
    fn number(&self, node: NodeId, name: &str) -> f64 {
        self.says(node, name).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("{node} has no {name}"))
    }

    fn choose(&mut self, nodes: &[NodeId]) {
        let sel = &mut self.ink.tabs.active_mut().unwrap().selection;
        (sel.active, sel.nodes) = (nodes.last().copied(), nodes.to_vec());
        self.frames(1);
    }

    /// The names of what the Box holds, top to bottom.
    fn held_in_box(&self) -> Vec<&'static str> {
        self.ink.toolbox.laid.iter().map(|(name, _)| *name).collect()
    }

    /// The pentagon, as the Box reads it: its sides, and a star's depth
    /// in percent.
    fn five(&self) -> (f64, Option<f64>) {
        let doc = self.ink.core.doc(self.doc()).unwrap();
        let (_, s, _) = shapebox::read(doc, FIVE, 0.5).expect("a regular polygon still");
        (s.sides, s.star.then_some((s.depth * 100.0).round()))
    }
}

#[test]
fn a_rectangles_dot_rounds_its_corners() {
    let mut r = shapes("handles-dot");
    let doc = r.doc();
    r.click(r.spot(20.0, 12.0));
    assert_eq!(r.selected(), [CARD]);
    // The dot: 28 px inside the first corner, while the corners are
    // square. Dragged in along its line, the drawing shows the corners
    // rounder as it goes, on whole units, and is itself untouched.
    let u = r.unit();
    let dot = r.spot(4.0, 4.0) + Vec2::new(28.0, 28.0);
    r.h.advance(1.0);
    r.h.move_to(dot);
    r.frames(1);
    r.h.press();
    r.frames(1);
    r.h.move_to(dot + Vec2::new(2.2 * u, 2.2 * u));
    r.frames(2);
    assert!(r.ink.core.gesturing(doc) && r.steps().is_empty() && r.svg() == SHAPES);
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("fill=\"#08f\" rx=\"2\"/>"));
    r.h.move_to(dot + Vec2::new(3.2 * u, 3.2 * u));
    r.frames(1);
    r.h.release();
    r.frames(2);
    // Landed: one step; one radius says both; the card is still what's
    // picked, where it was.
    assert_eq!((r.says(CARD, "rx").as_deref(), r.says(CARD, "ry"), r.steps(), r.selected()), (Some("3"), None, vec!["Corners".to_owned()], vec![CARD]));
    assert_eq!((r.number(CARD, "x"), r.number(CARD, "width")), (4.0, 20.0));
    // The dot went in with the corner's arc. With Ctrl, as far as it's
    // dragged, to what the file can say.
    let dot = r.spot(7.0, 7.0) + Vec2::new(28.0, 28.0);
    r.h.set_mods(Modifiers::CTRL);
    r.drag(dot, dot + Vec2::new(0.5 * u, 0.5 * u));
    r.h.set_mods(Modifiers::NONE);
    assert!((r.number(CARD, "rx") - 3.5).abs() < 0.01, "{:?}", r.says(CARD, "rx"));
    // No rounder than half its shorter side; no squarer than square.
    let dot = dot + Vec2::new(0.5 * u, 0.5 * u);
    r.drag(dot, dot + Vec2::new(30.0 * u, 30.0 * u));
    assert_eq!(r.number(CARD, "rx"), 6.0);
    let dot = r.spot(10.0, 10.0) + Vec2::new(28.0, 28.0);
    r.drag(dot, dot - Vec2::new(30.0 * u, 30.0 * u));
    assert_eq!((r.number(CARD, "rx"), r.steps().len()), (0.0, 4));
    // A click on the dot does nothing: nothing let go of, no step.
    let dot = r.spot(4.0, 4.0) + Vec2::new(28.0, 28.0);
    r.click(dot);
    assert_eq!((r.selected(), r.steps().len()), (vec![CARD], 4));
    // A drag given up is as if it never began.
    r.h.advance(1.0);
    r.h.move_to(dot);
    r.frames(1);
    r.h.press();
    r.frames(1);
    r.h.move_to(dot + Vec2::new(4.0 * u, 4.0 * u));
    r.frames(2);
    assert!(r.ink.core.gesturing(doc));
    r.key(Key::Escape, Modifiers::NONE);
    r.h.release();
    r.frames(2);
    assert_eq!((r.number(CARD, "rx"), r.steps().len(), r.ink.core.gesturing(doc)), (0.0, 4, false));
    // Each was one step: undone, the file as it was.
    for _ in 0..4 {
        r.key(Key::Char('z'), Modifiers::CTRL);
    }
    assert_eq!(r.svg(), SHAPES);
}

#[test]
fn only_a_rectangle_with_room_for_it_has_a_dot() {
    let mut r = shapes("handles-none");
    let u = r.unit();
    // Too thin on the screen for a dot clear of its box's handles: a
    // drag from where one would be moves it, as from anywhere inside.
    assert!(2.0 * u < 56.0, "the bar shows {} px tall", 2.0 * u);
    r.click(r.spot(14.0, 41.0));
    assert_eq!(r.selected(), [THIN]);
    let inside = r.spot(4.0, 40.0) + Vec2::new(28.0, 28.0);
    r.drag(inside, inside + Vec2::new(2.0 * u, 0.0));
    assert_eq!((r.steps(), r.says(THIN, "rx"), r.number(THIN, "x")), (vec!["Move".to_owned()], None, 6.0));
    // A polygon has none; nor has a rectangle that's locked.
    r.click(r.spot(36.0, 32.0));
    assert_eq!(r.selected(), [FIVE]);
    let inside = r.spot(26.489, 22.0) + Vec2::new(28.0, 28.0);
    r.drag(inside, inside + Vec2::new(u, u));
    assert_eq!(r.steps(), ["Move", "Move"]);
    r.choose(&[HELD]);
    let inside = r.spot(30.0, 4.0) + Vec2::new(28.0, 28.0);
    r.drag(inside, inside + Vec2::new(2.0 * u, 2.0 * u));
    assert_eq!((r.steps().len(), r.says(HELD, "rx")), (2, None));
    // And with two picked, the dot is neither's.
    r.choose(&[THIN, CARD]);
    let dot = r.spot(4.0, 4.0) + Vec2::new(28.0, 28.0);
    r.drag(dot, dot + Vec2::new(2.0 * u, 2.0 * u));
    assert_eq!((r.steps().last().map(String::as_str), r.says(CARD, "rx")), (Some("Move"), None));
}

#[test]
fn the_box_holds_the_selected_shapes_own_rows() {
    let mut r = shapes("handles-box");
    let doc = r.doc();
    let square = r.ink.toolbox.rect().expect("the Box, with the Pointer in hand");
    r.click(square.center());
    assert_eq!(r.held_in_box(), ["Scale strokes"]);
    // A rectangle's corners, typed: one step.
    r.choose(&[CARD]);
    r.frames(1);
    assert_eq!(r.held_in_box(), ["X", "Y", "W", "H", "Corners", "Scale strokes"]);
    r.type_in_box("Corners", "2.5");
    assert_eq!((r.says(CARD, "rx").as_deref(), r.steps()), (Some("2.5"), vec!["Corners".to_owned()]));
    // Dragged along, a tenth of a unit a pixel on a page this size:
    // shown as it goes, landed as one step.
    let at = r.in_box("Corners").center();
    r.h.advance(1.0);
    r.h.move_to(at);
    r.frames(1);
    r.h.press();
    r.frames(1);
    r.h.move_to(at + Vec2::new(20.0, 0.0));
    r.frames(2);
    assert!(r.ink.core.gesturing(doc) && r.ink.tuning.is_some() && r.steps().len() == 1 && r.says(CARD, "rx").as_deref() == Some("2.5"));
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("rx=\"4.5\""));
    r.h.move_to(at + Vec2::new(30.0, 0.0));
    r.frames(1);
    r.h.release();
    r.frames(2);
    assert_eq!((r.says(CARD, "rx").as_deref(), r.steps(), r.ink.tuning.is_none()), (Some("5.5"), vec!["Corners".to_owned(); 2], true));
    // No rounder than it can be.
    r.type_in_box("Corners", "40");
    assert_eq!(r.number(CARD, "rx"), 6.0);
    // Set on the one in hand, it's set on every rectangle picked: each
    // as far as it can go.
    r.choose(&[FIVE, THIN, CARD]);
    r.type_in_box("Corners", "2");
    assert_eq!((r.number(CARD, "rx"), r.number(THIN, "rx"), r.five()), (2.0, 1.0, (5.0, None)));
    // A polygon's sides: read off its corners, and set on the same
    // circle.
    r.choose(&[FIVE]);
    r.frames(1);
    assert_eq!(r.held_in_box(), ["X", "Y", "W", "H", "Sides", "Star", "Scale strokes"]);
    r.type_in_box("Sides", "6");
    assert_eq!((r.five(), r.says(FIVE, "points").map(|p| p.matches(',').count()), r.steps().last().map(String::as_str)), ((6.0, None), Some(6), Some("Sides")));
    // (Its middle and its reach are read off rounded numbers: right to
    // the file's last decimal.)
    let second: Vec<f64> = r.says(FIVE, "points").unwrap().split(' ').nth(1).unwrap().split(',').map(|v| v.parse().unwrap()).collect();
    assert!((second[0] - 44.66).abs() < 0.0015 && (second[1] - 27.0).abs() < 0.0015, "{:?}", r.says(FIVE, "points"));
    // A star: as many corners again between; then how deep they go.
    let star = r.in_box("Star");
    r.click(Vec2::new(star.min.x + 27.0, star.center().y));
    assert_eq!((r.five(), r.says(FIVE, "points").map(|p| p.matches(',').count())), ((6.0, Some(50.0)), Some(12)));
    assert_eq!(r.held_in_box(), ["X", "Y", "W", "H", "Sides", "Star", "Depth", "Scale strokes"]);
    let rail = r.in_box("Depth");
    r.click(Vec2::new(rail.min.x + rail.width() * 0.25, rail.center().y));
    let (sides, depth) = r.five();
    assert!(sides == 6.0 && depth.is_some_and(|d| d < 45.0), "{depth:?}");
    let steps = r.steps();
    assert_eq!(steps[steps.len() - 3..], ["Sides", "Star", "Depth"]);
    // What has no rows of its own, or is locked: the place and size
    // alone.
    r.choose(&[HELD]);
    r.frames(1);
    assert_eq!(r.held_in_box(), ["X", "Y", "W", "H", "Scale strokes"]);
    // Every one of them was a step: undone, the file as it was.
    for _ in 0..steps.len() {
        r.key(Key::Char('z'), Modifiers::CTRL);
    }
    assert_eq!(r.svg(), SHAPES);
}
