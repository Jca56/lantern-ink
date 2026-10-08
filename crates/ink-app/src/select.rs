//! What a tab keeps of its drawing beside the document (ARCHITECTURE §3:
//! view state stays out of it; LS3's `view.rs`): which nodes are
//! selected and which is in hand, which rows of the object tree are
//! open, and the row being renamed.

use std::collections::HashSet;

use ink_doc::style::prop;
use ink_doc::{Document, Kind, Node, NodeId};

/// A row of the object tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: NodeId,
    /// How many elements it's inside, the root aside.
    pub depth: usize,
    /// It holds elements of its own: it opens and closes.
    pub holds: bool,
    pub open: bool,
}

/// How a click on a row was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    Plain,
    /// Ctrl: in or out of the selection.
    Toggle,
    /// Shift: everything from the node in hand to here.
    Range,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    /// The node in hand: the last one picked, where a range grows from.
    pub active: Option<NodeId>,
    /// Every selected node, the one in hand among them.
    pub nodes: Vec<NodeId>,
    /// Rows the other way from how their kind starts out (groups open,
    /// everything else that holds elements closed).
    flipped: HashSet<NodeId>,
    /// The row being renamed, and its name so far.
    pub renaming: Option<(NodeId, String)>,
    /// The rename has only just begun: its field takes the keys and
    /// selects the name the first time it's drawn.
    pub rename_fresh: bool,
}

/// What an element is called in the tree when nothing names it.
pub fn kind_word(node: &Node) -> String {
    let word = match node.kind {
        Kind::Svg => "Drawing",
        Kind::G => "Group",
        Kind::A => "Link",
        Kind::Switch => "Switch",
        Kind::Path => "Path",
        Kind::Rect => "Rectangle",
        Kind::Circle => "Circle",
        Kind::Ellipse => "Ellipse",
        Kind::Line => "Line",
        Kind::Polyline => "Polyline",
        Kind::Polygon => "Polygon",
        Kind::Text => "Text",
        Kind::Defs => "Definitions",
        Kind::LinearGradient => "Linear gradient",
        Kind::RadialGradient => "Radial gradient",
        Kind::Stop => "Stop",
        Kind::ClipPath => "Clip path",
        Kind::Mask => "Mask",
        Kind::Pattern => "Pattern",
        Kind::Marker => "Marker",
        Kind::Symbol => "Symbol",
        Kind::Filter => "Filter",
        Kind::Style => "Style rules",
        Kind::Title => "Title",
        Kind::Desc => "Description",
        Kind::Metadata => "Metadata",
        _ => match node.name.as_str() {
            "tspan" => "Span",
            "use" => "Use",
            "image" => "Image",
            // A filter's steps, and whatever isn't SVG's: by its own
            // name.
            name => return name.to_owned(),
        },
    };
    word.to_owned()
}

/// What `id`'s row says: the name Alva gave it, else its `id`, else
/// what kind of thing it is. And whether that's a name of its own.
pub fn name_of(doc: &Document, id: NodeId) -> (String, bool) {
    let Some(node) = doc.get(id) else { return (String::new(), false) };
    if let Some(label) = doc.label(id) {
        return (label.to_owned(), true);
    }
    match node.attr("id").filter(|id| !id.trim().is_empty()) {
        Some(id) => (id.to_owned(), true),
        None => (kind_word(node), false),
    }
}

/// Whether `id` is drawn where it stands (so it has an eye): a group,
/// a shape or a text that isn't a definition.
pub fn is_drawn(doc: &Document, id: NodeId) -> bool {
    doc.get(id).is_some_and(|n| n.kind.is_group() || n.kind.is_shape() || n.kind == Kind::Text) && !doc.ancestors(id).any(|n| n.kind.is_never_drawn())
}

/// Whether `id` itself is hidden (`display: none`).
pub fn is_hidden(doc: &Document, id: NodeId) -> bool {
    doc.get(id).is_some_and(|n| prop(n, "display") == Some("none"))
}

impl Selection {
    /// Whether `id`'s row shows what it holds.
    pub fn is_open(&self, doc: &Document, id: NodeId) -> bool {
        doc.get(id).is_some_and(|n| n.kind.is_group()) != self.flipped.contains(&id)
    }

    /// Open or close `id`'s row. What it hides stays selected: closing
    /// is only how the tree looks.
    pub fn toggle_open(&mut self, id: NodeId) {
        if !self.flipped.remove(&id) {
            self.flipped.insert(id);
        }
    }

    /// The rows to show, front to back (the topmost first, as the
    /// picture stacks): every element that isn't inside a closed one.
    pub fn rows(&self, doc: &Document) -> Vec<Row> {
        fn walk(sel: &Selection, doc: &Document, node: &Node, depth: usize, out: &mut Vec<Row>) {
            for id in node.elements().rev() {
                let Some(child) = doc.get(id) else { continue };
                let (holds, open) = (child.elements().next().is_some(), sel.is_open(doc, id));
                out.push(Row { id, depth, holds, open: holds && open });
                if holds && open {
                    walk(sel, doc, child, depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        if let Some(root) = doc.get(doc.root()) {
            walk(self, doc, root, 0, &mut out);
        }
        out
    }

    pub fn is_selected(&self, id: NodeId) -> bool {
        self.nodes.contains(&id)
    }

    /// `id` alone, in hand.
    pub fn select_only(&mut self, id: NodeId) {
        self.active = Some(id);
        self.nodes = vec![id];
    }

    /// Nothing selected.
    pub fn clear(&mut self) {
        self.active = None;
        self.nodes.clear();
        self.renaming = None;
    }

    /// A click on `id`'s row.
    pub fn click(&mut self, doc: &Document, id: NodeId, how: Click) {
        match how {
            Click::Plain => self.select_only(id),
            Click::Toggle => {
                if !self.is_selected(id) {
                    self.nodes.push(id);
                    self.active = Some(id);
                } else {
                    self.nodes.retain(|n| *n != id);
                    if self.active == Some(id) {
                        self.active = self.nodes.last().copied();
                    }
                }
            }
            Click::Range => {
                let rows = self.rows(doc);
                let at = |n: NodeId| rows.iter().position(|r| r.id == n);
                let (Some(from), Some(to)) = (self.active.and_then(at), at(id)) else { return self.select_only(id) };
                // The node in hand stays in hand: the range grows from it.
                self.nodes = rows[from.min(to)..=from.max(to)].iter().map(|r| r.id).collect();
            }
        }
    }

    /// Begin renaming `id` (from the name it shows), alone in hand.
    pub fn rename(&mut self, id: NodeId, name: &str) {
        self.select_only(id);
        self.renaming = Some((id, name.to_owned()));
        self.rename_fresh = true;
    }

    /// Let go of whatever the document no longer has (a node deleted,
    /// an undo).
    pub fn prune(&mut self, doc: &Document) {
        let there = |id: &NodeId| doc.get(*id).is_some();
        self.nodes.retain(there);
        if self.active.is_some_and(|a| !there(&a)) {
            self.active = self.nodes.last().copied();
        }
        self.flipped.retain(there);
        if self.renaming.as_ref().is_some_and(|(id, _)| !there(id)) {
            self.renaming = None;
        }
    }

    /// The selected nodes that aren't inside another selected one, back
    /// to front (the file's order): what an action on "the selection"
    /// acts on. A group stands for what it holds.
    pub fn tops(&self, doc: &Document) -> Vec<NodeId> {
        doc.descendants(doc.root()).into_iter().filter(|&id| self.is_selected(id) && !self.nodes.iter().any(|&s| s != id && doc.is_within(id, s))).collect()
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    const N: fn(u64) -> NodeId = NodeId;

    /// Back to front: defs (a gradient with two stops), a, group g
    /// holding (b1, b2, a text with a span), c.
    fn doc() -> Document {
        Document::parse(DocId(1), r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:ink="urn:lantern:ink"><defs><linearGradient id="sky"><stop/><stop/></linearGradient></defs><rect id="a"/><g ink:label="Lamp"><circle/><path id="b2" display="none"/><text>hi <tspan>there</tspan></text></g><ellipse id="c"/></svg>"##).unwrap()
    }

    fn ids(rows: &[Row]) -> Vec<u64> {
        rows.iter().map(|r| r.id.0).collect()
    }

    #[test]
    fn rows_run_front_to_back_with_groups_open_and_the_rest_closed() {
        let d = doc();
        let mut s = Selection::default();
        // Front to back: c, the group and what it holds (the text shut,
        // its span out of sight), a, the definitions shut.
        let rows = s.rows(&d);
        assert_eq!(ids(&rows), [12, 7, 10, 9, 8, 6, 2]);
        assert_eq!(rows.iter().map(|r| r.depth).collect::<Vec<_>>(), [0, 0, 1, 1, 1, 0, 0]);
        assert_eq!(rows.iter().map(|r| (r.holds, r.open)).collect::<Vec<_>>(), [(false, false), (true, true), (true, false), (false, false), (false, false), (false, false), (true, false)]);
        // Closing the group hides what it holds; opening the
        // definitions shows the gradient, shut over its stops.
        s.toggle_open(N(7));
        s.toggle_open(N(2));
        assert_eq!(ids(&s.rows(&d)), [12, 7, 6, 2, 3]);
        s.toggle_open(N(3));
        assert_eq!(ids(&s.rows(&d)), [12, 7, 6, 2, 3, 5, 4]);
    }

    #[test]
    fn rows_are_named_by_label_then_id_then_kind() {
        let d = doc();
        let names: Vec<(String, bool)> = [7, 6, 8, 10, 2, 3, 4, 11].into_iter().map(|n| name_of(&d, N(n))).collect();
        let said: Vec<(&str, bool)> = names.iter().map(|(name, own)| (name.as_str(), *own)).collect();
        assert_eq!(said, [("Lamp", true), ("a", true), ("Circle", false), ("Text", false), ("Definitions", false), ("sky", true), ("Stop", false), ("Span", false)]);
        // What's drawn has an eye; a definition has none.
        assert!(is_drawn(&d, N(7)) && is_drawn(&d, N(9)) && !is_drawn(&d, N(2)) && !is_drawn(&d, N(3)) && !is_drawn(&d, N(11)));
        assert!(is_hidden(&d, N(9)) && !is_hidden(&d, N(8)));
    }

    #[test]
    fn clicks_select_toggle_and_take_ranges() {
        let d = doc();
        let mut s = Selection::default();
        s.click(&d, N(12), Click::Plain);
        assert_eq!((s.active, s.nodes.clone()), (Some(N(12)), vec![N(12)]));
        // Shift: the run of rows from the one in hand, which stays so.
        s.click(&d, N(9), Click::Range);
        assert_eq!((s.active, s.nodes.clone()), (Some(N(12)), vec![N(12), N(7), N(10), N(9)]));
        // Ctrl takes one out, and puts one in (which is then in hand).
        s.click(&d, N(7), Click::Toggle);
        s.click(&d, N(6), Click::Toggle);
        assert_eq!((s.active, s.nodes.clone()), (Some(N(6)), vec![N(12), N(10), N(9), N(6)]));
        // The last one out leaves nothing.
        s.select_only(N(6));
        s.click(&d, N(6), Click::Toggle);
        assert_eq!((s.active, s.nodes.len()), (None, 0));
        // A range with nothing in hand is a plain click.
        s.click(&d, N(8), Click::Range);
        assert_eq!(s.nodes, [N(8)]);
    }

    #[test]
    fn the_selection_acts_on_its_tops_and_lets_go_of_what_is_gone() {
        let mut d = doc();
        let mut s = Selection::default();
        for n in [12, 8, 7, 6] {
            s.click(&d, N(n), Click::Toggle);
        }
        // The circle is in the group, which stands for it. Back to front.
        assert_eq!(s.tops(&d), [N(6), N(7), N(12)]);
        s.toggle_open(N(7));
        d.apply(&ink_doc::Command::Delete { nodes: vec![N(7)] }).unwrap();
        s.prune(&d);
        assert_eq!((s.nodes.clone(), s.active, s.flipped.len()), (vec![N(12), N(6)], Some(N(6)), 0));
    }
}
