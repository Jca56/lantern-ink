//! A shape's gradient, as the Gradient tool has hold of it
//! (ARCHITECTURE §8): its line on the canvas, its stops, and the
//! Commands that change them. No window here (`grading.rs` has the
//! tool's frame and its Box).
//!
//! **A gradient is changed where it is while it's that shape's alone**
//! and says everything itself; one that other shapes are painted with
//! too, or that takes its stops from another, is never touched (Alva's
//! rule since M3b): the shape is given a copy of its own with the
//! change made, and the copy is what's changed from then on.

use ink_core::{Command, Document, NodeId, Place};
use ink_doc::gradient::{self, Gradient, NewStop, Spread, Stop, Units};
use ink_doc::refs::{self, Ids, href};
use ink_doc::{Kind, Precision, Viewport, geometry};
use ink_geom::Affine;
use lntrn_math::{Color, Vec2};

use crate::colour::palettes::to_hex;
use crate::paint::{self, Paint, Which};

/// A shape's gradient in hand.
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    pub shape: NodeId,
    pub which: Which,
    /// The gradient element, what it's called, and what it says.
    pub at: NodeId,
    pub id: String,
    pub said: Gradient,
    /// Its line in its own numbers (a radial one's: its middle, and a
    /// point of its circle), and what takes those to the drawing's
    /// coordinates.
    pub line: (Vec2, Vec2),
    pub to_doc: Affine,
    /// Its stops' own elements, when it's this shape's alone and says
    /// them itself: then it's changed where it is.
    own: Option<Vec<NodeId>>,
}

/// What changes about one.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Its line: from here to there, in the drawing's coordinates.
    Line(Vec2, Vec2),
    Stops(Vec<Stop>),
    /// Linear, or (`true`) radial: the same line, the same stops.
    Kind(bool),
}

/// The box a gradient on `shape` is measured by, in its coordinates.
fn bbox(drawing: &Document, shape: NodeId) -> Option<ink_geom::Rect> {
    geometry::path_of(drawing.get(shape)?).bounds()
}

fn view(drawing: &Document) -> Vec2 {
    drawing.get(drawing.root()).map_or(Vec2::ZERO, |root| Viewport::of(root).view)
}

impl Held {
    /// The gradient `which` paint of `shape` is, where it's one the
    /// tool can show: on a shape (not a text), with somewhere to be.
    pub fn of(drawing: &Document, shape: NodeId, which: Which) -> Option<Held> {
        let Paint::Server(id) = paint::read(drawing, shape).get(which).clone() else { return None };
        if !drawing.get(shape)?.kind.is_shape() {
            return None;
        }
        let ids = Ids::of(drawing);
        let at = ids.get(&id)?;
        let node = drawing.get(at)?;
        let said = Gradient::of(drawing, &ids, node)?;
        let to_doc = said.to_user(bbox(drawing, shape))?.then(&geometry::to_doc(drawing, shape)?);
        to_doc.inverse()?;
        let stops: Vec<NodeId> = node.elements().filter(|&s| drawing.get(s).is_some_and(|n| n.kind == Kind::Stop)).collect();
        let alone = refs::users(drawing).get(&id).is_some_and(|users| users.iter().all(|user| *user == shape)) && href(node).is_none() && stops.len() == said.stops.len();
        Some(Held { shape, which, at, id, line: said.line(view(drawing)), said, to_doc, own: alone.then_some(stops) })
    }

    /// Its line's two ends in the drawing's coordinates.
    pub fn ends(&self) -> (Vec2, Vec2) {
        (self.to_doc.apply(self.line.0), self.to_doc.apply(self.line.1))
    }

    /// The colour at `t` along it (0 to 1): between the stops either
    /// side.
    pub fn colour_at(stops: &[Stop], t: f64) -> Color {
        let Some(after) = stops.iter().position(|s| s.offset >= t) else { return stops.last().map_or(Color::BLACK, |s| s.color) };
        if after == 0 {
            return stops[0].color;
        }
        let (a, b) = (stops[after - 1], stops[after]);
        let share = if b.offset > a.offset { (t - a.offset) / (b.offset - a.offset) } else { 1.0 };
        let mix = |x: f64, y: f64| x + (y - x) * share;
        Color::rgba(mix(a.color.r, b.color.r), mix(a.color.g, b.color.g), mix(a.color.b, b.color.b), mix(a.color.a, b.color.a))
    }

    /// The Command that makes `change`.
    pub fn set(&self, drawing: &Document, change: &Change) -> Command {
        let p = Precision::of(drawing);
        let (mut radial, mut line, mut stops) = (self.said.radial, self.line, self.said.stops.clone());
        match change {
            Change::Line(from, to) => {
                let back = self.to_doc.inverse().unwrap_or(Affine::IDENTITY);
                line = (back.apply(*from), back.apply(*to));
            }
            Change::Stops(now) => stops = now.clone(),
            Change::Kind(now) => radial = *now,
        }
        // Its own, and still the kind of element it is: changed where
        // it is, only what's different.
        if let (Some(own), true) = (&self.own, radial == self.said.radial) {
            let mut steps: Vec<Command> = Vec::new();
            if line != self.line {
                steps.extend(coords(radial, line, &p).into_iter().map(|(name, value)| Command::SetAttr { node: self.at, name: name.to_owned(), value }));
            }
            if stops.len() == own.len() {
                for ((stop, now), was) in own.iter().zip(&stops).zip(&self.said.stops) {
                    if now.offset != was.offset {
                        steps.push(Command::SetAttr { node: *stop, name: "offset".to_owned(), value: Some(p.number(now.offset.clamp(0.0, 1.0))) });
                    }
                    if now.color != was.color {
                        steps.push(Command::SetStyle { nodes: vec![*stop], set: vec![("stop-color".to_owned(), Some(to_hex(now.color))), ("stop-opacity".to_owned(), (now.color.a < 1.0 - 1e-9).then(|| p.number(now.color.a)))] });
                    }
                }
            } else {
                steps.push(Command::Delete { nodes: own.clone() });
                steps.push(Command::Insert { place: Place::LastIn(self.at), elements: stops.iter().map(|s| new_stop(s).element(&p)).collect() });
            }
            return Command::Batch(steps);
        }
        // Another's too, or to be another kind: one of its own, made
        // with the change; and the old one goes, if nothing's left
        // using it.
        let ids = Ids::of(drawing);
        let id = fresh_id(&ids);
        let mut attrs: Vec<(&str, String)> = coords(radial, line, &p).into_iter().filter_map(|(name, value)| Some((name, value?))).collect();
        if self.said.units == Units::UserSpace {
            attrs.push(("gradientUnits", "userSpaceOnUse".to_owned()));
        }
        attrs.extend(p.transform(&self.said.transform).map(|t| ("gradientTransform", t)));
        attrs.extend(match self.said.spread {
            Spread::Pad => None,
            Spread::Reflect => Some(("spreadMethod", "reflect".to_owned())),
            Spread::Repeat => Some(("spreadMethod", "repeat".to_owned())),
        });
        let made = gradient::element(radial, &id, &attrs, &stops.iter().map(new_stop).collect::<Vec<_>>(), &p);
        let mut steps = vec![Command::Define { elements: vec![made] }, Command::SetStyle { nodes: vec![self.shape], set: paint::set(self.which, &Paint::Server(id)) }];
        if self.own.is_some() {
            steps.push(Command::Delete { nodes: vec![self.at] });
        }
        Command::Batch(steps)
    }
}

/// A name no gradient has yet.
fn fresh_id(ids: &Ids) -> String {
    (1..).map(|n| format!("gradient-{n}")).find(|id| ids.get(id).is_none()).expect("there is always another number")
}

fn new_stop(stop: &Stop) -> NewStop {
    NewStop { offset: stop.offset, color: to_hex(stop.color), opacity: (stop.color.a < 1.0 - 1e-9).then_some(stop.color.a) }
}

/// The attributes that say `line`: a linear gradient's two ends; a
/// radial one's middle and how far out its circle is (and no focus of
/// its own: the middle is it).
fn coords(radial: bool, line: (Vec2, Vec2), p: &Precision) -> Vec<(&'static str, Option<String>)> {
    let n = |v: f64| Some(p.number(v));
    if radial {
        vec![("cx", n(line.0.x)), ("cy", n(line.0.y)), ("r", n(line.0.distance(line.1))), ("fx", None), ("fy", None)]
    } else {
        vec![("x1", n(line.0.x)), ("y1", n(line.0.y)), ("x2", n(line.1.x)), ("y2", n(line.1.y))]
    }
}

/// The Command that paints `which` of `shape` with a new gradient of
/// its own: linear from `from` to `to` (the drawing's coordinates), or
/// radial about `from` out to `to`, through `stops`. Measured by the
/// shape's box, so it goes where the shape goes. None for a shape with
/// no box to measure by.
pub fn fresh(drawing: &Document, shape: NodeId, which: Which, radial: bool, (from, to): (Vec2, Vec2), stops: &[Stop]) -> Option<Command> {
    let b = bbox(drawing, shape).filter(|b| b.width() > 0.0 && b.height() > 0.0)?;
    let back = Affine::new(b.width(), 0.0, 0.0, b.height(), b.min.x, b.min.y).then(&geometry::to_doc(drawing, shape)?).inverse()?;
    let p = Precision::of(drawing);
    let ids = Ids::of(drawing);
    let id = fresh_id(&ids);
    let attrs: Vec<(&str, String)> = coords(radial, (back.apply(from), back.apply(to)), &p).into_iter().filter_map(|(name, value)| Some((name, value?))).collect();
    let made = gradient::element(radial, &id, &attrs, &stops.iter().map(new_stop).collect::<Vec<_>>(), &p);
    Some(Command::Batch(vec![Command::Define { elements: vec![made] }, Command::SetStyle { nodes: vec![shape], set: paint::set(which, &Paint::Server(id)) }]))
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    /// A card with a gradient of its own down it, two chips sharing
    /// one, and a plain square.
    fn doc() -> Document {
        Document::parse(
            DocId(1),
            r##"<svg viewBox="0 0 48 48"><defs><linearGradient id="own" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/></linearGradient><linearGradient id="both"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient></defs><rect id="card" x="10" y="10" width="20" height="10" fill="url(#own)"/><rect id="chip" width="4" height="4" fill="url(#both)"/><rect id="chip2" x="6" width="4" height="4" stroke="url(#both)"/><rect id="plain" x="30" y="30" width="10" height="10" fill="#08f"/></svg>"##,
        )
        .unwrap()
    }

    const CARD: NodeId = NodeId(9);
    const CHIP: NodeId = NodeId(10);
    const CHIP2: NodeId = NodeId(11);
    const PLAIN: NodeId = NodeId(12);

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn stop(offset: f64, hex: u32) -> Stop {
        Stop { offset, color: Color::hex(hex) }
    }

    #[test]
    fn a_gradient_is_held_where_it_paints() {
        let d = doc();
        // Down the card's left side, top to bottom.
        let held = Held::of(&d, CARD, Which::Fill).unwrap();
        assert_eq!((held.ends(), held.id.as_str(), held.said.stops.len(), held.own.as_ref().map(Vec::len)), ((v(10.0, 10.0), v(10.0, 20.0)), "own", 2, Some(2)));
        // One two shapes share isn't either's own; a stroke's is held as
        // the stroke's.
        assert_eq!((Held::of(&d, CHIP, Which::Fill).map(|h| (h.ends(), h.own)), Held::of(&d, CHIP2, Which::Stroke).map(|h| h.id)), (Some(((v(0.0, 0.0), v(4.0, 0.0)), None)), Some("both".to_owned())));
        // A plain colour, no paint of that kind, and no shape: none.
        assert_eq!((Held::of(&d, PLAIN, Which::Fill), Held::of(&d, CARD, Which::Stroke), Held::of(&d, NodeId(2), Which::Fill)), (None, None, None));
        // Between two stops, the colour part of the way.
        let stops = [stop(0.0, 0x000000), stop(0.5, 0xFFFFFF), stop(1.0, 0xFF0000)];
        assert_eq!((Held::colour_at(&stops, 0.25), Held::colour_at(&stops, 0.5), Held::colour_at(&stops, 2.0), Held::colour_at(&stops[1..], 0.1)), (Color::rgba(0.5, 0.5, 0.5, 1.0), Color::WHITE, Color::hex(0xFF0000), Color::WHITE));
    }

    #[test]
    fn its_own_is_changed_where_it_is() {
        let mut d = doc();
        let held = |d: &Document| Held::of(d, CARD, Which::Fill).unwrap();
        // Its line, in the shares of the card's box it's measured by.
        d.apply(&held(&d).set(&d, &Change::Line(v(10.0, 15.0), v(30.0, 15.0)))).unwrap();
        assert!(d.to_svg().contains(r#"<linearGradient id="own" x1="0" y1="0.5" x2="1" y2="0.5">"#), "{}", d.to_svg());
        // A stop moved and recoloured: only that is said anew.
        let applied = d.apply(&held(&d).set(&d, &Change::Stops(vec![stop(0.25, 0xFFFFFF), Stop { offset: 1.0, color: Color::hex(0x0088FF).with_alpha(0.5) }]))).unwrap();
        assert!(d.to_svg().contains(r##"<stop offset="0.25" stop-color="#fff"/><stop offset="1" stop-color="#0088ff" stop-opacity="0.5"/>"##), "{}", d.to_svg());
        assert_eq!(applied.changed.len(), 2);
        // One more stop: they're written out again, in order.
        d.apply(&held(&d).set(&d, &Change::Stops(vec![stop(0.0, 0xFFFFFF), stop(0.5, 0xFF0000), stop(1.0, 0x000000)]))).unwrap();
        assert!(d.to_svg().contains(r##"<stop offset="0" stop-color="#ffffff"/><stop offset="0.5" stop-color="#ff0000"/><stop offset="1" stop-color="#000000"/></linearGradient>"##), "{}", d.to_svg());
        // Made radial, it's another element: a new one of its own in the
        // old one's place, about where its line began.
        d.apply(&held(&d).set(&d, &Change::Kind(true))).unwrap();
        let now = held(&d);
        assert_eq!((now.said.radial, now.id.as_str(), now.ends(), now.said.stops.len(), d.to_svg().contains("id=\"own\"")), (true, "gradient-1", (v(10.0, 15.0), v(30.0, 15.0)), 3, false));
        assert!(d.to_svg().contains(r#"<radialGradient id="gradient-1" cx="0" cy="0.5" r="1">"#), "{}", d.to_svg());
    }

    #[test]
    fn a_shared_one_is_left_and_a_plain_shape_is_given_its_own() {
        let mut d = doc();
        // The chip's is the other chip's too: it gets a copy, changed.
        let held = Held::of(&d, CHIP, Which::Fill).unwrap();
        d.apply(&held.set(&d, &Change::Line(v(0.0, 0.0), v(0.0, 4.0)))).unwrap();
        assert_eq!((d.node(CHIP).unwrap().attr("fill"), d.node(CHIP2).unwrap().attr("stroke")), (Some("url(#gradient-1)"), Some("url(#both)")));
        assert!(d.to_svg().contains(r##"<linearGradient id="both"><stop offset="0" stop-color="#f00"/>"##) && d.to_svg().contains(r##"<linearGradient id="gradient-1" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient>"##), "{}", d.to_svg());
        // And that copy is its own from then on.
        assert!(Held::of(&d, CHIP, Which::Fill).unwrap().own.is_some());
        // A plain square: a new one across it where it was dragged,
        // measured by its box.
        d.apply(&fresh(&d, PLAIN, Which::Fill, false, (v(30.0, 30.0), v(40.0, 40.0)), &[stop(0.0, 0x0088FF), stop(1.0, 0x003366)]).unwrap()).unwrap();
        let made = Held::of(&d, PLAIN, Which::Fill).unwrap();
        assert_eq!((made.id.as_str(), made.ends(), made.said.units, made.own.is_some()), ("gradient-2", (v(30.0, 30.0), v(40.0, 40.0)), Units::BBox, true));
        // Round, about a point, out to another.
        d.apply(&fresh(&d, PLAIN, Which::Stroke, true, (v(35.0, 35.0), v(35.0, 40.0)), &[stop(0.0, 0xFFFFFF), stop(1.0, 0x000000)]).unwrap()).unwrap();
        assert!(d.to_svg().contains(r#"<radialGradient id="gradient-3" cx="0.5" cy="0.5" r="0.5">"#), "{}", d.to_svg());
        // No box to measure by, no gradient.
        let flat = Document::parse(DocId(1), r#"<svg><line x2="8"/></svg>"#).unwrap();
        assert_eq!(fresh(&flat, NodeId(2), Which::Stroke, false, (v(0.0, 0.0), v(8.0, 0.0)), &[]), None);
    }
}
