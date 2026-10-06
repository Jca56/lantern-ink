//! How a drawing sits on its page: the size its root asks for, and how
//! its `viewBox` goes into that, as `preserveAspectRatio` says.

use ink_geom::number::parse_list;
use ink_geom::{Affine, Vec2};

use crate::length::Length;
use crate::node::Node;

/// The side of a drawing that says nothing of its size: an icon's, as
/// Lantern's apps take it.
const UNSIZED: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// The page: the root's `width` and `height` in px, or what its
    /// `viewBox` makes of whichever is missing.
    pub size: Vec2,
    /// The drawing's coordinates (the `viewBox`'s) to the page's px.
    pub to_page: Affine,
    /// The size percentages are of: the `viewBox`'s, else the page's.
    pub view: Vec2,
}

impl Viewport {
    /// The viewport `root` (the `<svg>`) sets up.
    pub fn of(root: &Node) -> Viewport {
        let view_box = root.attr("viewBox").and_then(parse_list).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0).map(|v| (v[0], v[1], v[2], v[3]));
        let dim = |name: &str| match root.attr(name).and_then(Length::parse) {
            Some(Length::Px(v)) if v > 0.0 => Some(v),
            _ => None,
        };
        let (w, h) = (dim("width"), dim("height"));
        let aspect = root.attr("preserveAspectRatio").unwrap_or("xMidYMid").trim();
        // How far along its spare room the drawing sits, each way.
        let align = |min: &str, max: &str| if aspect.contains(min) { 0.0 } else if aspect.contains(max) { 1.0 } else { 0.5 };
        let (ax, ay) = (align("xMin", "xMax"), align("YMin", "YMax"));
        let fit = |view_box: Option<(f64, f64, f64, f64)>| -> Option<Viewport> {
            let (pw, ph) = match (w, h, view_box) {
                (Some(w), Some(h), _) => (w, h),
                (Some(w), None, Some(v)) => (w, w * v.3 / v.2),
                (None, Some(h), Some(v)) => (h * v.2 / v.3, h),
                (None, None, Some(v)) => (v.2, v.3),
                (w, h, None) => (w.unwrap_or(UNSIZED), h.or(w).unwrap_or(UNSIZED)),
            };
            let to_page = match view_box {
                None => Affine::IDENTITY,
                Some((vx, vy, vw, vh)) => {
                    let (sx, sy) = (pw / vw, ph / vh);
                    let (sx, sy) = if aspect.starts_with("none") {
                        (sx, sy)
                    } else if aspect.ends_with("slice") {
                        (sx.max(sy), sx.max(sy))
                    } else {
                        (sx.min(sy), sx.min(sy))
                    };
                    Affine::translate(-vx, -vy).then(&Affine::scale(sx, sy)).then(&Affine::translate((pw - vw * sx) * ax, (ph - vh * sy) * ay))
                }
            };
            let view = view_box.map_or(Vec2::new(pw, ph), |v| Vec2::new(v.2, v.3));
            (to_page.is_finite() && pw.is_finite() && ph.is_finite()).then_some(Viewport { size: Vec2::new(pw, ph), to_page, view })
        };
        // A viewBox too small (or far) to fit without overflowing is
        // ignored like a malformed one.
        view_box.and_then(|v| fit(Some(v))).or_else(|| fit(None)).unwrap_or(Viewport { size: Vec2::splat(UNSIZED), to_page: Affine::IDENTITY, view: Vec2::splat(UNSIZED) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::id::DocId;

    fn viewport(attrs: &str) -> Viewport {
        let d = Document::parse(DocId(1), &format!("<svg {attrs}/>")).unwrap();
        Viewport::of(d.node(d.root()).unwrap())
    }

    fn near(p: Vec2, x: f64, y: f64) -> bool {
        p.distance(Vec2::new(x, y)) < 1e-9
    }

    #[test]
    fn the_page_is_what_the_root_asks_for() {
        let v = viewport(r#"viewBox="0 0 24 24""#);
        assert_eq!((v.size, v.view, v.to_page), (Vec2::splat(24.0), Vec2::splat(24.0), Affine::IDENTITY));
        let v = viewport(r#"width="48" viewBox="10 20 24 12""#);
        assert_eq!(v.size, Vec2::new(48.0, 24.0), "the missing side follows the viewBox's shape");
        assert!(near(v.to_page.apply(Vec2::new(10.0, 20.0)), 0.0, 0.0) && near(v.to_page.apply(Vec2::new(34.0, 32.0)), 48.0, 24.0));
        assert_eq!(viewport(r#"width="72pt" height="1in""#).size, Vec2::new(96.0, 96.0));
        assert_eq!(viewport("").size, Vec2::splat(16.0));
        assert_eq!(viewport(r#"width="40""#).size, Vec2::splat(40.0));
        assert_eq!(viewport(r#"width="100%" height="-5" viewBox="0 0 8 4""#).size, Vec2::new(8.0, 4.0), "sizes that aren't ones are as if not given");
    }

    #[test]
    fn the_view_box_goes_in_as_the_aspect_says() {
        // A wide box in a square page: fitted and centred by default.
        let meet = viewport(r#"width="100" height="100" viewBox="0 0 200 100""#);
        assert!(near(meet.to_page.apply(Vec2::ZERO), 0.0, 25.0) && near(meet.to_page.apply(Vec2::new(200.0, 100.0)), 100.0, 75.0));
        assert_eq!(meet.view, Vec2::new(200.0, 100.0), "percentages are of the viewBox");
        let top = viewport(r#"width="100" height="100" viewBox="0 0 200 100" preserveAspectRatio="xMidYMin""#);
        assert!(near(top.to_page.apply(Vec2::ZERO), 0.0, 0.0));
        let slice = viewport(r#"width="100" height="100" viewBox="0 0 200 100" preserveAspectRatio="xMaxYMid slice""#);
        assert!(near(slice.to_page.apply(Vec2::new(200.0, 100.0)), 100.0, 100.0) && near(slice.to_page.apply(Vec2::ZERO), -100.0, 0.0));
        let none = viewport(r#"width="100" height="100" viewBox="0 0 200 100" preserveAspectRatio="none""#);
        assert!(near(none.to_page.apply(Vec2::new(200.0, 100.0)), 100.0, 100.0) && near(none.to_page.apply(Vec2::ZERO), 0.0, 0.0));
    }

    #[test]
    fn a_view_box_that_cannot_be_is_ignored() {
        for bad in ["0 0 0 10", "0 0 10", "0 0 -5 5", "a b c d"] {
            let v = viewport(&format!(r#"width="8" height="6" viewBox="{bad}""#));
            assert_eq!((v.size, v.to_page, v.view), (Vec2::new(8.0, 6.0), Affine::IDENTITY, Vec2::new(8.0, 6.0)), "{bad}");
        }
        let tiny = viewport(r#"width="8" height="6" viewBox="0 0 1e-320 1e-320""#);
        assert_eq!(tiny.to_page, Affine::IDENTITY, "one too small to scale up to");
    }
}
