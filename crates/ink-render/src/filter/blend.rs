//! One pixel with another: the blending a filter's stages are made of,
//! in linear light or as the colours are written.

use ink_doc::filter::{Curve, Operator};

use crate::raster::Pixel;

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// A premultiplied sRGB pixel in linear light, still premultiplied.
pub(super) fn light(p: Pixel) -> Pixel {
    if p[3] <= 0.0 {
        return [0.0; 4];
    }
    [to_linear(p[0] / p[3]) * p[3], to_linear(p[1] / p[3]) * p[3], to_linear(p[2] / p[3]) * p[3], p[3]]
}

/// A premultiplied pixel in linear light, sRGB-encoded again.
pub(super) fn encoded(p: Pixel) -> Pixel {
    if p[3] <= 0.0 {
        return [0.0; 4];
    }
    let ch = |v: f32| to_srgb((v / p[3]).clamp(0.0, 1.0)) * p[3];
    [ch(p[0]), ch(p[1]), ch(p[2]), p[3]]
}

/// `top` over `under` (both premultiplied sRGB), blended in linear light
/// when `linear`, as filters are unless they say otherwise.
pub(super) fn over(top: Pixel, under: Pixel, linear: bool) -> Pixel {
    if top[3] >= 1.0 {
        return top;
    }
    if top[3] <= 0.0 {
        return under;
    }
    if !linear {
        return [0, 1, 2, 3].map(|i| top[i] + under[i] * (1.0 - top[3]));
    }
    let (t, u) = (light(top), light(under));
    let a = t[3] + u[3] * (1.0 - t[3]);
    let ch = |i: usize| to_srgb(((t[i] + u[i] * (1.0 - t[3])) / a).clamp(0.0, 1.0)) * a;
    [ch(0), ch(1), ch(2), a]
}

/// `top` put with `under` as `op` says (both premultiplied sRGB).
pub(super) fn composite(top: Pixel, under: Pixel, op: Operator, linear: bool) -> Pixel {
    if op == Operator::Over {
        return over(top, under, linear);
    }
    let (t, u) = if linear { (light(top), light(under)) } else { (top, under) };
    let mixed: Pixel = match op {
        Operator::Over | Operator::In => t.map(|v| v * u[3]),
        Operator::Out => t.map(|v| v * (1.0 - u[3])),
        Operator::Atop => [0, 1, 2, 3].map(|i| t[i] * u[3] + u[i] * (1.0 - t[3])),
        Operator::Xor => [0, 1, 2, 3].map(|i| t[i] * (1.0 - u[3]) + u[i] * (1.0 - t[3])),
        Operator::Arithmetic([k1, k2, k3, k4]) => {
            let a = [0, 1, 2, 3].map(|i| (k1 as f32 * t[i] * u[i] + k2 as f32 * t[i] + k3 as f32 * u[i] + k4 as f32).clamp(0.0, 1.0));
            // A colour can't be more than its own alpha lets show.
            [a[0].min(a[3]), a[1].min(a[3]), a[2].min(a[3]), a[3]]
        }
    };
    if linear { encoded(mixed) } else { mixed }
}

/// One channel's value through its curve.
pub(super) fn curved(v: f32, curve: &Curve) -> f32 {
    let v = v as f64;
    let out = match curve {
        Curve::Identity => v,
        Curve::Linear { slope, intercept } => slope * v + intercept,
        Curve::Gamma { amplitude, exponent, offset } => amplitude * v.powf(*exponent) + offset,
        Curve::Table(values) if values.len() >= 2 => {
            let at = v.clamp(0.0, 1.0) * (values.len() - 1) as f64;
            let k = (at.floor() as usize).min(values.len() - 2);
            values[k] + (values[k + 1] - values[k]) * (at - k as f64)
        }
        Curve::Table(values) => values.first().copied().unwrap_or(v),
        Curve::Discrete(values) => values.get(((v.clamp(0.0, 1.0) * values.len() as f64) as usize).min(values.len().saturating_sub(1))).copied().unwrap_or(v),
    };
    if out.is_finite() { out.clamp(0.0, 1.0) as f32 } else { 0.0 }
}

/// A premultiplied sRGB pixel with each channel through its curve
/// (red, green, blue, alpha): on the colour itself, not on what its
/// alpha lets show.
pub(super) fn transfer(p: Pixel, curves: &[Curve; 4], linear: bool) -> Pixel {
    let straight = |i: usize| if p[3] > 0.0 { (p[i] / p[3]).clamp(0.0, 1.0) } else { 0.0 };
    let a = curved(p[3], &curves[3]);
    let ch = |i: usize| {
        let v = curved(if linear { to_linear(straight(i)) } else { straight(i) }, &curves[i]);
        (if linear { to_srgb(v) } else { v }) * a
    };
    [ch(0), ch(1), ch(2), a]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pixel, b: Pixel) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 2e-3)
    }

    const RED: Pixel = [1.0, 0.0, 0.0, 1.0];
    const CLEAR: Pixel = [0.0; 4];
    /// Half-covering green, premultiplied.
    const HALF: Pixel = [0.0, 0.5, 0.0, 0.5];

    #[test]
    fn part_clear_pixels_blend_in_linear_light() {
        // Half white over black: mid grey in linear light is 188, not 128.
        let out = over([0.5, 0.5, 0.5, 0.5], [0.0, 0.0, 0.0, 1.0], true);
        assert!((out[0] * 255.0 - 188.0).abs() < 1.0 && out[3] == 1.0, "{out:?}");
        assert_eq!(over([0.0; 4], [0.1, 0.2, 0.3, 0.4], true), [0.1, 0.2, 0.3, 0.4]);
        // As the colours are written, when the filter says so.
        assert_eq!(over([0.5, 0.5, 0.5, 0.5], [0.0, 0.0, 0.0, 1.0], false), [0.5, 0.5, 0.5, 1.0]);
    }


    #[test]
    fn two_pictures_are_put_together_as_the_operator_says() {
        let both = |op: Operator, linear: bool| composite(HALF, RED, op, linear);
        assert!(close(both(Operator::In, false), HALF), "all of the top, where the other is");
        assert!(close(both(Operator::Out, false), CLEAR));
        assert!(close(composite(RED, HALF, Operator::Out, false), [0.5, 0.0, 0.0, 0.5]), "the top where the other isn't");
        assert!(close(both(Operator::Atop, false), [0.5, 0.5, 0.0, 1.0]));
        assert!(close(both(Operator::Xor, false), [0.5, 0.0, 0.0, 0.5]));
        assert!(close(both(Operator::Over, false), [0.5, 0.5, 0.0, 1.0]));
        // In linear light, half green over red is lighter than halves
        // of each as they're written.
        let lit = both(Operator::Over, true);
        assert!(lit[3] == 1.0 && lit[0] > 0.7 && lit[1] > 0.7, "{lit:?}");
        // k1·a·b + k2·a + k3·b + k4, kept to what an alpha can hold.
        assert!(close(both(Operator::Arithmetic([0.0, 1.0, 0.5, 0.0]), false), [0.5, 0.5, 0.0, 1.0]));
        assert!(close(both(Operator::Arithmetic([0.0, 0.0, 0.0, 0.25]), false), [0.25; 4]));
        assert!(close(both(Operator::Arithmetic([0.0, 2.0, 0.0, -0.5]), false), [0.0, 0.5, 0.0, 0.5]), "a colour no more than its alpha");
    }

    #[test]
    fn a_channel_goes_through_its_curve() {
        let through = |v: f32, curve: Curve| curved(v, &curve);
        assert_eq!(through(0.3, Curve::Identity), 0.3);
        assert!((through(0.5, Curve::Linear { slope: 0.5, intercept: 0.25 }) - 0.5).abs() < 1e-6);
        assert!((through(0.5, Curve::Gamma { amplitude: 2.0, exponent: 2.0, offset: 0.1 }) - 0.6).abs() < 1e-6);
        assert_eq!(through(2.0, Curve::Linear { slope: 3.0, intercept: 0.0 }), 1.0, "kept within what a channel holds");
        // A table joins its values by straight lines; a discrete one
        // goes in steps.
        let table = || vec![0.0, 1.0, 0.5];
        assert!((through(0.25, Curve::Table(table())) - 0.5).abs() < 1e-6 && (through(0.75, Curve::Table(table())) - 0.75).abs() < 1e-6);
        assert_eq!((through(0.0, Curve::Table(table())), through(1.0, Curve::Table(table()))), (0.0, 0.5));
        assert_eq!((through(0.2, Curve::Discrete(table())), through(0.5, Curve::Discrete(table())), through(1.0, Curve::Discrete(table()))), (0.0, 1.0, 0.5));
        assert_eq!(through(0.4, Curve::Table(vec![0.7])), 0.7);
        // On the colour itself, not on what its alpha lets show.
        let curves = [Curve::Linear { slope: 0.0, intercept: 1.0 }, Curve::Identity, Curve::Identity, Curve::Linear { slope: 0.5, intercept: 0.0 }];
        assert!(close(transfer(HALF, &curves, false), [0.25, 0.25, 0.0, 0.25]));
        assert!(close(transfer(CLEAR, &curves, true), CLEAR));
    }
}
