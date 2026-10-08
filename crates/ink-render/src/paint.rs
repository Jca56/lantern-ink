//! What a shape is painted with: one colour, or a gradient fitted to it.
//! Stops blend in sRGB, straight alpha, as SVG says.

use ink_doc::gradient::{Gradient as Said, Spread, Units};
use ink_doc::length::Length;
use ink_geom::{Affine, Rect, Vec2};
use lntrn_math::Color;

/// A colour, straight alpha, each channel 0..1.
pub(crate) type Rgba = [f32; 4];

pub(crate) fn rgba(c: Color) -> Rgba {
    [c.r as f32, c.g as f32, c.b as f32, c.a as f32]
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Shape {
    /// Across the line from `from`, `axis` long.
    Linear { from: Vec2, axis: Vec2 },
    /// Rings growing from the focus `f` out to the circle at `c`.
    Radial { c: Vec2, r: f64, f: Vec2 },
}

/// A gradient fitted to a shape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gradient {
    shape: Shape,
    stops: Vec<(f64, Rgba)>,
    spread: Spread,
    /// The picture's px to the space `shape` is in.
    to_gradient: Affine,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Paint {
    Solid(Rgba),
    Gradient(Gradient),
}

impl Paint {
    /// The gradient `g` as it paints a shape whose box is `bbox` (in its
    /// own coordinates), drawn through `ctm`, in a viewport of `view`
    /// user units. `None` when it paints nothing: no stops, a shape with
    /// no area for a gradient measured against it, a transform that
    /// can't be undone.
    pub fn fit(g: &Said, bbox: Option<Rect>, ctm: &Affine, view: Vec2) -> Option<Paint> {
        let last = rgba(g.stops.last()?.color);
        if g.stops.len() == 1 {
            return Some(Paint::Solid(last));
        }
        // The space its numbers are in, mapped to the user's, and the
        // size a percentage there is of.
        let (to_user, (w, h)) = match g.units {
            Units::BBox => {
                let b = bbox.filter(|b| b.width() > 0.0 && b.height() > 0.0)?;
                (Affine::new(b.width(), 0.0, 0.0, b.height(), b.min.x, b.min.y), (1.0, 1.0))
            }
            Units::UserSpace => (Affine::IDENTITY, (view.x, view.y)),
        };
        // A coordinate: as written, or `default` (a fraction of `whole`).
        let len = |l: Option<Length>, default: f64, whole: f64| match g.units {
            Units::BBox => l.map_or(default, Length::fraction),
            Units::UserSpace => l.map_or(default * whole, |l| l.of(whole)),
        };
        let c = g.coords;
        let shape = if g.radial {
            let centre = Vec2::new(len(c[0], 0.5, w), len(c[1], 0.5, h));
            let r = len(c[2], 0.5, ((w * w + h * h) * 0.5).sqrt());
            if r <= 0.0 || !r.is_finite() {
                return Some(Paint::Solid(last));
            }
            let mut f = Vec2::new(c[3].map_or(centre.x, |l| len(Some(l), 0.0, w)), c[4].map_or(centre.y, |l| len(Some(l), 0.0, h)));
            // A focus outside the circle is brought to just inside it.
            let off = f - centre;
            if off.length() > r * 0.999 {
                f = centre + off * (r * 0.999 / off.length());
            }
            Shape::Radial { c: centre, r, f }
        } else {
            let from = Vec2::new(len(c[0], 0.0, w), len(c[1], 0.0, h));
            let axis = Vec2::new(len(c[2], 1.0, w), len(c[3], 0.0, h)) - from;
            if axis.length_squared() <= 0.0 || !axis.is_finite() {
                return Some(Paint::Solid(last));
            }
            Shape::Linear { from, axis }
        };
        let to_px = g.transform.then(&to_user).then(ctm);
        Some(Paint::Gradient(Gradient { shape, stops: g.stops.iter().map(|s| (s.offset, rgba(s.color))).collect(), spread: g.spread, to_gradient: to_px.inverse()? }))
    }

    /// The colour at the picture's point (x, y).
    pub fn at(&self, x: f64, y: f64) -> Rgba {
        match self {
            Paint::Solid(c) => *c,
            Paint::Gradient(g) => g.at(x, y),
        }
    }
}

impl Gradient {
    fn at(&self, x: f64, y: f64) -> Rgba {
        let p = self.to_gradient.apply(Vec2::new(x, y));
        let t = match self.shape {
            Shape::Linear { from, axis } => (p - from).dot(axis) / axis.length_squared(),
            Shape::Radial { c, r, f } => {
                // The ring through p is centred at f + t (c - f) with
                // radius t r: the larger root of |e - t d|² = t² r².
                let (e, d) = (p - f, c - f);
                let (a, b, cc) = (d.length_squared() - r * r, e.dot(d), e.length_squared());
                (b - (b * b - a * cc).max(0.0).sqrt()) / a
            }
        };
        let t = match self.spread {
            _ if !t.is_finite() => 1.0,
            Spread::Pad => t.clamp(0.0, 1.0),
            Spread::Repeat => t - t.floor(),
            Spread::Reflect => {
                let m = t.rem_euclid(2.0);
                if m > 1.0 { 2.0 - m } else { m }
            }
        };
        let s = &self.stops;
        // The first stop past t; of stops at one offset the later wins.
        let after = s.partition_point(|st| st.0 <= t);
        if after == 0 {
            return s[0].1;
        }
        if after == s.len() {
            return s[after - 1].1;
        }
        let ((o0, c0), (o1, c1)) = (s[after - 1], s[after]);
        let k = ((t - o0) / (o1 - o0)) as f32;
        std::array::from_fn(|i| c0[i] + (c1[i] - c0[i]) * k)
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::refs::Ids;
    use ink_doc::{DocId, Document};

    use super::*;

    /// The gradient with id `g` (or `id`) in `defs`, fitted.
    fn fit(defs: &str, id: &str, bbox: Option<Rect>, ctm: &Affine, view: (f64, f64)) -> Option<Paint> {
        let d = Document::parse(DocId(1), &format!("<svg>{defs}</svg>")).unwrap();
        let ids = Ids::of(&d);
        let said = Said::of(&d, &ids, d.node(ids.get(id).unwrap()).unwrap()).unwrap();
        Paint::fit(&said, bbox, ctm, Vec2::new(view.0, view.1))
    }

    fn near(a: Rgba, b: Rgba) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    const RED: Rgba = [1.0, 0.0, 0.0, 1.0];
    const BLUE: Rgba = [0.0, 0.0, 1.0, 1.0];
    const PURPLE: Rgba = [0.5, 0.0, 0.5, 1.0];

    #[test]
    fn a_linear_gradient_runs_across_the_box_by_default() {
        let defs = r##"<linearGradient id="g"><stop offset="0" stop-color="#f00"/><stop offset="100%" style="stop-color: rgb(0, 0, 255)"/></linearGradient>"##;
        let bbox = Some(Rect::from_xywh(10.0, 0.0, 20.0, 8.0));
        // Drawn at twice the size: the box is the picture's 20..60.
        let p = fit(defs, "g", bbox, &Affine::scale(2.0, 2.0), (100.0, 100.0)).unwrap();
        assert!(near(p.at(20.0, 5.0), RED) && near(p.at(60.0, 5.0), BLUE));
        assert!(near(p.at(40.0, 0.0), PURPLE), "halfway, in sRGB");
        assert!(near(p.at(0.0, 5.0), RED) && near(p.at(90.0, 5.0), BLUE), "padded past its ends");
        // A box with no area can't carry one.
        assert!(fit(defs, "g", Some(Rect::from_xywh(0.0, 5.0, 30.0, 0.0)), &Affine::IDENTITY, (100.0, 100.0)).is_none());
        assert!(fit(defs, "g", None, &Affine::IDENTITY, (100.0, 100.0)).is_none());
    }

    #[test]
    fn user_space_gradients_take_their_transform() {
        // Boxy's way: a vertical axis in user space, turned by a matrix.
        let defs = r##"<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="10" gradientTransform="matrix(0 1 -1 0 0 0)"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>"##;
        // The matrix turns (0, 10) to (-10, 0): the gradient runs left.
        let p = fit(defs, "g", None, &Affine::IDENTITY, (100.0, 100.0)).unwrap();
        assert!(near(p.at(0.0, 50.0), RED) && near(p.at(-10.0, 50.0), BLUE) && near(p.at(-5.0, 0.0), PURPLE));
        // Percentages in user space are of the viewport.
        let defs = r##"<linearGradient id="g" gradientUnits="userSpaceOnUse" x2="50%"><stop stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>"##;
        let p = fit(defs, "g", None, &Affine::IDENTITY, (200.0, 100.0)).unwrap();
        assert!(near(p.at(100.0, 0.0), BLUE) && near(p.at(50.0, 0.0), PURPLE));
    }

    #[test]
    fn a_radial_gradient_rings_out_from_its_focus() {
        let defs = r##"<radialGradient id="g" gradientUnits="userSpaceOnUse" cx="50" cy="50" r="40"><stop offset="0" stop-color="#f00"/><stop offset="1" stop-color="#00f"/></radialGradient><radialGradient id="f" href="#g" fx="70" fy="50"/>"##;
        let p = fit(defs, "g", None, &Affine::IDENTITY, (100.0, 100.0)).unwrap();
        assert!(near(p.at(50.0, 50.0), RED) && near(p.at(90.0, 50.0), BLUE) && near(p.at(50.0, 30.0), PURPLE));
        // With the focus moved right, red starts there, and the rim is
        // still blue all round.
        let p = fit(defs, "f", None, &Affine::IDENTITY, (100.0, 100.0)).unwrap();
        assert!(near(p.at(70.0, 50.0), RED), "{:?}", p.at(70.0, 50.0));
        assert!(near(p.at(90.0, 50.0), BLUE) && near(p.at(10.0, 50.0), BLUE) && near(p.at(50.0, 10.0), BLUE));
        assert!(near(p.at(40.0, 50.0), PURPLE), "halfway from the focus to the far rim");
    }

    #[test]
    fn stops_spread_and_degenerate_gradients() {
        let defs = r##"<linearGradient id="base" gradientUnits="userSpaceOnUse" x2="10" spreadMethod="reflect"><stop offset="0.5" stop-color="#f00"/><stop offset="0.5" stop-color="#00f"/><stop offset="0.2" stop-color="#fff" stop-opacity="0.5"/></linearGradient><linearGradient id="again" xlink:href="#base" spreadMethod="repeat"/><linearGradient id="one"><stop stop-color="#f00"/></linearGradient><linearGradient id="none"/><linearGradient id="point" gradientUnits="userSpaceOnUse" x2="0"><stop stop-color="#f00"/><stop offset="1" stop-color="#00f"/></linearGradient>"##;
        let at = |id: &str, x: f64| fit(defs, id, None, &Affine::IDENTITY, (100.0, 100.0)).unwrap().at(x, 0.0);
        let faint = [1.0, 1.0, 1.0, 0.5];
        // A hard edge at half; the third stop's offset can't go back, so
        // it sits there too, and it's the last one that shows.
        assert!(near(at("base", 4.9), RED) && near(at("base", 5.1), faint));
        assert!(near(at("base", 14.9), faint) && near(at("base", 15.1), RED), "reflected");
        assert!(near(at("again", 14.9), RED) && near(at("again", 15.1), faint), "the href's stops, repeated");
        assert!(matches!(fit(defs, "one", None, &Affine::IDENTITY, (1.0, 1.0)), Some(Paint::Solid(c)) if c == RED), "one stop is one colour");
        assert!(fit(defs, "none", None, &Affine::IDENTITY, (1.0, 1.0)).is_none(), "no stops paint nothing");
        assert!(near(at("point", 3.0), BLUE), "an axis of no length is the last stop");
        // A transform that can't be undone paints nothing.
        assert!(fit(defs, "base", None, &Affine::scale(0.0, 1.0), (1.0, 1.0)).is_none());
    }
}
