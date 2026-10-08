//! The open drawings as tabs (LS3's `docs.rs`): their order, which one
//! shows, and what the window keeps for each beside the core's
//! document: its camera, and the name it goes by until it has a file.

use ink_core::DocId;

use crate::camera::Camera;

pub struct Tab {
    pub doc: DocId,
    /// Made (fitted) the first time the tab is laid out.
    pub camera: Option<Camera>,
    /// What it's called while it has no file: `untitled`, `untitled 2`.
    pub name: String,
}

#[derive(Default)]
pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
    /// How many drawings have been made here without a file.
    untitled: usize,
}

impl Tabs {
    /// Open `doc` in a new tab after the others, and show it.
    pub fn add(&mut self, doc: DocId) {
        self.untitled += 1;
        let name = if self.untitled == 1 { "untitled".to_owned() } else { format!("untitled {}", self.untitled) };
        self.tabs.push(Tab { doc, camera: None, name });
        self.active = self.tabs.len() - 1;
    }

    /// A drawing opened from a file takes no `untitled` number.
    pub fn add_named(&mut self, doc: DocId) {
        self.tabs.push(Tab { doc, camera: None, name: String::new() });
        self.active = self.tabs.len() - 1;
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    pub fn active_doc(&self) -> Option<DocId> {
        self.active().map(|t| t.doc)
    }

    pub fn index_of(&self, doc: DocId) -> Option<usize> {
        self.tabs.iter().position(|t| t.doc == doc)
    }

    pub fn select(&mut self, i: usize) {
        if i < self.tabs.len() {
            self.active = i;
        }
    }

    /// Show `doc`'s tab, if it has one.
    pub fn show(&mut self, doc: DocId) -> bool {
        self.index_of(doc).map(|i| self.active = i).is_some()
    }

    /// Take `doc`'s tab away; the one after it shows (or the last).
    pub fn remove(&mut self, doc: DocId) {
        let Some(i) = self.index_of(doc) else { return };
        self.tabs.remove(i);
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_a_tab_shows_its_neighbour() {
        let mut t = Tabs::default();
        for n in 1..=4 {
            t.add(DocId(n));
        }
        assert_eq!(t.active_doc(), Some(DocId(4)));
        t.select(1);
        t.remove(DocId(2));
        assert_eq!(t.active_doc(), Some(DocId(3)), "the one after it");
        t.remove(DocId(1));
        assert_eq!(t.active_doc(), Some(DocId(3)), "a tab before the active one leaves it showing");
        t.select(1);
        t.remove(DocId(4));
        assert_eq!(t.active_doc(), Some(DocId(3)), "the last one closed: the one before");
        t.remove(DocId(3));
        assert_eq!(t.active_doc(), None);
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn new_drawings_are_numbered_and_opened_ones_arent() {
        let mut t = Tabs::default();
        t.add(DocId(1));
        t.add_named(DocId(2));
        t.add(DocId(3));
        assert_eq!(t.iter().map(|tab| tab.name.as_str()).collect::<Vec<_>>(), ["untitled", "", "untitled 2"]);
        assert!(t.show(DocId(1)) && t.active_index() == 0 && !t.show(DocId(9)));
    }
}
