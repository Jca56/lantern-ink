//! The one filter drawn so far: `feDropShadow`, or several one after
//! another. A filter with any other step in it isn't taken up at all:
//! its element draws unfiltered rather than wrong.

use lntrn_math::Color;

use crate::color;
use crate::document::Document;
use crate::gradient::Units;
use crate::kind::Kind;
use crate::length::{Length, number, numbers, unit};
use crate::node::Node;
use crate::style::prop;

/// One `feDropShadow`, in its filter's units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropShadow {
    pub dx: f64,
    pub dy: f64,
    /// The blur's standard deviation across and down.
    pub std: (f64, f64),
    /// Straight alpha, its `flood-opacity` multiplied in.
    pub color: Color,
}

/// A filter as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    /// What the region is measured in.
    pub units: Units,
    /// What the shadows' offsets and blurs are measured in.
    pub primitive_units: Units,
    /// x, y, width, height; `None`: not said (a tenth of the box more
    /// all round).
    pub region: [Option<Length>; 4],
    pub shadows: Vec<DropShadow>,
}

impl Filter {
    /// The filter `node` is, when it's one that can be drawn.
    pub fn of(doc: &Document, node: &Node) -> Option<Filter> {
        let steps: Vec<&Node> = node.elements().filter_map(|id| doc.get(id)).collect();
        if node.kind != Kind::Filter || steps.is_empty() || steps.iter().any(|s| s.local() != "feDropShadow" || s.kind != Kind::FilterPrimitive) {
            return None;
        }
        let shadows = steps
            .iter()
            .map(|fe| {
                let num = |name: &str, default: f64| fe.attr(name).and_then(number).unwrap_or(default);
                // One number blurs both ways alike; a negative one is no blur.
                let std = fe.attr("stdDeviation").and_then(numbers).unwrap_or_default();
                let (sx, sy) = (std.first().copied().unwrap_or(2.0), std.get(1).or(std.first()).copied().unwrap_or(2.0));
                // A flood with no colour is black; one that can't be read, clear.
                let color = prop(fe, "flood-color").map_or(Color::BLACK, |c| color::parse(c).unwrap_or(Color::TRANSPARENT));
                DropShadow { dx: num("dx", 2.0), dy: num("dy", 2.0), std: (sx.max(0.0), sy.max(0.0)), color: color.fade(prop(fe, "flood-opacity").and_then(unit).unwrap_or(1.0)) }
            })
            .collect();
        Some(Filter {
            units: node.attr("filterUnits").and_then(Units::parse).unwrap_or(Units::BBox),
            primitive_units: node.attr("primitiveUnits").and_then(Units::parse).unwrap_or(Units::UserSpace),
            region: ["x", "y", "width", "height"].map(|n| node.attr(n).and_then(Length::parse)),
            shadows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{DocId, NodeId};

    #[test]
    fn reads_drop_shadows_and_nothing_else() {
        let d = Document::parse(
            DocId(1),
            r##"<svg><filter id="s" x="-20%" y="-15%" width="150%" height="140%"><feDropShadow dx="0.5" dy="0.8" stdDeviation="0.6" flood-color="#000" flood-opacity="0.3"/></filter><filter id="blurry"><feGaussianBlur stdDeviation="2"/></filter><filter id="plain"><feDropShadow/><feDropShadow stdDeviation="1 -3" style="flood-color: red"/></filter><filter id="empty"/><g id="g"><feDropShadow/></g></svg>"##,
        )
        .unwrap();
        let filter = |n: u64| Filter::of(&d, d.node(NodeId(n)).unwrap());
        let s = filter(2).unwrap();
        assert_eq!(s.shadows, vec![DropShadow { dx: 0.5, dy: 0.8, std: (0.6, 0.6), color: Color::BLACK.with_alpha(0.3) }]);
        assert_eq!((s.units, s.primitive_units), (Units::BBox, Units::UserSpace));
        assert_eq!(s.region, [Some(Length::Percent(-20.0)), Some(Length::Percent(-15.0)), Some(Length::Percent(150.0)), Some(Length::Percent(140.0))]);
        assert!(filter(4).is_none(), "a blur isn't a drop shadow");
        // The defaults: 2 across, 2 down, 2 of blur, black.
        let plain = filter(6).unwrap();
        assert_eq!(plain.shadows[0], DropShadow { dx: 2.0, dy: 2.0, std: (2.0, 2.0), color: Color::BLACK });
        assert_eq!((plain.shadows[1].std, plain.shadows[1].color), ((1.0, 0.0), Color::RED));
        assert_eq!(plain.region, [None; 4]);
        assert!(filter(9).is_none() && filter(10).is_none(), "nothing in it; not a filter");
    }
}
