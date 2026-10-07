//! `Command::Transform` draws what the transform itself would (D13).
//! Every corpus file is put through a move, a turn, a scale, a mirror,
//! a stretch and a skew, and each result is drawn beside the file as it
//! was, seen through the same transform: however the Command wrote it
//! (into the shapes' numbers, into a `rotate`, into a `matrix`), the
//! two pictures are one picture.

use std::path::PathBuf;

use ink_doc::{Command, DocId, Document, Kind, Viewport};
use ink_geom::{Affine, Vec2};
use ink_render::{View, render};
use lntrn_image::Image;

/// The side of the pictures compared, px.
const SIZE: u32 = 128;

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

/// The transforms tried, about the middle of a drawing `view` across:
/// what each is called, and it.
fn transforms(view: Vec2) -> Vec<(&'static str, Affine)> {
    let about = |t: Affine| t.about(view * 0.5);
    vec![
        ("moved", Affine::translate(view.x * 0.1, view.y * -0.07)),
        ("turned", about(Affine::rotate(30f64.to_radians()))),
        ("a quarter turn", about(Affine::rotate(90f64.to_radians()))),
        ("shrunk", about(Affine::scale(0.6, 0.6))),
        ("mirrored", about(Affine::scale(-1.0, 1.0))),
        ("stretched", about(Affine::scale(0.8, 0.5))),
        ("skewed", about(Affine::skew_x(0.3).then(&Affine::rotate(-0.4)).then(&Affine::scale(0.7, 0.7)))),
    ]
}

/// How far two pictures are apart: the average over every channel of
/// every pixel, in levels of 255 (colours weighed by their alpha), and
/// the share of pixels with a channel more than 16 levels out.
fn diff(a: &Image, b: &Image) -> (f64, f64) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut sum, mut far) = (0.0f64, 0usize);
    for (p, q) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
        let (pa, qa) = (p[3] as f64 / 255.0, q[3] as f64 / 255.0);
        let mut pixel = (p[3] as f64 - q[3] as f64).abs();
        sum += pixel;
        for i in 0..3 {
            let d = (p[i] as f64 * pa - q[i] as f64 * qa).abs();
            sum += d;
            pixel = pixel.max(d);
        }
        far += usize::from(pixel > 16.0);
    }
    let n = (a.width * a.height) as f64;
    (sum / (n * 4.0), far as f64 / n)
}

/// `text` put through `by` by the Command, and `text` as it is seen
/// through `by`: how far apart the two are drawn.
fn apart(text: &str, by: &Affine) -> (f64, f64) {
    let original = Document::parse(DocId(1), text).unwrap();
    let mut put_through = original.clone();
    put_through.apply(&Command::Transform { nodes: vec![put_through.root()], by: *by }).unwrap();
    compare(&original, &put_through, by)
}

/// `put_through` as it is, and `original` seen through `by`: how far
/// apart the two are drawn.
fn compare(original: &Document, put_through: &Document, by: &Affine) -> (f64, f64) {
    let viewport = Viewport::of(original.node(original.root()).unwrap());
    // Past the page's edge too: what a move brings in has to be there.
    let view = View { clip_to_page: false, ..View::icon(&viewport, SIZE) };
    // The drawing's coordinates, through `by`, then onto the page as
    // ever.
    let Some(back) = viewport.to_page.inverse() else { return (0.0, 0.0) };
    let seen_through = View { page_to_px: back.then(by).then(&viewport.to_page).then(&view.page_to_px), ..view };
    diff(&render(put_through, &view).unwrap(), &render(original, &seen_through).unwrap())
}

/// The limits are what was measured on 2026-10-06, with room to spare:
/// of the 1008 pictures the worst was 0.08 levels apart on average, and
/// the worst had 0.02 % of its pixels over 16 levels out. The two
/// differ only by what three decimals round away and by where each
/// curve's flattening puts its corners.
///
/// It found two things on the way, both now rules: dashes along a rect
/// start where a rect's outline starts, so a dashed one can't take a
/// turn or a mirror into its numbers (`settle.rs`); and an arc near a
/// half turn is pulled flat by its ends being rounded, so its radii
/// are written as finely as it takes (`ink-geom`'s `data.rs`).
#[test]
fn a_transform_put_into_the_file_draws_what_the_transform_would() {
    for (name, text) in corpus() {
        let doc = Document::parse(DocId(1), &text).unwrap();
        let view = Viewport::of(doc.node(doc.root()).unwrap()).view;
        for (what, by) in transforms(view) {
            let (mean, far) = apart(&text, &by);
            assert!(mean <= 0.25, "{name} {what}: {mean:.3} levels apart on average");
            assert!(far <= 0.002, "{name} {what}: {:.2} % of its pixels are over 16 levels apart", far * 100.0);
        }
    }
}

/// Every group that can be taken away, taken away (`Command::Ungroup`),
/// in every corpus file: the picture is the one it was. A group's
/// transform went to each thing in it, and what they had from it by
/// inheritance is said on them. (The groups left are the ones that
/// hold a filter or a clip path for what's in them.)
#[test]
fn groups_taken_away_leave_the_picture_as_it_was() {
    let (mut gone, mut kept) = (0, 0);
    for (name, text) in corpus() {
        let original = Document::parse(DocId(1), &text).unwrap();
        let mut flat = original.clone();
        // Outermost first, and again for the groups that were in them.
        loop {
            let groups: Vec<_> = flat.descendants(flat.root()).into_iter().filter(|&id| flat.node(id).is_ok_and(|n| n.kind == Kind::G)).collect();
            let before = gone;
            for group in groups {
                gone += usize::from(flat.get(group).is_some() && flat.apply(&Command::Ungroup { nodes: vec![group], drop: false }).is_ok());
            }
            if gone == before {
                kept += flat.descendants(flat.root()).into_iter().filter(|&id| flat.node(id).is_ok_and(|n| n.kind == Kind::G)).count();
                break;
            }
        }
        let (mean, far) = compare(&original, &flat, &Affine::IDENTITY);
        assert!(mean <= 0.25, "{name}: {mean:.3} levels apart on average");
        assert!(far <= 0.002, "{name}: {:.2} % of its pixels are over 16 levels apart", far * 100.0);
    }
    assert!(gone > 100 && kept > 100, "{gone} groups taken away, {kept} left: the corpus has plenty of each");
}

/// How far apart every file's two pictures are, worst first:
/// `cargo test -p ink-render --test settle report -- --ignored --nocapture`.
#[test]
#[ignore]
fn report() {
    let mut rows: Vec<(f64, f64, String)> = Vec::new();
    for (name, text) in corpus() {
        let doc = Document::parse(DocId(1), &text).unwrap();
        let view = Viewport::of(doc.node(doc.root()).unwrap()).view;
        for (what, by) in transforms(view) {
            let (mean, far) = apart(&text, &by);
            rows.push((mean, far, format!("{name} {what}")));
        }
    }
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (mean, far, name) in rows.iter().take(40) {
        println!("{mean:7.3} levels  {:6.2} % far  {name}", far * 100.0);
    }
    let by_far = rows.iter().map(|r| r.1).fold(0.0, f64::max);
    println!("{} pictures; the worst share of far pixels: {:.2} %", rows.len(), by_far * 100.0);
}

/// One file, node by node: each thing in its root alone in the drawing,
/// put through each transform, with what the Command made of it.
/// `INK_FILE=name.svg cargo test -p ink-render --test settle inspect -- --ignored --nocapture`.
#[test]
#[ignore]
fn inspect() {
    let name = std::env::var("INK_FILE").expect("INK_FILE names a corpus file");
    let text = corpus().into_iter().find(|(n, _)| *n == name).expect("no such corpus file").1;
    let whole = Document::parse(DocId(1), &text).unwrap();
    let root = whole.root();
    let view = Viewport::of(whole.node(root).unwrap()).view;
    let drawn: Vec<_> = whole.node(root).unwrap().elements().filter(|&id| whole.node(id).is_ok_and(|n| n.kind.is_shape() || n.kind.is_group())).collect();
    for &keep in &drawn {
        // The drawing with only this one of its root's shapes in it.
        let mut alone = whole.clone();
        alone.apply(&Command::Delete { nodes: drawn.iter().copied().filter(|&id| id != keep).collect() }).unwrap();
        for (what, by) in transforms(view) {
            let mut put_through = alone.clone();
            put_through.apply(&Command::Transform { nodes: vec![keep], by }).unwrap();
            let (mean, far) = compare(&alone, &put_through, &by);
            if mean > 0.02 {
                println!("{mean:7.3} levels  {:6.2} % far  {what}\n    from {}\n    to   {}", far * 100.0, alone.markup(keep).unwrap().replace('\n', " "), put_through.markup(keep).unwrap().replace('\n', " "));
            }
        }
    }
}
