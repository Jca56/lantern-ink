//! The `transform` attribute: a list of matrix, translate, scale, rotate,
//! skewX and skewY, read into one [`Affine`].

use ink_geom::number::parse_list;
use ink_geom::{Affine, Vec2};

use crate::length::Length;
use crate::node::Node;
use crate::style::prop;

/// A `transform` value. The transforms listed apply right to left, as
/// the spec says. One that names NaN or infinity, or overflows to it, is
/// invalid, and so no transform at all.
pub fn parse(text: &str) -> Affine {
    let mut result = Affine::IDENTITY;
    let mut rest = text.trim();
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let Some(close) = rest[open..].find(')') else { break };
        let Some(args) = parse_list(&rest[open + 1..open + close]) else { return Affine::IDENTITY };
        let a = |i: usize| args.get(i).copied().unwrap_or(0.0);
        let t = match name {
            "matrix" if args.len() >= 6 => Affine::new(a(0), a(1), a(2), a(3), a(4), a(5)),
            "translate" => Affine::translate(a(0), a(1)),
            "scale" => Affine::scale(a(0), if args.len() > 1 { a(1) } else { a(0) }),
            "rotate" if args.len() >= 3 => Affine::rotate(a(0).to_radians()).about(Vec2::new(a(1), a(2))),
            "rotate" => Affine::rotate(a(0).to_radians()),
            "skewX" => Affine::skew_x(a(0).to_radians()),
            "skewY" => Affine::skew_y(a(0).to_radians()),
            _ => Affine::IDENTITY,
        };
        // The next one listed goes inside this one.
        result = t.then(&result);
        rest = &rest[open + close + 1..];
    }
    if result.is_finite() { result } else { Affine::IDENTITY }
}

/// Where `transform-origin` puts the point a transform turns about;
/// percentages and `center` are of the viewport (`view`).
fn origin(s: &str, view: Vec2) -> Option<Vec2> {
    let one = |v: &str, whole: f64| match v {
        "left" | "top" => Some(0.0),
        "center" => Some(whole * 0.5),
        "right" | "bottom" => Some(whole),
        _ => Length::parse(v).map(|l| l.of(whole)),
    };
    let mut parts = s.split_whitespace();
    let x = one(parts.next()?, view.x)?;
    let y = parts.next().map_or(Some(view.y * 0.5), |v| one(v, view.y))?;
    Some(Vec2::new(x, y))
}

/// `node`'s own transform, about its `transform-origin` if it has one.
/// `None` when it has none (or one that can't be used).
pub fn of(node: &Node, view: Vec2) -> Option<Affine> {
    let t = parse(node.attr("transform")?);
    match prop(node, "transform-origin").and_then(|o| origin(o, view)) {
        Some(o) => Some(t.about(o)).filter(Affine::is_finite),
        None => Some(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::id::{DocId, NodeId};

    fn near(a: Vec2, x: f64, y: f64) -> bool {
        a.distance(Vec2::new(x, y)) < 1e-9
    }

    #[test]
    fn transforms_compose_like_svg() {
        assert!(near(parse("translate(10,0) scale(2)").apply(Vec2::new(1.0, 1.0)), 12.0, 2.0), "scale first, then translate");
        assert!(near(parse("rotate(90)").apply(Vec2::X), 0.0, 1.0));
        assert!(near(parse("rotate(90 10 10)").apply(Vec2::new(20.0, 10.0)), 10.0, 20.0));
        assert!(near(parse("matrix(1 0 0 1 5 6)").apply(Vec2::ZERO), 5.0, 6.0));
        assert!(near(parse(" scale(2 3) , translate(1 1)").apply(Vec2::ZERO), 2.0, 3.0));
        assert!(near(parse("skewX(45)").apply(Vec2::new(0.0, 1.0)), 1.0, 1.0));
        assert_eq!(parse(""), Affine::IDENTITY);
        assert_eq!(parse("spin(3)"), Affine::IDENTITY, "an unknown one is none");
    }

    #[test]
    fn non_finite_transforms_are_no_transform() {
        for t in ["scale(NaN)", "translate(inf 0)", "matrix(1 0 0 1 -infinity 0)", "scale(1e200) scale(1e200)", "translate(1e999)"] {
            assert_eq!(parse(t), Affine::IDENTITY, "{t}");
        }
    }

    #[test]
    fn a_transform_turns_about_its_origin() {
        let d = Document::parse(DocId(1), r#"<svg><g transform="rotate(90)" transform-origin="10 10"/><g transform="scale(2)" style="transform-origin: center"/><g/></svg>"#).unwrap();
        let view = Vec2::new(100.0, 60.0);
        assert!(near(of(d.node(NodeId(2)).unwrap(), view).unwrap().apply(Vec2::new(20.0, 10.0)), 10.0, 20.0));
        assert!(near(of(d.node(NodeId(3)).unwrap(), view).unwrap().apply(Vec2::new(50.0, 30.0)), 50.0, 30.0), "the viewport's middle stays put");
        assert_eq!(of(d.node(NodeId(4)).unwrap(), view), None);
    }
}
