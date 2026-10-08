//! How fast, in release:
//! `cargo test --release -p ink-render --test speed -- --ignored --nocapture`
//! (`speed`: big pictures drawn whole; `tiles`: the window's canvas;
//! `drags`: what a step of a drag costs there).

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

/// What one step of a drag costs on a 4K canvas (ARCHITECTURE §8): the
/// drawing laid out again, held against what shows, and the tiles the
/// step touched drawn again. For the thing at the bottom of the stack
/// (an icon's body, usually), the one at the top, and one deep inside.
#[test]
#[ignore]
fn drags() {
    use ink_doc::{Command, Kind, NodeId};
    use ink_geom::Affine;
    use ink_render::Plan;

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    let (view_w, view_h) = (3000.0, 1800.0);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).saturating_sub(1).max(1) as f64;
    let mut rows = Vec::new();
    for path in &files {
        let mut doc = Document::parse(DocId(1), &std::fs::read_to_string(path).unwrap()).unwrap();
        doc.adopt();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        let zoom = (view_w * 0.9 / viewport.size.x).min(view_h * 0.9 / viewport.size.y);
        let lay = |doc: &Document| Plan::new(doc, &Affine::scale(zoom, zoom), false);
        let before = lay(&doc);
        let side: u32 = match before.reach() {
            0..=64 => 256,
            65..=256 => 512,
            _ => 1024,
        };
        let s = f64::from(side);
        let (x0, y0) = ((viewport.size.x * zoom - view_w) / 2.0, (viewport.size.y * zoom - view_h) / 2.0);
        let view = ink_geom::Rect::from_xywh(x0, y0, view_w, view_h);
        let seen: Vec<(i64, i64)> = ((y0 / s).floor() as i64..(y0 + view_h).div_euclid(s) as i64 + 1).flat_map(|j| ((x0 / s).floor() as i64..(x0 + view_w).div_euclid(s) as i64 + 1).map(move |i| (i, j))).collect();
        let painted = seen.iter().filter(|(i, j)| before.bounds().is_some_and(|b| b.intersects(&ink_geom::Rect::from_xywh(*i as f64 * s, *j as f64 * s, s, s)))).count();
        let draws = |id: NodeId| doc.get(id).is_some_and(|n| n.kind.is_shape() || n.kind.is_group() || n.kind == Kind::Text);
        let top: Vec<NodeId> = doc.node(doc.root()).unwrap().elements().filter(|&id| draws(id)).collect();
        let deep = doc.descendants(doc.root()).into_iter().rev().find(|&id| doc.get(id).is_some_and(|n| n.kind.is_shape()) && !doc.ancestors(id).any(|n| matches!(n.kind, Kind::Defs | Kind::ClipPath | Kind::Mask | Kind::Pattern | Kind::Marker | Kind::Symbol)));
        let step = viewport.view.x.max(1e-6) / 96.0;
        for (which, id) in [("bottom", top.first().copied()), ("top", top.last().copied()), ("deep", deep)] {
            let Some(id) = id else { continue };
            let mut moved = doc.clone();
            let t = Instant::now();
            if moved.apply(&Command::Transform { nodes: vec![id], by: Affine::translate(step, step * 0.5) }).is_err() {
                continue;
            }
            let applied = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let after = lay(&moved);
            let laid = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let changed = after.changed_from(&before);
            let held = t.elapsed().as_secs_f64() * 1000.0;
            let touched: Vec<&(i64, i64)> = seen.iter().filter(|(i, j)| changed.iter().any(|c| c.intersects(&ink_geom::Rect::from_xywh(*i as f64 * s, *j as f64 * s, s, s)) && c.intersects(&view))).collect();
            let (mut all, mut worst) = (0f64, 0f64);
            for (i, j) in &touched {
                let one = Instant::now();
                after.part(i * i64::from(side), j * i64::from(side), side, side).unwrap();
                let ms = one.elapsed().as_secs_f64() * 1000.0;
                all += ms;
                worst = worst.max(ms);
            }
            // On the pool: its tiles side by side, none faster than the
            // slowest of them.
            let pool = applied + laid + held + worst.max(all / threads);
            rows.push((pool, applied, laid, held, touched.len(), painted, all, worst, which, path.file_name().unwrap().to_string_lossy().into_owned()));
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("{:>8} {:>8} {:>8} {:>8} {:>11} {:>9} {:>9}  what", "step ms", "apply", "laid", "held", "tiles", "1 core", "worst");
    for (pool, applied, laid, held, touched, painted, all, worst, which, name) in rows.iter().take(40) {
        println!("{pool:>8.1} {applied:>8.2} {laid:>8.2} {held:>8.2} {:>11} {all:>9.1} {worst:>9.1}  {name}, the {which}", format!("{touched} of {painted}"));
    }
    let at = |share: f64| rows[((rows.len() - 1) as f64 * (1.0 - share)) as usize].0;
    let within = |ms: f64| 100.0 * rows.iter().filter(|r| r.0 <= ms).count() as f64 / rows.len() as f64;
    let kept: f64 = rows.iter().map(|r| 1.0 - r.4 as f64 / r.5.max(1) as f64).sum::<f64>() / rows.len() as f64;
    println!("{} drags on {threads} threads: a step is {:.1} ms at the median, {:.1} at nine in ten, {:.1} at worst; {:.0} % within a frame (16.7 ms), {:.0} % within 50 ms; {:.0} % of the tiles kept on average", rows.len(), at(0.5), at(0.9), rows[0].0, within(16.7), within(50.0), 100.0 * kept);
}
