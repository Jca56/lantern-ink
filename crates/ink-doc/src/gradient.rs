//! Gradients as their elements say them: linear or radial, with stops,
//! units, a transform and a spread, and what one takes over from another
//! by `href`. Fitting one to a shape is the renderer's.

use ink_geom::Affine;
use lntrn_math::Color;

use crate::color;
use crate::document::Document;
use crate::kind::Kind;
use crate::length::{Length, unit};
use crate::node::Node;
use crate::refs::{Ids, href};
use crate::style::prop;
use crate::transform;

/// How deep a chain of `href`s is followed.
const MAX_HREFS: usize = 8;

/// What a gradient's or a filter's numbers are measured in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// The element's own coordinates.
    UserSpace,
    /// Fractions of the box around the element.
    BBox,
}

impl Units {
    pub fn parse(s: &str) -> Option<Units> {
        match s.trim() {
            "userSpaceOnUse" => Some(Units::UserSpace),
            "objectBoundingBox" => Some(Units::BBox),
            _ => None,
        }
    }
}

/// What a gradient does past its ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spread {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// 0..1, and no less than the stop before.
    pub offset: f64,
    /// Straight alpha, its `stop-opacity` multiplied in.
    pub color: Color,
}

/// A gradient with everything it and the gradients it points at say.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub radial: bool,
    /// x1 y1 x2 y2, or cx cy r fx fy; `None`: not said anywhere.
    pub coords: [Option<Length>; 5],
    pub units: Units,
    pub transform: Affine,
    pub spread: Spread,
    pub stops: Vec<Stop>,
}

/// One gradient element's own say; `None` is "not said here".
struct Said {
    radial: bool,
    coords: [Option<Length>; 5],
    units: Option<Units>,
    transform: Option<Affine>,
    spread: Option<Spread>,
    stops: Vec<Stop>,
}

fn said(doc: &Document, g: &Node) -> Said {
    let radial = g.kind == Kind::RadialGradient;
    let names: [&str; 5] = if radial { ["cx", "cy", "r", "fx", "fy"] } else { ["x1", "y1", "x2", "y2", ""] };
    let mut stops: Vec<Stop> = Vec::new();
    for stop in g.elements().filter_map(|id| doc.get(id)).filter(|n| n.kind == Kind::Stop) {
        let offset = stop.attr("offset").and_then(Length::parse).map_or(0.0, Length::fraction).clamp(0.0, 1.0);
        let offset = stops.last().map_or(offset, |last| offset.max(last.offset));
        // A stop with no colour is black; one that can't be read, clear.
        let color = prop(stop, "stop-color").map_or(Color::BLACK, |c| color::parse(c).unwrap_or(Color::TRANSPARENT));
        stops.push(Stop { offset, color: color.fade(prop(stop, "stop-opacity").and_then(unit).unwrap_or(1.0)) });
    }
    Said {
        radial,
        coords: names.map(|n| g.attr(n).and_then(Length::parse)),
        units: g.attr("gradientUnits").and_then(Units::parse),
        transform: g.attr("gradientTransform").map(transform::parse),
        spread: match g.attr("spreadMethod").map(str::trim) {
            Some("pad") => Some(Spread::Pad),
            Some("reflect") => Some(Spread::Reflect),
            Some("repeat") => Some(Spread::Repeat),
            _ => None,
        },
        stops,
    }
}

fn is_gradient(node: &Node) -> bool {
    matches!(node.kind, Kind::LinearGradient | Kind::RadialGradient)
}

impl Gradient {
    /// The gradient `node` is, with what it leaves unsaid taken from the
    /// gradients it points at. `None` when `node` isn't a gradient.
    pub fn of(doc: &Document, ids: &Ids, node: &Node) -> Option<Gradient> {
        if !is_gradient(node) {
            return None;
        }
        let mut g = said(doc, node);
        let mut next = href(node);
        for _ in 0..MAX_HREFS {
            let Some(base_node) = next.and_then(|id| ids.get(id)).and_then(|id| doc.get(id)).filter(|n| is_gradient(n)) else { break };
            let base = said(doc, base_node);
            // Where it runs is only another's to give if it's of the
            // same kind.
            if base.radial == g.radial {
                for (mine, theirs) in g.coords.iter_mut().zip(base.coords) {
                    *mine = mine.or(theirs);
                }
            }
            g.units = g.units.or(base.units);
            g.transform = g.transform.or(base.transform);
            g.spread = g.spread.or(base.spread);
            if g.stops.is_empty() {
                g.stops = base.stops;
            }
            next = href(base_node);
        }
        Some(Gradient { radial: g.radial, coords: g.coords, units: g.units.unwrap_or(Units::BBox), transform: g.transform.unwrap_or(Affine::IDENTITY), spread: g.spread.unwrap_or(Spread::Pad), stops: g.stops })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    fn gradient(defs: &str, id: &str) -> Option<Gradient> {
        let d = Document::parse(DocId(1), &format!("<svg>{defs}</svg>")).unwrap();
        let ids = Ids::of(&d);
        Gradient::of(&d, &ids, d.node(ids.get(id)?).unwrap())
    }

    #[test]
    fn reads_stops_in_order() {
        let g = gradient(
            r##"<linearGradient id="g" x2="10" gradientUnits="userSpaceOnUse" spreadMethod="reflect" gradientTransform="scale(2)"><stop offset="0.5" stop-color="#f00"/><stop offset="50%" style="stop-color: rgb(0, 0, 255)"/><stop offset="0.2" stop-color="#fff" stop-opacity="0.5"/><stop stop-color="nonsense"/><stop offset="7"/></linearGradient>"##,
            "g",
        )
        .unwrap();
        assert_eq!(g.stops.iter().map(|s| s.offset).collect::<Vec<_>>(), vec![0.5, 0.5, 0.5, 0.5, 1.0], "an offset can't go back");
        assert_eq!(g.stops[1].color, Color::BLUE);
        assert_eq!(g.stops[2].color, Color::WHITE.with_alpha(0.5));
        assert_eq!((g.stops[3].color, g.stops[4].color), (Color::TRANSPARENT, Color::BLACK), "unreadable is clear; unsaid is black");
        assert_eq!((g.radial, g.units, g.spread), (false, Units::UserSpace, Spread::Reflect));
        assert_eq!(g.coords, [None, None, Some(Length::Px(10.0)), None, None]);
        assert_eq!(g.transform, Affine::scale(2.0, 2.0));
    }

    #[test]
    fn what_is_unsaid_comes_from_the_href() {
        let defs = r##"<linearGradient id="base" x1="1" x2="10" gradientUnits="userSpaceOnUse" spreadMethod="repeat"><stop stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>
            <linearGradient id="again" xlink:href="#base" x2="20"/>
            <radialGradient id="round" href="#again" r="5"/>
            <linearGradient id="loop" href="#loop"/>
            <linearGradient id="lost" href="#nowhere"/>"##;
        let again = gradient(defs, "again").unwrap();
        assert_eq!(again.coords, [Some(Length::Px(1.0)), None, Some(Length::Px(20.0)), None, None]);
        assert_eq!((again.units, again.spread, again.stops.len()), (Units::UserSpace, Spread::Repeat, 2));
        // A radial one takes a linear one's stops and manner, not its line.
        let round = gradient(defs, "round").unwrap();
        assert_eq!(round.coords, [None, None, Some(Length::Px(5.0)), None, None]);
        assert_eq!((round.radial, round.units, round.spread, round.stops.len()), (true, Units::UserSpace, Spread::Repeat, 2));
        // The defaults, and no going round for ever.
        let lost = gradient(defs, "lost").unwrap();
        assert_eq!((lost.units, lost.spread, lost.transform, lost.stops.len()), (Units::BBox, Spread::Pad, Affine::IDENTITY, 0));
        assert!(gradient(defs, "loop").is_some());
        assert!(gradient("<g id=\"g\"/>", "g").is_none());
    }
}
