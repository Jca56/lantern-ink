//! The paint section: what's selected shows its fill and its stroke,
//! and a paint picked goes on it as one step, from the palette, from
//! the kind buttons, and from the picker (dragged, or typed as hex).

use ink_core::NodeId;
use lntrn_math::Color;

use super::*;
use crate::paint::{Paint, Which};

/// A blue square with a black line, and a group of two plain shapes.
const PAINTED: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"a\" x=\"4\" y=\"4\" width=\"12\" height=\"12\" fill=\"#0088ff\" stroke=\"#000000\"/>\n  <g id=\"g\">\n    <circle id=\"b\" cx=\"30\" cy=\"10\" r=\"4\"/>\n    <rect id=\"c\" x=\"24\" y=\"20\" width=\"16\" height=\"4\"/>\n  </g>\n</svg>\n";
const A: NodeId = NodeId(2);
const G: NodeId = NodeId(3);
const B: NodeId = NodeId(4);
const C: NodeId = NodeId(5);

fn painted(name: &str) -> Running {
    let path = scratch(name).join("painted.svg");
    std::fs::write(&path, PAINTED).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r
}

impl Running {
    /// Where the paint section drew what it calls `name`.
    fn paint_at(&self, name: &str) -> Vec2 {
        self.ink.paint_panel.laid.iter().find(|(n, _)| n == name).map(|(_, r)| r.center()).unwrap_or_else(|| panic!("the paint section has no {name}"))
    }

    fn select(&mut self, nodes: &[NodeId]) {
        let sel = &mut self.ink.tabs.active_mut().unwrap().selection;
        (sel.active, sel.nodes) = (nodes.last().copied(), nodes.to_vec());
        self.frames(2);
    }

    fn said(&self, node: NodeId, name: &str) -> Option<String> {
        self.ink.core.doc(self.doc()).unwrap().node(node).unwrap().attr(name).map(str::to_owned)
    }
}

#[test]
fn the_section_sits_over_the_tree_and_shows_what_is_selected() {
    let mut r = painted("paint-shows");
    let panel = r.ink.layout.panel;
    let (fill, swatch) = (r.paint_at("Fill"), r.paint_at("swatch 0"));
    assert!(panel.contains(fill) && panel.contains(swatch) && fill.y < swatch.y);
    // The object tree starts under it.
    let first_row = r.ink.tree.laid.first().expect("a row").1;
    assert!(first_row.min.y > swatch.y, "{first_row:?}");
    // Nothing selected: what the next shape will be painted with.
    assert_eq!(r.ink.paints.fill, Paint::Color(Color::hex(0xF3B700)));
    // The built-in palette's first colour, pressed with nothing
    // selected: no step, and the next shape's fill.
    r.click(swatch);
    assert!(r.steps().is_empty());
    assert_eq!(r.ink.paints.fill, Paint::Color(Color::hex(0xff8080)));
}

#[test]
fn a_palette_swatch_paints_the_fill_and_with_shift_the_stroke() {
    let mut r = painted("paint-swatch");
    r.select(&[A]);
    r.click(r.paint_at("swatch 0"));
    assert_eq!((r.said(A, "fill"), r.said(A, "stroke"), r.steps()), (Some("#ff8080".into()), Some("#000000".into()), vec!["Fill".to_owned()]));
    r.click_with(r.paint_at("swatch 8"), Modifiers::SHIFT);
    assert_eq!((r.said(A, "fill"), r.said(A, "stroke"), r.steps().last().cloned()), (Some("#ff8080".into()), Some("#ff0000".into()), Some("Stroke".to_owned())));
    // No fill at all; and a colour again is the colour it was.
    r.click(r.paint_at("Fill none"));
    assert_eq!(r.said(A, "fill"), Some("none".into()));
    r.click(r.paint_at("Fill colour"));
    assert_eq!((r.said(A, "fill"), r.steps().len()), (Some("#ff8080".into()), 4));
    r.click(r.paint_at("Stroke none"));
    assert_eq!(r.said(A, "stroke"), Some("none".into()));
    // Undone, each is one step back.
    for _ in 0..5 {
        r.key(Key::Char('z'), Modifiers::CTRL);
    }
    assert_eq!(r.svg(), PAINTED);
    // A group: its shapes take the paint, each for itself.
    r.select(&[G]);
    r.click(r.paint_at("swatch 9"));
    assert_eq!((r.said(B, "fill"), r.said(C, "fill"), r.said(G, "fill"), r.steps().len()), (Some("#ff8000".into()), Some("#ff8000".into()), None, 1));
}

#[test]
fn the_picker_paints_as_it_is_dragged_and_lands_once() {
    let mut r = painted("paint-picker");
    let doc = r.doc();
    r.select(&[A]);
    r.click(r.paint_at("Fill"));
    let picker = r.ink.picker.rect().expect("the picker, open for the fill");
    // Its square: saturation across, value up. From its middle to its
    // top right corner, which is the hue at its fullest.
    let square = Rect::from_xywh(picker.min.x + 14.0, picker.min.y + 14.0 + 34.0, 300.0, 300.0);
    let (from, to) = (square.center(), Vec2::new(square.max.x - 1.0, square.min.y + 1.0));
    r.h.advance(1.0);
    r.h.move_to(from);
    r.frames(1);
    r.h.press();
    r.frames(2);
    // Going: the drawing shows it, and is itself untouched.
    assert!(r.ink.core.gesturing(doc));
    assert_eq!((r.said(A, "fill"), r.steps().len()), (Some("#0088ff".into()), 0));
    assert_ne!(r.ink.core.shown(doc).unwrap().0.node(A).unwrap().attr("fill"), Some("#0088ff"));
    r.h.move_to(to);
    r.frames(2);
    r.h.release();
    r.frames(2);
    assert!(!r.ink.core.gesturing(doc));
    let fill = r.said(A, "fill").unwrap();
    assert_eq!(r.steps(), ["Fill"]);
    // Nearly the pure hue of the blue it was (0088ff: 208°).
    let c = Color::parse_hex(&fill).unwrap();
    assert!(c.b > 0.98 && c.r < 0.02 && (c.g - 0.53).abs() < 0.03, "{fill}");
    // Its alpha bar: half way along is half see-through, said beside
    // the colour.
    let alpha = Rect::from_xywh(picker.min.x + 14.0, square.max.y + 12.0, picker.width() - 28.0, 28.0);
    r.click(alpha.center());
    let opacity: f64 = r.said(A, "fill-opacity").expect("an opacity").parse().unwrap();
    assert!((opacity - 0.5).abs() < 0.01 && r.said(A, "fill") == Some(fill.clone()), "{opacity}");
    // Escape puts it away; nothing more is done.
    let steps = r.steps().len();
    r.key(Key::Escape, Modifiers::NONE);
    assert!(r.ink.picker.rect().is_none());
    assert_eq!((r.steps().len(), r.selected()), (steps, vec![A]));
    // What was picked is what the next shape gets.
    assert!(matches!(r.ink.paints.get(Which::Fill), Paint::Color(c) if (c.a - 0.5).abs() < 0.01));
}
