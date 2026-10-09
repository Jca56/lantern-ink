//! What things are painted with, as the window reads it off the drawing
//! and sets it (ARCHITECTURE §8): a fill and a stroke, each nothing, a
//! colour with its opacity, or a gradient by its name; the line a
//! stroke is drawn with; and how see-through the whole thing is. Read
//! from a node's style as it's drawn (what it says, and what it
//! inherits); set as properties, through `Command::SetStyle`, so each
//! is written where its node has it (D14).

use ink_doc::length::unit;
use ink_doc::style::{Paint as Said, Style, prop};
use ink_doc::{Document, Kind, NodeId};
use ink_geom::{Cap, Join};
use lntrn_math::Color;

use crate::colour::palettes::to_hex;
use crate::controls::written;

/// A fill or a stroke.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    None,
    /// A plain colour; its alpha is the paint's opacity (`fill-opacity`,
    /// `stroke-opacity`: SVG keeps it beside the colour, not in it).
    Color(Color),
    /// A gradient (or a pattern), by its `id`.
    Server(String),
}

/// Which of a shape's two paints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Fill,
    Stroke,
}

impl Which {
    pub const BOTH: [Which; 2] = [Which::Fill, Which::Stroke];

    /// Its name: in its row, and for the step that changes it.
    pub fn label(self) -> &'static str {
        match self {
            Which::Fill => "Fill",
            Which::Stroke => "Stroke",
        }
    }

    fn property(self) -> (&'static str, &'static str) {
        match self {
            Which::Fill => ("fill", "fill-opacity"),
            Which::Stroke => ("stroke", "stroke-opacity"),
        }
    }
}

/// The line a stroke is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    /// On and off lengths in turn; none for a solid line.
    pub dashes: Vec<f64>,
}

impl Default for Line {
    /// SVG's own: one wide, flat ends, pointed corners, solid.
    fn default() -> Line {
        Line { width: 1.0, cap: Cap::Butt, join: Join::Miter, dashes: Vec::new() }
    }
}

/// What a shape is painted with: what the paint section shows for the
/// selection, and what the next shape drawn gets.
#[derive(Clone, Debug, PartialEq)]
pub struct Paints {
    pub fill: Paint,
    pub stroke: Paint,
    pub line: Line,
    /// How much of the whole thing shows, 0 to 1.
    pub opacity: f64,
}

impl Default for Paints {
    /// Lantern's gold, and no line: what the first shape drawn gets.
    fn default() -> Paints {
        Paints { fill: Paint::Color(Color::hex(0xF3B700)), stroke: Paint::None, line: Line::default(), opacity: 1.0 }
    }
}

/// Something the paint section sets.
#[derive(Clone, Debug, PartialEq)]
pub enum Set {
    Paint(Which, Paint),
    /// Make this paint a gradient: the one it last was, or a new one.
    Gradient(Which),
    Width(f64),
    Cap(Cap),
    Join(Join),
    Dashes(Vec<f64>),
    Opacity(f64),
}

impl Set {
    /// What the step that does it is called.
    pub fn label(&self) -> &'static str {
        match self {
            Set::Paint(which, _) | Set::Gradient(which) => which.label(),
            Set::Width(_) => "Stroke Width",
            Set::Cap(_) => "Stroke Caps",
            Set::Join(_) => "Stroke Joins",
            Set::Dashes(_) => "Dashes",
            Set::Opacity(_) => "Opacity",
        }
    }

    /// The properties that say it, for the ones that are plain
    /// properties (a gradient has to be made first: the window's).
    pub fn properties(&self) -> Vec<(String, Option<String>)> {
        let one = |name: &str, value: String| vec![(name.to_owned(), Some(value))];
        match self {
            Set::Paint(which, paint) => set(*which, paint),
            Set::Gradient(_) => Vec::new(),
            Set::Width(w) => one("stroke-width", written(w.max(0.0), 3)),
            Set::Cap(cap) => one(
                "stroke-linecap",
                match cap {
                    Cap::Butt => "butt",
                    Cap::Round => "round",
                    Cap::Square => "square",
                }
                .to_owned(),
            ),
            Set::Join(join) => one(
                "stroke-linejoin",
                match join {
                    Join::Miter => "miter",
                    Join::Round => "round",
                    Join::Bevel => "bevel",
                }
                .to_owned(),
            ),
            // Said outright, solid too: a group above may say dashes.
            Set::Dashes(dashes) => one("stroke-dasharray", if dashes.is_empty() { "none".to_owned() } else { dashes.iter().map(|d| written(*d, 3)).collect::<Vec<_>>().join(" ") }),
            // Whole is how everything starts: nothing need say it.
            Set::Opacity(o) => vec![("opacity".to_owned(), (*o < 1.0 - 1e-9).then(|| written(o.clamp(0.0, 1.0), 3)))],
        }
    }
}

impl Paints {
    pub fn get(&self, which: Which) -> &Paint {
        match which {
            Which::Fill => &self.fill,
            Which::Stroke => &self.stroke,
        }
    }

    pub fn set(&mut self, which: Which, paint: Paint) {
        match which {
            Which::Fill => self.fill = paint,
            Which::Stroke => self.stroke = paint,
        }
    }

    /// Take `set` as what the next shape gets too.
    pub fn take(&mut self, set: &Set) {
        match set {
            Set::Paint(which, paint) => self.set(*which, paint.clone()),
            // A gradient is one drawing's: the next shape may be
            // another's.
            Set::Gradient(_) => {}
            Set::Width(w) => self.line.width = w.max(0.0),
            Set::Cap(cap) => self.line.cap = *cap,
            Set::Join(join) => self.line.join = *join,
            Set::Dashes(dashes) => self.line.dashes.clone_from(dashes),
            Set::Opacity(o) => self.opacity = o.clamp(0.0, 1.0),
        }
    }

    /// What a node drawn as `style` says is painted with.
    fn of(style: &Style) -> Paints {
        let read = |said: &Said, opacity: f64| match said {
            Said::None => Paint::None,
            Said::Color(c) => Paint::Color(c.with_alpha((c.a * opacity).clamp(0.0, 1.0))),
            Said::Server { id, .. } => Paint::Server(id.clone()),
        };
        let line = Line { width: style.line.width, cap: style.line.cap, join: style.line.join, dashes: style.line.dashes.clone() };
        Paints { fill: read(&style.fill, style.fill_opacity), stroke: read(&style.stroke, style.stroke_opacity), line, opacity: 1.0 }
    }
}

/// `id` as it's drawn: what it says for itself over what everything
/// it's in hands down.
pub fn style_of(doc: &Document, id: NodeId) -> Style {
    let above: Vec<_> = doc.ancestors(id).collect();
    let inherited = above.iter().rev().fold(Style::default(), |style, node| style.cascade(node));
    doc.get(id).map_or(inherited.clone(), |node| inherited.cascade(node))
}

/// What `id` is painted with. (Its own opacity: nothing hands that
/// down.)
pub fn read(doc: &Document, id: NodeId) -> Paints {
    Paints { opacity: opacity_of(doc, id), ..Paints::of(&style_of(doc, id)) }
}

/// How much of `id` shows: what it says for itself, whole if nothing.
pub fn opacity_of(doc: &Document, id: NodeId) -> f64 {
    doc.get(id).and_then(|node| prop(node, "opacity")).and_then(unit).map_or(1.0, |o| o.clamp(0.0, 1.0))
}

/// The things `tops` are that are drawn, back to front: what an
/// opacity set on the selection is set on (a group fades as one).
pub fn faded(doc: &Document, tops: &[NodeId]) -> Vec<NodeId> {
    tops.iter().copied().filter(|&id| crate::select::is_drawn(doc, id)).collect()
}

/// The shapes and texts that `tops` are or hold, back to front: what a
/// paint set on the selection is set on. (Set on a group it would be
/// outvoted by whatever in the group says its own.)
pub fn painted(doc: &Document, tops: &[NodeId]) -> Vec<NodeId> {
    let takes = |id: NodeId| doc.get(id).is_some_and(|n| n.kind.is_shape() || n.kind == Kind::Text) && !doc.ancestors(id).any(|n| n.kind == Kind::Text);
    let mut out = Vec::new();
    for &top in tops {
        if !crate::select::is_drawn(doc, top) {
            continue;
        }
        for id in doc.descendants(top) {
            if takes(id) && !out.contains(&id) {
                out.push(id);
            }
        }
    }
    out
}

/// The properties that make `which` of a shape `paint`: its colour or
/// its gradient, and its opacity beside it (taken off when it's whole).
pub fn set(which: Which, paint: &Paint) -> Vec<(String, Option<String>)> {
    let (name, opacity) = which.property();
    match paint {
        Paint::None => vec![(name.to_owned(), Some("none".to_owned()))],
        Paint::Color(c) => vec![(name.to_owned(), Some(to_hex(*c))), (opacity.to_owned(), (c.a < 1.0 - 1e-9).then(|| written(c.a, 3)))],
        Paint::Server(id) => vec![(name.to_owned(), Some(format!("url(#{id})")))],
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::{Command, DocId};

    use super::*;

    /// N2 defs (N3 a gradient), N4 a red rect, N5 a group that says
    /// green and half see-through fills, holding N6 a circle (which
    /// says nothing) and N7 a path with a gradient and a blue line, N8
    /// a text holding N9 a span.
    fn doc() -> Document {
        Document::parse(DocId(1), r##"<svg><defs><linearGradient id="sky"/></defs><rect fill="#ff0000"/><g fill="#00ff00" fill-opacity="0.5"><circle/><path fill="url(#sky)" stroke="#0000ff" style="stroke-opacity: 0.25"/></g><text>a<tspan>b</tspan></text></svg>"##).unwrap()
    }

    const N: fn(u64) -> NodeId = NodeId;

    #[test]
    fn paints_are_read_as_drawn_and_set_as_properties() {
        let d = doc();
        assert_eq!((read(&d, N(4)).fill, read(&d, N(4)).stroke), (Paint::Color(Color::hex(0xff0000)), Paint::None));
        // What it inherits is what it's drawn with.
        assert_eq!(read(&d, N(6)).fill, Paint::Color(Color::hex(0x00ff00).with_alpha(0.5)));
        assert_eq!((read(&d, N(7)).fill, read(&d, N(7)).stroke), (Paint::Server("sky".into()), Paint::Color(Color::hex(0x0000ff).with_alpha(0.25))));
        // With nothing said anywhere: SVG's black fill, and no line.
        assert_eq!(read(&d, N(8)), Paints { fill: Paint::Color(Color::BLACK), stroke: Paint::None, line: Line::default(), opacity: 1.0 });
        // Set: the colour, and its opacity beside it only when it has one.
        let pairs = |which, paint: Paint| set(which, &paint).into_iter().map(|(name, value)| format!("{name}={}", value.unwrap_or_else(|| "-".into()))).collect::<Vec<_>>().join(" ");
        assert_eq!(pairs(Which::Fill, Paint::Color(Color::hex(0xf3b700))), "fill=#f3b700 fill-opacity=-");
        assert_eq!(pairs(Which::Stroke, Paint::Color(Color::hex(0x102030).with_alpha(0.4))), "stroke=#102030 stroke-opacity=0.4");
        assert_eq!((pairs(Which::Fill, Paint::None), pairs(Which::Stroke, Paint::Server("sky".into()))), ("fill=none".to_owned(), "stroke=url(#sky)".to_owned()));
        // And read back, it's what was set.
        let mut d = doc();
        let half = Paint::Color(Color::hex(0x336699).with_alpha(0.5));
        d.apply(&Command::SetStyle { nodes: vec![N(7)], set: set(Which::Stroke, &half) }).unwrap();
        assert_eq!(read(&d, N(7)).stroke, half);
    }

    #[test]
    fn a_line_and_an_opacity_are_read_and_said_as_properties() {
        let mut d = Document::parse(DocId(1), r##"<svg><g opacity="0.5" stroke-linecap="round"><path stroke="#000" stroke-width="2.5" stroke-dasharray="4 2" stroke-linejoin="bevel"/></g></svg>"##).unwrap();
        let shown = read(&d, N(3));
        assert_eq!(shown.line, Line { width: 2.5, cap: Cap::Round, join: Join::Bevel, dashes: vec![4.0, 2.0] });
        // An opacity is its own node's: the group's isn't the path's.
        assert_eq!((shown.opacity, opacity_of(&d, N(2)), faded(&d, &[N(2), N(3)])), (1.0, 0.5, vec![N(2), N(3)]));
        let said = |set: Set| set.properties().into_iter().map(|(name, value)| format!("{name}={}", value.unwrap_or_else(|| "-".into()))).collect::<Vec<_>>().join(" ");
        assert_eq!((said(Set::Width(0.75)), said(Set::Cap(Cap::Square)), said(Set::Join(Join::Round))), ("stroke-width=0.75".to_owned(), "stroke-linecap=square".to_owned(), "stroke-linejoin=round".to_owned()));
        assert_eq!((said(Set::Dashes(vec![1.0, 0.5])), said(Set::Dashes(Vec::new()))), ("stroke-dasharray=1 0.5".to_owned(), "stroke-dasharray=none".to_owned()));
        assert_eq!((said(Set::Opacity(0.25)), said(Set::Opacity(1.0))), ("opacity=0.25".to_owned(), "opacity=-".to_owned()));
        assert_eq!((Set::Width(1.0).label(), Set::Opacity(1.0).label(), Set::Gradient(Which::Stroke).label()), ("Stroke Width", "Opacity", "Stroke"));
        // Set and read back; and taken as what the next shape gets.
        d.apply(&Command::SetStyle { nodes: vec![N(3)], set: Set::Dashes(Vec::new()).properties() }).unwrap();
        assert!(read(&d, N(3)).line.dashes.is_empty());
        let mut next = Paints::default();
        for set in [Set::Width(3.0), Set::Cap(Cap::Round), Set::Opacity(0.4), Set::Paint(Which::Stroke, Paint::Color(Color::BLACK)), Set::Gradient(Which::Fill)] {
            next.take(&set);
        }
        assert_eq!((next.line.width, next.line.cap, next.opacity, &next.stroke, &next.fill), (3.0, Cap::Round, 0.4, &Paint::Color(Color::BLACK), &Paints::default().fill));
    }

    #[test]
    fn a_paint_goes_on_the_shapes_and_texts_of_what_is_selected() {
        let d = doc();
        // A group stands for what it holds; a text is one thing, spans
        // and all; a definition takes no paint.
        assert_eq!(painted(&d, &[N(4), N(5), N(8)]), [N(4), N(6), N(7), N(8)]);
        assert_eq!(painted(&d, &[N(2), N(3)]), Vec::<NodeId>::new());
        assert_eq!(Paints::default().fill, Paint::Color(Color::hex(0xF3B700)));
    }
}
