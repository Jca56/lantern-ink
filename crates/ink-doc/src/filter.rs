//! Filters as their elements say them (ARCHITECTURE §5.1): a chain of
//! steps, each working on the element's own picture or on what earlier
//! steps made of it. The steps Ink draws are the ones the Lantern
//! projects' drawings use: a blur, an offset, a flood of colour, a
//! composite of two pictures, a merge of several, a transfer curve for
//! each channel, and a drop shadow. A filter with any other step in
//! it, or with a step given a region of its own, isn't taken up at all:
//! its element draws unfiltered rather than wrong.

use std::collections::HashMap;

use lntrn_math::Color;

use crate::color;
use crate::document::Document;
use crate::gradient::Units;
use crate::kind::Kind;
use crate::length::{Length, number, numbers, unit};
use crate::node::Node;
use crate::style::prop;

/// What a step works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// The element as it's drawn.
    Graphic,
    /// The element's shape alone: its alpha, with no colour.
    Alpha,
    /// What the step of this index made.
    Step(usize),
    /// Something Ink has no picture of (what's behind the element, its
    /// fill as a paint): clear.
    Nothing,
}

/// How `feComposite` puts one picture with another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operator {
    Over,
    /// The top one, where the other is.
    In,
    /// The top one, where the other isn't.
    Out,
    /// The top one where the other is, and the other elsewhere.
    Atop,
    /// Each where the other isn't.
    Xor,
    /// k1·top·under + k2·top + k3·under + k4.
    Arithmetic([f64; 4]),
}

/// What `feComponentTransfer` does to one channel.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Identity,
    Linear { slope: f64, intercept: f64 },
    Gamma { amplitude: f64, exponent: f64, offset: f64 },
    /// Joined by straight lines.
    Table(Vec<f64>),
    /// In steps.
    Discrete(Vec<f64>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `std` is the deviation across and down.
    Blur { of: Input, std: (f64, f64) },
    Offset { of: Input, dx: f64, dy: f64 },
    /// Straight alpha, its `flood-opacity` multiplied in.
    Flood { color: Color },
    Composite { top: Input, under: Input, op: Operator },
    /// Laid one over another, the first lowest.
    Merge { of: Vec<Input> },
    /// A curve for each of red, green, blue and alpha.
    Transfer { of: Input, curves: [Curve; 4] },
    /// `of` over a blurred, offset, tinted copy of its own shape.
    DropShadow { of: Input, dx: f64, dy: f64, std: (f64, f64), color: Color },
}

/// One step of a filter.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub effect: Effect,
    /// Worked out in linear light (as filters are unless they say
    /// `color-interpolation-filters: sRGB`).
    pub linear: bool,
}

/// A filter as written.
#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    /// What the region is measured in.
    pub units: Units,
    /// What the steps' offsets and blurs are measured in.
    pub primitive_units: Units,
    /// x, y, width, height; `None`: not said (a tenth of the box more
    /// all round).
    pub region: [Option<Length>; 4],
    /// In order; the last one's result is what shows.
    pub steps: Vec<Step>,
}

/// One number for both ways, or two; a negative one is nothing.
fn deviation(fe: &Node, default: f64) -> (f64, f64) {
    let std = fe.attr("stdDeviation").and_then(numbers).unwrap_or_default();
    (std.first().copied().unwrap_or(default).max(0.0), std.get(1).or(std.first()).copied().unwrap_or(default).max(0.0))
}

/// A flood's colour: black unless said, clear if it can't be read, its
/// `flood-opacity` multiplied in.
fn flood(fe: &Node) -> Color {
    let color = prop(fe, "flood-color").map_or(Color::BLACK, |c| color::parse(c).unwrap_or(Color::TRANSPARENT));
    color.fade(prop(fe, "flood-opacity").and_then(unit).unwrap_or(1.0))
}

fn curve(func: &Node) -> Curve {
    let num = |name: &str, default: f64| func.attr(name).and_then(number).unwrap_or(default);
    let table = || func.attr("tableValues").and_then(numbers).filter(|t| !t.is_empty());
    match (func.attr("type").map(str::trim), table()) {
        (Some("linear"), _) => Curve::Linear { slope: num("slope", 1.0), intercept: num("intercept", 0.0) },
        (Some("gamma"), _) => Curve::Gamma { amplitude: num("amplitude", 1.0), exponent: num("exponent", 1.0), offset: num("offset", 0.0) },
        (Some("table"), Some(values)) => Curve::Table(values),
        (Some("discrete"), Some(values)) => Curve::Discrete(values),
        _ => Curve::Identity,
    }
}

impl Filter {
    /// The filter `node` is, when it's one that can be drawn.
    pub fn of(doc: &Document, node: &Node) -> Option<Filter> {
        if node.kind != Kind::Filter {
            return None;
        }
        // Linear light, unless the filter or the step says otherwise.
        let linear = |fe: &Node| !matches!(prop(fe, "color-interpolation-filters").or_else(|| prop(node, "color-interpolation-filters")), Some("sRGB"));
        let mut steps: Vec<Step> = Vec::new();
        let mut named: HashMap<&str, usize> = HashMap::new();
        for fe in node.elements().filter_map(|id| doc.get(id)).filter(|n| n.kind == Kind::FilterPrimitive) {
            if ["x", "y", "width", "height"].iter().any(|a| fe.attr(a).is_some()) {
                return None;
            }
            let i = steps.len();
            // What a step works on when it doesn't say: the step before,
            // or for the first the element itself.
            let before = if i == 0 { Input::Graphic } else { Input::Step(i - 1) };
            let input = |said: Option<&str>| match said.map(str::trim) {
                None | Some("") => before,
                Some("SourceGraphic") => Input::Graphic,
                Some("SourceAlpha") => Input::Alpha,
                Some("BackgroundImage" | "BackgroundAlpha" | "FillPaint" | "StrokePaint") => Input::Nothing,
                Some(name) => named.get(name).map_or(before, |&k| Input::Step(k)),
            };
            let num = |name: &str, default: f64| fe.attr(name).and_then(number).unwrap_or(default);
            let of = input(fe.attr("in"));
            let effect = match fe.local() {
                "feGaussianBlur" => Effect::Blur { of, std: deviation(fe, 0.0) },
                "feOffset" => Effect::Offset { of, dx: num("dx", 0.0), dy: num("dy", 0.0) },
                "feFlood" => Effect::Flood { color: flood(fe) },
                "feComposite" => {
                    let op = match fe.attr("operator").map(str::trim) {
                        Some("in") => Operator::In,
                        Some("out") => Operator::Out,
                        Some("atop") => Operator::Atop,
                        Some("xor") => Operator::Xor,
                        Some("arithmetic") => Operator::Arithmetic(["k1", "k2", "k3", "k4"].map(|k| num(k, 0.0))),
                        _ => Operator::Over,
                    };
                    Effect::Composite { top: of, under: input(fe.attr("in2")), op }
                }
                "feMerge" => Effect::Merge { of: fe.elements().filter_map(|id| doc.get(id)).filter(|n| n.local() == "feMergeNode").map(|n| input(n.attr("in"))).collect() },
                "feComponentTransfer" => {
                    let func = |name: &str| fe.elements().filter_map(|id| doc.get(id)).rfind(|n| n.local() == name).map_or(Curve::Identity, curve);
                    Effect::Transfer { of, curves: [func("feFuncR"), func("feFuncG"), func("feFuncB"), func("feFuncA")] }
                }
                "feDropShadow" => Effect::DropShadow { of, dx: num("dx", 2.0), dy: num("dy", 2.0), std: deviation(fe, 2.0), color: flood(fe) },
                _ => return None,
            };
            steps.push(Step { effect, linear: linear(fe) });
            if let Some(name) = fe.attr("result").map(str::trim).filter(|n| !n.is_empty()) {
                named.insert(name, i);
            }
        }
        if steps.is_empty() {
            return None;
        }
        Some(Filter {
            units: node.attr("filterUnits").and_then(Units::parse).unwrap_or(Units::BBox),
            primitive_units: node.attr("primitiveUnits").and_then(Units::parse).unwrap_or(Units::UserSpace),
            region: ["x", "y", "width", "height"].map(|n| node.attr(n).and_then(Length::parse)),
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;
    use crate::refs::Ids;

    fn filter(defs: &str, id: &str) -> Option<Filter> {
        let d = Document::parse(DocId(1), &format!("<svg>{defs}</svg>")).unwrap();
        Filter::of(&d, d.node(Ids::of(&d).get(id)?).unwrap())
    }

    fn effects(defs: &str, id: &str) -> Vec<Effect> {
        filter(defs, id).unwrap().steps.into_iter().map(|s| s.effect).collect()
    }

    #[test]
    fn reads_drop_shadows() {
        let defs = r##"<filter id="s" x="-20%" y="-15%" width="150%" height="140%"><feDropShadow dx="0.5" dy="0.8" stdDeviation="0.6" flood-color="#000" flood-opacity="0.3"/></filter><filter id="plain"><feDropShadow/><feDropShadow stdDeviation="1 -3" style="flood-color: red"/></filter>"##;
        let s = filter(defs, "s").unwrap();
        assert_eq!(s.steps, vec![Step { effect: Effect::DropShadow { of: Input::Graphic, dx: 0.5, dy: 0.8, std: (0.6, 0.6), color: Color::BLACK.with_alpha(0.3) }, linear: true }]);
        assert_eq!((s.units, s.primitive_units), (Units::BBox, Units::UserSpace));
        assert_eq!(s.region, [Some(Length::Percent(-20.0)), Some(Length::Percent(-15.0)), Some(Length::Percent(150.0)), Some(Length::Percent(140.0))]);
        // The defaults: 2 across, 2 down, 2 of blur, black; the second
        // is the shadow of the first's result.
        let plain = filter(defs, "plain").unwrap();
        assert_eq!(plain.steps[0].effect, Effect::DropShadow { of: Input::Graphic, dx: 2.0, dy: 2.0, std: (2.0, 2.0), color: Color::BLACK });
        assert_eq!(plain.steps[1].effect, Effect::DropShadow { of: Input::Step(0), dx: 2.0, dy: 2.0, std: (1.0, 0.0), color: Color::RED });
        assert_eq!(plain.region, [None; 4]);
    }

    #[test]
    fn reads_a_chain_of_steps_by_what_each_works_on() {
        // A glow: the blur, then the element over it.
        let glow = r##"<filter id="f"><feGaussianBlur stdDeviation="6" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>"##;
        assert_eq!(effects(glow, "f"), vec![Effect::Blur { of: Input::Graphic, std: (6.0, 6.0) }, Effect::Merge { of: vec![Input::Step(0), Input::Graphic] }]);
        // A shadow built by hand, as Boxy writes one.
        let boxy = r##"<filter id="f" color-interpolation-filters="sRGB"><feGaussianBlur in="SourceAlpha" stdDeviation="0"/><feOffset dx="10" dy="10"/><feComponentTransfer result="offsetblur"><feFuncA type="linear" slope="0.5"/></feComponentTransfer><feFlood flood-color="#000" flood-opacity="0.3"/><feComposite in2="offsetblur" operator="in"/><feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>"##;
        let f = filter(boxy, "f").unwrap();
        assert!(f.steps.iter().all(|s| !s.linear), "as its colours are written, since it says so");
        assert_eq!(
            f.steps.into_iter().map(|s| s.effect).collect::<Vec<_>>(),
            vec![
                Effect::Blur { of: Input::Alpha, std: (0.0, 0.0) },
                Effect::Offset { of: Input::Step(0), dx: 10.0, dy: 10.0 },
                Effect::Transfer { of: Input::Step(1), curves: [Curve::Identity, Curve::Identity, Curve::Identity, Curve::Linear { slope: 0.5, intercept: 0.0 }] },
                Effect::Flood { color: Color::BLACK.with_alpha(0.3) },
                Effect::Composite { top: Input::Step(3), under: Input::Step(2), op: Operator::In },
                Effect::Merge { of: vec![Input::Step(4), Input::Graphic] },
            ]
        );
        let others = r##"<filter id="f"><feComposite in="SourceGraphic" in2="BackgroundImage" operator="arithmetic" k1="1" k2="0.5" k4="-0.1" result="a"/><feComposite in="nobody" in2="a" operator="xor"/><feComponentTransfer><feFuncR type="table" tableValues="0 1 0"/><feFuncG type="discrete" tableValues="0, 1"/><feFuncB type="gamma" exponent="2"/><feFuncA type="table"/></feComponentTransfer></filter>"##;
        assert_eq!(
            effects(others, "f"),
            vec![
                Effect::Composite { top: Input::Graphic, under: Input::Nothing, op: Operator::Arithmetic([1.0, 0.5, 0.0, -0.1]) },
                Effect::Composite { top: Input::Step(0), under: Input::Step(0), op: Operator::Xor },
                Effect::Transfer { of: Input::Step(1), curves: [Curve::Table(vec![0.0, 1.0, 0.0]), Curve::Discrete(vec![0.0, 1.0]), Curve::Gamma { amplitude: 1.0, exponent: 2.0, offset: 0.0 }, Curve::Identity] },
            ],
            "a name nobody has is the step before"
        );
    }

    #[test]
    fn a_filter_it_cannot_draw_is_not_a_filter() {
        for none in [
            r##"<filter id="f"/>"##,
            r##"<filter id="f"><title>nothing</title></filter>"##,
            r##"<filter id="f"><feTurbulence baseFrequency="0.1"/></filter>"##,
            r##"<filter id="f"><feGaussianBlur stdDeviation="2"/><feMorphology radius="1"/></filter>"##,
            r##"<filter id="f"><feFlood x="0" y="0" width="4" height="4"/></filter>"##,
            r##"<g id="f"><feDropShadow/></g>"##,
        ] {
            assert!(filter(none, "f").is_none(), "{none}");
        }
    }
}
