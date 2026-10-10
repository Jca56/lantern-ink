//! File > New…, Page… and Export…, and Edit > Tidy: a drawing's page
//! asked for, changed and written out, and what nothing uses dropped.

use super::*;
use crate::menus;

/// A 500-unit icon, as another editor leaves one: a gradient nothing
/// uses, an empty group, a guide.
const BIG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" width=\"500\" height=\"500\" viewBox=\"0 0 500 500\" ink:guides=\"x250\">\n  <defs>\n    <linearGradient id=\"spare\"/>\n  </defs>\n  <g id=\"hollow\"/>\n  <rect id=\"a\" x=\"100\" y=\"50\" width=\"300\" height=\"400\" fill=\"#08f\"/>\n</svg>\n";

fn big(name: &str) -> (Running, PathBuf) {
    let path = scratch(name).join("big.svg");
    std::fs::write(&path, BIG).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    (r, path)
}

impl Running {
    /// The page of the drawing that shows: its size, px.
    fn page(&self) -> Vec2 {
        self.ink.viewport().unwrap().size
    }

    /// Wait for the status bar to say something new, and what.
    fn told(&mut self) -> String {
        self.until("something said", |r| r.ink.toast_text().is_some());
        let said = self.ink.toast_text().unwrap().to_owned();
        (self.ink.pending_toast, self.ink.toast) = (None, None);
        said
    }
}

#[test]
fn new_asks_a_size_and_remembers_it() {
    let mut r = Running::start(1280.0, 800.0, 1.0);
    r.key(Key::Char('n'), Modifiers::CTRL);
    // The dialog starts at the last size made: an icon's grid at first.
    assert_eq!((r.ink.paging.width.as_str(), r.ink.paging.height.as_str(), r.ink.tabs.len()), ("24", "24", 1));
    // What's no size makes nothing: Enter waits for one.
    r.ink.paging.width = "wide".to_owned();
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!(r.ink.tabs.len(), 1);
    (r.ink.paging.width, r.ink.paging.height) = ("32".to_owned(), "48".to_owned());
    r.frames(1);
    r.key(Key::Enter, Modifiers::NONE);
    assert_eq!((r.ink.tabs.len(), r.page(), r.ink.settings.new_width, r.ink.settings.new_height), (2, Vec2::new(32.0, 48.0), 32.0, 48.0));
    assert!(r.svg().contains("width=\"32\" height=\"48\" viewBox=\"0 0 32 48\""));
    // The tab bar's + makes another that size, unasked; and the next
    // New… starts from it.
    r.ink.new_document();
    r.frames(2);
    assert_eq!((r.ink.tabs.len(), r.page()), (3, Vec2::new(32.0, 48.0)));
    r.key(Key::Char('n'), Modifiers::CTRL);
    assert_eq!(r.ink.paging.width, "32");
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!(r.ink.tabs.len(), 3);
}

#[test]
fn page_sets_the_size_and_fits_the_drawing_to_it() {
    let (mut r, _) = big("pages-page");
    let zoomed = r.camera().zoom;
    r.run(menus::PAGE);
    // The dialog says the page as it is: 500 square, its viewBox the
    // same, so that's not asked for by itself.
    assert_eq!((r.ink.paging.width.as_str(), r.ink.paging.plain, r.ink.paging.fit), ("500", true, false));
    (r.ink.paging.width, r.ink.paging.height, r.ink.paging.fit) = ("24".to_owned(), "24".to_owned(), true);
    r.run(menus::PAGE_APPLY);
    // One step: the page, the drawing and the guide, all in the new
    // coordinates; and the view fitted to the page it is now.
    assert_eq!(r.steps(), ["Page"]);
    assert!(r.svg().contains("width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" ink:guides=\"x12\""), "{}", r.svg());
    assert!(r.svg().contains("<rect id=\"a\" x=\"4.8\" y=\"2.4\" width=\"14.4\" height=\"19.2\" fill=\"#08f\"/>"));
    assert!(r.page() == Vec2::new(24.0, 24.0) && r.camera().fitted && r.camera().zoom > zoomed * 10.0);
    r.undo(1);
    assert_eq!((r.svg().as_str(), r.page()), (BIG, Vec2::new(500.0, 500.0)));
    // Without the fit, the drawing stays where its numbers are; a view
    // box of its own is asked for number by number.
    r.run(menus::PAGE);
    r.ink.paging.plain = false;
    r.ink.paging.view = ["100", "50", "300", "400"].map(str::to_owned);
    (r.ink.paging.width, r.ink.paging.height) = ("30".to_owned(), "40".to_owned());
    r.run(menus::PAGE_APPLY);
    assert!(r.svg().contains("width=\"30\" height=\"40\" viewBox=\"100 50 300 400\" ink:guides=\"x250\"") && r.svg().contains("x=\"100\" y=\"50\" width=\"300\""));
    // Asked again, it says that view box; set to what it is, nothing's
    // done.
    r.run(menus::PAGE);
    assert_eq!((r.ink.paging.plain, r.ink.paging.view.clone()), (false, ["100", "50", "300", "400"].map(str::to_owned)));
    r.run(menus::PAGE_APPLY);
    assert_eq!(r.steps().len(), 1);
}

#[test]
fn export_writes_a_clean_copy_or_pictures_and_leaves_the_drawing_alone() {
    let (mut r, own) = big("pages-export");
    let dir = own.parent().unwrap().to_owned();
    r.run(menus::EXPORT);
    assert_eq!((r.ink.exporting.format, r.ink.exporting.chosen(), r.ink.exporting.ready()), (0, vec![None], true));
    // A clean copy: tidied, without Ink's marks.
    r.ink.export_to(dir.join("clean"));
    assert_eq!(r.told(), "Exported clean.svg: a clean copy");
    assert_eq!(std::fs::read_to_string(dir.join("clean.svg")).unwrap(), "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"500\" height=\"500\" viewBox=\"0 0 500 500\">\n  <rect id=\"a\" x=\"100\" y=\"50\" width=\"300\" height=\"400\" fill=\"#08f\"/>\n</svg>\n");
    // Not over the drawing's own file.
    r.ink.export_to(own.clone());
    assert_eq!(r.told(), "big.svg is this drawing\u{2019}s own file: Save saves the drawing itself. Give the clean copy another name");
    assert_eq!(std::fs::read_to_string(&own).unwrap(), BIG);
    // Pictures, at two sizes: each beside the name chosen.
    (r.ink.exporting.format, r.ink.exporting.own, r.ink.exporting.sizes) = (1, false, [true, false, false, false, true, false, false, false]);
    r.ink.export_to(dir.join("lamp.png"));
    assert_eq!(r.told(), "Exported 2 pictures: lamp-16.png \u{2026} lamp-64.png");
    let size = |name: &str| lntrn_image::decode(&std::fs::read(dir.join(name)).unwrap()).map(|i| (i.width, i.height)).ok();
    assert_eq!((size("lamp-16.png"), size("lamp-64.png")), (Some((16, 16)), Some((64, 64))));
    // At no size there's nothing to export; the drawing was never
    // touched.
    r.ink.exporting.sizes = [false; 8];
    assert!(!r.ink.exporting.ready() && r.steps().is_empty() && r.svg() == BIG && !r.ink.is_modified(r.doc()));
}

#[test]
fn tidy_drops_what_nothing_uses_and_says_so() {
    let (mut r, _) = big("pages-tidy");
    r.run(menus::TIDY);
    assert_eq!(r.told(), "Tidied away 1 definition nothing used and 2 empty groups");
    assert_eq!((r.steps(), r.svg().contains("spare") || r.svg().contains("hollow"), r.svg().contains("ink:guides=\"x250\"")), (vec!["Tidy".to_owned()], false, true));
    r.run(menus::TIDY);
    assert_eq!((r.told().as_str(), r.steps().len()), ("Nothing to tidy: everything here is in use", 1));
    r.undo(1);
    assert_eq!(r.svg(), BIG);
}
