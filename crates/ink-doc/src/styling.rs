//! Setting how nodes are painted (ARCHITECTURE §3.4): `SetStyle`. A
//! property is one SVG knows, and what it's set to is checked where
//! Ink draws with it (a colour must be a colour), so a slip is refused
//! with its reason rather than written into the file to do nothing.
//! So is painting with what would paint nothing: a `url(#…)` that names
//! nothing in the drawing, and a gradient measured by a box on a shape
//! that has none (a level line). Where it's written is D14's:
//! [`Document::set_prop`].

use crate::color;
use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::geometry::path_of;
use crate::gradient::{Gradient, Units};
use crate::id::NodeId;
use crate::length::{Length, number, numbers, unit};
use crate::refs::{Ids, named};
use crate::style::{Paint, declarations};

/// What a property is set to, as far as Ink checks it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Takes {
    /// A colour, `none`, or a `url(#…)` with either after it.
    Paint,
    Color,
    /// 0 to 1, or a percentage.
    Ratio,
    /// A length that isn't less than nothing.
    Width,
    /// Any length.
    Length,
    /// A number, 1 or more.
    Limit,
    /// `none`, or lengths that aren't less than nothing.
    Dashes,
    OneOf(&'static [&'static str]),
    /// Any of these words, in any order.
    Words(&'static [&'static str]),
    /// Whatever is written: Ink doesn't draw with it, or has no more to
    /// check than that it's there.
    Anything,
}

/// The properties SVG paints and lays out with, and what each takes.
const PROPERTIES: [(&str, Takes); 59] = [
    ("alignment-baseline", Takes::Anything),
    ("baseline-shift", Takes::Anything),
    ("clip-path", Takes::Anything),
    ("clip-rule", Takes::OneOf(&["nonzero", "evenodd"])),
    ("color", Takes::Color),
    ("color-interpolation", Takes::Anything),
    ("color-interpolation-filters", Takes::OneOf(&["auto", "sRGB", "linearRGB"])),
    ("cursor", Takes::Anything),
    ("direction", Takes::Anything),
    ("display", Takes::Anything),
    ("dominant-baseline", Takes::Anything),
    ("fill", Takes::Paint),
    ("fill-opacity", Takes::Ratio),
    ("fill-rule", Takes::OneOf(&["nonzero", "evenodd"])),
    ("filter", Takes::Anything),
    ("flood-color", Takes::Color),
    ("flood-opacity", Takes::Ratio),
    ("font-family", Takes::Anything),
    ("font-size", Takes::Anything),
    ("font-size-adjust", Takes::Anything),
    ("font-stretch", Takes::Anything),
    ("font-style", Takes::Anything),
    ("font-variant", Takes::Anything),
    ("font-weight", Takes::Anything),
    ("image-rendering", Takes::Anything),
    ("isolation", Takes::Anything),
    ("letter-spacing", Takes::Anything),
    ("lighting-color", Takes::Color),
    ("marker", Takes::Anything),
    ("marker-end", Takes::Anything),
    ("marker-mid", Takes::Anything),
    ("marker-start", Takes::Anything),
    ("mask", Takes::Anything),
    ("mix-blend-mode", Takes::Anything),
    ("opacity", Takes::Ratio),
    ("overflow", Takes::Anything),
    ("paint-order", Takes::Words(&["normal", "fill", "stroke", "markers"])),
    ("pointer-events", Takes::Anything),
    ("shape-rendering", Takes::Anything),
    ("stop-color", Takes::Color),
    ("stop-opacity", Takes::Ratio),
    ("stroke", Takes::Paint),
    ("stroke-dasharray", Takes::Dashes),
    ("stroke-dashoffset", Takes::Length),
    ("stroke-linecap", Takes::OneOf(&["butt", "round", "square"])),
    ("stroke-linejoin", Takes::OneOf(&["miter", "round", "bevel", "arcs", "miter-clip"])),
    ("stroke-miterlimit", Takes::Limit),
    ("stroke-opacity", Takes::Ratio),
    ("stroke-width", Takes::Width),
    ("text-anchor", Takes::OneOf(&["start", "middle", "end"])),
    ("text-decoration", Takes::Anything),
    ("text-rendering", Takes::Anything),
    ("transform-box", Takes::Anything),
    ("transform-origin", Takes::Anything),
    ("unicode-bidi", Takes::Anything),
    ("vector-effect", Takes::Anything),
    ("visibility", Takes::OneOf(&["visible", "hidden", "collapse"])),
    ("word-spacing", Takes::Anything),
    ("writing-mode", Takes::Anything),
];

/// The words any property takes, which say where to get its value.
const EVERYWHERE: [&str; 4] = ["inherit", "initial", "unset", "revert"];

/// How many letters apart two names are: put in, left out, or changed.
fn apart(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut left = i + 1;
        let mut corner = i;
        for (j, cb) in b.iter().enumerate() {
            let here = (row[j + 1] + 1).min(left + 1).min(corner + usize::from(ca != cb));
            corner = row[j + 1];
            row[j] = left;
            left = here;
        }
        row[b.len()] = left;
    }
    row[b.len()]
}

/// Whether `value` can be what the property `name` is set to. An error
/// says what's wrong with it, or with the name.
pub fn check(name: &str, value: &str) -> Result<(), String> {
    // A custom property (`--glow`) is the drawing's own.
    let takes = match PROPERTIES.iter().find(|(known, _)| *known == name) {
        Some((_, takes)) => *takes,
        None if name.starts_with("--") && name.len() > 2 && !name.contains(|c: char| c.is_whitespace() || matches!(c, ':' | ';' | '"' | '\'')) => Takes::Anything,
        None => {
            let near = PROPERTIES.iter().map(|(known, _)| (apart(name, known), *known)).filter(|(d, _)| *d <= 2).min();
            return Err(match near {
                Some((_, meant)) => format!("there's no property \"{name}\" (did you mean \"{meant}\"?)"),
                None => format!("there's no property \"{name}\": a style is set by SVG's own names (fill, stroke, stroke-width, opacity, …)"),
            });
        }
    };
    let value = value.trim();
    // It must be one value: no `;` to start another declaration with.
    if value.is_empty() || declarations(&format!("{name}:{value}")).map(|d| d.value == value).collect::<Vec<_>>() != [true] {
        return Err(format!("\"{value}\" can't be what {name} is set to: a value is one thing, with no \";\" in it (null takes a property off)"));
    }
    if EVERYWHERE.contains(&value) || value.starts_with("var(") {
        return Ok(());
    }
    let length = |v: &str| match Length::parse(v) {
        Some(Length::Px(n) | Length::Percent(n)) => Some(n),
        None => None,
    };
    let wrong = |wants: &str| Err(format!("{name} can't be \"{value}\": it takes {wants}"));
    match takes {
        Takes::Paint if Paint::parse(value).is_some() || matches!(value, "context-fill" | "context-stroke") => Ok(()),
        Takes::Paint => wrong("a colour (\"#rrggbb\", a name), \"none\", or \"url(#id)\" for a gradient"),
        Takes::Color if color::parse(value).is_some() => Ok(()),
        Takes::Color => wrong("a colour (\"#rrggbb\", \"rgb(…)\", a name)"),
        Takes::Ratio if unit(value).is_some() => Ok(()),
        Takes::Ratio => wrong("a number from 0 to 1, or a percentage"),
        Takes::Width if length(value).is_some_and(|n| n >= 0.0) => Ok(()),
        Takes::Width => wrong("a length that isn't less than nothing"),
        Takes::Length if length(value).is_some() => Ok(()),
        Takes::Length => wrong("a length"),
        Takes::Limit if number(value).is_some_and(|n| n >= 1.0) => Ok(()),
        Takes::Limit => wrong("a number, 1 or more"),
        Takes::Dashes if value == "none" || numbers(value).is_some_and(|d| !d.is_empty() && d.iter().all(|n| *n >= 0.0)) => Ok(()),
        Takes::Dashes => wrong("\"none\", or the lengths of the dashes and gaps (\"4 2\")"),
        Takes::OneOf(words) if words.contains(&value) => Ok(()),
        Takes::OneOf(words) => wrong(&format!("one of {}", words.join(", "))),
        Takes::Words(words) if value.split_whitespace().all(|w| words.contains(&w)) => Ok(()),
        Takes::Words(words) => wrong(&format!("any of {}", words.join(", "))),
        Takes::Anything => Ok(()),
    }
}

impl Document {
    /// What the drawing has to say of setting `name` to `value` on
    /// `id`: what a `url(#…)` names is there (or the paint says what
    /// to use if it isn't), and a gradient measured by the box of what
    /// it paints has a box to go by. SVG paints nothing otherwise.
    fn fits(&self, ids: &Ids, id: NodeId, name: &str, value: &str) -> Result<(), DocError> {
        let paint = matches!(name, "fill" | "stroke").then(|| Paint::parse(value)).flatten();
        let spare = matches!(paint, Some(Paint::Server { fallback: Some(_), .. }));
        for used in named(name, value) {
            let Some(found) = ids.get(used).and_then(|found| self.get(found)) else {
                if spare {
                    continue;
                }
                return invalid(format!("nothing in the drawing is called \"{used}\", so {name}: {value} would draw nothing. Name what's there (doc_info lists what <defs> holds), or make it first"));
            };
            let node = self.node(id)?;
            let by_box = paint.is_some() && node.kind.is_shape() && Gradient::of(self, ids, found).is_some_and(|g| g.units == Units::BBox);
            if let Some(lacks) = path_of(node).bounds().filter(|_| by_box).and_then(|b| if b.height() <= 0.0 { Some("height") } else if b.width() <= 0.0 { Some("width") } else { None }) {
                return invalid(format!("{id} has no {lacks} (strokes aside), and #{used} is measured by the box of what it paints: it would paint nothing. Paint it with a gradient in its own coordinates (gradient_add with units: \"user\", and from and to)"));
            }
        }
        Ok(())
    }

    /// Set each of `set` (a property, and what it's set to or `None` to
    /// take it off) on each of `nodes`, written where the node has it.
    /// Everything is checked before anything is set. Returns the nodes
    /// it changed.
    pub(crate) fn set_style(&mut self, nodes: &[NodeId], set: &[(String, Option<String>)]) -> Result<Vec<NodeId>, DocError> {
        if nodes.is_empty() || set.is_empty() {
            return invalid(if nodes.is_empty() { "there's nothing to style: name at least one node" } else { "there's nothing to set: name at least one property" });
        }
        for (name, value) in set {
            check(name, value.as_deref().unwrap_or("inherit")).or_else(invalid)?;
        }
        let ids = Ids::of(self);
        for &id in nodes {
            self.node(id)?;
            for (name, value) in set {
                if let Some(value) = value {
                    self.fits(&ids, id, name, value)?;
                }
            }
        }
        let mut changed = Vec::new();
        for &id in nodes {
            let mut touched = false;
            for (name, value) in set {
                touched |= self.set_prop(id, name, value.as_deref().map(str::trim))?;
            }
            if touched && !changed.contains(&id) {
                changed.push(id);
            }
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;

    #[test]
    fn a_property_takes_what_it_can_be() {
        for (name, value) in [
            ("fill", "#ffc800"),
            ("fill", "none"),
            ("fill", "url(#glow) red"),
            ("stroke", "currentColor"),
            ("stroke", " rgb(0, 128, 255) "),
            ("stroke-width", "1.5"),
            ("stroke-width", "2px"),
            ("stroke-width", "0"),
            ("stroke-dasharray", "4 2"),
            ("stroke-dasharray", "none"),
            ("stroke-dashoffset", "-3"),
            ("stroke-linecap", "round"),
            ("stroke-miterlimit", "4"),
            ("opacity", "0.5"),
            ("fill-opacity", "50%"),
            ("fill-rule", "evenodd"),
            ("paint-order", "stroke fill"),
            ("visibility", "hidden"),
            ("filter", "url(#shadow)"),
            ("font-family", "'Inter', sans-serif"),
            ("--glow", "anything at all"),
            ("fill", "inherit"),
            ("stroke-width", "var(--w)"),
        ] {
            assert_eq!(check(name, value), Ok(()), "{name}: {value}");
        }
    }

    #[test]
    fn a_slip_is_refused_with_what_it_should_have_been() {
        let why = |name: &str, value: &str| check(name, value).unwrap_err();
        assert_eq!(why("fil", "red"), "there's no property \"fil\" (did you mean \"fill\"?)");
        assert_eq!(why("stroke_width", "2"), "there's no property \"stroke_width\" (did you mean \"stroke-width\"?)");
        assert_eq!(why("colour", "red"), "there's no property \"colour\" (did you mean \"color\"?)");
        assert!(why("thickness", "2").starts_with("there's no property \"thickness\": a style is set by SVG's own names"));
        assert_eq!(why("fill", "blurple"), "fill can't be \"blurple\": it takes a colour (\"#rrggbb\", a name), \"none\", or \"url(#id)\" for a gradient");
        assert_eq!(why("stroke-width", "-1"), "stroke-width can't be \"-1\": it takes a length that isn't less than nothing");
        assert_eq!(why("stroke-width", "thick"), "stroke-width can't be \"thick\": it takes a length that isn't less than nothing");
        assert_eq!(why("opacity", "half"), "opacity can't be \"half\": it takes a number from 0 to 1, or a percentage");
        assert_eq!(why("stroke-linecap", "pointy"), "stroke-linecap can't be \"pointy\": it takes one of butt, round, square");
        assert_eq!(why("stroke-dasharray", "4 -2"), "stroke-dasharray can't be \"4 -2\": it takes \"none\", or the lengths of the dashes and gaps (\"4 2\")");
        assert_eq!(why("stroke-miterlimit", "0.5"), "stroke-miterlimit can't be \"0.5\": it takes a number, 1 or more");
        assert_eq!(why("paint-order", "stroke first"), "paint-order can't be \"stroke first\": it takes any of normal, fill, stroke, markers");
        assert_eq!(why("stop-color", "url(#g)"), "stop-color can't be \"url(#g)\": it takes a colour (\"#rrggbb\", \"rgb(…)\", a name)");
        // One value: nothing that would start another declaration.
        assert!(why("fill", "red; stroke: blue").contains("a value is one thing"));
        assert!(why("fill", "  ").contains("a value is one thing"));
        assert_eq!((apart("fil", "fill"), apart("stroke", "stroke"), apart("abc", "xyz"), apart("", "ab")), (1, 0, 3, 2));
    }

    #[test]
    fn a_style_is_set_on_every_node_where_each_has_it() {
        let mut d = Document::parse(DocId(1), r##"<svg><style>.a { stroke: red }</style><path fill="red"/><rect style="fill: red; opacity: 1"/><circle class="a"/></svg>"##).unwrap();
        let set = |pairs: &[(&str, Option<&str>)]| pairs.iter().map(|(n, v)| ((*n).to_owned(), v.map(str::to_owned))).collect::<Vec<_>>();
        let nodes = vec![NodeId(3), NodeId(4), NodeId(5)];
        let applied = d.apply(&Command::SetStyle { nodes: nodes.clone(), set: set(&[("fill", Some("#ffc800")), ("stroke", Some(" none ")), ("opacity", None)]) }).unwrap();
        assert_eq!(applied.changed, nodes);
        assert_eq!(d.to_svg(), r##"<svg><style>.a { stroke: red }</style><path fill="#ffc800" stroke="none"/><rect style="fill: #ffc800" stroke="none"/><circle class="a" fill="#ffc800" style="stroke: none"/></svg>"##);
        // The same again changes nothing; a slip changes nothing either.
        assert!(d.apply(&Command::SetStyle { nodes: nodes.clone(), set: set(&[("fill", Some("#ffc800"))]) }).unwrap().is_nothing());
        let before = d.to_svg();
        for (bad, why) in [(set(&[("fill", Some("red")), ("stroke", Some("blurple"))]), "stroke can't be \"blurple\""), (set(&[("fil", Some("red"))]), "there's no property \"fil\""), (set(&[]), "there's nothing to set")] {
            match d.apply(&Command::SetStyle { nodes: nodes.clone(), set: bad }) {
                Err(DocError::Invalid(said)) => assert!(said.starts_with(why), "{said}"),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(d.apply(&Command::SetStyle { nodes: vec![NodeId(3), NodeId(9)], set: set(&[("fill", Some("red"))]) }), Err(DocError::NoSuchNode(NodeId(9))));
        assert!(matches!(d.apply(&Command::SetStyle { nodes: vec![], set: set(&[("fill", Some("red"))]) }), Err(DocError::Invalid(_))));
        assert_eq!(d.to_svg(), before);
    }

    #[test]
    fn what_would_paint_nothing_is_refused() {
        let text = r##"<svg><linearGradient id="by-box"/><linearGradient id="own" gradientUnits="userSpaceOnUse" x2="9"/><linearGradient id="its" href="#own"/><line x2="9"/><path d="M0 0 V9"/><rect width="9" height="4"/><g/></svg>"##;
        // N2 by-box, N3 own, N4 its, N5 a level line, N6 an upright one, N7 a rect, N8 a group.
        let paint = |node: u64, name: &str, value: &str| Document::parse(DocId(1), text).unwrap().apply(&Command::SetStyle { nodes: vec![NodeId(node)], set: vec![(name.to_owned(), Some(value.to_owned()))] }).map(|applied| applied.changed.len()).map_err(|e| e.to_string());
        // A name for nothing: unless the paint says what to use then.
        assert_eq!(paint(7, "fill", "url(#nope)").unwrap_err(), "nothing in the drawing is called \"nope\", so fill: url(#nope) would draw nothing. Name what's there (doc_info lists what <defs> holds), or make it first");
        assert!(paint(7, "filter", "url(#nope)").unwrap_err().starts_with("nothing in the drawing is called \"nope\", so filter: url(#nope)"));
        assert_eq!((paint(7, "fill", "url(#nope) red"), paint(7, "fill", "url(#by-box)")), (Ok(1), Ok(1)));
        // A box with no height, or no width, is no box to measure by.
        assert_eq!(paint(5, "stroke", "url(#by-box)").unwrap_err(), "N5 has no height (strokes aside), and #by-box is measured by the box of what it paints: it would paint nothing. Paint it with a gradient in its own coordinates (gradient_add with units: \"user\", and from and to)");
        assert!(paint(6, "stroke", "url(#by-box)").unwrap_err().starts_with("N6 has no width (strokes aside), and #by-box"));
        // One in the shape's own coordinates paints it, however it says so.
        assert_eq!((paint(5, "stroke", "url(#own)"), paint(5, "stroke", "url(#its)"), paint(8, "stroke", "url(#by-box)")), (Ok(1), Ok(1), Ok(1)));
    }
}
