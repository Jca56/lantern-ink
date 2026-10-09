//! The Path menu: its rows lit by what's selected, each one step on the
//! shapes it's for, by way of the action a row sends.

use ink_core::NodeId;
use ink_geom::Combine;
use lntrn_ui::HostCx;

use super::*;
use crate::menus;
use crate::pathops::PathOp;

/// Back to front: a blue square, a gold one over its corner, and a
/// dark one with a line round it.
const SQUARES: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"16\" height=\"16\" fill=\"#08f\"/>\n  <rect id=\"b\" x=\"12\" y=\"12\" width=\"16\" height=\"16\" fill=\"#fc0\"/>\n  <rect id=\"c\" x=\"30\" y=\"4\" width=\"10\" height=\"10\" fill=\"#333\" stroke=\"#000\" stroke-width=\"2\"/>\n</svg>\n";
const A: NodeId = NodeId(2);
const B: NodeId = NodeId(3);
const C: NodeId = NodeId(4);

fn squares(name: &str) -> Running {
    let path = scratch(name).join("squares.svg");
    std::fs::write(&path, SQUARES).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    fn take(&mut self, nodes: &[NodeId]) {
        let sel = &mut self.ink.tabs.active_mut().unwrap().selection;
        (sel.active, sel.nodes) = (nodes.last().copied(), nodes.to_vec());
        self.frames(1);
    }

    /// The Path menu's row for `op`, pressed.
    fn path(&mut self, op: PathOp) {
        let mut requests = Vec::new();
        self.ink.act(&menus::path_action(op), &mut HostCx { pointer: Vec2::ZERO, requests: &mut requests });
        self.frames(2);
    }

    /// The Path menu's rows that are lit.
    fn lit(&self) -> Vec<&'static str> {
        let can = self.ink.menu_state().paths;
        PathOp::ROWS.iter().flat_map(|group| group.iter()).filter(|op| can.does(**op)).map(|op| op.label()).collect()
    }
}

#[test]
fn the_path_menu_makes_shapes_one() {
    let mut r = squares("paths-combine");
    // Nothing selected, nothing lit, and a row pressed does nothing.
    assert!(r.lit().is_empty());
    r.path(PathOp::Combine(Combine::Union));
    assert!(r.steps().is_empty());
    // Two shapes, picked front one first: the one further back still
    // takes the result, and keeps its paint and its place.
    r.take(&[B, A]);
    assert_eq!(r.lit(), ["Object to Path", "Union", "Subtract", "Intersect", "Exclude"]);
    r.path(PathOp::Combine(Combine::Subtract));
    assert_eq!((r.says(A, "d").as_deref(), r.says(A, "fill").as_deref(), r.steps(), r.selected()), (Some("M4 4 H20 V12 H12 V20 H4 Z"), Some("#08f"), vec!["Subtract".to_owned()], vec![A]));
    assert!(r.ink.core.doc(r.doc()).unwrap().get(B).is_none());
    r.undo(1);
    assert_eq!(r.svg(), SQUARES);
    r.take(&[A, B]);
    r.path(PathOp::Combine(Combine::Union));
    assert_eq!(r.says(A, "d").as_deref(), Some("M4 4 H20 V12 H28 V28 H12 V20 H4 Z"));
    r.undo(1);
    r.take(&[A, B]);
    r.path(PathOp::Combine(Combine::Intersect));
    assert_eq!(r.says(A, "d").as_deref(), Some("M20 12 V20 H12 V12 Z"));
    r.undo(1);
    r.take(&[A, B]);
    r.path(PathOp::Combine(Combine::Exclude));
    assert_eq!((r.says(A, "d").map(|d| d.matches('M').count()), r.steps()), (Some(2), vec!["Exclude".to_owned()]));
    r.undo(1);
    assert_eq!(r.svg(), SQUARES);
}

#[test]
fn the_path_menu_works_on_outlines() {
    let mut r = squares("paths-outline");
    // A shape made a path; then it has segments to turn round.
    r.take(&[C]);
    assert_eq!(r.lit(), ["Object to Path", "Union", "Outline Stroke"]);
    r.path(PathOp::ToPath);
    assert_eq!((r.says(C, "d").as_deref(), r.steps(), r.lit()), (Some("M30 4 H40 V14 H30 Z"), vec!["Object to Path".to_owned()], vec!["Union", "Outline Stroke", "Simplify", "Reverse"]));
    r.path(PathOp::Reverse);
    assert_eq!((r.says(C, "d").as_deref(), r.steps().last().map(String::as_str)), (Some("M30 4 V14 H40 V4 Z"), Some("Reverse")));
    // Its stroke outlined: it keeps its fill, and the line round it is
    // a path of its own over it; both are selected.
    r.path(PathOp::Outline);
    let made = r.selected();
    assert_eq!((made.len(), made[0], r.says(C, "stroke").as_deref(), r.steps().last().map(String::as_str)), (2, C, Some("none"), Some("Outline Stroke")));
    assert_eq!((r.says(made[1], "fill").as_deref(), r.says(made[1], "d").map(|d| d.matches('M').count())), (Some("#000"), Some(2)));
    r.undo(3);
    assert_eq!(r.svg(), SQUARES);
}
