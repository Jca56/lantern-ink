//! The corpus (`tests/corpus`, ARCHITECTURE §10) drawn by Ink and by
//! `lntrn-svg`, which is what Lantern's apps draw an icon with: on what
//! both draw, the two have to agree (§5.4, D22).

use std::path::PathBuf;

use ink_doc::{DocId, Document, Kind, Viewport};
use ink_render::{View, render};
use lntrn_image::Image;

/// Every corpus file: its name and its text.
fn corpus() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "svg"))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()))))
        .collect();
    files.sort();
    assert!(files.len() >= 144, "the corpus is {} files", files.len());
    files
}

/// `text` as an icon `size` px square, drawn by Ink.
fn ink(text: &str, size: u32) -> Image {
    let doc = Document::parse(DocId(1), text).unwrap();
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    render(&doc, &View::icon(&viewport, size)).unwrap()
}

/// How two pictures of one size differ, in levels of 255, their colours
/// weighed by their alpha (a nearly clear pixel's colour hardly shows).
struct Diff {
    /// The average over every channel of every pixel.
    mean: f64,
    /// The worst channel of the worst pixel.
    worst: f64,
    /// The share of pixels with a channel more than 16 levels out.
    far: f64,
}

fn diff(a: &Image, b: &Image) -> Diff {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut sum, mut worst, mut far) = (0.0f64, 0.0f64, 0usize);
    for (p, q) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
        let (pa, qa) = (p[3] as f64 / 255.0, q[3] as f64 / 255.0);
        let mut pixel = (p[3] as f64 - q[3] as f64).abs();
        sum += pixel;
        for i in 0..3 {
            let d = (p[i] as f64 * pa - q[i] as f64 * qa).abs();
            sum += d;
            pixel = pixel.max(d);
        }
        worst = worst.max(pixel);
        far += usize::from(pixel > 16.0);
    }
    let n = (a.width * a.height) as f64;
    Diff { mean: sum / (n * 4.0), worst, far: far as f64 / n }
}

/// Whether `text` has in it what Ink draws and `lntrn-svg` doesn't:
/// `<style>` rules, and filters that are more than drop shadows. On
/// those files the two can't agree; `rsvg-convert` is what Ink is read
/// against there (`third_opinion`).
fn past_lntrn_svg(text: &str) -> bool {
    let doc = Document::parse(DocId(1), text).unwrap();
    doc.descendants(doc.root()).into_iter().filter_map(|id| doc.get(id)).any(|n| n.kind == Kind::Style || (n.kind == Kind::FilterPrimitive && !matches!(n.local(), "feDropShadow")))
}

/// Ink and `lntrn-svg` agree on every file both draw all of (D22), as
/// closely as two
/// renderers that anti-alias differently can: `lntrn-svg` samples 16
/// heights in each pixel row, Ink takes exact areas, so an edge pixel
/// can differ by a few levels while the picture as a whole doesn't. The
/// limits are what was measured on 2026-10-06 (the worst file's mean
/// was 0.95 levels at 64 px and 0.30 at 256 px), with room to spare.
///
/// Where the two still part ways, by a few pixels: dashes round a curve
/// (Ink measures along the true curve) and shadows thrown in from past
/// the picture's edge (`lntrn-svg`, like `rsvg-convert`, has nothing
/// there to throw).
///
/// Nine files are left out: the ones with blurs, glows and `<style>`
/// rules, which Ink draws since M3b and `lntrn-svg` doesn't. Against
/// `rsvg-convert` (2026-10-06, 256 px) Ink's eight with blurs are 0.27
/// to 0.61 levels apart on average, where `lntrn-svg`'s are 0.63 to 9.3.
#[test]
fn ink_and_lntrn_svg_agree_on_every_file() {
    let (beyond, both): (Vec<_>, Vec<_>) = corpus().into_iter().partition(|(_, text)| past_lntrn_svg(text));
    assert_eq!(beyond.len(), 9, "the files lntrn-svg can't draw all of: {:?}", beyond.iter().map(|(name, _)| name).collect::<Vec<_>>());
    for (size, mean, far) in [(64, 1.25, 0.04), (256, 0.5, 0.015)] {
        for (name, text) in &both {
            let d = diff(&ink(text, size), &lntrn_svg::render(text, size).unwrap());
            assert!(d.mean <= mean, "{name} at {size} px: {:.3} levels apart on average", d.mean);
            assert!(d.far <= far, "{name} at {size} px: {:.2} % of its pixels are over 16 levels apart", d.far * 100.0);
        }
    }
}

/// How Ink and `lntrn-svg` differ on every file, worst first:
/// `cargo test -p ink-render --test corpus report -- --ignored --nocapture`.
#[test]
#[ignore]
fn report() {
    for size in [64, 256] {
        let mut rows: Vec<(f64, String)> = corpus()
            .iter()
            .map(|(name, text)| {
                let d = diff(&ink(text, size), &lntrn_svg::render(text, size).unwrap());
                (d.mean, format!("{:8.3} {:6.1} {:7.3}%  {name}", d.mean, d.worst, d.far * 100.0))
            })
            .collect();
        rows.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("--- {size} px: mean, worst, share over 16 levels ---");
        for (_, row) in &rows {
            println!("{row}");
        }
    }
}

/// `text` drawn by `rsvg-convert` as the page at `scale`, if it's on
/// this machine.
fn rsvg(name: &str, scale: f64) -> Option<Image> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(name);
    let out = std::process::Command::new("rsvg-convert").arg("--zoom").arg(scale.to_string()).arg("--format").arg("png").arg(&path).output().ok()?;
    lntrn_image::png::decode(&out.stdout).ok()
}

/// Ink and `lntrn-svg` each against `rsvg-convert`:
/// `cargo test -p ink-render --test corpus third_opinion -- --ignored --nocapture`.
#[test]
#[ignore]
fn third_opinion() {
    let mut rows = Vec::new();
    for (name, text) in corpus() {
        let doc = Document::parse(DocId(1), &text).unwrap();
        let viewport = Viewport::of(doc.node(doc.root()).unwrap());
        let scale = 256.0 / viewport.size.x.max(viewport.size.y);
        let ours = render(&doc, &View::page(&viewport, scale)).unwrap();
        let Some(theirs) = rsvg(&name, scale) else { continue };
        if (ours.width, ours.height) != (theirs.width, theirs.height) {
            rows.push((f64::MAX, format!("   sizes differ: ours {}x{}, rsvg {}x{}  {name}", ours.width, ours.height, theirs.width, theirs.height)));
            continue;
        }
        // lntrn-svg centres the page in a square: only a page that
        // fills the square lines up with rsvg's picture of it.
        let a = diff(&ours, &theirs);
        let square = (viewport.size.x * scale - 256.0).abs() < 1e-6 && (viewport.size.y * scale - 256.0).abs() < 1e-6;
        let lantern = if square {
            let b = diff(&lntrn_svg::render(&text, 256).unwrap(), &theirs);
            format!("{:7.3} {:6.1} {:6.2}%", b.mean, b.worst, b.far * 100.0)
        } else {
            format!("{:>23}", "(page isn't square)")
        };
        rows.push((a.mean, format!("{:7.3} {:6.1} {:6.2}% | {lantern}  {name}", a.mean, a.worst, a.far * 100.0)));
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("--- ink vs rsvg (mean, worst, far) | lntrn-svg vs rsvg ---");
    for (_, row) in &rows {
        println!("{row}");
    }
}

/// The pixels of one file (`INK_FILE`) where Ink is furthest from
/// `rsvg-convert`, with what each renderer has there:
/// `INK_FILE=studio--lasso.svg cargo test -p ink-render --test corpus inspect -- --ignored --nocapture`.
#[test]
#[ignore]
fn inspect() {
    let Ok(wanted) = std::env::var("INK_FILE") else { return };
    let (name, text) = corpus().into_iter().find(|(n, _)| *n == wanted).expect("no such corpus file");
    let doc = Document::parse(DocId(1), &text).unwrap();
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    let scale = 256.0 / viewport.size.x.max(viewport.size.y);
    let ours = render(&doc, &View::page(&viewport, scale)).unwrap();
    let theirs = rsvg(&name, scale).expect("rsvg-convert");
    let square = lntrn_svg::render(&text, 256).unwrap();
    let (ox, oy) = ((256 - ours.width as usize) / 2, (256 - ours.height as usize) / 2);
    let w = ours.width as usize;
    let mut worst: Vec<(i32, usize, usize)> = Vec::new();
    for y in 0..ours.height as usize {
        for x in 0..w {
            let (p, q) = (&ours.rgba[(y * w + x) * 4..][..4], &theirs.rgba[(y * w + x) * 4..][..4]);
            let d = (0..4).map(|i| (p[i] as i32 * if i < 3 { p[3] as i32 } else { 255 } / 255 - q[i] as i32 * if i < 3 { q[3] as i32 } else { 255 } / 255).abs()).max().unwrap();
            worst.push((d, x, y));
        }
    }
    worst.sort_by_key(|w| std::cmp::Reverse(w.0));
    println!("{name}: page {:?}, scale {scale:.4}; user units = px / scale", viewport.size);
    for (d, x, y) in worst.iter().take(24) {
        let at = |img: &Image, x: usize, y: usize| img.rgba[(y * img.width as usize + x) * 4..][..4].to_vec();
        println!("{d:4} at ({x:3},{y:3}) user ({:7.2},{:7.2})  ink {:?}  rsvg {:?}  lntrn {:?}", *x as f64 / scale, *y as f64 / scale, at(&ours, *x, *y), at(&theirs, *x, *y), at(&square, x + ox, y + oy));
    }
}
