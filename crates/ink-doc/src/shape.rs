//! A shape's own numbers, typed (ARCHITECTURE §3.1): a `<rect>`'s
//! corner, size and radii, a `<circle>`'s centre and radius, a
//! `<path>`'s segments. Read from the node's attributes, and written
//! back to them: only the ones whose meaning changed.

use ink_geom::number::parse_list;
use ink_geom::{Affine, Path, Rect, Vec2};

use crate::kind::Kind;
use crate::length::number;
use crate::node::Node;
use crate::value::Precision;

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    /// `rx` and `ry` as they come to (one given is both).
    Rect { x: f64, y: f64, width: f64, height: f64, rx: f64, ry: f64 },
    Circle { c: Vec2, r: f64 },
    Ellipse { c: Vec2, rx: f64, ry: f64 },
    Line { from: Vec2, to: Vec2 },
    /// A `<polyline>`, or closed a `<polygon>`.
    Poly { points: Vec<Vec2>, closed: bool },
    Path(Path),
}

/// An attribute to set (or, `None`, to take off).
pub type AttrEdit = (&'static str, Option<String>);

impl Geometry {
    /// What `node`'s attributes say it is. `None` for what isn't a
    /// shape, and for one whose numbers can't all be read (a
    /// percentage, path data that stops making sense part-way): it is
    /// drawn as far as it reads, but can't be written afresh without
    /// losing what the file said.
    pub fn of(node: &Node) -> Option<Geometry> {
        // An attribute that isn't there is 0; one that's there must read.
        let n = |name: &str| node.attr(name).map_or(Some(0.0), number);
        let some = |name: &str| node.attr(name).map(number);
        Some(match node.kind {
            Kind::Rect => {
                let (rx, ry) = match (some("rx"), some("ry")) {
                    (None, None) => (0.0, 0.0),
                    (Some(r), None) | (None, Some(r)) => (r?, r?),
                    (Some(rx), Some(ry)) => (rx?, ry?),
                };
                Geometry::Rect { x: n("x")?, y: n("y")?, width: n("width")?, height: n("height")?, rx, ry }
            }
            Kind::Circle => Geometry::Circle { c: Vec2::new(n("cx")?, n("cy")?), r: n("r")? },
            Kind::Ellipse => Geometry::Ellipse { c: Vec2::new(n("cx")?, n("cy")?), rx: n("rx")?, ry: n("ry")? },
            Kind::Line => Geometry::Line { from: Vec2::new(n("x1")?, n("y1")?), to: Vec2::new(n("x2")?, n("y2")?) },
            Kind::Polyline | Kind::Polygon => {
                let numbers = node.attr("points").map_or(Some(Vec::new()), parse_list)?;
                Geometry::Poly { points: numbers.chunks_exact(2).map(|p| Vec2::new(p[0], p[1])).collect(), closed: node.kind == Kind::Polygon }
            }
            Kind::Path => {
                let parsed = Path::parse(node.attr("d").unwrap_or(""));
                if parsed.stopped_at.is_some() {
                    return None;
                }
                Geometry::Path(parsed.path)
            }
            _ => return None,
        })
    }

    /// The outline it draws.
    pub fn path(&self) -> Path {
        match self {
            Geometry::Rect { x, y, width, height, rx, ry } => Path::rect(*x, *y, *width, *height, *rx, *ry),
            Geometry::Circle { c, r } => Path::ellipse(*c, *r, *r),
            Geometry::Ellipse { c, rx, ry } => Path::ellipse(*c, *rx, *ry),
            Geometry::Line { from, to } => Path::polyline(&[*from, *to], false),
            Geometry::Poly { points, closed } => Path::polyline(points, *closed),
            Geometry::Path(path) => path.clone(),
        }
    }

    /// The box around it, in its own coordinates: around what its
    /// numbers say, even where that draws nothing (a rect of no height).
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Geometry::Rect { x, y, width, height, .. } => Some(Rect::from_xywh(*x, *y, *width, *height)),
            Geometry::Circle { c, r } => Some(Rect::from_center_size(*c, Vec2::splat(2.0 * r))),
            Geometry::Ellipse { c, rx, ry } => Some(Rect::from_center_size(*c, Vec2::new(2.0 * rx, 2.0 * ry))),
            Geometry::Line { from, to } => Some(Rect::new(from.min(*to), from.max(*to))),
            Geometry::Poly { points, .. } => points.iter().fold(None, |b: Option<Rect>, &p| Some(b.map_or(Rect::new(p, p), |b| Rect::new(b.min.min(p), b.max.max(p))))),
            Geometry::Path(path) => path.bounds(),
        }
    }

    /// The middle of its box: what a turn it can't take into its numbers
    /// is written about.
    pub fn center(&self) -> Vec2 {
        self.bounds().map_or(Vec2::ZERO, |b| b.center())
    }

    /// How far from the origin it reaches.
    pub fn reach(&self) -> f64 {
        self.bounds().map_or(0.0, |b| b.min.abs().max(b.max.abs()).max_element())
    }

    /// This shape once through `t`, if a shape of its kind can be what
    /// `t` makes of it (as far as `p` can say): a path, a line and a
    /// polygon always; a circle while it stays round; a rect and an
    /// ellipse while their sides stay level and upright (a quarter turn
    /// swaps them).
    pub fn through(&self, t: &Affine, p: &Precision) -> Option<Geometry> {
        // Where a half-width along x and a half-height along y go: each
        // must still lie along one axis. Their sizes are the new ones.
        let upright = |along_x: f64, along_y: f64| -> Option<(f64, f64)> {
            let (u, v) = (t.linear(Vec2::new(along_x, 0.0)), t.linear(Vec2::new(0.0, along_y)));
            let level = u.y.abs() <= p.within() && v.x.abs() <= p.within();
            let swapped = u.x.abs() <= p.within() && v.y.abs() <= p.within();
            (level || swapped).then(|| (u.x.abs() + v.x.abs(), u.y.abs() + v.y.abs()))
        };
        Some(match self {
            Geometry::Rect { x, y, width, height, rx, ry } => {
                let (hw, hh) = upright(width / 2.0, height / 2.0)?;
                let (rx, ry) = upright(*rx, *ry)?;
                let c = t.apply(Vec2::new(x + width / 2.0, y + height / 2.0));
                Geometry::Rect { x: c.x - hw, y: c.y - hh, width: 2.0 * hw, height: 2.0 * hh, rx, ry }
            }
            Geometry::Circle { c, r } => {
                let axes = t.axes()?;
                let round = axes.is_uniform(*r, p.within()) && axes.linear().gap(&t.without_move(), *r) <= p.within();
                round.then(|| Geometry::Circle { c: t.apply(*c), r: r * axes.sx })?
            }
            Geometry::Ellipse { c, rx, ry } => {
                let (rx, ry) = upright(*rx, *ry)?;
                Geometry::Ellipse { c: t.apply(*c), rx, ry }
            }
            Geometry::Line { from, to } => Geometry::Line { from: t.apply(*from), to: t.apply(*to) },
            Geometry::Poly { points, closed } => Geometry::Poly { points: points.iter().map(|p| t.apply(*p)).collect(), closed: *closed },
            Geometry::Path(path) => Geometry::Path(path.transformed(t)),
        })
    }

    /// This shape with its numbers as `p` writes them: what the file
    /// will hold.
    pub fn rounded(&self, p: &Precision) -> Geometry {
        let n = |v: f64| p.round(v);
        let v = |v: Vec2| Vec2::new(p.round(v.x), p.round(v.y));
        match self {
            Geometry::Rect { x, y, width, height, rx, ry } => Geometry::Rect { x: n(*x), y: n(*y), width: n(*width), height: n(*height), rx: n(*rx), ry: n(*ry) },
            Geometry::Circle { c, r } => Geometry::Circle { c: v(*c), r: n(*r) },
            Geometry::Ellipse { c, rx, ry } => Geometry::Ellipse { c: v(*c), rx: n(*rx), ry: n(*ry) },
            Geometry::Line { from, to } => Geometry::Line { from: v(*from), to: v(*to) },
            Geometry::Poly { points, closed } => Geometry::Poly { points: points.iter().map(|p| v(*p)).collect(), closed: *closed },
            Geometry::Path(path) => Geometry::Path(Path::parse(&p.path(path)).path),
        }
    }

    /// The attributes to set on `node` (a shape of this kind) for it to
    /// be this: only those whose meaning changes. What already says the
    /// same number keeps the file's own way of saying it, and nothing
    /// is added to say 0.
    pub fn write(&self, node: &Node, p: &Precision) -> Vec<AttrEdit> {
        let mut edits: Vec<AttrEdit> = Vec::new();
        let mut set = |name: &'static str, v: f64| {
            let text = p.number(v);
            let same = match node.attr(name) {
                Some(old) => number(old).is_some_and(|old| p.number(old) == text),
                None => text == "0",
            };
            if !same {
                edits.push((name, Some(text)));
            }
        };
        match self {
            Geometry::Rect { x, y, width, height, rx, ry } => {
                set("x", *x);
                set("y", *y);
                set("width", *width);
                set("height", *height);
                // One radius says both while they're the same: where
                // the node says it, or as rx.
                if p.number(*rx) == p.number(*ry) && !(node.attr("rx").is_some() && node.attr("ry").is_some()) {
                    set(if node.attr("ry").is_some() { "ry" } else { "rx" }, *rx);
                } else {
                    set("rx", *rx);
                    set("ry", *ry);
                }
            }
            Geometry::Circle { c, r } => {
                set("cx", c.x);
                set("cy", c.y);
                set("r", *r);
            }
            Geometry::Ellipse { c, rx, ry } => {
                set("cx", c.x);
                set("cy", c.y);
                set("rx", *rx);
                set("ry", *ry);
            }
            Geometry::Line { from, to } => {
                set("x1", from.x);
                set("y1", from.y);
                set("x2", to.x);
                set("y2", to.y);
            }
            Geometry::Poly { points, .. } => {
                let text = p.points(points);
                let was = match Geometry::of(node) {
                    Some(Geometry::Poly { points, .. }) => p.points(&points),
                    _ => String::new(),
                };
                if was != text || node.attr("points").is_none() && !points.is_empty() {
                    edits.push(("points", Some(text)));
                }
            }
            Geometry::Path(path) => {
                let text = p.path(path);
                let was = node.attr("d").map(|d| p.path(&Path::parse(d).path));
                if was.as_deref() != Some(text.as_str()) && !(was.is_none() && path.is_empty()) {
                    edits.push(("d", Some(text)));
                }
            }
        }
        edits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::id::{DocId, NodeId};

    fn node(element: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 24 24\">{element}</svg>")).unwrap()
    }

    fn geometry(element: &str) -> Option<Geometry> {
        Geometry::of(node(element).node(NodeId(2)).unwrap())
    }

    const P: Precision = Precision { decimals: 3, reach: 24.0 };

    /// The attributes `element` is given to be what `t` makes of it.
    fn through(element: &str, t: &Affine) -> Option<String> {
        let d = node(element);
        let n = d.node(NodeId(2)).unwrap();
        let edits = Geometry::of(n)?.through(t, &P)?.write(n, &P);
        Some(edits.iter().map(|(name, v)| format!("{name}={}", v.as_deref().unwrap_or("-"))).collect::<Vec<_>>().join(" "))
    }

    #[test]
    fn a_shape_is_what_its_attributes_say() {
        assert_eq!(geometry(r#"<rect x="1" width="10" height="4pt" rx="2"/>"#), Some(Geometry::Rect { x: 1.0, y: 0.0, width: 10.0, height: 16.0 / 3.0, rx: 2.0, ry: 2.0 }));
        assert_eq!(geometry(r#"<rect width="10" height="4" rx="1" ry="3"/>"#), Some(Geometry::Rect { x: 0.0, y: 0.0, width: 10.0, height: 4.0, rx: 1.0, ry: 3.0 }));
        assert_eq!(geometry(r#"<circle cx="5" r="2"/>"#), Some(Geometry::Circle { c: Vec2::new(5.0, 0.0), r: 2.0 }));
        assert_eq!(geometry(r#"<polygon points="0,0 4,0 4,4 9"/>"#), Some(Geometry::Poly { points: vec![Vec2::ZERO, Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0)], closed: true }));
        assert_eq!(geometry(r#"<path d="M0 0h4"/>"#).map(|g| g.path().to_data(3)), Some("M0 0 H4".into()));
        // What can't all be read can't be written afresh.
        for unread in [r#"<rect width="50%" height="4"/>"#, r#"<circle r="big"/>"#, r#"<path d="M0 0 L5 5 nonsense"/>"#, r#"<polyline points="0 0 four 0"/>"#, "<g/>", "<text/>"] {
            assert_eq!(geometry(unread), None, "{unread}");
        }
        let rect = geometry(r#"<rect x="2" y="4" width="10" height="4"/>"#).unwrap();
        assert_eq!((rect.center(), rect.reach(), rect.path().to_data(3)), (Vec2::new(7.0, 6.0), 12.0, "M2 4 H12 V8 H2 Z".into()));
        assert_eq!(geometry(r#"<line x1="5" y2="-9"/>"#).unwrap().reach(), 9.0);
    }

    #[test]
    fn a_shape_takes_what_its_kind_can_say() {
        let turn = |deg: f64| Affine::rotate(deg.to_radians()).about(Vec2::new(7.0, 6.0));
        let rect = r#"<rect x="2" y="4" width="10" height="4" rx="1"/>"#;
        assert_eq!(through(rect, &Affine::translate(2.0, 3.0)).as_deref(), Some("x=4 y=7"));
        assert_eq!(through(rect, &Affine::scale(2.0, 0.5)).as_deref(), Some("x=4 y=2 width=20 height=2 rx=2 ry=0.5"));
        assert_eq!(through(rect, &Affine::scale(-1.0, 1.0)).as_deref(), Some("x=-12"), "mirrored, a rect is a rect");
        // A quarter turn swaps its sides; any other turn it can't say.
        assert_eq!(through(rect, &turn(90.0)).as_deref(), Some("x=5 y=1 width=4 height=10"));
        assert_eq!(through(rect, &turn(180.0)).as_deref(), Some(""));
        assert_eq!(through(rect, &turn(45.0)), None);
        assert_eq!(through(rect, &Affine::skew_x(0.2)), None);
        // A circle stays one however it's turned, while it's round.
        let circle = r#"<circle cx="5" cy="6" r="2"/>"#;
        assert_eq!(through(circle, &Affine::rotate(1.0).about(Vec2::new(5.0, 6.0)).then(&Affine::scale(2.0, -2.0))).as_deref(), Some("cx=10 cy=-12 r=4"));
        assert_eq!(through(circle, &Affine::scale(2.0, 1.0)), None);
        let ellipse = r#"<ellipse cx="5" cy="6" rx="3" ry="1"/>"#;
        assert_eq!(through(ellipse, &Affine::scale(2.0, 1.0)).as_deref(), Some("cx=10 rx=6"));
        assert_eq!(through(ellipse, &Affine::rotate(-90f64.to_radians())).as_deref(), Some("cx=6 cy=-5 rx=1 ry=3"));
        assert_eq!(through(ellipse, &turn(30.0)), None);
        // Lines, polygons and paths take anything.
        let skew = Affine::skew_x(45f64.to_radians());
        assert_eq!(through(r#"<line x1="0" y1="0" x2="0" y2="4"/>"#, &skew).as_deref(), Some("x2=4"));
        assert_eq!(through(r#"<polygon points="0,0 4,0 4,4"/>"#, &skew).as_deref(), Some("points=0,0 4,0 8,4"));
        assert_eq!(through(r#"<path d="m0 0h4v4z"/>"#, &skew).as_deref(), Some("d=M0 0 H4 L8 4 Z"));
    }

    #[test]
    fn only_what_changed_is_written() {
        // The same number keeps the file's own way of saying it.
        assert_eq!(through(r#"<rect x="2.0" y="4px" width="1e1" height="4"/>"#, &Affine::IDENTITY).as_deref(), Some(""));
        assert_eq!(through(r#"<path d="m0 0h4v4z"/>"#, &Affine::IDENTITY).as_deref(), Some(""), "a path keeps its own commands");
        assert_eq!(through(r#"<polygon points="0 0,4 0 , 4 4"/>"#, &Affine::IDENTITY).as_deref(), Some(""));
        assert_eq!(through("<path/>", &Affine::translate(1.0, 1.0)).as_deref(), Some(""));
        // A radius is said where the node says it.
        assert_eq!(through(r#"<rect width="10" height="4" ry="1"/>"#, &Affine::scale(2.0, 2.0)).as_deref(), Some("width=20 height=8 ry=2"));
        assert_eq!(through(r#"<rect width="10" height="4" rx="1" ry="1"/>"#, &Affine::scale(2.0, 2.0)).as_deref(), Some("width=20 height=8 rx=2 ry=2"));
        assert_eq!(through(r#"<rect width="10" height="4"/>"#, &Affine::scale(2.0, 3.0)).as_deref(), Some("width=20 height=12"), "no radius, none added");
        let fine = Geometry::Circle { c: Vec2::new(1.00049, 2.0), r: 0.33333 };
        assert_eq!(fine.rounded(&P), Geometry::Circle { c: Vec2::new(1.0, 2.0), r: 0.333 });
    }
}
