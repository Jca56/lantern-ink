//! A stroke's outline draws what the stroke drew (ARCHITECTURE §3.4,
//! §10). The renderer strokes a line its own way (flattened, a polygon
//! for each piece of it); `ink-geom` outlines the same stroke as one
//! shape with its curves kept. Filled, that shape has to be the same
//! picture: over shapes of every kind, open and closed, with every cap
//! and join, wide and fine, dashed and whole.
//!
//! Which of the two is right, where they differ, isn't asked here:
//! `ink-geom/tests/outline.rs` holds the outline to the stroke's own
//! definition, place by place, and it is exact. What's left between
//! the pictures is the renderer's: where a wide line is cut off square
//! on a tight bend (a butt cap, a dash's end), its boxes along the
//! flattened line stand a little past the cut on the inside of the
//! bend, or short of it. A fifth of a pixel for an icon's strokes;
//! three quarters of one for a stroke six times as wide as its bend.

#[path = "../../ink-geom/tests/shapes/mod.rs"]
mod shapes;

use ink_doc::{DocId, Document, Viewport};
use ink_geom::{Cap, Join, Path, Stroke, outline_stroke};
use ink_render::{View, render};
use shapes::{Rng, line, pen};

const SCALE: f64 = 6.0;

/// How much of each pixel `body` covers, drawn in a 24-unit drawing.
fn drawn(body: &str) -> Vec<f64> {
    let text = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">{body}</svg>"#);
    let doc = Document::parse(DocId(1), &text).unwrap();
    let view = View::page(&Viewport::of(doc.node(doc.root()).unwrap()), SCALE);
    render(&doc, &view).unwrap().rgba.chunks_exact(4).map(|p| p[3] as f64 / 255.0).collect()
}

fn stroked(path: &Path, st: &Stroke) -> String {
    let cap = match st.cap {
        Cap::Butt => "butt",
        Cap::Round => "round",
        Cap::Square => "square",
    };
    let join = match st.join {
        Join::Miter => "miter",
        Join::Round => "round",
        Join::Bevel => "bevel",
    };
    let dashes = if st.dashes.is_empty() { "none".to_owned() } else { st.dashes.iter().map(f64::to_string).collect::<Vec<_>>().join(" ") };
    format!(r#"<path d="{}" fill="none" stroke="black" stroke-width="{}" stroke-linecap="{cap}" stroke-linejoin="{join}" stroke-miterlimit="{}" stroke-dasharray="{dashes}" stroke-dashoffset="{}"/>"#, path.to_data(9), st.width, st.miter_limit, st.dash_offset)
}

/// The limits are what was measured on 2026-10-07, with room to spare:
/// see the test's own report (`-- --nocapture`).
#[test]
fn a_strokes_outline_draws_what_the_stroke_drew() {
    let mut rng = Rng(20261007);
    let (mut worst, mut mean) = ((0.0f64, 0.0f64), (0.0f64, 0.0f64));
    const LINES: usize = 1500;
    for round in 0..LINES {
        // The line as its file would have it: the renderer reads it
        // from one, and a dash that ends on the line's very end to
        // the last digit must do so for both.
        let (path, st) = (Path::parse(&line(&mut rng).to_data(9)).path, pen(&mut rng));
        let made = outline_stroke(&path, &st, 0.000125, 0.0005).unwrap_or_else(|why| panic!("round {round}: the outline of {} with {st:?} couldn't be worked out: {}", path.to_data(9), why.0));
        let (theirs, ours) = (drawn(&stroked(&path, &st)), drawn(&format!(r#"<path d="{}"/>"#, made.to_data(9))));
        let (far, sum) = theirs.iter().zip(&ours).fold((0.0f64, 0.0), |(far, sum), (a, b)| (far.max((a - b).abs()), sum + (a - b).abs()));
        let average = sum / theirs.len() as f64;
        // Where a line is cut off square on a bend (a butt cap, a
        // dash's end), the renderer's stroke is a little out: see
        // above. Everywhere else the two are one picture.
        let cut_square = st.cap == Cap::Butt || !st.dashes.is_empty();
        let limit = if cut_square { 0.85 } else { 0.1 };
        assert!(far <= limit && average <= 0.004, "round {round}: {far:.3} of a pixel out at worst, {average:.5} on average\n  line: {}\n  pen: {st:?}\n  made: {}", path.to_data(6), made.to_data(6));
        if cut_square {
            (worst.1, mean.1) = (worst.1.max(far), mean.1 + average / LINES as f64);
        } else {
            (worst.0, mean.0) = (worst.0.max(far), mean.0 + average / LINES as f64);
        }
    }
    println!("{LINES} strokes: worst pixel {:.3} of a pixel out ({:.3} where a line is cut off square), {:.6} on average ({:.6})", worst.0, worst.1, mean.0, mean.1);
}

/// Every stroke in the corpus, made a shape by the Command that does
/// it: each file draws as it did. (Real strokes, drawn with: round
/// caps on curves, dashes round circles, strokes over fills, in groups
/// that fade.)
#[test]
fn every_stroke_in_the_corpus_outlined_draws_as_it_did() {
    use ink_doc::{Command, Kind, NodeId, style::prop};

    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    let (mut outlined, mut passed_over, mut worst, mut worst_mean) = (0, 0, (0.0f64, String::new()), 0.0f64);
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let before = Document::parse(DocId(1), &std::fs::read_to_string(&file).unwrap()).unwrap();
        let mut after = before.clone();
        let shapes: Vec<NodeId> = before.descendants(before.root()).into_iter().filter(|id| before.get(*id).is_some_and(|n| matches!(n.kind, Kind::Path | Kind::Rect | Kind::Circle | Kind::Ellipse | Kind::Line | Kind::Polyline | Kind::Polygon))).collect();
        for id in shapes {
            // What's measured against a shape's box is measured against
            // another box once the shape is its stroke's outline (a
            // stroke isn't in a box; an outline is): a gradient across
            // it, and the reach of a filter on it or on a group it's
            // in. Not the same picture, and not this test's to say.
            let chain = || std::iter::once(before.get(id).unwrap()).chain(before.ancestors(id));
            let by_box = chain().find_map(|n| prop(n, "stroke")).is_some_and(|paint| paint.trim_start().starts_with("url(")) || chain().any(|n| prop(n, "filter").is_some() || prop(n, "mask").is_some());
            if by_box {
                passed_over += 1;
                continue;
            }
            match after.apply(&Command::OutlineStroke { nodes: vec![id], tolerance: None }) {
                Ok(_) => outlined += 1,
                Err(e) => {
                    let why = e.to_string();
                    assert!(why.contains("has no stroke to outline") || why.contains("covers nothing") || why.contains("can't all be read"), "{name} {id}: {why}");
                }
            }
        }
        let view = View::icon(&Viewport::of(before.node(before.root()).unwrap()), 192);
        let (theirs, ours) = (render(&before, &view).unwrap(), render(&after, &view).unwrap());
        // As it looks: each channel weighed by its alpha.
        let apart = |a: &[u8], b: &[u8]| (0..3).map(|k| (a[k] as f64 * a[3] as f64 - b[k] as f64 * b[3] as f64).abs() / 65025.0).fold((a[3] as f64 - b[3] as f64).abs() / 255.0, f64::max);
        let (far, sum) = theirs.rgba.chunks_exact(4).zip(ours.rgba.chunks_exact(4)).fold((0.0f64, 0.0), |(far, sum), (a, b)| (far.max(apart(a, b)), sum + apart(a, b)));
        let mean = sum / (192.0 * 192.0);
        // What was measured on 2026-10-07, with room to spare: the
        // worst pixel 0.047 out, the worst file 0.0004 on average.
        assert!(far <= 0.12 && mean <= 0.001, "{name}: {far:.3} out at worst, {mean:.5} on average");
        if far > worst.0 {
            worst = (far, name);
        }
        worst_mean = worst_mean.max(mean);
    }
    println!("{outlined} strokes outlined ({passed_over} passed over): the worst pixel {:.3} out ({}), the worst file {worst_mean:.5} on average", worst.0, worst.1);
    assert!(outlined > 300, "{outlined} strokes");
}
