//! How markup sits on its lines (ARCHITECTURE §3.2). A file's
//! indentation is its own: Ink reads what the siblings have and writes
//! the same, so what's put in looks as if it had always been there.

use crate::document::Document;
use crate::error::DocError;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::{Child, Node};

/// One step of indentation, where the file gives no other to copy.
const UNIT: &str = "  ";
/// What XML counts as white space.
const SPACE: [char; 4] = [' ', '\t', '\n', '\r'];

/// The line break and indentation `text` ends with, when it ends at the
/// start of a line: what puts the element after it on a line of its own.
pub(crate) fn indentation(text: &str) -> Option<&str> {
    let nl = text.rfind('\n')?;
    if !text[nl + 1..].bytes().all(|b| b == b' ' || b == b'\t') {
        return None;
    }
    Some(&text[if text[..nl].ends_with('\r') { nl - 1 } else { nl }..])
}

/// The indentation of the child at `idx`: what the text before it ends
/// with.
pub(crate) fn indent_before(children: &[Child], idx: usize) -> Option<String> {
    match idx.checked_sub(1).and_then(|i| children.get(i)) {
        Some(Child::Text(text)) => indentation(text).map(str::to_owned),
        _ => None,
    }
}

/// How much of `text` its first piece that isn't white space takes: a
/// comment, a CDATA section or a processing instruction whole, whatever
/// is in it; anything else up to the next white space or markup.
fn piece(text: &str) -> usize {
    let closed = |open: &str, close: &str| text.strip_prefix(open).map(|rest| open.len() + rest.find(close).map_or(rest.len(), |at| at + close.len()));
    if let Some(len) = closed("<!--", "-->").or_else(|| closed("<![CDATA[", "]]>")).or_else(|| closed("<?", "?>")) {
        return len;
    }
    let first = text.chars().next().map_or(0, char::len_utf8);
    text[first..].find(|c: char| SPACE.contains(&c) || c == '<').map_or(text.len(), |at| first + at)
}

/// `gap` (the white space and comments between two tags) with every
/// line that starts in it indented by `inner`, and ending in `last`:
/// each a line break and the indentation after it. Blank lines, and a
/// comment that shares a line with the tag before it, stay as they are.
fn line_up(gap: &str, inner: &str, last: &str) -> String {
    let mut out = String::with_capacity(gap.len() + last.len());
    let mut rest = gap;
    loop {
        let (run, after) = rest.split_at(rest.len() - rest.trim_start_matches(SPACE).len());
        let lines = indentation(run).map(|tail| &run[..run.len() - tail.len()]);
        if after.is_empty() {
            out.push_str(lines.unwrap_or(""));
            out.push_str(last);
            return out;
        }
        match lines {
            Some(lines) => {
                out.push_str(lines);
                out.push_str(inner);
            }
            None => out.push_str(run),
        }
        let (kept, more) = after.split_at(piece(after));
        out.push_str(kept);
        rest = more;
    }
}

/// `gap` with every line that starts in it and is indented by `was`
/// (or by more) indented by `now` instead, what's more kept.
fn reindent(gap: &str, was: &str, now: &str) -> String {
    let mut out = String::with_capacity(gap.len() + now.len());
    let mut rest = gap;
    while !rest.is_empty() {
        let (run, after) = rest.split_at(rest.len() - rest.trim_start_matches(SPACE).len());
        match indentation(run).and_then(|tail| tail.trim_start_matches(['\r', '\n']).strip_prefix(was).map(|more| (tail, more))) {
            Some((tail, more)) => {
                out.push_str(&run[..run.len() - tail.trim_start_matches(['\r', '\n']).len()]);
                out.push_str(now);
                out.push_str(more);
            }
            None => out.push_str(run),
        }
        let (kept, more) = after.split_at(piece(after).min(after.len()));
        out.push_str(kept);
        rest = more;
    }
    out
}

/// Whether `raw` is only white space, comments and processing
/// instructions: nothing that is said.
fn is_gap(raw: &str) -> bool {
    let mut rest = raw.trim_start_matches(SPACE);
    while rest.starts_with("<!--") || rest.starts_with("<?") {
        rest = rest[piece(rest)..].trim_start_matches(SPACE);
    }
    rest.is_empty()
}

/// Whether what's inside `node` may be set on lines of its own: only
/// where white space means nothing. Not among words (a `<text>`'s, a
/// `<title>`'s, a `<style>`'s rules), nor in what isn't SVG's.
fn takes_lines(node: &Node) -> bool {
    let words = matches!(node.kind, Kind::Text | Kind::TSpan | Kind::Style | Kind::Title | Kind::Desc | Kind::Metadata | Kind::Other);
    !words && node.children.iter().all(|c| matches!(c, Child::Node(_)) || matches!(c, Child::Text(raw) if is_gap(raw)))
}

impl Document {
    /// One step of indentation, as this file takes them: what `node`'s
    /// own indentation (`own`, without its line break) divides into by
    /// its depth.
    pub(crate) fn unit(&self, node: NodeId, own: &str) -> String {
        let levels = self.depth(node) - 1;
        if levels > 0 && !own.is_empty() && own.len().is_multiple_of(levels) { own[..own.len() / levels].to_owned() } else { UNIT.to_owned() }
    }

    /// Lay what's inside `id` out as the file is laid out around it:
    /// every element in it on a line of its own, one step further in
    /// than its parent, however the markup came. Where `id` has no line
    /// of its own (a file all on one line), nothing moves.
    pub(crate) fn lay_out(&mut self, id: NodeId) -> Result<(), DocError> {
        let Some(parent) = self.node(id)?.parent else { return Ok(()) };
        let siblings = &self.node(parent)?.children;
        let Some(own) = siblings.iter().position(|c| *c == Child::Node(id)).and_then(|i| indent_before(siblings, i)) else { return Ok(()) };
        let unit = self.unit(id, own.trim_start_matches(['\r', '\n']));
        self.lay_inside(id, &own, &unit)
    }

    /// The line break and indentation `id`'s own line starts with, when
    /// it has a line of its own.
    pub(crate) fn own_line(&self, id: NodeId) -> Option<String> {
        let siblings = &self.get(self.get(id)?.parent?)?.children;
        siblings.iter().position(|c| *c == Child::Node(id)).and_then(|i| indent_before(siblings, i))
    }

    /// Shift the lines inside `id` along with it: it stood on a line
    /// indented by `was` and now stands on one indented by `now` (each
    /// as [`Document::own_line`] gives it). What's inside keeps its
    /// layout, as far in or out as `id` itself went. Words are left
    /// alone, as ever.
    pub(crate) fn shift(&mut self, id: NodeId, was: &str, now: &str) -> Result<(), DocError> {
        let (was, now) = (was.trim_start_matches(['\r', '\n']), now.trim_start_matches(['\r', '\n']));
        if was == now {
            return Ok(());
        }
        let mut inside = vec![id];
        while let Some(id) = inside.pop() {
            let node = self.node(id)?;
            if !takes_lines(node) {
                continue;
            }
            inside.extend(node.elements());
            let shifted: Vec<Child> = node
                .children
                .iter()
                .map(|child| match child {
                    Child::Text(gap) => Child::Text(reindent(gap, was, now)),
                    Child::Node(id) => Child::Node(*id),
                })
                .collect();
            if shifted != node.children {
                self.edit(id)?.children = shifted;
            }
        }
        Ok(())
    }

    /// The same, for a node whose line starts with `own` (its line break
    /// and indentation).
    fn lay_inside(&mut self, id: NodeId, own: &str, unit: &str) -> Result<(), DocError> {
        let node = self.node(id)?;
        let elements: Vec<NodeId> = node.elements().collect();
        if elements.is_empty() || !takes_lines(node) {
            return Ok(());
        }
        let inner = format!("{own}{unit}");
        let mut laid: Vec<Child> = Vec::with_capacity(elements.len() * 2 + 1);
        let mut gap = String::new();
        for child in &node.children {
            match child {
                Child::Text(raw) => gap.push_str(raw),
                Child::Node(element) => {
                    laid.push(Child::Text(line_up(&gap, &inner, &inner)));
                    laid.push(Child::Node(*element));
                    gap.clear();
                }
            }
        }
        laid.push(Child::Text(line_up(&gap, &inner, own)));
        if laid != node.children {
            self.edit(id)?.children = laid;
        }
        for element in elements {
            self.lay_inside(element, &inner, unit)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, elements};
    use crate::edit::Place;
    use crate::id::DocId;

    /// `markup` put into `file` as its root's last child.
    fn put(file: &str, markup: &str) -> String {
        let mut d = Document::parse(DocId(1), file).unwrap();
        let root = d.root();
        d.apply(&Command::Insert { place: Place::LastIn(root), elements: elements(markup).unwrap() }).unwrap();
        d.to_svg()
    }

    #[test]
    fn markup_on_one_line_is_laid_out_like_the_file() {
        let defs = "<defs><linearGradient id=\"g\"><stop offset=\"0\"/><stop offset=\"1\"/></linearGradient></defs>";
        assert_eq!(put("<svg>\n</svg>\n", defs), "<svg>\n  <defs>\n    <linearGradient id=\"g\">\n      <stop offset=\"0\"/>\n      <stop offset=\"1\"/>\n    </linearGradient>\n  </defs>\n</svg>\n");
        // In the file's own steps and line ends.
        assert_eq!(put("<svg>\r\n\t<a/>\r\n</svg>", "<g><b/></g>"), "<svg>\r\n\t<a/>\r\n\t<g>\r\n\t\t<b/>\r\n\t</g>\r\n</svg>");
        assert_eq!(put("<svg>\n    <a/>\n</svg>", "<g><g><b/></g></g>"), "<svg>\n    <a/>\n    <g>\n        <g>\n            <b/>\n        </g>\n    </g>\n</svg>");
        // A file all on one line stays so.
        assert_eq!(put("<svg><a/></svg>", "<g><b/></g>"), "<svg><a/><g><b/></g></svg>");
    }

    #[test]
    fn markup_with_lines_of_its_own_takes_the_files() {
        // Written from the margin, and written four spaces deep.
        let want = "<svg>\n  <g>\n    <a/>\n    <b/>\n  </g>\n</svg>";
        assert_eq!(put("<svg>\n</svg>", "<g>\n  <a/>\n  <b/>\n</g>"), want);
        assert_eq!(put("<svg>\n</svg>", "\n        <g>\n            <a/><b/>\n        </g>\n"), want);
        // Blank lines and comments are kept; a comment after a tag stays
        // on its line.
        assert_eq!(put("<svg>\n</svg>", "<g>\n\n<!-- the eyes -->\n<a/> <!-- left -->\n<b/></g>"), "<svg>\n  <g>\n\n    <!-- the eyes -->\n    <a/> <!-- left -->\n    <b/>\n  </g>\n</svg>");
    }

    #[test]
    fn words_are_left_as_written() {
        // White space among a <text>'s words is part of them.
        let text = "<text x=\"1\">Hello <tspan>big</tspan><tspan>world</tspan></text>";
        assert_eq!(put("<svg>\n</svg>", &format!("<g>{text}<title>a  b</title></g>")), format!("<svg>\n  <g>\n    {text}\n    <title>a  b</title>\n  </g>\n</svg>"));
        // Nor is anything moved inside what isn't SVG's.
        let other = "<metadata><x:y xmlns:x=\"urn:x\"><x:z/></x:y></metadata>";
        assert_eq!(put("<svg>\n</svg>", other), format!("<svg>\n  {other}\n</svg>"));
        // Stray character data keeps a group as it was written.
        assert_eq!(put("<svg>\n</svg>", "<g>&#32;<a/></g>"), "<svg>\n  <g>&#32;<a/></g>\n</svg>");
    }

    #[test]
    fn what_is_inside_a_moved_node_moves_in_or_out_with_it() {
        let mut d = Document::parse(DocId(1), "<svg>\n  <g>\n    <a>\n      <!-- note -->\n      <b/>\n\n    </a>\n  </g>\n  <defs>\n   <path/>\n  </defs>\n  <text>\n    words\n  </text>\n</svg>").unwrap();
        let ids: std::collections::HashMap<String, NodeId> = d.descendants(d.root()).into_iter().map(|id| (d.node(id).unwrap().name.clone(), id)).collect();
        // Out of its group, to the root: one step out, all the way down.
        d.apply(&Command::Move { nodes: vec![ids["a"]], place: Place::Before(ids["g"]) }).unwrap();
        assert!(d.to_svg().starts_with("<svg>\n  <a>\n    <!-- note -->\n    <b/>\n\n  </a>\n  <g>\n  </g>\n"), "{}", d.to_svg());
        // Into it again, with what was laid out its own way and a
        // <text>'s words as they were.
        d.apply(&Command::Move { nodes: vec![ids["a"], ids["defs"], ids["text"]], place: Place::LastIn(ids["g"]) }).unwrap();
        assert_eq!(d.to_svg(), "<svg>\n  <g>\n    <a>\n      <!-- note -->\n      <b/>\n\n    </a>\n    <defs>\n     <path/>\n    </defs>\n    <text>\n    words\n  </text>\n  </g>\n</svg>");
        assert_eq!(reindent("\n\t\t<!-- a\n b -->\n\t\t\t", "\t\t", " "), "\n <!-- a\n b -->\n \t");
        assert_eq!(reindent("\n  ", "    ", ""), "\n  ", "a line not indented that far stays");
    }

    #[test]
    fn the_gap_between_tags_lines_up() {
        assert_eq!(line_up("", "\n  ", "\n"), "\n");
        assert_eq!(line_up("   ", "\n  ", "\n  "), "\n  ");
        assert_eq!(line_up("\n\n\t\t", "\n  ", "\n  "), "\n\n  ");
        assert_eq!(line_up("<!-- a\n   b --><?pi?>", "\n  ", "\n"), "<!-- a\n   b --><?pi?>\n");
        assert!(is_gap(" \n<!-- a --> <?pi?>\n") && is_gap("") && !is_gap(" <![CDATA[ ]]>") && !is_gap("<!-- a -->b"));
        assert_eq!(line_up("\r\n<!--a-->\r\n", "\r\n\t", "\r\n"), "\r\n\t<!--a-->\r\n");
        assert_eq!(piece("<!-- never closed"), 17);
        assert_eq!(piece("é b"), 2);
    }
}
