//! The Lantern preview strip: the drawing as Lantern's apps will show
//! it as an icon, at the bottom of the right panel, drawn again as the
//! drawing changes.

use super::*;

/// A drawing with words in it: Lantern's apps draw none.
const WORDY: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\">\n  <rect width=\"24\" height=\"24\" fill=\"#08f\"/>\n  <text x=\"2\" y=\"12\" font-family=\"Ink Test\" font-size=\"6\">Hi</text>\n</svg>\n";

impl Running {
    /// Wait for the strip of the drawing as it looks now.
    fn stripped(&mut self) {
        self.until("the strip", |r| {
            let look = r.ink.core.shown(r.doc()).unwrap().1;
            r.ink.strip.shown().is_some_and(|(doc, shown, _)| doc == r.doc() && shown == look)
        });
        self.frames(2);
    }
}

#[test]
fn the_strip_shows_the_drawing_at_the_icon_sizes() {
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&corpus(ICON));
    r.stripped();
    assert_eq!(r.strips.live.len(), 5);
    // Each size with its own pixels, their feet on one line, left to
    // right, at the panel's foot under the object tree.
    let laid = r.ink.strip.laid.clone();
    assert_eq!(laid.iter().map(|(size, at)| (*size, at.width(), at.height())).collect::<Vec<_>>(), [(16, 16.0, 16.0), (24, 24.0, 24.0), (32, 32.0, 32.0), (48, 48.0, 48.0), (64, 64.0, 64.0)]);
    assert!(laid.windows(2).all(|w| w[0].1.max.y == w[1].1.max.y && w[0].1.max.x < w[1].1.min.x), "{laid:?}");
    let (panel, rows) = (r.ink.layout.panel, r.ink.tree.laid.clone());
    assert!(laid.iter().all(|(_, at)| panel.contains(at.min) && at.max.y <= panel.max.y && at.min.x.fract() == 0.0 && at.min.y.fract() == 0.0));
    // (The tree scrolls in the room it's left: its first rows are over
    // the strip, the rest are scrolled to.)
    assert!(rows.first().is_some_and(|(_, row)| row.max.y <= r.ink.strip.head.min.y), "the tree's rows start over it");
    // The drawing changed: drawn again, the old pictures let go.
    let before = r.ink.strip.shown();
    r.key(Key::Char('a'), Modifiers::CTRL);
    r.key(Key::Delete, Modifiers::NONE);
    assert_eq!(r.steps(), ["Delete"]);
    r.stripped();
    assert!(r.ink.strip.shown() != before && r.strips.live.len() == 5);
    // Its heading folds it away, and brings it back.
    let (head, open) = (r.ink.strip.head, r.ink.strip.height(1.0, false));
    r.click(head.center());
    assert!(r.ink.settings.strip_folded && r.ink.strip.laid.is_empty());
    assert!(r.ink.strip.height(1.0, true) < open && r.ink.strip.head.min.y > head.min.y);
    r.click(r.ink.strip.head.center());
    assert!(!r.ink.settings.strip_folded && r.ink.strip.laid.len() == 5);
}

#[test]
fn the_strip_has_a_screens_pixels_and_says_what_an_app_will_not_draw() {
    let path = scratch("strip-wordy").join("wordy.svg");
    std::fs::write(&path, WORDY).unwrap();
    let mut r = Running::start(1792.0, 1120.0, 1.4);
    r.open(&path);
    r.stripped();
    // At 1.4, a 16 px icon is 22 px of the screen, shown one for one.
    assert_eq!(r.ink.strip.laid.iter().map(|(_, at)| at.width()).collect::<Vec<_>>(), [22.0, 34.0, 45.0, 67.0, 90.0]);
    assert_eq!(r.ink.strip.misses(), ["<text>"]);
    // The note under the icons makes the section taller than without.
    let wordless = Running::start(1792.0, 1120.0, 1.4);
    assert!(r.ink.strip.height(1.4, false) > wordless.ink.strip.height(1.4, false));
}
