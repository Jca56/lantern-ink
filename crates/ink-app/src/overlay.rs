//! What's drawn over the canvas in screen px, with LUI2's own lines, so
//! it's the same size at every zoom (ARCHITECTURE §8): the selection's
//! box and its handles, a box round each selected thing, the marquee,
//! and the edge of the group the Pointer has gone into.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::Ui;

use crate::handles::{self, HIT, SIZE};
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
    if let Some(m) = scene.marquee {
        let r = Rect::new(Vec2::new(m.min.x.round(), m.min.y.round()), Vec2::new(m.max.x.round(), m.max.y.round()));
        ui.draw.rect(r, ACCENT.with_alpha(0.14));
        ui.draw.stroke_rect(r.expand(w), w * 3.0, 0.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
        ui.draw.stroke_rect(r, w * 2.0, 0.0, ACCENT);
    }
    ui.draw.pop_clip();
}
