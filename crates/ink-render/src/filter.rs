//! Drop shadows: a blurred, offset, tinted copy of a layer's alpha laid
//! under it, in linear light as filters work. Fitting a filter to an
//! element (its region and shadows, in the picture's px) is here too.

use ink_doc::filter::Filter;
use ink_doc::gradient::Units;
use ink_doc::length::Length;
use ink_geom::{Affine, Rect, Vec2};

use crate::paint::{Rgba, rgba};
use crate::raster::Pixel;

/// One shadow, in the picture's px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shadow {
    pub offset: Vec2,
    /// The blur's standard deviation across and down.
    pub sigma: (f32, f32),
    /// Straight alpha.
    pub color: Rgba,
}

impl Shadow {
    /// How far from a pixel this shadow looks, px: what a band needs of
    /// the rows beyond it to get the shadow right.
    pub fn reach(&self) -> f64 {
        self.offset.x.abs().max(self.offset.y.abs()) + 3.0 * self.sigma.0.max(self.sigma.1) as f64 + 3.0
    }
}

/// A filter fitted to an element.
#[derive(Clone, Debug)]
pub(crate) struct Fitted {
    pub shadows: Vec<Shadow>,
    /// The region's corners in the picture: nothing shows outside them.
    pub region: [Vec2; 4],
}

/// `filter` on an element whose box is `bbox` (in its own coordinates),
/// drawn through `ctm`, in a viewport of `view` user units. `None` when
/// its region has no area: nothing of the element shows then.
pub(crate) fn fit(filter: &Filter, bbox: Rect, ctm: &Affine, view: Vec2) -> Option<Fitted> {
    let (bw, bh) = (bbox.width(), bbox.height());
    let [x, y, w, h] = filter.region;
    let (x, y, w, h) = match filter.units {
        Units::BBox => {
            let f = |l: Option<Length>, default: f64| l.map_or(default, Length::fraction);
            (bbox.min.x + f(x, -0.1) * bw, bbox.min.y + f(y, -0.1) * bh, f(w, 1.2) * bw, f(h, 1.2) * bh)
        }
        Units::UserSpace => {
            let f = |l: Option<Length>, default: f64, whole: f64| l.map_or(default * whole, |l| l.of(whole));
            (f(x, -0.1, view.x), f(y, -0.1, view.y), f(w, 1.2, view.x), f(h, 1.2, view.y))
        }
    };
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
        return None;
    }
    let region = [Vec2::new(x, y), Vec2::new(x + w, y), Vec2::new(x + w, y + h), Vec2::new(x, y + h)].map(|p| ctm.apply(p));
    let (ux, uy) = match filter.primitive_units {
        Units::UserSpace => (1.0, 1.0),
        Units::BBox => (bw, bh),
    };
    // How long the transform makes a step across, and a step down.
    let (kx, ky) = (ctm.linear(Vec2::X).length(), ctm.linear(Vec2::Y).length());
    let shadows = filter.shadows.iter().map(|s| Shadow { offset: ctm.linear(Vec2::new(s.dx * ux, s.dy * uy)), sigma: ((s.std.0 * ux * kx) as f32, (s.std.1 * uy * ky) as f32), color: rgba(s.color) }).collect();
    Some(Fitted { shadows, region })
}

/// Put each of `shadows` under what `layer` (`w` × `h`) holds, in turn:
/// the second is the shadow of the first's result.
pub(crate) fn drop_shadows(layer: &mut [Pixel], w: usize, h: usize, shadows: &[Shadow]) {
    for s in shadows {
        if !(s.offset.is_finite() && s.sigma.0.is_finite() && s.sigma.1.is_finite()) || s.color[3] <= 0.0 {
            continue;
        }
        let mut alpha: Vec<f32> = layer.iter().map(|p| p[3]).collect();
        blur(&mut alpha, w, h, false, s.sigma.0);
        blur(&mut alpha, w, h, true, s.sigma.1);
        let alpha = shifted(&alpha, w, h, s.offset);
        let [r, g, b, a] = s.color;
        for (px, &cover) in layer.iter_mut().zip(&alpha) {
            let k = a * cover;
            if k > 0.0 {
                *px = over(*px, [r * k, g * k, b * k, k]);
            }
        }
    }
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// `top` over `under` (both premultiplied sRGB), blended in linear light
/// as filters are.
fn over(top: Pixel, under: Pixel) -> Pixel {
    if top[3] >= 1.0 {
        return top;
    }
    if top[3] <= 0.0 {
        return under;
    }
    let linear = |p: Pixel| [to_linear(p[0] / p[3]) * p[3], to_linear(p[1] / p[3]) * p[3], to_linear(p[2] / p[3]) * p[3], p[3]];
    let (t, u) = (linear(top), linear(under));
    let a = t[3] + u[3] * (1.0 - t[3]);
    let ch = |i: usize| to_srgb(((t[i] + u[i] * (1.0 - t[3])) / a).clamp(0.0, 1.0)) * a;
    [ch(0), ch(1), ch(2), a]
}

/// Blur `a` (`w` × `h`) along its rows, or down its columns, by a
/// Gaussian of deviation `sigma` px. Past the edges there's nothing.
fn blur(a: &mut [f32], w: usize, h: usize, down: bool, sigma: f32) {
    if sigma < 1e-3 {
        return;
    }
    let (lines, len) = if down { (w, h) } else { (h, w) };
    let at = |line: usize, i: usize| if down { i * w + line } else { line * w + i };
    let mut src = vec![0.0f32; len];
    let mut out = vec![0.0f32; len];
    // Under 2 px, the Gaussian itself; from there, three box blurs come
    // within a few percent of it (and cost the same however wide).
    let kernel: Vec<f32> = if sigma < 2.0 {
        let r = (sigma * 3.0).ceil() as i32;
        let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let sum: f32 = k.iter().sum();
        k.iter().map(|v| v / sum).collect()
    } else {
        Vec::new()
    };
    // The box's width: an odd one sits on the pixel three times; an even
    // one sits half a pixel left, half a pixel right, then one wider sits
    // on it.
    let d = ((sigma * 3.0 * (std::f32::consts::TAU).sqrt() / 4.0 + 0.5).floor() as usize).max(1);
    let boxes: [(usize, usize); 3] = if d % 2 == 1 { [(d / 2, d / 2); 3] } else { [(d / 2, d / 2 - 1), (d / 2 - 1, d / 2), (d / 2, d / 2)] };
    let mut sums = vec![0.0f64; len + 1];
    for line in 0..lines {
        for (i, v) in src.iter_mut().enumerate() {
            *v = a[at(line, i)];
        }
        if kernel.is_empty() {
            for (before, after) in boxes {
                for i in 0..len {
                    sums[i + 1] = sums[i] + src[i] as f64;
                }
                for (i, o) in out.iter_mut().enumerate() {
                    let (lo, hi) = (i.saturating_sub(before), (i + after + 1).min(len));
                    *o = ((sums[hi] - sums[lo]) / (before + after + 1) as f64) as f32;
                }
                std::mem::swap(&mut src, &mut out);
            }
        } else {
            let r = kernel.len() / 2;
            for (i, o) in out.iter_mut().enumerate() {
                *o = kernel.iter().enumerate().filter_map(|(k, wt)| (i + k).checked_sub(r).and_then(|j| src.get(j)).map(|v| v * wt)).sum();
            }
            std::mem::swap(&mut src, &mut out);
        }
        for (i, v) in src.iter().enumerate() {
            a[at(line, i)] = *v;
        }
    }
}

/// `a` moved by `by` px, part pixels blended; what moves in from outside
/// is nothing.
fn shifted(a: &[f32], w: usize, h: usize, by: Vec2) -> Vec<f32> {
    let (ix, iy) = (by.x.floor(), by.y.floor());
    let (fx, fy) = ((by.x - ix) as f32, (by.y - iy) as f32);
    let (ix, iy) = (ix as i64, iy as i64);
    let get = |x: i64, y: i64| if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h { a[y as usize * w + x as usize] } else { 0.0 };
    let mut out = vec![0.0f32; a.len()];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x as i64 - ix, y as i64 - iy);
            out[y * w + x] = (get(sx, sy) * (1.0 - fx) + get(sx - 1, sy) * fx) * (1.0 - fy) + (get(sx, sy - 1) * (1.0 - fx) + get(sx - 1, sy - 1) * fx) * fy;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use ink_doc::{DocId, Document, NodeId};

    use super::*;

    fn spread(a: &[f32], w: usize) -> (f32, f32) {
        let total: f32 = a.iter().sum();
        let mean: f32 = a.iter().enumerate().map(|(i, v)| (i % w) as f32 * v).sum::<f32>() / total;
        let var: f32 = a.iter().enumerate().map(|(i, v)| ((i % w) as f32 - mean).powi(2) * v).sum::<f32>() / total;
        (total, var.sqrt())
    }

    #[test]
    fn a_blur_is_a_gaussian_of_the_deviation_asked() {
        for sigma in [0.6f32, 1.5, 2.0, 2.5, 3.0, 7.3] {
            let (w, h) = (101, 3);
            let mut a = vec![0.0f32; w * h];
            a[w + 50] = 1.0;
            blur(&mut a, w, h, false, sigma);
            let (total, dev) = spread(&a[w..2 * w], w);
            assert!((total - 1.0).abs() < 1e-4, "{sigma}: keeps what's there, {total}");
            assert!((dev - sigma).abs() < sigma * 0.12 + 0.02, "{sigma}: spread {dev}");
            assert!((a[w + 50 - 3] - a[w + 50 + 3]).abs() < 1e-6, "{sigma}: even both ways");
            assert_eq!(a[50], 0.0, "{sigma}: rows stay apart");
            // It reaches no further than a band is told it does.
            let reach = Shadow { offset: Vec2::ZERO, sigma: (sigma, sigma), color: [0.0; 4] }.reach() as usize;
            assert!(a[w..2 * w].iter().enumerate().all(|(i, v)| *v == 0.0 || i.abs_diff(50) <= reach), "{sigma}: past its reach");
            // The same down the columns.
            let mut b = vec![0.0f32; 3 * 101];
            b[50 * 3 + 1] = 1.0;
            blur(&mut b, 3, 101, true, sigma);
            assert!((b[47 * 3 + 1] - a[w + 47]).abs() < 1e-6, "{sigma}");
        }
        // No blur, and one far wider than the picture.
        let mut a = vec![0.0, 1.0, 0.0];
        blur(&mut a, 3, 1, false, 0.0);
        assert_eq!(a, vec![0.0, 1.0, 0.0]);
        blur(&mut a, 3, 1, false, 1e9);
        assert!(a.iter().all(|v| v.is_finite() && *v < 1e-6));
    }

    #[test]
    fn a_shift_moves_part_pixels_too() {
        let a = vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(shifted(&a, 3, 3, Vec2::new(1.0, 1.0))[5], 1.0);
        let half = shifted(&a, 3, 3, Vec2::new(0.5, 0.0));
        assert_eq!((half[1], half[2]), (0.5, 0.5));
        let back = shifted(&a, 3, 3, Vec2::new(-1.0, 0.25));
        assert_eq!((back[0], back[3]), (0.75, 0.25));
        assert!(shifted(&a, 3, 3, Vec2::new(1e12, -1e12)).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_shadow_goes_under_and_beside() {
        // An opaque red square in the middle; a black shadow at 60 %, two
        // px right and down, unblurred.
        let (w, h) = (12, 12);
        let mut layer = vec![[0.0f32; 4]; w * h];
        for y in 4..8 {
            for x in 4..8 {
                layer[y * w + x] = [1.0, 0.0, 0.0, 1.0];
            }
        }
        let shadow = Shadow { offset: Vec2::new(2.0, 2.0), sigma: (0.0, 0.0), color: [0.0, 0.0, 0.0, 0.6] };
        drop_shadows(&mut layer, w, h, &[shadow]);
        assert_eq!(layer[5 * w + 5], [1.0, 0.0, 0.0, 1.0], "the square is as it was");
        assert_eq!(layer[9 * w + 9], [0.0, 0.0, 0.0, 0.6], "its shadow shows past it");
        assert_eq!(layer[3 * w + 3], [0.0; 4]);
        // Blurred, the shadow fades outward.
        let mut soft = vec![[0.0f32; 4]; w * h];
        soft[6 * w + 6] = [1.0, 1.0, 1.0, 1.0];
        drop_shadows(&mut soft, w, h, &[Shadow { offset: Vec2::ZERO, sigma: (1.0, 1.0), color: [1.0, 0.8, 0.0, 1.0] }]);
        let (near, far) = (soft[6 * w + 7], soft[6 * w + 9]);
        assert!(near[3] > far[3] && far[3] > 0.0);
        assert!((near[0] / near[3] - 1.0).abs() < 1e-5 && (near[1] / near[3] - 0.8).abs() < 1e-5, "in the flood's colour");
    }

    #[test]
    fn part_clear_pixels_blend_in_linear_light() {
        // Half white over black: mid grey in linear light is 188, not 128.
        let out = over([0.5, 0.5, 0.5, 0.5], [0.0, 0.0, 0.0, 1.0]);
        assert!((out[0] * 255.0 - 188.0).abs() < 1.0 && out[3] == 1.0, "{out:?}");
        assert_eq!(over([0.0; 4], [0.1, 0.2, 0.3, 0.4]), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn a_filter_is_fitted_to_its_element() {
        let d = Document::parse(
            DocId(1),
            r##"<svg><filter x="-20%" y="-15%" width="150%" height="140%"><feDropShadow dx="0.5" dy="0.8" stdDeviation="0.6" flood-color="#000" flood-opacity="0.3"/></filter><filter><feDropShadow/></filter><filter primitiveUnits="objectBoundingBox" filterUnits="userSpaceOnUse" x="1" y="2" width="30" height="40"><feDropShadow dx="0.5" dy="0" stdDeviation="0.1 0.2"/></filter></svg>"##,
        )
        .unwrap();
        let filter = |n: u64| Filter::of(&d, d.node(NodeId(n)).unwrap()).unwrap();
        let near = |a: Vec2, b: (f64, f64)| (a.x - b.0).abs() < 1e-4 && (a.y - b.1).abs() < 1e-4;
        let (bbox, view) = (Rect::from_xywh(10.0, 20.0, 20.0, 40.0), Vec2::new(100.0, 100.0));
        let f = fit(&filter(2), bbox, &Affine::scale(2.0, 2.0), view).unwrap();
        let s = f.shadows[0];
        assert!(near(s.offset, (1.0, 1.6)) && near(Vec2::new(s.sigma.0 as f64, s.sigma.1 as f64), (1.2, 1.2)), "{s:?}");
        assert_eq!((f.shadows.len(), s.color), (1, [0.0, 0.0, 0.0, 0.3]));
        assert!(near(f.region[0], (12.0, 28.0)) && near(f.region[2], (72.0, 140.0)), "{:?}", f.region);
        // The defaults: 2 across, 2 down, 2 of blur, black; a tenth of
        // the box more all round.
        let f = fit(&filter(4), bbox, &Affine::IDENTITY, view).unwrap();
        assert_eq!(f.shadows, vec![Shadow { offset: Vec2::new(2.0, 2.0), sigma: (2.0, 2.0), color: [0.0, 0.0, 0.0, 1.0] }]);
        assert!(near(f.region[0], (8.0, 16.0)) && near(f.region[2], (32.0, 64.0)), "{:?}", f.region);
        // Fractions of the box, in a region in user units.
        let f = fit(&filter(6), bbox, &Affine::IDENTITY, view).unwrap();
        assert!(near(f.shadows[0].offset, (10.0, 0.0)) && near(Vec2::new(f.shadows[0].sigma.0 as f64, f.shadows[0].sigma.1 as f64), (2.0, 8.0)));
        assert!(near(f.region[0], (1.0, 2.0)) && near(f.region[2], (31.0, 42.0)));
        // A box with no width has no region to show in.
        assert!(fit(&filter(2), Rect::from_xywh(0.0, 0.0, 0.0, 9.0), &Affine::IDENTITY, view).is_none());
    }
}
