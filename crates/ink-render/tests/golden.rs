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
