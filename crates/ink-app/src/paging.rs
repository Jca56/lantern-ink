//! File > New… and File > Page… (`docs/M4.md`, slice f): a drawing's
//! page asked for in a dialog. New… asks a size, with the icon sizes as
//! presets, and remembers the last one made. Page… sets the size the
//! drawing is shown at and the coordinates that fill it (its viewBox),
//! and can fit the drawing to them: how a 500-unit icon becomes a
//! 24-unit one. The Command is `ink_doc::page`'s, the same one Claude's
//! `doc_set` makes.
//!
//! The tab bar's + makes a drawing the size of the last one made, with
//! no questions.

use ink_core::DocId;
use ink_doc::page::{self, Page};
use lntrn_ui::{Action, Dialog, HostCx, ShellRequest, Ui};

use crate::controls;
use crate::ink::Ink;
use crate::menus::{DIALOG_NEW, DIALOG_PAGE, NEW_CREATE, PAGE_APPLY};

/// The sizes offered: an icon's, square.
pub const PRESETS: [f64; 8] = [16.0, 24.0, 32.0, 48.0, 64.0, 128.0, 256.0, 512.0];
/// The most a page is, a side.
pub const MOST: f64 = 100_000.0;

/// What the New and Page dialogs hold: each number as it's typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paging {
    /// The drawing Page… is of; none for New….
    doc: Option<DocId>,
    pub width: String,
    pub height: String,
    /// The viewBox: where it starts, and its size.
    pub view: [String; 4],
    /// The viewBox is the page's own size, from nothing: its numbers
    /// aren't asked for.
    pub plain: bool,
    /// Everything in the drawing goes to the new coordinates.
    pub fit: bool,
}

/// `v` as a field says it: as few decimals as say it.
fn said(v: f64) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// What's typed, as a size: a number more than nothing.
fn size(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0 && *v <= MOST)
}

/// What's typed, as a place: any number.
fn place(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite() && v.abs() <= MOST)
}

fn check_size(text: &str) -> Option<String> {
    size(text).is_none().then(|| "A size is a number more than nothing".to_owned())
}

fn check_place(text: &str) -> Option<String> {
    place(text).is_none().then(|| "A number".to_owned())
}

impl Paging {
    /// For a new drawing `width` by `height`.
    pub fn fresh(width: f64, height: f64) -> Paging {
        Paging { width: said(width), height: said(height), plain: true, ..Paging::default() }
    }

    /// For `doc`, whose page is `size` and whose viewBox is `view`.
    pub fn of(doc: DocId, size: (f64, f64), view: [f64; 4]) -> Paging {
        let plain = view == [0.0, 0.0, size.0, size.1];
        Paging { doc: Some(doc), width: said(size.0), height: said(size.1), view: view.map(said), plain, fit: false }
    }

    /// The size typed, if it's one.
    pub fn size(&self) -> Option<(f64, f64)> {
        size(&self.width).zip(size(&self.height))
    }

    /// The page asked for, if every number is one: its size, and its
    /// viewBox (the page's own size from nothing, while it's `plain`).
    pub fn page(&self) -> Option<Page> {
        let (width, height) = self.size()?;
        let view_box = if self.plain { [0.0, 0.0, width, height] } else { [place(&self.view[0])?, place(&self.view[1])?, size(&self.view[2])?, size(&self.view[3])?] };
        Some(Page { width: Some(width), height: Some(height), view_box: Some(view_box), fit: self.fit, decimals: None })
    }

    /// The preset the size typed is: 0 for none of them.
    fn preset(&self) -> usize {
        match self.size() {
            Some((w, h)) if w == h => PRESETS.iter().position(|p| *p == w).map_or(0, |i| i + 1),
            _ => 0,
        }
    }

    /// The rows both dialogs have: a preset, and the size. Whether
    /// Enter was pressed in a field.
    fn size_rows(&mut self, ui: &mut Ui) -> bool {
        let names: Vec<String> = std::iter::once("Custom".to_owned()).chain(PRESETS.iter().map(|p| format!("Icon {p} \u{00d7} {p}"))).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut picked = self.preset();
        ui.labelled("Preset", |ui| {
            if controls::dropdown(ui, "Preset", &mut picked, &names, false) && picked > 0 {
                (self.width, self.height) = (said(PRESETS[picked - 1]), said(PRESETS[picked - 1]));
            }
        });
        let mut entered = false;
        ui.labelled("Width", |ui| {
            // The width is what's typed first.
            let id = ui.id("Width");
            ui.request_focus_once(id);
            entered |= controls::field_validated(ui, "Width", &mut self.width, &check_size).committed;
        });
        ui.labelled("Height", |ui| entered |= controls::field_validated(ui, "Height", &mut self.height, &check_size).committed);
        entered
    }

    /// New…'s rows. Whether anything changed, and whether Enter was
    /// pressed in a field.
    pub fn new_rows(&mut self, ui: &mut Ui) -> (bool, bool) {
        let before = self.clone();
        let entered = self.size_rows(ui);
        (*self != before, entered)
    }

    /// Page…'s rows.
    pub fn page_rows(&mut self, ui: &mut Ui) -> (bool, bool) {
        let before = self.clone();
        let mut entered = self.size_rows(ui);
        controls::toggle(ui, "The view box is the page\u{2019}s size", &mut self.plain);
        if self.plain {
            // (Kept in step, for when it's unticked.)
            if let Some((w, h)) = self.size() {
                self.view = [said(0.0), said(0.0), said(w), said(h)];
            }
        } else {
            for (i, name) in ["View X", "View Y", "View Width", "View Height"].into_iter().enumerate() {
                let check: &dyn Fn(&str) -> Option<String> = if i < 2 { &check_place } else { &check_size };
                ui.labelled(name, |ui| entered |= controls::field_validated(ui, name, &mut self.view[i], check).committed);
            }
        }
        controls::toggle(ui, "Fit the drawing to it", &mut self.fit);
        (*self != before, entered)
    }
}

impl Ink {
    /// File > New…: ask what size.
    pub(crate) fn ask_new(&mut self, cx: &mut HostCx) {
        self.paging = Paging::fresh(self.settings.new_width, self.settings.new_height);
        cx.request(ShellRequest::Dialog(Dialog::new("New Drawing", "").content(DIALOG_NEW).button("Cancel", None).button("Create", Some(Action::new(NEW_CREATE))).default_button(1)));
    }

    /// New…'s Create: a drawing that size, in a new tab; and the size
    /// is the next one's too.
    pub(crate) fn new_created(&mut self) {
        let Some((width, height)) = self.paging.size() else { return };
        if (self.settings.new_width, self.settings.new_height) != (width, height) {
            (self.settings.new_width, self.settings.new_height) = (width, height);
            self.settings.save();
        }
        let doc = self.core.new_doc_with(width, height, self.settings.decimals());
        self.tabs.add(doc);
    }

    /// File > Page…: the page of the drawing that shows, to change.
    pub(crate) fn ask_page(&mut self, cx: &mut HostCx) {
        let Some(doc) = self.tabs.active_doc() else { return };
        let Ok(drawing) = self.core.doc(doc) else { return };
        let (size, view) = page::of(drawing);
        self.paging = Paging::of(doc, (size.x, size.y), view);
        cx.request(ShellRequest::Dialog(Dialog::new("Page", "").content(DIALOG_PAGE).button("Cancel", None).button("Apply", Some(Action::new(PAGE_APPLY))).default_button(1)));
    }

    /// Page…'s Apply: one step, and the view fitted to the page it is
    /// now.
    pub(crate) fn page_applied(&mut self) {
        let Some((doc, asked)) = self.paging.doc.zip(self.paging.page()) else { return };
        let command = match self.core.doc(doc).map_err(|e| e.to_string()).and_then(|drawing| page::set(drawing, &asked).map_err(|e| e.to_string())) {
            Ok(command) => command,
            Err(why) => return self.toast(why),
        };
        if self.edit(doc, &command, "Page").is_some_and(|applied| !applied.is_nothing())
            && let Some(tab) = self.tabs.iter_mut().find(|t| t.doc == doc)
        {
            tab.camera = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_typed_is_a_page_once_every_number_is_one() {
        let mut p = Paging::fresh(24.0, 24.0);
        assert_eq!((p.width.as_str(), p.size(), p.preset(), p.plain), ("24", Some((24.0, 24.0)), 2, true));
        assert_eq!(p.page(), Some(Page { width: Some(24.0), height: Some(24.0), view_box: Some([0.0, 0.0, 24.0, 24.0]), fit: false, decimals: None }));
        // A comma for the point; not square, no preset; nothing, no size.
        (p.width, p.height) = ("12,5".to_owned(), " 40 ".to_owned());
        assert_eq!((p.size(), p.preset()), (Some((12.5, 40.0)), 0));
        for bad in ["", "0", "-4", "wide", "1e9", "inf"] {
            p.width = bad.to_owned();
            assert!(p.size().is_none() && p.page().is_none() && check_size(bad).is_some(), "{bad}");
        }
        // A drawing whose viewBox isn't its page's size says its own;
        // one whose is, has no numbers to ask.
        let boxy = Paging::of(DocId(1), (24.0, 24.0), [0.0, 0.0, 500.0, 500.0]);
        assert_eq!((boxy.plain, boxy.view.clone(), boxy.page().unwrap().view_box), (false, ["0", "0", "500", "500"].map(str::to_owned), Some([0.0, 0.0, 500.0, 500.0])));
        assert!(Paging::of(DocId(1), (32.0, 24.0), [0.0, 0.0, 32.0, 24.0]).plain);
        let mut moved = Paging { view: ["-2".into(), "x".into(), "10".into(), "10".into()], ..boxy };
        assert!(moved.page().is_none());
        moved.view[1] = "3.5".into();
        moved.fit = true;
        assert_eq!(moved.page().map(|p| (p.view_box, p.fit)), Some((Some([-2.0, 3.5, 10.0, 10.0]), true)));
        assert_eq!((said(0.30000000000000004), said(512.0), said(0.0)), ("0.3".to_owned(), "512".to_owned(), "0".to_owned()));
    }
}
