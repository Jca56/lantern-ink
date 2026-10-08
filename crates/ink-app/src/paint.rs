//! What things are painted with, as the window reads it off the drawing
//! and sets it (ARCHITECTURE §8): a fill and a stroke, each nothing, a
//! colour with its opacity, or a gradient by its name. Read from a
//! node's style as it's drawn (what it says, and what it inherits);
//! set as properties, through `Command::SetStyle`, so each is written
//! where its node has it (D14).

use ink_doc::style::{Paint as Said, Style};
use ink_doc::{Document, Kind, NodeId};
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

/// What a shape is painted with: what the paint section shows for the
/// selection, and what the next shape drawn gets.
#[derive(Clone, Debug, PartialEq)]
pub struct Paints {
    pub fill: Paint,
    pub stroke: Paint,
}

impl Default for Paints {
    /// Lantern's gold, and no line: what the first shape drawn gets.
    fn default() -> Paints {
        Paints { fill: Paint::Color(Color::hex(0xF3B700)), stroke: Paint::None }
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

    /// What a node drawn as `style` says is painted with.
    fn of(style: &Style) -> Paints {
        let read = |said: &Said, opacity: f64| match said {
            Said::None => Paint::None,
            Said::Color(c) => Paint::Color(c.with_alpha((c.a * opacity).clamp(0.0, 1.0))),
            Said::Server { id, .. } => Paint::Server(id.clone()),
        };
        Paints { fill: read(&style.fill, style.fill_opacity), stroke: read(&style.stroke, style.stroke_opacity) }
    }
}

/// `id` as it's drawn: what it says for itself over what everything
/// it's in hands down.
pub fn style_of(doc: &Document, id: NodeId) -> Style {
    let above: Vec<_> = doc.ancestors(id).collect();
    let inherited = above.iter().rev().fold(Style::default(), |style, node| style.cascade(node));
    doc.get(id).map_or(inherited.clone(), |node| inherited.cascade(node))
}

/// What `id` is painted with.
pub fn read(doc: &Document, id: NodeId) -> Paints {
    Paints::of(&style_of(doc, id))
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
        assert_eq!(read(&d, N(4)), Paints { fill: Paint::Color(Color::hex(0xff0000)), stroke: Paint::None });
        // What it inherits is what it's drawn with.
        assert_eq!(read(&d, N(6)).fill, Paint::Color(Color::hex(0x00ff00).with_alpha(0.5)));
        assert_eq!(read(&d, N(7)), Paints { fill: Paint::Server("sky".into()), stroke: Paint::Color(Color::hex(0x0000ff).with_alpha(0.25)) });
        // With nothing said anywhere: SVG's black fill, and no line.
        assert_eq!(read(&d, N(8)), Paints { fill: Paint::Color(Color::BLACK), stroke: Paint::None });
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
    fn a_paint_goes_on_the_shapes_and_texts_of_what_is_selected() {
        let d = doc();
        // A group stands for what it holds; a text is one thing, spans
        // and all; a definition takes no paint.
        assert_eq!(painted(&d, &[N(4), N(5), N(8)]), [N(4), N(6), N(7), N(8)]);
        assert_eq!(painted(&d, &[N(2), N(3)]), Vec::<NodeId>::new());
        assert_eq!(Paints::default().fill, Paint::Color(Color::hex(0xF3B700)));
    }
}
