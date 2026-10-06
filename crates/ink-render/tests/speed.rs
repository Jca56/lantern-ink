//! How fast, in release:
//! `cargo test --release -p ink-render --test speed -- --ignored --nocapture`.

use std::fmt::Write;
use std::time::Instant;

use ink_doc::{DocId, Document, Viewport};
use ink_render::{View, render};

/// 4096² of 200 half-clear ellipses over a big one, a fat round stroke
/// of 50 curves, and both (LS3's speed test, as an SVG).
#[test]
#[ignore]
fn speed() {
    let mut x = 7u32;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x % 4096) as f64
    };
    let mut fills = String::from(r##"<ellipse cx="2048" cy="2048" rx="2000" ry="1500" fill="#fff"/>"##);
    for _ in 0..200 {
        let (cx, cy, r) = (next(), next(), next() / 8.0 + 10.0);
        write!(fills, r##"<ellipse cx="{cx}" cy="{cy}" rx="{r}" ry="{}" fill="#3380e6" fill-opacity="0.5"/>"##, r * 0.6).unwrap();
    }
    let mut stroke = String::from(r##"<path fill="none" stroke="#f00" stroke-width="24" stroke-linejoin="round" stroke-linecap="round" d="M100 100"##);
    for i in 0..50 {
        write!(stroke, " C{} {} {} {} {} {}", next(), next(), next(), next(), 100.0 + i as f64 * 70.0, 3900.0 - i as f64 * 70.0).unwrap();
    }
    stroke.push_str(r#""/>"#);
    for (what, inner) in [("201 ellipses", fills.clone()), ("a fat stroke of 50 curves", stroke.clone()), ("both", format!("{fills}{stroke}"))] {
        let doc = Document::parse(DocId(1), &format!(r#"<svg viewBox="0 0 4096 4096">{inner}</svg>"#)).unwrap();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        let t = Instant::now();
        let image = render(&doc, &View::page(&viewport, 1.0)).unwrap();
        println!("4096² of {what}: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(image.rgba.len(), 4096 * 4096 * 4);
    }
    // And an icon, as one is drawn over and over.
    let icon = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/goldens/kitchen-sink.svg")).unwrap();
    let doc = Document::parse(DocId(2), &icon).unwrap();
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    for size in [32, 256, 1024] {
        let t = Instant::now();
        for _ in 0..20 {
            render(&doc, &View::icon(&viewport, size)).unwrap();
        }
        println!("the kitchen sink at {size} px: {:.2} ms", t.elapsed().as_secs_f64() * 1000.0 / 20.0);
    }
}
