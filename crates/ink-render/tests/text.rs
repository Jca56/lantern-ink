//! Text made into paths draws what the text drew (ARCHITECTURE §5.5):
//! `Command::TextToPath` on every text of the text golden (set in the
//! tests' own font) and of every corpus file that has one (in whatever
//! this machine sets them in: the same fonts before and after), each
//! drawing drawn before and after.
//!
//! The two can't be the same to the byte: a path's numbers are written
//! to the drawing's three decimals. And what's measured across a box
//! with a text in it (a gradient across the text, the glow of a group
//! it's in) is measured across a tighter one afterwards: a text's box
//! is its glyphs' cells, a path's is its outline. Measured 2026-10-07
//! at 320 px: thirteen of the nineteen corpus files are within 0.002
//! levels of what they were; the six with such a box (the casino, the
//! arcade and the cyberpunk folders, twice each) are 0.04 to 0.20
//! levels apart, at most 0.54 % of their pixels by more than 16.

use std::path::PathBuf;

use ink_doc::{Command, DocId, Document, Kind, Viewport};
use ink_render::{View, render};
use lntrn_image::Image;

fn test_fonts() {
    static ADDED: std::sync::Once = std::sync::Once::new();
    ADDED.call_once(|| {
        for font in ["InkTest-Regular.ttf", "InkTest-Bold.ttf"] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fonts").join(font);
            ink_doc::fonts::add(std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap();
        }
    });
}

/// `doc` drawn 320 px across its longer side.
fn drawn(doc: &Document) -> Image {
    let viewport = Viewport::of(doc.node(doc.root()).unwrap());
    render(doc, &View::page(&viewport, 320.0 / viewport.size.x.max(viewport.size.y))).unwrap()
}

/// How far apart two pictures are: the average over every channel of
/// every pixel, in levels of 255, and the share of pixels with a
/// channel more than 16 levels out. Colours are weighed by their alpha.
fn apart(a: &Image, b: &Image) -> (f64, f64) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut sum, mut far) = (0.0f64, 0usize);
    for (p, q) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)) {
        let (pa, qa) = (p[3] as f64 / 255.0, q[3] as f64 / 255.0);
        let mut worst = (p[3] as f64 - q[3] as f64).abs();
        sum += worst;
        for i in 0..3 {
            let d = (p[i] as f64 * pa - q[i] as f64 * qa).abs();
            sum += d;
            worst = worst.max(d);
        }
        far += usize::from(worst > 16.0);
    }
    let n = (a.width * a.height) as f64;
    (sum / (n * 4.0), far as f64 / n)
}

/// `svg`'s texts made paths: how many were, and how far the drawing
/// then is from what it was.
fn pathed(svg: &str) -> (usize, f64, f64) {
    let mut doc = Document::parse(DocId(1), svg).unwrap();
    let before = drawn(&doc);
    let texts: Vec<_> = doc.descendants(doc.root()).into_iter().filter(|id| doc.get(*id).is_some_and(|n| n.kind == Kind::Text)).collect();
    let mut made = 0;
    for id in texts {
        // One that can't be set, or says nothing, stays as it is.
        made += usize::from(doc.apply(&Command::TextToPath { nodes: vec![id], as_drawn: true }).is_ok());
    }
    assert!(!doc.descendants(doc.root()).into_iter().filter_map(|id| doc.get(id)).any(|n| n.kind == Kind::TSpan) || made == 0, "no spans are left behind");
    // What was written reads back as the same drawing.
    let again = Document::parse(DocId(2), &doc.to_svg()).unwrap();
    let after = drawn(&again);
    let (mean, far) = apart(&before, &after);
    (made, mean, far)
}

#[test]
fn text_made_paths_draws_as_the_text_did() {
    test_fonts();
    let golden = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/text.svg")).unwrap();
    let (made, mean, far) = pathed(&golden);
    println!("text.svg: {made} texts, {mean:.4} levels apart, {:.3} % far", far * 100.0);
    assert_eq!(made, 9, "all but the one that can't be set");
    assert!(mean < 0.25 && far < 0.004, "text.svg: {mean:.4} levels apart, {:.3} % of pixels far", far * 100.0);

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect();
    files.sort();
    let (mut with_text, mut total, mut same) = (0, 0, 0);
    for path in files {
        let svg = std::fs::read_to_string(&path).unwrap();
        if !svg.contains("<text") {
            continue;
        }
        let (made, mean, far) = pathed(&svg);
        println!("{}: {made} texts, {mean:.4} levels apart, {:.3} % far", path.file_name().unwrap().to_string_lossy(), far * 100.0);
        assert!(mean < 0.3 && far < 0.008, "{}: {mean:.4} levels apart, {:.3} % of pixels far", path.display(), far * 100.0);
        with_text += 1;
        total += made;
        same += usize::from(mean < 0.005 && far == 0.0);
    }
    assert_eq!(with_text, 19, "the corpus files with text");
    assert!(same >= 13, "only {same} files draw just as they did: the ones with nothing measured across a text's box should");
    assert!(total >= 100, "{total} texts made paths");
}
