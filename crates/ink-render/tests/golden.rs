//! Ink's own pictures, to the byte (ARCHITECTURE §10, D22): a drawing
//! that uses everything the renderer draws, against the picture of it
//! that was looked at and kept. The renderer is pure CPU and has no
//! randomness or clock in it, so on one machine nothing but a change to
//! the renderer changes a byte.
//!
//! After a change that's meant to change the picture, look at the new
//! one and keep it: `INK_BLESS=1 cargo test -p ink-render --test golden`.

use std::path::PathBuf;

use ink_doc::{DocId, Document, Viewport};
use ink_render::{View, render};
use lntrn_image::Image;

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens").join(name)
}

fn check(svg: &str, png: &str, view: impl Fn(&Viewport) -> View) {
    let text = std::fs::read_to_string(golden(svg)).unwrap();
    let doc = Document::parse(DocId(1), &text).unwrap();
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    let got = render(&doc, &view(&viewport)).unwrap();
    if std::env::var_os("INK_BLESS").is_some() {
        std::fs::write(golden(png), lntrn_image::png::encode(&got)).unwrap();
        return;
    }
    let kept: Image = lntrn_image::png::decode(&std::fs::read(golden(png)).unwrap_or_else(|e| panic!("{png}: {e} (make it with INK_BLESS=1)"))).unwrap();
    assert_eq!((got.width, got.height), (kept.width, kept.height), "{png}");
    let differing = got.rgba.chunks_exact(4).zip(kept.rgba.chunks_exact(4)).filter(|(a, b)| a != b).count();
    assert!(differing == 0, "{png}: {differing} pixels differ from the kept picture");
}

#[test]
fn the_kitchen_sink_at_its_own_size() {
    check("kitchen-sink.svg", "kitchen-sink.png", |v| View::page(v, 1.0));
}

#[test]
fn the_kitchen_sink_as_a_small_icon() {
    check("kitchen-sink.svg", "kitchen-sink-32.png", |v| View::icon(v, 32));
}

#[test]
fn the_kitchen_sink_enlarged_and_off_the_grid() {
    check("kitchen-sink.svg", "kitchen-sink-x2.7.png", |v| View::page(v, 2.7));
}

#[test]
fn filters_and_rules_at_their_own_size() {
    check("filters-and-rules.svg", "filters-and-rules.png", |v| View::page(v, 1.0));
}

#[test]
fn filters_and_rules_enlarged_and_off_the_grid() {
    check("filters-and-rules.svg", "filters-and-rules-x2.3.png", |v| View::page(v, 2.3));
}

/// The goldens' drawings as `rsvg-convert` draws them, if it's on this
/// machine: how far Ink's picture of each is from its, and in which
/// squares of the drawing. Read on 2026-10-06, `filters-and-rules.svg`
/// is 0.34 levels apart on average with its last rect's `rotate` taken
/// off, no pixel more than 11 out. With it on, that rect is where the
/// two part ways: `rsvg-convert` keeps a filter's region upright where
/// Ink turns it with its element, as the element's own coordinates say.
/// `cargo test -p ink-render --test golden against_rsvg -- --ignored --nocapture`.
#[test]
#[ignore]
fn against_rsvg() {
    for svg in ["kitchen-sink.svg", "filters-and-rules.svg"] {
        for scale in [1.0, 3.0] {
            let text = std::fs::read_to_string(golden(svg)).unwrap();
            let doc = Document::parse(DocId(1), &text).unwrap();
            let viewport = Viewport::of(doc.node(doc.root()).unwrap());
            let ours = render(&doc, &View::page(&viewport, scale)).unwrap();
            let out = std::process::Command::new("rsvg-convert").arg("--zoom").arg(scale.to_string()).arg("--format").arg("png").arg(golden(svg)).output().expect("rsvg-convert");
            let theirs: Image = lntrn_image::png::decode(&out.stdout).unwrap();
            assert_eq!((ours.width, ours.height), (theirs.width, theirs.height));
            let (mut sum, mut far, mut worst) = (0.0f64, 0usize, (0.0f64, 0usize));
            for (i, (p, q)) in ours.rgba.chunks_exact(4).zip(theirs.rgba.chunks_exact(4)).enumerate() {
                let d = (0..4).map(|c| (p[c] as f64 - q[c] as f64).abs()).fold(0.0, f64::max);
                sum += d;
                far += usize::from(d > 16.0);
                if d > worst.0 {
                    worst = (d, i);
                }
            }
            let n = (ours.width * ours.height) as f64;
            let (x, y) = (worst.1 % ours.width as usize, worst.1 / ours.width as usize);
            // Where they differ, in squares of 20 user units: each
            // square's average, when it's more than a level.
            let (w, side) = (ours.width as usize, (20.0 * scale) as usize);
            for cy in 0..ours.height as usize / side {
                let row: Vec<String> = (0..w / side)
                    .map(|cx| {
                        let mut sum = 0.0;
                        for y in cy * side..(cy + 1) * side {
                            for x in cx * side..(cx + 1) * side {
                                let i = (y * w + x) * 4;
                                sum += (0..4).map(|c| (ours.rgba[i + c] as f64 - theirs.rgba[i + c] as f64).abs()).fold(0.0, f64::max);
                            }
                        }
                        let mean = sum / (side * side) as f64;
                        if mean > 1.0 { format!("{mean:5.1}") } else { "    .".to_owned() }
                    })
                    .collect();
                println!("    {}", row.join(" "));
            }
            println!("{svg} x{scale}: {:.3} levels apart on average, {:.2} % over 16; worst {} at ({x}, {y}): ink {:?} rsvg {:?}", sum / n, far as f64 / n * 100.0, worst.0, &ours.rgba[worst.1 * 4..worst.1 * 4 + 4], &theirs.rgba[worst.1 * 4..worst.1 * 4 + 4]);
        }
    }
}
