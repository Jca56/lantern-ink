//! Shapes as the shape tools draw them (LS3's `shapes.rs`): which shape
//! a tool draws, what a drag across the canvas makes of it, and the
//! markup of the element that is. No window here (`shaping.rs` has the
//! gesture).
//!
//! Alva's choices, as in LS3: Shift draws a square, a circle, a regular
//! polygon, or a line at a multiple of 45°; Alt draws out from the
//! middle. A new shape is painted as the last one was. And as LS3's
//! land on whole pixels, Ink's land on whole units of the drawing (an
//! icon's own pixels), so their numbers are plain ones; Ctrl draws
//! free of that. (Half units, other shapes' edges and a switch for it
//! all are slice f's snapping.)

use ink_doc::Precision;
use ink_geom::{Cap, Join};
use lntrn_math::{Color, Rect, Vec2};

use crate::handles::Keys;
use crate::paint::{self, Paint, Paints, Set, Which};
use crate::tools::Tool;

/// What a shape tool draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rect,
    Ellipse,
    Line,
    Polygon,
}

impl Kind {
    /// What the step that draws one is called.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Rect => "Rectangle",
            Kind::Ellipse => "Ellipse",
            Kind::Line => "Line",
            Kind::Polygon => "Polygon",
        }
    }
}

/// The kind of shape `tool` draws.
pub fn kind_of(tool: Tool) -> Option<Kind> {
    match tool {
        Tool::Rect => Some(Kind::Rect),
        Tool::Ellipse => Some(Kind::Ellipse),
        Tool::Line => Some(Kind::Line),
        Tool::Polygon => Some(Kind::Polygon),
        _ => None,
    }
}

/// The shape tools' own settings (the Box holds them): what the next
/// shape is made with, beside its paint.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// A rectangle's corners' rounding, in the drawing's units.
    pub radius: f64,
    /// How many sides a polygon has (a star: how many points).
    pub sides: f64,
    pub star: bool,
    /// How far in a star's inner corners are, of the way to its middle:
    /// 0 is no star at all, 1 all the way.
    pub depth: f64,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { radius: 0.0, sides: 5.0, star: false, depth: 0.5 }
    }
}

/// What a new shape's corners land on, for a page `page` units along
/// its longer side: whole units, or on a page of only a few units, as
/// fine as leaves it some sixteen steps across.
pub fn grid_for(page: f64) -> f64 {
    10f64.powf((page.max(1e-9) / 16.0).log10().floor()).min(1.0)
}

/// `p` on the nearest crossing of a grid `grid` units apart.
pub fn on_grid(p: Vec2, grid: f64) -> Vec2 {
    if grid > 0.0 { Vec2::new((p.x / grid).round() * grid, (p.y / grid).round() * grid) } else { p }
}

/// The fewest and the most sides a polygon is drawn with.
pub const SIDES: (f64, f64) = (3.0, 64.0);

/// The corners of a polygon of `sides` sides on a circle of radius 1
/// about the origin, the first straight up, then clockwise (the page's
/// y runs down). A star's inner corners, `depth` of the way in, stand
/// between them.
pub fn ring(sides: usize, star: Option<f64>) -> Vec<Vec2> {
    let n = sides.max(3);
    let turn = std::f64::consts::TAU / n as f64;
    let at = |angle: f64, radius: f64| Vec2::new(radius * angle.sin(), -radius * angle.cos());
    let mut out = Vec::with_capacity(n * 2);
    for k in 0..n {
        out.push(at(turn * k as f64, 1.0));
        if let Some(depth) = star {
            out.push(at(turn * (k as f64 + 0.5), (1.0 - depth).clamp(0.02, 1.0)));
        }
    }
    out
}

/// The two ends of what a drag from `from` to `to` draws: a box's
/// opposite corners (or a line's ends), once Shift has squared it (a
/// line: turned it to a multiple of 45°) and Alt has drawn it out from
/// the middle. None for a drag shorter than `least` both ways.
pub fn dragged(kind: Kind, from: Vec2, to: Vec2, keys: Keys, least: f64) -> Option<(Vec2, Vec2)> {
    let mut by = to - from;
    if by.x.abs() < least && by.y.abs() < least {
        return None;
    }
    if keys.shift {
        by = match kind {
            Kind::Line => {
                let step = std::f64::consts::FRAC_PI_4;
                let angle = (by.y.atan2(by.x) / step).round() * step;
                Vec2::new(angle.cos(), angle.sin()) * by.length()
            }
            _ => {
                let side = by.x.abs().max(by.y.abs());
                Vec2::new(side.copysign(by.x), side.copysign(by.y))
            }
        };
    }
    Some(if keys.alt { (from - by, from + by) } else { (from, from + by) })
}

/// How a new shape is painted: `paints`, as attributes. A line is all
/// stroke: its own, or with none, the fill's colour.
fn painted(kind: Kind, paints: &Paints) -> String {
    let mut pairs: Vec<(String, Option<String>)> = Vec::new();
    let plain = |paint: &Paint| if let Paint::Server(_) = paint { Paint::Color(Color::hex(0xF3B700)) } else { paint.clone() };
    let (fill, stroke) = (plain(&paints.fill), plain(&paints.stroke));
    let stroke = match kind {
        Kind::Line if stroke == Paint::None => match fill {
            Paint::None => Paint::Color(Color::BLACK),
            ref colour => colour.clone(),
        },
        _ => stroke,
    };
    if kind != Kind::Line {
        pairs.extend(paint::set(Which::Fill, &fill));
    }
    if stroke != Paint::None {
        pairs.extend(paint::set(Which::Stroke, &stroke));
        let line = &paints.line;
        // What SVG starts from needn't be said.
        if (line.width - 1.0).abs() > 1e-9 {
            pairs.extend(Set::Width(line.width).properties());
        }
        if line.cap != Cap::Butt {
            pairs.extend(Set::Cap(line.cap).properties());
        }
        if line.join != Join::Miter {
            pairs.extend(Set::Join(line.join).properties());
        }
        if !line.dashes.is_empty() {
            pairs.extend(Set::Dashes(line.dashes.clone()).properties());
        }
    }
    pairs.extend(Set::Opacity(paints.opacity).properties());
    pairs.into_iter().filter_map(|(name, value)| Some(format!(" {name}=\"{}\"", value?))).collect()
}

/// The element a shape of `kind` between `a` and `b` (see [`dragged`];
/// in the coordinates it's put in) is, as markup. `regular`: a polygon
/// keeps its own proportions inside the box (Shift), where otherwise
/// it's stretched to fill it.
pub fn markup(kind: Kind, a: Vec2, b: Vec2, regular: bool, s: &Settings, paints: &Paints, p: &Precision) -> String {
    let n = |v: f64| p.number(v);
    let paint = painted(kind, paints);
    let (lo, hi) = (a.min(b), a.max(b));
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    match kind {
        Kind::Rect => {
            let radius = s.radius.clamp(0.0, w.min(h) / 2.0);
            let round = if radius > 0.0 { format!(" rx=\"{}\"", n(radius)) } else { String::new() };
            format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{round}{paint}/>", n(lo.x), n(lo.y), n(w), n(h))
        }
        Kind::Ellipse => {
            let (c, rx, ry) = ((lo + hi) * 0.5, w / 2.0, h / 2.0);
            // As round as its numbers can say: a circle.
            if n(rx) == n(ry) { format!("<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{paint}/>", n(c.x), n(c.y), n(rx)) } else { format!("<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{paint}/>", n(c.x), n(c.y), n(rx), n(ry)) }
        }
        Kind::Line => format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{paint}/>", n(a.x), n(a.y), n(b.x), n(b.y)),
        Kind::Polygon => {
            let corners = ring(s.sides.round().clamp(SIDES.0, SIDES.1) as usize, s.star.then_some(s.depth));
            // Its own box on the unit circle, put onto the box dragged.
            let own = corners.iter().fold(Rect::new(corners[0], corners[0]), |r, &c| Rect::new(r.min.min(c), r.max.max(c)));
            let (mut sx, mut sy) = (w / own.width().max(1e-12), h / own.height().max(1e-12));
            if regular {
                (sx, sy) = (sx.min(sy), sx.min(sy));
            }
            // Kept regular, it sits in the middle of the box.
            let (from, middle) = (own.center(), (lo + hi) * 0.5);
            let points: Vec<Vec2> = corners.iter().map(|&c| Vec2::new(middle.x + (c.x - from.x) * sx, middle.y + (c.y - from.y) * sy)).collect();
            format!("<polygon points=\"{}\"{paint}/>", p.points(&points))
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::{DocId, Document};

    use super::*;

    const PLAIN: Keys = Keys { shift: false, alt: false };
    const SHIFT: Keys = Keys { shift: true, alt: false };
    const ALT: Keys = Keys { shift: false, alt: true };

    fn precision() -> Precision {
        Precision::of(&Document::parse(DocId(1), "<svg/>").unwrap())
    }

    fn made(kind: Kind, from: (f64, f64), to: (f64, f64), keys: Keys, s: &Settings, paints: &Paints) -> String {
        let (a, b) = dragged(kind, Vec2::new(from.0, from.1), Vec2::new(to.0, to.1), keys, 0.1).expect("a drag long enough");
        markup(kind, a, b, keys.shift, s, paints, &precision())
    }

    #[test]
    fn a_drag_draws_a_box_and_shift_and_alt_shape_it() {
        let (s, gold) = (Settings::default(), Paints::default());
        assert_eq!(made(Kind::Rect, (10.0, 10.0), (20.0, 16.0), PLAIN, &s, &gold), r##"<rect x="10" y="10" width="10" height="6" fill="#f3b700"/>"##);
        // Dragged up and left, it's the same box.
        assert_eq!(made(Kind::Rect, (20.0, 16.0), (10.0, 10.0), PLAIN, &s, &gold), r##"<rect x="10" y="10" width="10" height="6" fill="#f3b700"/>"##);
        // Shift: as wide as it's tall, by the longer way; Alt: out from
        // where it began.
        assert_eq!(made(Kind::Rect, (10.0, 10.0), (20.0, 6.0), SHIFT, &s, &gold), r##"<rect x="10" y="0" width="10" height="10" fill="#f3b700"/>"##);
        assert_eq!(made(Kind::Rect, (10.0, 10.0), (14.0, 12.0), ALT, &s, &gold), r##"<rect x="6" y="8" width="8" height="4" fill="#f3b700"/>"##);
        // Rounded as the Box says, no further than half its shorter side.
        let round = Settings { radius: 5.0, ..Settings::default() };
        assert_eq!(made(Kind::Rect, (0.0, 0.0), (20.0, 6.0), PLAIN, &round, &gold), r##"<rect x="0" y="0" width="20" height="6" rx="3" fill="#f3b700"/>"##);
        assert_eq!(made(Kind::Ellipse, (0.0, 0.0), (20.0, 6.0), PLAIN, &s, &gold), r##"<ellipse cx="10" cy="3" rx="10" ry="3" fill="#f3b700"/>"##);
        assert_eq!(made(Kind::Ellipse, (0.0, 0.0), (20.0, 6.0), SHIFT, &s, &gold), r##"<circle cx="10" cy="10" r="10" fill="#f3b700"/>"##);
        // A drag too short to be one draws nothing.
        assert_eq!(dragged(Kind::Rect, Vec2::ZERO, Vec2::new(0.05, 0.05), PLAIN, 0.1), None);
    }

    #[test]
    fn a_line_is_all_stroke_and_shift_turns_it_to_45_degrees() {
        let (s, gold) = (Settings::default(), Paints::default());
        // No stroke of its own: the fill's colour. It keeps the way it
        // was drawn.
        assert_eq!(made(Kind::Line, (10.0, 10.0), (4.0, 12.0), PLAIN, &s, &gold), r##"<line x1="10" y1="10" x2="4" y2="12" stroke="#f3b700"/>"##);
        assert_eq!(made(Kind::Line, (0.0, 0.0), (10.0, 9.0), SHIFT, &s, &gold), r##"<line x1="0" y1="0" x2="9.513" y2="9.513" stroke="#f3b700"/>"##);
        assert_eq!(made(Kind::Line, (0.0, 0.0), (10.0, 1.0), SHIFT, &s, &gold), r##"<line x1="0" y1="0" x2="10.05" y2="0" stroke="#f3b700"/>"##);
        // Its own stroke, and the line it's drawn with, where that
        // isn't what SVG starts from.
        let inked = Paints { fill: Paint::None, stroke: Paint::Color(Color::hex(0x102030).with_alpha(0.5)), line: paint::Line { width: 2.0, cap: Cap::Round, join: Join::Miter, dashes: vec![4.0, 2.0] }, opacity: 0.8 };
        assert_eq!(made(Kind::Line, (0.0, 0.0), (8.0, 0.0), PLAIN, &s, &inked), r##"<line x1="0" y1="0" x2="8" y2="0" stroke="#102030" stroke-opacity="0.5" stroke-width="2" stroke-linecap="round" stroke-dasharray="4 2" opacity="0.8"/>"##);
        assert_eq!(made(Kind::Rect, (0.0, 0.0), (8.0, 4.0), PLAIN, &s, &inked), r##"<rect x="0" y="0" width="8" height="4" fill="none" stroke="#102030" stroke-opacity="0.5" stroke-width="2" stroke-linecap="round" stroke-dasharray="4 2" opacity="0.8"/>"##);
    }

    #[test]
    fn a_polygon_fills_its_box_and_shift_keeps_it_regular() {
        let gold = Paints::default();
        // A square, standing on a corner: stretched to the box dragged.
        let four = Settings { sides: 4.0, ..Settings::default() };
        assert_eq!(made(Kind::Polygon, (0.0, 0.0), (20.0, 10.0), PLAIN, &four, &gold), r##"<polygon points="10,0 20,5 10,10 0,5" fill="#f3b700"/>"##);
        // A triangle, kept regular in a square box: as wide as the box,
        // and less tall, in the middle of it.
        let three = Settings { sides: 3.0, ..Settings::default() };
        assert_eq!(made(Kind::Polygon, (0.0, 0.0), (20.0, 20.0), SHIFT, &three, &gold), r##"<polygon points="10,1.34 20,18.66 0,18.66" fill="#f3b700"/>"##);
        // A star: as many points again between, half way in.
        let star = Settings { sides: 4.0, star: true, depth: 0.5, ..Settings::default() };
        assert_eq!(made(Kind::Polygon, (0.0, 0.0), (20.0, 20.0), PLAIN, &star, &gold), r##"<polygon points="10,0 13.536,6.464 20,10 13.536,13.536 10,20 6.464,13.536 0,10 6.464,6.464" fill="#f3b700"/>"##);
        assert_eq!((ring(5, None).len(), ring(5, Some(0.4)).len(), ring(1, None).len()), (5, 10, 3));
        // Whole units on an icon's page and on a big one; finer only
        // where a unit is most of the page.
        assert_eq!((grid_for(24.0), grid_for(512.0), grid_for(16.0), grid_for(8.0), grid_for(1.0)), (1.0, 1.0, 1.0, 0.1, 0.01));
        assert_eq!((on_grid(Vec2::new(3.4, 29.5), 1.0), on_grid(Vec2::new(0.26, -0.24), 0.1).x), (Vec2::new(3.0, 30.0), 0.30000000000000004));
        assert_eq!((kind_of(Tool::Polygon), kind_of(Tool::Pointer), Kind::Ellipse.label()), (Some(Kind::Polygon), None, "Ellipse"));
    }
}
