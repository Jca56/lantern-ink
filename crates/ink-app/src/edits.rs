//! What Alva does to a drawing from the window (LS3's `edits.rs`): each
//! is a Command applied through the core as one step of hers. What the
//! drawing refuses is said in the status bar, in the names its rows go
//! by.

use ink_core::{Actor, Applied, Command, DocId};
use ink_doc::{Document, NodeId};

use crate::ink::Ink;
use crate::select;
use crate::tree::Intent;

/// `why` (a refusal, which names nodes as `N7`) with each node called
/// what its row is called.
pub(crate) fn in_row_names(doc: &Document, why: &str) -> String {
    let mut out = String::with_capacity(why.len());
    let mut rest = why;
    while let Some(at) = rest.find('N') {
        let (before, from) = rest.split_at(at);
        let digits = from[1..].chars().take_while(char::is_ascii_digit).count();
        // A node's id is a word of its own: an N, then its number.
        let starts = before.chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let ends = from[1 + digits..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        let node = (digits > 0 && starts && ends).then(|| from[1..1 + digits].parse().ok().map(NodeId)).flatten().filter(|&id| doc.get(id).is_some());
        out.push_str(before);
        match node {
            Some(id) => {
                let (name, own) = select::name_of(doc, id);
                out.push_str(&if own { format!("\u{201c}{name}\u{201d}") } else { format!("the {}", name.to_lowercase()) });
                rest = &from[1 + digits..];
            }
            None => {
                out.push('N');
                rest = &from[1..];
            }
        }
    }
    out.push_str(rest);
    // A sentence of the status bar's own.
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => out,
    }
}

impl Ink {
    /// Do `command` to `doc` as one step of Alva's, called `label` in
    /// the Edit menu. What it did; or nothing, with the drawing's
    /// reason in the status bar.
    pub(crate) fn edit(&mut self, doc: DocId, command: &Command, label: &str) -> Option<Applied> {
        match self.core.apply(doc, command, Actor::Alva, label) {
            Ok(applied) => Some(applied),
            Err(e) => {
                let why = e.to_string();
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
                None
            }
        }
    }

    /// Carry out what the object tree asked for.
    pub(crate) fn tree_asked(&mut self, doc: DocId, intent: Intent) {
        match intent {
            Intent::Hide(node, hide) => {
                let set = vec![("display".to_owned(), hide.then(|| "none".to_owned()))];
                self.edit(doc, &Command::SetStyle { nodes: vec![node], set }, if hide { "Hide" } else { "Show" });
            }
            Intent::Lock(node, lock) => {
                self.edit(doc, &Command::SetLocked { nodes: vec![node], locked: lock }, if lock { "Lock" } else { "Unlock" });
            }
            Intent::Rename(node, name) => {
                self.edit(doc, &Command::SetLabel { node, label: (!name.is_empty()).then_some(name) }, "Rename");
            }
            Intent::Move(nodes, place) => {
                self.edit(doc, &Command::Move { nodes, place }, "Restack");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    #[test]
    fn a_refusal_names_nodes_as_their_rows_do() {
        let doc = Document::parse(DocId(1), r##"<svg xmlns:ink="urn:lantern:ink"><g ink:label="Lamp glass" ink:locked="true"><path id="flame"/></g><rect/></svg>"##).unwrap();
        let said = |why: &str| in_row_names(&doc, why);
        assert_eq!(said("N3 is in N2, which is locked: nothing in it changes until N2 is unlocked"), "\u{201c}flame\u{201d} is in \u{201c}Lamp glass\u{201d}, which is locked: nothing in it changes until \u{201c}Lamp glass\u{201d} is unlocked");
        assert_eq!(said("N4 is locked: nothing about it changes until it's unlocked"), "The rectangle is locked: nothing about it changes until it's unlocked");
        // Only a node this drawing has, and only a word of its own.
        assert_eq!(said("N99 isn't there, and NaN and N2x aren't nodes"), "N99 isn't there, and NaN and N2x aren't nodes");
        assert_eq!(said("there's nothing to copy"), "There's nothing to copy");
    }
}
