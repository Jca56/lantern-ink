//! Edit > Tidy (`docs/M4.md`, slice f): what nothing uses is dropped
//! from the drawing, as one step, and the status bar says what went.
//! Only what's sure to be no loss (`Command::Tidy` with nothing asked
//! for by name): definitions nothing refers to, groups and `<defs>`
//! with nothing in them, namespace declarations nothing uses. Nothing
//! that shows changes. Comments, ids and titles stay: someone wrote
//! those on purpose, and a clean copy to ship (File > Export) leaves
//! the comments out anyway.

use ink_core::Command;
use ink_doc::tidy::{self, Dropped};

use crate::ink::Ink;

/// What tidying dropped, in a line for the status bar.
pub fn said(dropped: &Dropped) -> String {
    let count = |n: usize, one: &str, many: &str| if n == 1 { format!("1 {one}") } else { format!("{n} {many}") };
    let parts: Vec<String> = [
        (dropped.unused.len(), "definition nothing used", "definitions nothing used"),
        (dropped.empty.len(), "empty group", "empty groups"),
        (dropped.declarations.len(), "namespace declaration nothing used", "namespace declarations nothing used"),
    ]
    .into_iter()
    .filter(|(n, ..)| *n > 0)
    .map(|(n, one, many)| count(n, one, many))
    .collect();
    match parts.as_slice() {
        [] => "Nothing to tidy: everything here is in use".to_owned(),
        [one] => format!("Tidied away {one}"),
        [most @ .., last] => format!("Tidied away {} and {last}", most.join(", ")),
    }
}

impl Ink {
    /// Edit > Tidy, on the drawing that shows.
    pub(crate) fn tidy(&mut self) {
        let Some(doc) = self.tabs.active_doc() else { return };
        // What will go is counted from the drawing as it is.
        let Ok(dropped) = self.core.doc(doc).map(|drawing| tidy::plan(drawing, &[])) else { return };
        if dropped.is_nothing() || self.edit(doc, &Command::Tidy { also: Vec::new() }, "Tidy").is_some() {
            self.toast(said(&dropped));
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_core::{DocId, Document};

    use super::*;

    #[test]
    fn what_went_is_said_in_a_line() {
        let plan = |inner: &str| tidy::plan(&Document::parse(DocId(1), &format!("<svg xmlns=\"http://www.w3.org/2000/svg\"{inner}</svg>")).unwrap(), &[]);
        assert_eq!(said(&plan("><rect width=\"4\" height=\"4\"/>")), "Nothing to tidy: everything here is in use");
        assert_eq!(said(&plan("><defs><linearGradient id=\"a\"/></defs><rect width=\"4\" height=\"4\"/>")), "Tidied away 1 definition nothing used and 1 empty group");
        assert_eq!(said(&plan(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"><g/><g/><rect width=\"4\" height=\"4\"/>")), "Tidied away 2 empty groups and 1 namespace declaration nothing used");
        assert_eq!(said(&plan(" xmlns:x=\"urn:x\"><defs><linearGradient id=\"a\"/><clipPath id=\"b\"/></defs><rect width=\"4\" height=\"4\"/>")), "Tidied away 2 definitions nothing used, 1 empty group and 1 namespace declaration nothing used");
    }
}
