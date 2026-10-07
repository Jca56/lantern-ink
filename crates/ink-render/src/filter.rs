//! Filters: a chain of stages run on a layer's pixels (ARCHITECTURE
//! §5.1). Each stage works on the layer as it was drawn, on its shape
//! alone, or on what an earlier stage made, in linear light unless the
//! filter says otherwise. Fitting a filter to an element (its region
//! and its stages, in the picture's px) is here too.

use ink_doc::filter::{Curve, Effect, Filter, Input, Operator};
use ink_doc::gradient::Units;
use ink_doc::length::Length;
use ink_geom::{Affine, Rect, Vec2};

use self::blend::{composite, encoded, light, over, transfer};
use self::blur::{blur, shifted};
use crate::paint::{Rgba, rgba};
use crate::raster::Pixel;

mod blend;
mod blur;

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
        self.offset.x.abs().max(self.offset.y.abs()) + blur::reach(self.sigma)
    }
}

/// One stage of a filter, in the picture's px. `linear`: worked out in
/// linear light.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Stage {
    Blur { of: Input, sigma: (f32, f32), linear: bool },
    Offset { of: Input, by: Vec2, linear: bool },
    Flood { color: Rgba },
    Composite { top: Input, under: Input, op: Operator, linear: bool },
    Merge { of: Vec<Input>, linear: bool },
    Transfer { of: Input, curves: [Curve; 4], linear: bool },
    Shadow { of: Input, shadow: Shadow, linear: bool },
}

impl Stage {
    /// How far from a pixel this stage looks, px.
    fn reach(&self) -> f64 {
        match self {
            Stage::Blur { sigma, .. } => blur::reach(*sigma),
            Stage::Offset { by, .. } => by.x.abs().max(by.y.abs()) + 1.0,
            Stage::Shadow { shadow, .. } => shadow.reach(),
            _ => 0.0,
        }
    }
}

/// A filter fitted to an element.
#[derive(Clone, Debug)]
pub(crate) struct Fitted {
    pub stages: Vec<Stage>,
    /// The region's corners in the picture: nothing shows outside them.
    pub region: [Vec2; 4],
}

impl Fitted {
    /// How far from a pixel the whole chain looks, px: what a band
    /// needs of the rows beyond it to get the filter right.
    pub fn reach(&self) -> f64 {
        self.stages.iter().map(Stage::reach).sum()
    }
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
    let sigma = |std: (f64, f64)| ((std.0 * ux * kx) as f32, (std.1 * uy * ky) as f32);
    let offset = |dx: f64, dy: f64| ctm.linear(Vec2::new(dx * ux, dy * uy));
    let stages = filter
        .steps
        .iter()
        .map(|step| {
            let linear = step.linear;
            match &step.effect {
                Effect::Blur { of, std } => Stage::Blur { of: *of, sigma: sigma(*std), linear },
                Effect::Offset { of, dx, dy } => Stage::Offset { of: *of, by: offset(*dx, *dy), linear },
                Effect::Flood { color } => Stage::Flood { color: rgba(*color) },
                Effect::Composite { top, under, op } => Stage::Composite { top: *top, under: *under, op: *op, linear },
                Effect::Merge { of } => Stage::Merge { of: of.clone(), linear },
                Effect::Transfer { of, curves } => Stage::Transfer { of: *of, curves: curves.clone(), linear },
                Effect::DropShadow { of, dx, dy, std, color } => Stage::Shadow { of: *of, shadow: Shadow { offset: offset(*dx, *dy), sigma: sigma(*std), color: rgba(*color) }, linear },
            }
        })
        .collect();
    Some(Fitted { stages, region })
}

/// Run `stages` on what `layer` (`w` × `h`) holds: it becomes what the
/// last of them made.
pub(crate) fn apply(layer: &mut [Pixel], w: usize, h: usize, stages: &[Stage]) {
    let mut made: Vec<Vec<Pixel>> = Vec::with_capacity(stages.len());
    for stage in stages {
        let pick = |input: &Input| -> Vec<Pixel> {
            match input {
                Input::Graphic => layer.to_vec(),
                Input::Alpha => layer.iter().map(|p| [0.0, 0.0, 0.0, p[3]]).collect(),
                Input::Step(k) => made.get(*k).cloned().unwrap_or_else(|| vec![[0.0; 4]; w * h]),
                Input::Nothing => vec![[0.0; 4]; w * h],
            }
        };
        let out = match stage {
            Stage::Blur { of, sigma, linear } => {
                let mut img = pick(of);
                if sigma.0.is_finite() && sigma.1.is_finite() {
                    planes(&mut img, *linear, |plane| {
                        blur(plane, w, h, false, sigma.0);
                        blur(plane, w, h, true, sigma.1);
                    });
                }
                img
            }
            Stage::Offset { of, by, linear } => {
                let mut img = pick(of);
                if by.is_finite() {
                    // Whole pixels mix nothing: no light to convert.
                    let whole = by.x.fract() == 0.0 && by.y.fract() == 0.0;
                    planes(&mut img, *linear && !whole, |plane| *plane = shifted(plane, w, h, *by));
                }
                img
            }
            Stage::Flood { color } => vec![[color[0] * color[3], color[1] * color[3], color[2] * color[3], color[3]]; w * h],
            Stage::Composite { top, under, op, linear } => pick(top).into_iter().zip(pick(under)).map(|(a, b)| composite(a, b, *op, *linear)).collect(),
            Stage::Merge { of, linear } => of.iter().fold(vec![[0.0; 4]; w * h], |under, input| pick(input).into_iter().zip(under).map(|(a, b)| over(a, b, *linear)).collect()),
            Stage::Transfer { of, curves, linear } => pick(of).into_iter().map(|p| transfer(p, curves, *linear)).collect(),
            Stage::Shadow { of, shadow, linear } => {
                let mut img = pick(of);
                cast(&mut img, w, h, shadow, *linear);
                img
            }
        };
        made.push(out);
    }
    if let Some(last) = made.pop() {
        layer.copy_from_slice(&last);
    }
}

/// Put `s` under what `layer` (`w` × `h`) holds: a blurred, offset,
/// tinted copy of its shape.
pub(crate) fn cast(layer: &mut [Pixel], w: usize, h: usize, s: &Shadow, linear: bool) {
    if !(s.offset.is_finite() && s.sigma.0.is_finite() && s.sigma.1.is_finite()) || s.color[3] <= 0.0 {
        return;
    }
    let mut alpha: Vec<f32> = layer.iter().map(|p| p[3]).collect();
    blur(&mut alpha, w, h, false, s.sigma.0);
    blur(&mut alpha, w, h, true, s.sigma.1);
    let alpha = shifted(&alpha, w, h, s.offset);
    let [r, g, b, a] = s.color;
    for (px, &cover) in layer.iter_mut().zip(&alpha) {
        let k = a * cover;
        if k > 0.0 {
            *px = over(*px, [r * k, g * k, b * k, k], linear);
        }
    }
}

/// Work `f` on each of `img`'s four channels as a plane of its own, in
/// linear light if `linear`.
fn planes(img: &mut [Pixel], linear: bool, mut f: impl FnMut(&mut Vec<f32>)) {
    if linear {
        img.iter_mut().for_each(|p| *p = light(*p));
    }
    for c in 0..4 {
        let mut plane: Vec<f32> = img.iter().map(|p| p[c]).collect();
        f(&mut plane);
        img.iter_mut().zip(&plane).for_each(|(p, v)| p[c] = *v);
    }
    if linear {
        img.iter_mut().for_each(|p| *p = encoded(*p));
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::{DocId, Document, NodeId};

    use super::*;

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
        cast(&mut layer, w, h, &shadow, true);
        assert_eq!(layer[5 * w + 5], [1.0, 0.0, 0.0, 1.0], "the square is as it was");
        assert_eq!(layer[9 * w + 9], [0.0, 0.0, 0.0, 0.6], "its shadow shows past it");
        assert_eq!(layer[3 * w + 3], [0.0; 4]);
        // Blurred, the shadow fades outward.
        let mut soft = vec![[0.0f32; 4]; w * h];
        soft[6 * w + 6] = [1.0, 1.0, 1.0, 1.0];
        cast(&mut soft, w, h, &Shadow { offset: Vec2::ZERO, sigma: (1.0, 1.0), color: [1.0, 0.8, 0.0, 1.0] }, true);
        let (near, far) = (soft[6 * w + 7], soft[6 * w + 9]);
        assert!(near[3] > far[3] && far[3] > 0.0);
        assert!((near[0] / near[3] - 1.0).abs() < 1e-5 && (near[1] / near[3] - 0.8).abs() < 1e-5, "in the flood's colour");
    }

    /// What `stages` make of one row of `pixels`.
    fn run(pixels: &[Pixel], stages: &[Stage]) -> Vec<Pixel> {
        let mut layer = pixels.to_vec();
        apply(&mut layer, pixels.len(), 1, stages);
        layer
    }

    const RED: Pixel = [1.0, 0.0, 0.0, 1.0];
    const CLEAR: Pixel = [0.0; 4];
    /// Half-covering green, premultiplied.
    const HALF: Pixel = [0.0, 0.5, 0.0, 0.5];

    #[test]
    fn a_chain_runs_each_stage_on_what_it_names() {
        let row = [CLEAR, RED, CLEAR, CLEAR];
        // The element's shape, moved one px: where it was is clear.
        let moved = run(&row, &[Stage::Offset { of: Input::Alpha, by: Vec2::new(1.0, 0.0), linear: true }]);
        assert_eq!(moved, [CLEAR, CLEAR, [0.0, 0.0, 0.0, 1.0], CLEAR]);
        // A flood, kept where that moved shape is, under the element.
        let stages = [
            Stage::Offset { of: Input::Alpha, by: Vec2::new(1.0, 0.0), linear: false },
            Stage::Flood { color: [0.0, 0.0, 1.0, 0.5] },
            Stage::Composite { top: Input::Step(1), under: Input::Step(0), op: Operator::In, linear: false },
            Stage::Merge { of: vec![Input::Step(2), Input::Graphic], linear: false },
        ];
        assert_eq!(run(&row, &stages), [CLEAR, RED, [0.0, 0.0, 0.5, 0.5], CLEAR]);
        // What Ink has no picture of is clear; a step that isn't there
        // too.
        assert_eq!(run(&row, &[Stage::Merge { of: vec![Input::Nothing, Input::Step(7)], linear: true }]), [CLEAR; 4]);
        assert_eq!(run(&row, &[]), row, "no stages, no change");
    }

    #[test]
    fn a_blur_spreads_colour_in_linear_light() {
        // White beside black, blurred: the pixel between them is mid
        // grey in light, which is written 188, not 128.
        let (white, black) = ([1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]);
        let row: Vec<Pixel> = (0..40).map(|i| if i < 20 { white } else { black }).collect();
        let grey = |linear: bool| {
            let out = run(&row, &[Stage::Blur { of: Input::Graphic, sigma: (2.0, 0.0), linear }]);
            ((out[19][0] + out[20][0]) * 0.5 * 255.0, out[19][3])
        };
        let ((lit, alpha), (plain, _)) = (grey(true), grey(false));
        assert!((lit - 188.0).abs() < 2.0 && (plain - 127.5).abs() < 1.0 && alpha > 0.999, "{lit} {plain}");
        // It keeps what's there, and looks no further than it says.
        let dot: Vec<Pixel> = (0..41).map(|i| if i == 20 { HALF } else { CLEAR }).collect();
        let stage = Stage::Blur { of: Input::Graphic, sigma: (2.5, 0.0), linear: true };
        let out = run(&dot, std::slice::from_ref(&stage));
        assert!((out.iter().map(|p| p[3]).sum::<f32>() - 0.5).abs() < 1e-4);
        assert!(out.iter().enumerate().all(|(i, p)| p[3] == 0.0 || (i.abs_diff(20) as f64) <= stage.reach()));
        assert!(out.iter().all(|p| p[3] == 0.0 || (p[1] / p[3] - 1.0).abs() < 1e-3), "still green all through");
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
        let shadow = |f: &Fitted| match f.stages.as_slice() {
            [Stage::Shadow { of: Input::Graphic, shadow, linear: true }] => *shadow,
            other => panic!("{other:?}"),
        };
        let s = shadow(&f);
        assert!(near(s.offset, (1.0, 1.6)) && near(Vec2::new(s.sigma.0 as f64, s.sigma.1 as f64), (1.2, 1.2)), "{s:?}");
        assert_eq!(s.color, [0.0, 0.0, 0.0, 0.3]);
        assert!(near(f.region[0], (12.0, 28.0)) && near(f.region[2], (72.0, 140.0)), "{:?}", f.region);
        // The defaults: 2 across, 2 down, 2 of blur, black; a tenth of
        // the box more all round.
        let f = fit(&filter(4), bbox, &Affine::IDENTITY, view).unwrap();
        assert_eq!(shadow(&f), Shadow { offset: Vec2::new(2.0, 2.0), sigma: (2.0, 2.0), color: [0.0, 0.0, 0.0, 1.0] });
        assert!(near(f.region[0], (8.0, 16.0)) && near(f.region[2], (32.0, 64.0)), "{:?}", f.region);
        // Fractions of the box, in a region in user units.
        let f = fit(&filter(6), bbox, &Affine::IDENTITY, view).unwrap();
        assert!(near(shadow(&f).offset, (10.0, 0.0)) && near(Vec2::new(shadow(&f).sigma.0 as f64, shadow(&f).sigma.1 as f64), (2.0, 8.0)));
        assert!(near(f.region[0], (1.0, 2.0)) && near(f.region[2], (31.0, 42.0)));
        // A box with no width has no region to show in.
        assert!(fit(&filter(2), Rect::from_xywh(0.0, 0.0, 0.0, 9.0), &Affine::IDENTITY, view).is_none());
    }
}
