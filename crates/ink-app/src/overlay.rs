//! What's drawn over the canvas in screen px, with LUI2's own lines, so
//! it's the same size at every zoom (ARCHITECTURE §8): the selection's
//! box and its handles, a box round each selected thing, the marquee,
//! the edge of the group the Pointer has gone into, a rectangle's
//! corner dot, and with the Node tool a path's line, its anchors and
//! their handles (LS3's look for its pen's).

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::Ui;

use crate::handles::{self, HIT, SIZE};
use crate::nodes::DRAWN;
use crate::rounding::DOT;
use crate::theme::ACCENT;

/// What to draw over the canvas this frame, in window px.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    /// Each selected thing's box, its corners from the top left
    /// clockwise (turned, while a turn is being dragged).
    pub boxes: Vec<[Vec2; 4]>,
    /// The box round them all.
    pub joint: Option<[Vec2; 4]>,
    /// Whether the joint box has its handles (the Pointer is in hand).
    pub handles: bool,
    /// A marquee being dragged.
    pub marquee: Option<Rect>,
    /// The group gone into.
    pub entered: Option<[Vec2; 4]>,
    /// A rectangle's corner dot: where its middle is, and whether the
    /// pointer has it.
    pub dot: Option<(Vec2, bool)>,
    /// The lines of the shapes whose anchors show: each run, and
    /// whether it's closed.
    pub paths: Vec<(Vec<Vec2>, bool)>,
    /// Their anchors, and whether each is picked.
    pub anchors: Vec<(Vec2, bool)>,
    /// A picked anchor's handles: from the anchor, to the handle.
    pub levers: Vec<(Vec2, Vec2)>,
    /// The end of a path a press of the Pen would close it on.
    pub ring: Option<Vec2>,
    /// The line of the gradient the Gradient tool has in hand.
    pub axis: Option<crate::grading::Axis>,
    /// The caret of the text being typed into: its top, and its bottom.
    pub caret: Option<(Vec2, Vec2)>,
}

/// A closed line round `quad`, gold over a dark edge: seen on the tan
/// ground and over any drawing.
fn outline(ui: &mut Ui, quad: &[Vec2; 4], gold: f64, dark: f64, strength: f64) {
    ui.draw.polyline(quad, dark, Color::rgba(0.0, 0.0, 0.0, 0.55 * strength), true);
    ui.draw.polyline(quad, gold, ACCENT.with_alpha(strength), true);
}

/// A corner on a whole pixel, so upright lines stay crisp.
fn crisp(quad: &[Vec2; 4]) -> [Vec2; 4] {
    quad.map(|p| Vec2::new(p.x.round(), p.y.round()))
}

pub fn draw(ui: &mut Ui, area: Rect, scene: &Scene) {
    let s = ui.m.scale;
    let w = s.round().max(1.0);
    ui.draw.push_clip(area);
    if let Some(entered) = &scene.entered {
        outline(ui, &crisp(entered), w, w * 3.0, 0.6);
    }
    // Each thing's own box, when there's more than the one.
    if scene.boxes.len() > 1 || scene.joint.is_none() {
        for quad in &scene.boxes {
            outline(ui, &crisp(quad), w, w * 3.0, 0.85);
        }
    }
    if let Some(joint) = &scene.joint {
        let quad = crisp(joint);
        outline(ui, &quad, w * 2.0, w * 4.0, 1.0);
        if scene.handles {
            let half = (SIZE / 2.0 * s).round();
            let dark = Color::rgba(0.0, 0.0, 0.0, 0.75);
            let mid = handles::sides(quad);
            // A side too short for a handle between its corners has
            // none (as a press finds none there).
            let room = |i: usize| (quad[i] - quad[(i + 1) % 4]).length() >= HIT * s * 4.0;
            let places = quad.into_iter().chain((0..4).filter(|&i| room(i)).map(|i| mid[i]));
            for at in places {
                let r = Rect::from_xywh((at.x - half).round(), (at.y - half).round(), half * 2.0, half * 2.0);
                ui.draw.rect(r.expand(w * 2.0), dark);
                ui.draw.rect(r, Color::WHITE);
            }
        }
    }
    // A path's line, gold over a dark edge; a white line out to each
    // handle's dot; a square at each anchor, gold where it's picked.
    let dark = Color::rgba(0.0, 0.0, 0.0, 0.75);
    for (line, closed) in scene.paths.iter().filter(|(line, _)| line.len() >= 2) {
        ui.draw.polyline(line, w * 3.0, dark, *closed);
        ui.draw.polyline(line, w * 1.5, ACCENT, *closed);
    }
    // A gradient's line: a round end where it starts and a square one
    // where it stops, and each stop's colour along it.
    if let Some(axis) = &scene.axis {
        let radius = (DRAWN * s).round();
        ui.draw.line(axis.from, axis.to, w * 4.0, dark);
        ui.draw.line(axis.from, axis.to, w * 2.0, Color::WHITE);
        for (offset, color) in &axis.stops {
            let at = axis.from + (axis.to - axis.from) * offset.clamp(0.0, 1.0);
            ui.draw.circle(at, radius * 0.7 + w * 2.0, dark);
            ui.draw.circle(at, radius * 0.7 + w, Color::WHITE);
            ui.draw.circle(at, radius * 0.7, color.with_alpha(1.0));
        }
        ui.draw.circle(axis.from, radius + w * 2.0, dark);
        ui.draw.circle(axis.from, radius, Color::WHITE);
        let end = Rect::from_xywh((axis.to.x - radius).round(), (axis.to.y - radius).round(), radius * 2.0, radius * 2.0);
        ui.draw.rect(end.expand(w * 2.0), dark);
        ui.draw.rect(end, Color::WHITE);
    }
    // A caret: gold over a dark edge, as wide as it takes to see.
    if let Some((top, bottom)) = scene.caret {
        ui.draw.line(top, bottom, w * 5.0, Color::rgba(0.0, 0.0, 0.0, 0.7));
        ui.draw.line(top, bottom, w * 3.0, ACCENT);
    }
    if let Some(at) = scene.ring {
        ui.draw.circle(at, crate::penning::CLOSE * s / 2.0, ACCENT.with_alpha(0.35));
    }
    let radius = (DRAWN * s).round();
    for (from, to) in &scene.levers {
        ui.draw.line(*from, *to, w * 3.0, dark);
        ui.draw.line(*from, *to, w * 1.5, Color::WHITE);
        ui.draw.circle(*to, radius + w, dark);
        ui.draw.circle(*to, radius, Color::WHITE);
    }
    for (at, picked) in &scene.anchors {
        let r = Rect::from_xywh((at.x - radius).round(), (at.y - radius).round(), radius * 2.0, radius * 2.0);
        ui.draw.rect(r.expand(w * 2.0), dark);
        ui.draw.rect(r, if *picked { ACCENT } else { Color::WHITE });
    }
    // Round, and gold in a white ring: a handle of the shape's own, not
    // one of its box's squares.
    if let Some((at, lit)) = scene.dot {
        let half = ((DOT / 2.0 + if lit { 2.0 } else { 0.0 }) * s).round();
        let disc = |ui: &mut Ui, r: f64, color: Color| ui.draw.rounded_rect(Rect::from_xywh(at.x.round() - r, at.y.round() - r, r * 2.0, r * 2.0), r, color);
        disc(ui, half + w * 2.0, Color::rgba(0.0, 0.0, 0.0, 0.75));
        disc(ui, half, Color::WHITE);
        disc(ui, half - w * 3.0, ACCENT);
    }
    if let Some(m) = scene.marquee {
        let r = Rect::new(Vec2::new(m.min.x.round(), m.min.y.round()), Vec2::new(m.max.x.round(), m.max.y.round()));
        ui.draw.rect(r, ACCENT.with_alpha(0.14));
        ui.draw.stroke_rect(r.expand(w), w * 3.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
        ui.draw.stroke_rect(r, w * 2.0, 0.0, ACCENT);
    }
    ui.draw.pop_clip();
}
