//! Other editors' private marks (ARCHITECTURE §3.1, D25). A drawing Ink
//! opens becomes Ink's: what another editor wrote for itself alone comes
//! out, and Ink's own namespace takes the place of theirs. This is an
//! edit made on the way in. The reader and writer stay lossless, and the
//! file on disk changes only when it's next saved.

use crate::document::Document;
use crate::id::NodeId;
use crate::kind::{INK_NS, INK_PREFIX, Kind};
use crate::node::{Child, Raw, prefix};

/// An editor Ink knows by the namespace it writes its own data in.
struct Editor {
    name: &'static str,
    namespace: &'static str,
}

/// The editors whose marks are taken out. Boxy SVG is the one the
/// Lantern projects' files carry; others join when a file brings them.
const EDITORS: [Editor; 1] = [Editor { name: "Boxy SVG", namespace: "https://boxy-svg.com" }];

fn editor_of(namespace: Option<&str>) -> Option<&'static str> {
    EDITORS.iter().find(|e| Some(e.namespace) == namespace).map(|e| e.name)
}

/// What taking a drawing over took out of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adopted {
    /// The editors whose marks were found, by name.
    pub editors: Vec<&'static str>,
    /// How many of their elements came out (with what was in them).
    pub elements: usize,
    /// How many of their attributes came off other elements.
    pub attributes: usize,
}

impl Adopted {
    /// There was nothing of another editor's in it.
    pub fn is_nothing(&self) -> bool {
        self.editors.is_empty()
    }
}

impl Document {
    /// Take the drawing over from whatever editor made it: every element
    /// and attribute in that editor's namespace goes, and its namespace
    /// declaration becomes Ink's, in the same place.
    pub fn adopt(&mut self) -> Adopted {
        let mut adopted = Adopted::default();
        let all = self.descendants(self.root);
        // Their elements: the outermost of each run of them.
        let theirs: Vec<NodeId> = all.iter().copied().filter(|&id| self.editor_of_element(id).is_some() && !self.ancestors(id).any(|a| self.editor_of_element(a.id).is_some())).collect();
        let mut emptied: Vec<NodeId> = Vec::new();
        for id in theirs {
            let Some(name) = self.editor_of_element(id) else { continue };
            note(&mut adopted.editors, name);
            adopted.elements += self.descendants(id).len();
            let parent = self.nodes[&id].parent;
            if self.remove(id).is_ok() {
                emptied.extend(parent);
            }
        }
        // A <defs> left with nothing in it was only there for them.
        for id in emptied {
            let hollow = self.get(id).is_some_and(|n| n.kind == Kind::Defs && n.children.iter().all(|c| matches!(c, Child::Text(t) if t.trim().is_empty())));
            if hollow {
                let _ = self.remove(id);
            }
        }
        // Their attributes on what's left, and their declarations.
        let mut declarations: Vec<(NodeId, String)> = Vec::new();
        for id in self.descendants(self.root) {
            let node = &self.nodes[&id];
            let mut off: Vec<String> = Vec::new();
            for attr in &node.attrs {
                if let Some(declared) = attr.name.strip_prefix("xmlns:") {
                    if let Some(name) = editor_of(Some(attr.value.as_str())) {
                        note(&mut adopted.editors, name);
                        declarations.push((id, declared.to_owned()));
                    }
                } else if let Some(name) = prefix(&attr.name).and_then(|p| editor_of(self.namespace(id, Some(p)))) {
                    note(&mut adopted.editors, name);
                    off.push(attr.name.clone());
                }
            }
            adopted.attributes += off.len();
            for name in off {
                let _ = self.set_attr(id, &name, None);
            }
        }
        for (id, declared) in declarations {
            self.redeclare(id, &declared);
        }
        adopted
    }

    /// The editor whose element `id` is, if it's one's.
    fn editor_of_element(&self, id: NodeId) -> Option<&'static str> {
        let node = self.get(id)?;
        editor_of(self.namespace(id, prefix(&node.name)))
    }

    /// Turn `xmlns:{declared}` on `id` into Ink's declaration, where it
    /// stands; or take it off, when it isn't on the root or Ink's is
    /// there already (the root then gets Ink's if it has none).
    fn redeclare(&mut self, id: NodeId, declared: &str) {
        let ink = format!("xmlns:{INK_PREFIX}");
        let theirs = format!("xmlns:{declared}");
        let root = self.root;
        let has_ink = self.nodes[&root].attr(&ink).is_some();
        if id == root && !has_ink {
            if let Ok(node) = self.edit(id)
                && let Some(attr) = node.attrs.iter_mut().find(|a| a.name == theirs)
            {
                attr.name.clone_from(&ink);
                attr.value = INK_NS.to_owned();
                attr.raw = Raw::Fresh;
            }
        } else {
            let _ = self.set_attr(id, &theirs, None);
            if !has_ink {
                let _ = self.set_attr(root, &ink, Some(INK_NS));
            }
        }
    }
}

/// Add `name` to `names` once.
fn note(names: &mut Vec<&'static str>, name: &'static str) {
    if !names.contains(&name) {
        names.push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    fn adopt(text: &str) -> (String, Adopted) {
        let mut doc = Document::parse(DocId(1), text).unwrap();
        let adopted = doc.adopt();
        (doc.to_svg(), adopted)
    }

    #[test]
    fn boxys_marks_come_out_and_inks_go_in() {
        let text = "<svg viewBox=\"0 0 24 24\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:bx=\"https://boxy-svg.com\">\n  <defs>\n    <bx:export>\n      <bx:file format=\"svg\" path=\"line.svg\"/>\n    </bx:export>\n    <linearGradient id=\"g\"/>\n  </defs>\n  <path d=\"M0 0h1\" bx:shape=\"rect 1 2 3 4\" fill=\"url(#g)\"/>\n</svg>";
        let (out, adopted) = adopt(text);
        assert_eq!(out, "<svg viewBox=\"0 0 24 24\" xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\">\n  <defs>\n    <linearGradient id=\"g\"/>\n  </defs>\n  <path d=\"M0 0h1\" fill=\"url(#g)\"/>\n</svg>");
        assert_eq!(adopted, Adopted { editors: vec!["Boxy SVG"], elements: 2, attributes: 1 });
    }

    #[test]
    fn a_declaration_alone_is_still_taken_over() {
        // 74 of the corpus's files: Boxy's namespace declared and nothing in it.
        let (out, adopted) = adopt("<svg xmlns='http://www.w3.org/2000/svg' xmlns:bx='https://boxy-svg.com'><g/></svg>");
        assert_eq!(out, "<svg xmlns='http://www.w3.org/2000/svg' xmlns:ink='urn:lantern:ink'><g/></svg>", "in its place, in the file's own quotes");
        assert_eq!((adopted.editors.len(), adopted.elements, adopted.attributes), (1, 0, 0));
    }

    #[test]
    fn a_defs_that_held_only_their_marks_goes_with_them() {
        let (out, _) = adopt("<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:bx=\"https://boxy-svg.com\">\n  <defs>\n    <bx:export><bx:file format=\"svg\"/></bx:export>\n  </defs>\n  <g/>\n</svg>\n");
        assert_eq!(out, "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\">\n  <g/>\n</svg>\n");
        // A <defs> that was empty before any of this is not Ink's to remove.
        let (kept, _) = adopt("<svg xmlns:bx=\"https://boxy-svg.com\"><defs/></svg>");
        assert_eq!(kept, "<svg xmlns:ink=\"urn:lantern:ink\"><defs/></svg>");
    }

    #[test]
    fn nothing_of_another_editors_means_nothing_changes() {
        let text = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\">\n  <metadata><x:y xmlns:x=\"urn:mine\"/></metadata>\n  <use xlink:href=\"#a\"/>\n</svg>";
        let (out, adopted) = adopt(text);
        assert_eq!(out, text);
        assert!(adopted.is_nothing());
    }

    #[test]
    fn inks_declaration_is_never_doubled() {
        let (out, _) = adopt("<svg xmlns:ink=\"urn:lantern:ink\" xmlns:bx=\"https://boxy-svg.com\"><g xmlns:b2=\"https://boxy-svg.com\" b2:x=\"1\"><b2:y/></g></svg>");
        assert_eq!(out, "<svg xmlns:ink=\"urn:lantern:ink\"><g></g></svg>");
        // Declared somewhere inside only: it goes, and the root gets Ink's.
        let (out, _) = adopt("<svg><g xmlns:bx=\"https://boxy-svg.com\" bx:x=\"1\"/></svg>");
        assert_eq!(out, "<svg xmlns:ink=\"urn:lantern:ink\"><g/></svg>");
    }
}
