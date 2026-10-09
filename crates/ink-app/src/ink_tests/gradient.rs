//! The Gradient tool: a drag across a shape paints it with a gradient
//! along the drag, its ends stay as handles, and its stops are on a bar
//! in the Box. Each drag is a gesture that lands as one step.

use ink_core::NodeId;

use super::*;

/// A blue card and a gold chip.
const TWO: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"card\" x=\"8\" y=\"8\" width=\"20\" height=\"10\" fill=\"#08f\"/>\n  <rect id=\"chip\" x=\"32\" y=\"30\" width=\"10\" height=\"10\" fill=\"#fc0\"/>\n</svg>\n";
const CARD: NodeId = NodeId(2);
const CHIP: NodeId = NodeId(3);

fn two(name: &str) -> Running {
    let path = scratch(name).join("two.svg");
    std::fs::write(&path, TWO).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r.key(Key::Char('g'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Gradient);
    r
}

impl Running {
    /// A click on a toggle of the Box.
    fn flip(&mut self, name: &str) {
        let row = self.in_box(name);
        self.click(Vec2::new(row.min.x + 27.0, row.center().y));
    }

    /// The gradient the card is filled with, as the file says it: its
    /// opening tag, and how many stops it has.
    fn gradient(&self) -> (String, usize) {
        let svg = self.svg();
        let from = svg.find("Gradient id=").map(|at| svg[..at].rfind('<').unwrap()).expect("a gradient");
        let tag = svg[from..].split('>').next().unwrap().to_owned() + ">";
        (tag, svg.matches("<stop ").count())
    }
}

#[test]
fn a_drag_across_a_shape_paints_it_with_a_gradient() {
    let mut r = two("gradient-drag");
    let doc = r.doc();
    // A press on a shape takes it up; the drag is the gradient's line,
    // shown as it goes: from the shape's colour to a darker one,
    // measured by its box.
    r.drag_to(r.spot(8.0, 13.0), r.spot(28.0, 13.0));
    assert!(r.ink.core.gesturing(doc) && r.steps().is_empty() && r.svg() == TWO && r.selected() == [CARD]);
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("fill=\"url(#gradient-1)\""));
    r.let_go();
    assert_eq!((r.says(CARD, "fill").as_deref(), r.steps(), r.gradient()), (Some("url(#gradient-1)"), vec!["Gradient".to_owned()], ("<linearGradient id=\"gradient-1\" x1=\"0\" y1=\"0.5\" x2=\"1\" y2=\"0.5\">".to_owned(), 2)));
    assert!(r.svg().contains("<stop offset=\"0\" stop-color=\"#0088ff\"/>") && r.svg().contains("<stop offset=\"1\" stop-color=\"#005cad\"/>"), "{}", r.svg());
    // Its ends stay as handles: each dragged, the gradient is changed
    // where it is.
    r.drag(r.spot(28.0, 13.0), r.spot(28.0, 18.0));
    assert_eq!((r.gradient().0.as_str(), r.steps().len()), ("<linearGradient id=\"gradient-1\" x1=\"0\" y1=\"0.5\" x2=\"1\" y2=\"1\">", 2));
    r.drag(r.spot(8.0, 13.0), r.spot(18.0, 8.0));
    assert_eq!(r.gradient().0, "<linearGradient id=\"gradient-1\" x1=\"0.5\" y1=\"0\" x2=\"1\" y2=\"1\">");
    // A drag anywhere else is a new line for it; with Shift, held to
    // 45°. A press that goes nowhere is nothing; nor is a drag given
    // up.
    r.h.set_mods(Modifiers::SHIFT);
    r.drag(r.spot(8.0, 8.0), r.spot(28.0, 9.0));
    r.h.set_mods(Modifiers::NONE);
    assert_eq!((r.gradient().0.as_str(), r.steps().len()), ("<linearGradient id=\"gradient-1\" x1=\"0\" y1=\"0\" x2=\"1.001\" y2=\"0\">", 4));
    r.click(r.spot(2.0, 2.0));
    r.drag_to(r.spot(2.0, 2.0), r.spot(20.0, 20.0));
    r.key(Key::Escape, Modifiers::NONE);
    r.let_go();
    assert_eq!((r.steps().len(), r.ink.core.gesturing(doc)), (4, false));
    // "Radial" makes it the other kind, about where its line began; and
    // the next one drawn is round too, about where the drag begins.
    r.click(r.ink.toolbox.rect().expect("the Gradient tool's Box").center());
    r.flip("Radial");
    assert_eq!((r.gradient().0.as_str(), r.steps().len(), r.says(CARD, "fill").as_deref()), ("<radialGradient id=\"gradient-2\" cx=\"0\" cy=\"0\" r=\"1.001\">", 5, Some("url(#gradient-2)")));
    r.drag(r.spot(37.0, 35.0), r.spot(42.0, 35.0));
    assert_eq!((r.selected(), r.says(CHIP, "fill").as_deref()), (vec![CHIP], Some("url(#gradient-1)")));
    assert!(r.svg().contains("<radialGradient id=\"gradient-1\" cx=\"0.5\" cy=\"0.5\" r=\"0.5\">"), "{}", r.svg());
    r.undo(6);
    assert_eq!(r.svg(), TWO);
}

#[test]
fn the_box_has_the_gradients_stops_on_a_bar() {
    let mut r = two("gradient-stops");
    let doc = r.doc();
    r.drag(r.spot(8.0, 13.0), r.spot(28.0, 13.0));
    r.click(r.ink.toolbox.rect().unwrap().center());
    assert_eq!(r.box_rows(), ["Radial", "Stroke", "Stops", "Stop Colour", "At", "Remove Stop"]);
    // A press on the bar puts a stop there, the colour the gradient is
    // at that place; and it's the one picked.
    let bar = r.in_box("Stops");
    let along = |share: f64| Vec2::new(bar.min.x + 2.0 + (bar.width() - 4.0) * share, bar.center().y);
    r.click(along(0.5));
    assert_eq!((r.gradient().1, r.steps()), (3, vec!["Gradient".to_owned(); 2]));
    assert!(r.svg().contains("<stop offset=\"0.5\" stop-color=\"#0072d6\"/>"), "{}", r.svg());
    // Dragged along the bar: shown as it goes, one step, and no further
    // than its neighbours.
    r.drag_to(along(0.5), along(0.25));
    assert!(r.ink.core.gesturing(doc) && r.steps().len() == 2);
    r.let_go();
    assert!(r.svg().contains("<stop offset=\"0.25\" stop-color=\"#0072d6\"/>") && r.steps().len() == 3, "{}", r.svg());
    r.drag(along(0.25), along(1.4));
    assert!(r.svg().contains("<stop offset=\"1\" stop-color=\"#0072d6\"/><stop offset=\"1\"") || r.svg().matches("offset=\"1\"").count() == 2, "{}", r.svg());
    // Where it is, typed; and taken off again. Two stops are the fewest.
    r.type_in_box("At", "60");
    assert!(r.svg().contains("<stop offset=\"0.6\" stop-color=\"#0072d6\"/>"), "{}", r.svg());
    r.press_in_box("Remove Stop");
    assert_eq!((r.gradient().1, r.steps().last().map(String::as_str)), (2, Some("Gradient")));
    let steps = r.steps().len();
    r.press_in_box("Remove Stop");
    assert_eq!((r.gradient().1, r.steps().len()), (2, steps));
    // The stroke has no gradient: nothing on the bar to show.
    r.flip("Stroke");
    assert_eq!(r.box_rows(), ["Radial", "Stroke"]);
    r.undo(steps);
    assert_eq!(r.svg(), TWO);
}
