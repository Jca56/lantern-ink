//! Pixels: a band of the frame's rows, and the layers being drawn into.
//! Drawing goes to the top of a stack of layers: a group that fades,
//! clips or casts a shadow as one draws into a layer of its own, laid
//! over the one below when it's done. Pixels are premultiplied floats
//! until the picture is finished. What a shape covers of each pixel
//! comes from [`crate::coverage`], exact, so the pixels are the same
//! however the frame is cut into bands.

use ink_geom::Vec2;

use crate::coverage::{Scratch, Shape, cover};
use crate::paint::Paint;

/// Premultiplied RGBA, each 0..1, sRGB-encoded.
pub(crate) type Pixel = [f32; 4];

/// A band of the frame's rows, and the layers being drawn into.
pub(crate) struct Canvas {
    /// The frame's width, px.
    pub w: usize,
    /// The first of the rows held, and how many.
    pub y0: usize,
    pub h: usize,
    /// Bottom to top; drawing goes to the top one.
    layers: Vec<Vec<Pixel>>,
    scratch: Scratch,
}

impl Canvas {
    /// The rows `y0..y0 + h` of a frame `w` px wide, clear.
    pub fn new(w: usize, y0: usize, h: usize) -> Canvas {
        Canvas { w, y0, h, layers: vec![vec![[0.0; 4]; w * h]], scratch: Scratch::new(w) }
    }

    /// Fill `shape` with `paint`, `alpha` multiplied in, over the top
    /// layer. `origin` is where the frame's corner is in the px `paint`
    /// is fitted to.
    pub fn fill(&mut self, shape: &Shape, paint: &Paint, alpha: f32, origin: Vec2) {
        let alpha = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.0 };
        if alpha <= 0.0 {
            return;
        }
        let Canvas { w, y0, h, layers, scratch } = self;
        let Some(top) = layers.last_mut() else { return };
        cover(scratch, *w, *y0, *h, shape, |r, first, coverage| {
            let y = (*y0 + r) as f64 + 0.5 + origin.y;
            for (k, (px, &c)) in top[r * *w + first..].iter_mut().zip(coverage).enumerate() {
                if c <= 0.0 {
                    continue;
                }
                let color = paint.at((first + k) as f64 + 0.5 + origin.x, y);
                let a = (c * alpha * color[3]).clamp(0.0, 1.0);
                for i in 0..3 {
                    px[i] = color[i].clamp(0.0, 1.0) * a + px[i] * (1.0 - a);
                }
                px[3] = a + px[3] * (1.0 - a);
            }
        });
    }

    /// How much of each of the band's pixels `shape` covers, 0..1.
    pub fn mask(&mut self, shape: &Shape) -> Vec<f32> {
        let Canvas { w, y0, h, scratch, .. } = self;
        let mut mask = vec![0.0f32; *w * *h];
        cover(scratch, *w, *y0, *h, shape, |r, first, coverage| mask[r * *w + first..r * *w + first + coverage.len()].copy_from_slice(coverage));
        mask
    }

    /// Start a clear layer for what's drawn next.
    pub fn push_layer(&mut self) {
        self.layers.push(vec![[0.0; 4]; self.w * self.h]);
    }

    /// Take the top layer off. The first one stays.
    pub fn pop_layer(&mut self) -> Vec<Pixel> {
        if self.layers.len() > 1 { self.layers.pop().unwrap_or_default() } else { Vec::new() }
    }

    /// Lay `layer` over the top one at `opacity`.
    pub fn composite(&mut self, layer: &[Pixel], opacity: f32) {
        let opacity = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 0.0 };
        let Some(top) = self.layers.last_mut() else { return };
        for (dst, src) in top.iter_mut().zip(layer) {
            let a = src[3] * opacity;
            if a <= 0.0 {
                continue;
            }
            for i in 0..4 {
                dst[i] = src[i] * opacity + dst[i] * (1.0 - a);
            }
        }
    }

    /// The band's pixels: its first layer.
    pub fn finish(mut self) -> Vec<Pixel> {
        if self.layers.is_empty() { Vec::new() } else { self.layers.swap_remove(0) }
    }
}

/// Premultiplied float pixels as straight-alpha bytes, onto `out`.
pub(crate) fn to_bytes(pixels: &[Pixel], out: &mut Vec<u8>) {
    for px in pixels {
        let a = px[3].clamp(0.0, 1.0);
        let un = if a > 0.0 { 255.0 / a } else { 0.0 };
        out.extend_from_slice(&[(px[0] * un).round().clamp(0.0, 255.0) as u8, (px[1] * un).round().clamp(0.0, 255.0) as u8, (px[2] * un).round().clamp(0.0, 255.0) as u8, (a * 255.0).round() as u8]);
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use ink_geom::{Cap, FillRule, Join, Path, Polyline, Stroke, stroke};

    use super::*;

    const WHITE: Paint = Paint::Solid([1.0, 1.0, 1.0, 1.0]);

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn rect_poly(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Vec2> {
        vec![v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)]
    }

    /// Each pixel's alpha, 0..1, of `polys` filled white into `w` × `h`.
    fn alpha(w: usize, h: usize, polys: &[Vec<Vec2>], rule: FillRule) -> Vec<f64> {
        let mut c = Canvas::new(w, 0, h);
        if let Some(shape) = Shape::new(polys, rule, w, h) {
            c.fill(&shape, &WHITE, 1.0, Vec2::ZERO);
        }
        c.finish().iter().map(|p| p[3] as f64).collect()
    }

    fn area(w: usize, h: usize, polys: &[Vec<Vec2>]) -> f64 {
        alpha(w, h, polys, FillRule::NonZero).iter().sum()
    }

    fn circle(c: Vec2, r: f64) -> Vec<Vec2> {
        Path::ellipse(c, r, r).flatten(0.05).remove(0).points
    }

    #[test]
    fn a_rectangles_coverage_is_exact_in_every_pixel() {
        let got = alpha(8, 8, &[rect_poly(1.25, 2.5, 5.75, 6.5)], FillRule::NonZero);
        for y in 0..8 {
            for x in 0..8 {
                let span = |a: f64, b: f64, lo: f64| (b.min(lo + 1.0) - a.max(lo)).max(0.0);
                let want = span(1.25, 5.75, x as f64) * span(2.5, 6.5, y as f64);
                assert!((got[y * 8 + x] - want).abs() <= 1e-6, "({x},{y}): {} vs {want}", got[y * 8 + x]);
            }
        }
    }

    #[test]
    fn a_triangle_and_a_circle_have_their_areas() {
        let tri = vec![v(0.3, 0.7), v(37.6, 4.2), v(13.3, 41.9)];
        let exact = (tri[1] - tri[0]).perp_dot(tri[2] - tri[0]).abs() / 2.0;
        assert!((area(48, 48, &[tri]) - exact).abs() < 1e-3, "triangle");
        let disc = area(48, 48, &[circle(v(24.0, 24.0), 20.0)]);
        assert!((disc / (PI * 400.0) - 1.0).abs() < 1e-5, "circle: {disc}");
    }

    #[test]
    fn fill_rules_differ_where_outlines_overlap() {
        let nested = vec![rect_poly(0.0, 0.0, 10.0, 10.0), rect_poly(3.0, 3.0, 7.0, 7.0)];
        let at = |a: &[f64]| a[5 * 10 + 5];
        assert_eq!(at(&alpha(10, 10, &nested, FillRule::NonZero)), 1.0, "nonzero: filled");
        assert_eq!(at(&alpha(10, 10, &nested, FillRule::EvenOdd)), 0.0, "even-odd: a hole");
        let mut reversed = rect_poly(3.0, 3.0, 7.0, 7.0);
        reversed.reverse();
        let holed = alpha(10, 10, &[rect_poly(0.0, 0.0, 10.0, 10.0), reversed], FillRule::NonZero);
        assert_eq!((at(&holed), holed[1]), (0.0, 1.0), "nonzero: an opposite inner outline cuts a hole");
    }

    #[test]
    fn outlines_reaching_past_the_frame_still_fill_it() {
        // Past the left and the top: every pixel up to 10.5 is inside.
        let a = alpha(20, 10, &[rect_poly(-600.0, -550.0, 10.5, 850.0)], FillRule::NonZero);
        assert!((0..10).all(|y| a[y * 20..y * 20 + 10].iter().all(|&c| c == 1.0) && (a[y * 20 + 10] - 0.5).abs() < 1e-6 && a[y * 20 + 11] == 0.0));
        // A sliver entirely right of the frame, or below it, adds nothing.
        assert!(Shape::new(&[rect_poly(0.0, 10.0, 30.0, 20.0)], FillRule::NonZero, 10, 10).is_none());
        assert_eq!(area(10, 10, &[rect_poly(10.0, 0.0, 30.0, 10.0)]), 0.0);
        // Points that aren't numbers make a polygon nothing.
        assert!(Shape::new(&[vec![v(f64::NAN, 1.0), v(6.0, 2.0), v(6.0, f64::INFINITY)]], FillRule::NonZero, 10, 10).is_none());
    }

    #[test]
    fn banding_never_changes_a_pixel() {
        let (w, h) = (61, 70);
        let curve = Path::parse("M2 5 C60 -20 -10 90 50 60").path.flatten(0.05);
        let pen = Stroke { width: 3.5, join: Join::Round, cap: Cap::Round, ..Stroke::default() };
        let shapes = [
            (Shape::new(&[Path::ellipse(v(30.0, 40.0), 25.0, 13.0).flatten(0.05).remove(0).points], FillRule::NonZero, w, h).unwrap(), Paint::Solid([1.0, 1.0, 1.0, 1.0])),
            (Shape::new(&[vec![v(0.0, 70.0), v(40.0, 0.0), v(55.0, 71.0), v(10.0, 30.0)]], FillRule::EvenOdd, w, h).unwrap(), Paint::Solid([1.0, 0.2, 0.1, 0.6])),
            (Shape::new(&stroke(&curve, &pen, 0.05), FillRule::NonZero, w, h).unwrap(), Paint::Solid([0.0, 0.0, 1.0, 1.0])),
        ];
        let banded = |band: usize| -> Vec<Pixel> {
            let mut out = Vec::new();
            for y0 in (0..h).step_by(band) {
                let mut c = Canvas::new(w, y0, band.min(h - y0));
                for (shape, paint) in &shapes {
                    c.fill(shape, paint, 1.0, Vec2::ZERO);
                }
                out.extend(c.finish());
            }
            out
        };
        let reference = banded(h);
        for band in [1, 7, 32, 69] {
            assert!(banded(band) == reference, "{band}-row bands");
        }
    }

    #[test]
    fn later_fills_lie_over_earlier_ones() {
        let mut c = Canvas::new(4, 0, 1);
        c.fill(&Shape::new(&[rect_poly(0.0, 0.0, 3.0, 1.0)], FillRule::NonZero, 4, 1).unwrap(), &Paint::Solid([1.0, 0.0, 0.0, 1.0]), 1.0, Vec2::ZERO);
        c.fill(&Shape::new(&[rect_poly(1.0, 0.0, 4.0, 1.0)], FillRule::NonZero, 4, 1).unwrap(), &Paint::Solid([0.0, 0.0, 1.0, 1.0]), 0.5, Vec2::ZERO);
        let mut px = Vec::new();
        to_bytes(&c.finish(), &mut px);
        assert_eq!(&px[0..4], &[255, 0, 0, 255], "red alone");
        assert_eq!(&px[4..8], &[128, 0, 128, 255], "half blue over red");
        assert_eq!(&px[12..16], &[0, 0, 255, 128], "half blue alone");
    }

    /// The stroke of a line 2 wide, its area.
    fn stroke_area(points: &[Vec2], closed: bool, join: Join, cap: Cap, miter_limit: f64) -> f64 {
        let pen = Stroke { width: 2.0, join, cap, miter_limit, ..Stroke::default() };
        area(16, 16, &stroke(&[Polyline { points: points.to_vec(), closed }], &pen, 0.05))
    }

    #[test]
    fn caps_and_joins_add_what_they_should() {
        let near = |got: f64, want: f64| assert!((got - want).abs() < 1e-3, "{got} vs {want}");
        let line = [v(2.0, 5.0), v(10.0, 5.0)];
        near(stroke_area(&line, false, Join::Bevel, Cap::Butt, 4.0), 16.0);
        near(stroke_area(&line, false, Join::Bevel, Cap::Square, 4.0), 20.0);
        near(stroke_area(&line, false, Join::Bevel, Cap::Round, 4.0), 16.0 + PI);
        // A right angle, 2 wide: two 8×2 arms overlapping by 1×1, plus
        // the outside corner's 1×1 square: all of it (miter), half
        // (bevel), or a quarter disc (round).
        let corner = [v(2.0, 2.0), v(10.0, 2.0), v(10.0, 10.0)];
        near(stroke_area(&corner, false, Join::Miter, Cap::Butt, 4.0), 32.0);
        near(stroke_area(&corner, false, Join::Bevel, Cap::Butt, 4.0), 31.5);
        near(stroke_area(&corner, false, Join::Round, Cap::Butt, 4.0), 31.0 + PI / 4.0);
        // A 90° miter is √2 widths long: a limit under that bevels it.
        near(stroke_area(&corner, false, Join::Miter, Cap::Butt, 1.4), 31.5);
        // A closed square outline: the ring between 1..11 and 3..9.
        near(stroke_area(&[v(2.0, 2.0), v(10.0, 2.0), v(10.0, 10.0), v(2.0, 10.0)], true, Join::Miter, Cap::Butt, 4.0), 64.0);
    }

    #[test]
    fn pieces_that_overlap_cover_what_their_union_does() {
        // Two squares over the same half of a pixel: half covered, not
        // all of it.
        let half = rect_poly(2.0, 2.0, 2.5, 3.0);
        assert_eq!(alpha(4, 4, &[half.clone(), half.clone()], FillRule::NonZero)[2 * 4 + 2], 0.5);
        assert_eq!(alpha(4, 4, &[half.clone(), half], FillRule::EvenOdd)[2 * 4 + 2], 0.0, "even-odd: twice is none");
        // Two that cross within a pixel: the union of two triangles.
        let (down, up) = (vec![v(0.0, 0.0), v(1.0, 0.0), v(0.0, 1.0)], vec![v(0.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)]);
        assert!((alpha(2, 2, &[down, up], FillRule::NonZero)[0] - 0.75).abs() < 1e-6);
        // A bow tie's two wings wind opposite ways: both are inside.
        let bow = vec![v(0.0, 0.0), v(4.0, 4.0), v(4.0, 0.0), v(0.0, 4.0)];
        assert!((area(4, 4, std::slice::from_ref(&bow)) - 8.0).abs() < 1e-6);
        // A ring drawn as a round-joined stroke of a circle: the pieces
        // overlap all the way round its inside, and it still has a
        // ring's area, no more.
        let ring = Path::ellipse(v(32.0, 32.0), 20.0, 20.0).flatten(0.05);
        let pen = Stroke { width: 6.0, join: Join::Round, ..Stroke::default() };
        let got = area(64, 64, &stroke(&ring, &pen, 0.05));
        let want = PI * (23.0 * 23.0 - 17.0 * 17.0);
        assert!((got / want - 1.0).abs() < 2e-3, "{got} vs {want}");
    }

    #[test]
    fn a_stroke_has_no_seams_where_its_pieces_meet() {
        let points: Vec<Vec2> = (0..=10).map(|x| v(x as f64 + 2.0, 5.0)).collect();
        let pen = Stroke { width: 2.0, join: Join::Round, ..Stroke::default() };
        let a = alpha(16, 10, &stroke(&[Polyline { points, closed: false }], &pen, 0.05), FillRule::NonZero);
        for y in [4, 5] {
            assert!((2..12).all(|x| a[y * 16 + x] == 1.0), "row {y}: {:?}", &a[y * 16..y * 16 + 16]);
        }
    }

    #[test]
    fn layers_fade_as_one_and_masks_say_what_is_covered() {
        // Two overlapping opaque squares in a layer at half opacity: the
        // overlap is no stronger than the rest.
        let mut c = Canvas::new(8, 0, 4);
        c.push_layer();
        c.fill(&Shape::new(&[rect_poly(0.0, 0.0, 5.0, 4.0)], FillRule::NonZero, 8, 4).unwrap(), &Paint::Solid([1.0, 0.0, 0.0, 1.0]), 1.0, Vec2::ZERO);
        c.fill(&Shape::new(&[rect_poly(3.0, 0.0, 8.0, 4.0)], FillRule::NonZero, 8, 4).unwrap(), &Paint::Solid([0.0, 0.0, 1.0, 1.0]), 1.0, Vec2::ZERO);
        let layer = c.pop_layer();
        c.composite(&layer, 0.5);
        let m = c.mask(&Shape::new(&[rect_poly(1.0, 1.0, 2.5, 3.0)], FillRule::NonZero, 8, 4).unwrap());
        assert_eq!((m[8 + 1], m[8 + 2], m[0], m[8 + 3], m[3 * 8 + 1]), (1.0, 0.5, 0.0, 0.0, 0.0));
        let mut px = Vec::new();
        to_bytes(&c.finish(), &mut px);
        assert_eq!(&px[4..8], &[255, 0, 0, 128]);
        assert_eq!(&px[16..20], &[0, 0, 255, 128], "the top square alone shows where they overlap");
        // The first layer is never taken off.
        let mut c = Canvas::new(2, 0, 2);
        assert!(c.pop_layer().is_empty());
        assert_eq!(c.finish().len(), 4);
    }
}
