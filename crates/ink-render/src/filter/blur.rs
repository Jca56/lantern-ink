//! A plane of values blurred, and moved: what a blur, an offset and a
//! shadow are made of.

use ink_geom::Vec2;

/// How far from a pixel a blur of `sigma` looks, px: what a band needs
/// of the rows beyond it to get the blur right.
pub(super) fn reach(sigma: (f32, f32)) -> f64 {
    3.0 * sigma.0.max(sigma.1) as f64 + 3.0
}

/// Blur `a` (`w` × `h`) along its rows, or down its columns, by a
/// Gaussian of deviation `sigma` px. Past the edges there's nothing.
pub(super) fn blur(a: &mut [f32], w: usize, h: usize, down: bool, sigma: f32) {
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
pub(super) fn shifted(a: &[f32], w: usize, h: usize, by: Vec2) -> Vec<f32> {
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
            let reach = reach((sigma, sigma)) as usize;
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
}
