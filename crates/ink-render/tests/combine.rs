//! Boolean operations draw what they should (ARCHITECTURE §3.4, §10).
//! `ink-geom` checks each result against the shapes point by point and
//! to the last digit of its area; here the renderer has its say, which
//! knows nothing of how the results were made. Every pair of shapes is
//! drawn alone, and one clipped by the other; every result is drawn;
//! and the pictures have to add up, pixel by pixel:
//!
//! - intersect is the first shape clipped by the second,
//! - union is both, less that,
//! - subtract is the first, less that,
//! - exclude is both, less that twice.
//!
//! Where both outlines cross one pixel, a clip (two coverages
//! multiplied) is only near what the pixel truly holds: a quarter of a
//! pixel out at the worst, and twice that for exclude. A result that
//! had a piece too many or too few would be a whole pixel out, and more
//! than one of them.

#[path = "../../ink-geom/tests/shapes/mod.rs"]
mod shapes;

use ink_doc::{DocId, Document, Viewport};
use ink_geom::{Combine, FillRule, Path, combine};
use ink_render::{View, render};
use shapes::{Rng, pair};

/// Px to a unit: the shapes' half-unit grid falls on pixel edges, so
/// level and upright sides cover whole pixels.
const SCALE: f64 = 4.0;

fn rule(rule: FillRule) -> &'static str {
    match rule {
        FillRule::NonZero => "nonzero",
        FillRule::EvenOdd => "evenodd",
    }
}

/// How much of each pixel `body` covers, drawn in a 24-unit drawing.
fn drawn(body: &str) -> Vec<f64> {
    let text = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{body}</svg>"#);
    let doc = Document::parse(DocId(1), &text).unwrap();
    let view = View::page(&Viewport::of(doc.node(doc.root()).unwrap()), SCALE);
    render(&doc, &view).unwrap().rgba.chunks_exact(4).map(|p| p[3] as f64 / 255.0).collect()
}

fn shape(path: &Path, by: FillRule) -> String {
    format!(r#"<path d="{}" fill-rule="{}"/>"#, path.to_data(9), rule(by))
}

/// How far two pictures are apart: at their worst pixel, and on
/// average, both as a share of a pixel.
fn apart(a: &[f64], b: impl Iterator<Item = f64>) -> (f64, f64) {
    let (worst, sum) = a.iter().zip(b).fold((0.0f64, 0.0), |(worst, sum), (x, y)| (worst.max((x - y).abs()), sum + (x - y).abs()));
    (worst, sum / a.len() as f64)
}

/// The limits are what was measured on 2026-10-07, with room to spare:
/// see the test's own report (`-- --nocapture`).
#[test]
fn every_result_draws_as_the_renderer_says_it_should() {
    let mut rng = Rng(20261007);
    let (mut worst, mut mean) = ([0.0f64; 4], [0.0f64; 4]);
    const PAIRS: usize = 2000;
    for round in 0..PAIRS {
        let (a, b) = pair(&mut rng);
        let pick = |rng: &mut Rng| if rng.below(5) == 0 { FillRule::EvenOdd } else { FillRule::NonZero };
        let (rule_a, rule_b) = (pick(&mut rng), pick(&mut rng));
        let (alone_a, alone_b) = (drawn(&shape(&a, rule_a)), drawn(&shape(&b, rule_b)));
        let clipped = drawn(&format!(r#"<clipPath id="c"><path d="{}" clip-rule="{}"/></clipPath><path d="{}" fill-rule="{}" clip-path="url(#c)"/>"#, b.to_data(9), rule(rule_b), a.to_data(9), rule(rule_a)));
        let both = alone_a.iter().zip(&alone_b).zip(&clipped);
        let wanted: [Vec<f64>; 4] = [
            both.clone().map(|((a, b), i)| a + b - i).collect(),
            both.clone().map(|((a, _), i)| a - i).collect(),
            clipped.clone(),
            both.clone().map(|((a, b), i)| a + b - 2.0 * i).collect(),
        ];
        for (n, op) in [Combine::Union, Combine::Subtract, Combine::Intersect, Combine::Exclude].into_iter().enumerate() {
            let made = combine(&[(&a, rule_a), (&b, rule_b)], op, 0.0).unwrap();
            let picture = drawn(&shape(&made, FillRule::NonZero));
            let (far, average) = apart(&picture, wanted[n].iter().copied());
            let limit = if op == Combine::Exclude { 0.52 } else { 0.27 };
            assert!(far <= limit, "round {round}, {op:?}: {far:.3} of a pixel out at worst, {average:.5} on average\n  a ({rule_a:?}): {}\n  b ({rule_b:?}): {}\n  made: {}", a.to_data(6), b.to_data(6), made.to_data(6));
            // And it draws the same whichever rule fills it.
            let other = drawn(&shape(&made, FillRule::EvenOdd));
            assert!(apart(&picture, other.into_iter()).0 <= 0.005, "round {round}, {op:?}: it fills differently by the other rule: {}", made.to_data(6));
            (worst[n], mean[n]) = (worst[n].max(far), mean[n] + average / PAIRS as f64);
        }
    }
    println!("union, subtract, intersect, exclude over {PAIRS} pairs: worst pixel {worst:.3?} of a pixel out, {mean:.5?} on average");
}
