//! How fast, in release:
//! `cargo test --release -p ink-render --test speed -- --ignored --nocapture`
//! (`speed`: big pictures drawn whole; `tiles`: the window's canvas).

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

/// The window's canvas: every corpus file fitted to a 4K screen's
/// canvas (3000 × 1800 px) and drawn as the window draws it, laid out
/// once and then tile by tile. The times are one core's: the window
/// spreads the tiles over the pool.
#[test]
#[ignore]
fn tiles() {
    use ink_geom::Affine;
    use ink_render::Plan;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    let (view_w, view_h) = (3000.0, 1800.0);
    let mut rows = Vec::new();
    for path in &files {
        let doc = Document::parse(DocId(1), &std::fs::read_to_string(path).unwrap()).unwrap();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        let zoom = (view_w * 0.9 / viewport.size.x).min(view_h * 0.9 / viewport.size.y);
        let t = Instant::now();
        let plan = Plan::new(&doc, &Affine::scale(zoom, zoom), false);
        let laid = t.elapsed().as_secs_f64() * 1000.0;
        let side: u32 = match plan.reach() {
            0..=64 => 256,
            65..=256 => 512,
            _ => 1024,
        };
        // The view, centred on the page; only tiles something is painted in.
        let (x0, y0) = ((viewport.size.x * zoom - view_w) / 2.0, (viewport.size.y * zoom - view_h) / 2.0);
        let (mut drawn, mut worst) = (0u32, 0f64);
        let t = Instant::now();
        let s = f64::from(side);
        for j in (y0 / s).floor() as i64..(y0 + view_h).div_euclid(s) as i64 + 1 {
            for i in (x0 / s).floor() as i64..(x0 + view_w).div_euclid(s) as i64 + 1 {
                let tile = ink_geom::Rect::from_xywh(i as f64 * s, j as f64 * s, s, s);
                if !plan.bounds().is_some_and(|b| b.intersects(&tile)) {
                    continue;
                }
                let one = Instant::now();
                plan.part(i * i64::from(side), j * i64::from(side), side, side).unwrap();
                worst = worst.max(one.elapsed().as_secs_f64() * 1000.0);
                drawn += 1;
            }
        }
        rows.push((t.elapsed().as_secs_f64() * 1000.0, laid, drawn, worst, side, plan.reach(), zoom, path.file_name().unwrap().to_string_lossy().into_owned()));
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("{:>9} {:>8} {:>6} {:>9} {:>5} {:>6} {:>7}  file", "tiles ms", "laid ms", "tiles", "worst ms", "side", "reach", "zoom");
    for (all, laid, drawn, worst, side, reach, zoom, name) in &rows {
        println!("{all:>9.1} {laid:>8.2} {drawn:>6} {worst:>9.1} {side:>5} {reach:>6} {zoom:>7.2}  {name}");
    }
    let total: f64 = rows.iter().map(|r| r.0).sum();
    let median = rows[rows.len() / 2].0;
    println!("{} files: the median {median:.1} ms of one core for a screen of tiles, {:.1} ms on average", rows.len(), total / rows.len() as f64);
}
