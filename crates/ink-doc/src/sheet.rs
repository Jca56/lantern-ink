//! `<style>` rules (ARCHITECTURE §3.1): the part of CSS that drawings
//! use. A rule is a list of selectors and the declarations they give
//! what they match. A selector here is made of element names, `.class`,
//! `#id` and `*`, alone or together (`path.dim`), nested by a space
//! (somewhere inside) or `>` (directly inside). Anything else in one
//! (`[attr]`, `:hover`, `+`, `~`) and it matches nothing; an `@` rule is
//! stepped over.
//!
//! What the rules say of each node is worked out when the document is
//! read and after every change to it ([`Document::restyle`]), and kept
//! on the node, so [`crate::style::prop`] can answer from the node
//! alone.

use std::sync::Arc;

use crate::document::Document;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::Node;
use crate::style::declarations;

/// One thing a rule says of a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub name: Box<str>,
    pub value: Box<str>,
    /// `!important`: it outvotes the node's own `style`.
    pub important: bool,
}

/// One step of a selector: an element that is all of these.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Compound {
    /// Its name (`None`: any).
    element: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
}

/// How a step sits against the one to its left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nesting {
    /// Somewhere inside it.
    Within,
    /// Directly inside it.
    Child,
}

/// A selector: its steps, the rightmost (what it picks out) last, each
/// with how it sits against the one before.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Selector {
    steps: Vec<(Nesting, Compound)>,
}

impl Compound {
    /// One step, as written: `path.dim#a`. `None` for what Ink doesn't
    /// read.
    fn parse(text: &str) -> Option<Compound> {
        let mut out = Compound::default();
        let start = text.find(['.', '#']).unwrap_or(text.len());
        match &text[..start] {
            "" | "*" => {}
            name if name.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ':')) => out.element = Some(name.to_owned()),
            _ => return None,
        }
        let mut rest = &text[start..];
        while let Some(mark) = rest.chars().next() {
            let end = rest[1..].find(['.', '#']).map_or(rest.len(), |i| i + 1);
            let name = &rest[1..end];
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_')) {
                return None;
            }
            match mark {
                '.' => out.classes.push(name.to_owned()),
                _ => out.id = Some(name.to_owned()),
            }
            rest = &rest[end..];
        }
        Some(out)
    }

    fn matches(&self, node: &Node) -> bool {
        self.element.as_deref().is_none_or(|e| e == node.local())
            && self.id.as_deref().is_none_or(|id| node.attr("id") == Some(id))
            && self.classes.iter().all(|class| node.attr("class").is_some_and(|said| said.split_whitespace().any(|c| c == class)))
    }
}

impl Selector {
    fn parse(text: &str) -> Option<Selector> {
        let mut steps = Vec::new();
        let mut nesting = Nesting::Within;
        // `>` may stand alone or against its neighbours.
        for word in text.replace('>', " > ").split_whitespace() {
            if word == ">" {
                nesting = Nesting::Child;
                continue;
            }
            steps.push((nesting, Compound::parse(word)?));
            nesting = Nesting::Within;
        }
        (!steps.is_empty()).then_some(Selector { steps })
    }

    /// How much it singles out: ids, then classes, then names.
    fn weight(&self) -> (usize, usize, usize) {
        self.steps.iter().fold((0, 0, 0), |(ids, classes, names), (_, c)| (ids + usize::from(c.id.is_some()), classes + c.classes.len(), names + usize::from(c.element.is_some())))
    }

    /// Whether the first `upto` steps match with the last of them on
    /// `node`.
    fn matches(&self, doc: &Document, node: &Node, upto: usize) -> bool {
        let (nesting, step) = &self.steps[upto - 1];
        if !step.matches(node) {
            return false;
        }
        if upto == 1 {
            return true;
        }
        match nesting {
            Nesting::Child => node.parent.and_then(|p| doc.get(p)).is_some_and(|parent| self.matches(doc, parent, upto - 1)),
            Nesting::Within => doc.ancestors(node.id).any(|above| self.matches(doc, above, upto - 1)),
        }
    }
}

/// A document's rules, the ones that count for more last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    rules: Vec<(Selector, Vec<Said>)>,
}

/// `css` without its `/* comments */`.
fn uncommented(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(open) = rest.find("/*") {
        out.push_str(&rest[..open]);
        rest = rest[open + 2..].find("*/").map_or("", |close| &rest[open + 2 + close + 2..]);
    }
    out.push_str(rest);
    out
}

impl Sheet {
    /// The rules in `css`.
    pub fn parse(css: &str) -> Sheet {
        let css = uncommented(css);
        let mut rules: Vec<(Selector, Vec<Said>)> = Vec::new();
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            let head = rest[..open].trim();
            // The block's end: braces inside it (an `@media`'s rules)
            // are its own.
            let mut depth = 0usize;
            let Some(close) = rest[open..].char_indices().find_map(|(i, c)| {
                depth = match c {
                    '{' => depth + 1,
                    '}' => depth - 1,
                    _ => depth,
                };
                (depth == 0).then_some(open + i)
            }) else {
                break;
            };
            // An `@import …;` before a rule leaves its `;` in the head.
            let head = head.rsplit(';').next().unwrap_or(head).trim();
            if !head.starts_with('@') {
                let said: Vec<Said> = declarations(&rest[open + 1..close]).filter(|d| !d.name.is_empty()).map(|d| Said { name: d.name.into(), value: d.value.into(), important: d.important }).collect();
                rules.extend(head.split(',').filter_map(Selector::parse).map(|selector| (selector, said.clone())));
            }
            rest = &rest[close + 1..];
        }
        // By how much each singles out; of equals, as they came (the
        // sort keeps their order).
        rules.sort_by_key(|(selector, _)| selector.weight());
        Sheet { rules }
    }

    /// Every `<style>` in `doc`, as one sheet, in the file's order.
    pub fn of(doc: &Document) -> Sheet {
        let css: String = doc.descendants(doc.root()).into_iter().filter_map(|id| doc.get(id)).filter(|n| n.kind == Kind::Style && n.attr("type").is_none_or(|t| t.trim().eq_ignore_ascii_case("text/css"))).map(|n| n.text() + "\n").collect();
        if css.trim().is_empty() { Sheet::default() } else { Sheet::parse(&css) }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// What the rules say of `node`: every declaration of every rule
    /// that matches it, the ones that count for more last.
    fn says(&self, doc: &Document, node: &Node) -> Vec<Said> {
        self.rules.iter().filter(|(selector, _)| selector.matches(doc, node, selector.steps.len())).flat_map(|(_, said)| said.iter().cloned()).collect()
    }
}

impl Document {
    /// Work out again what the `<style>` rules say of every node. Only
    /// the nodes it changes for are touched (and stamped: they look
    /// different). A document with no rules, and none before, is left
    /// alone without a look.
    pub(crate) fn restyle(&mut self) {
        let sheet = Sheet::of(self);
        if sheet.is_empty() && !self.nodes.values().any(|n| n.ruled.is_some()) {
            return;
        }
        let changes: Vec<(NodeId, Option<Arc<[Said]>>)> = self
            .descendants(self.root)
            .into_iter()
            .filter_map(|id| {
                let node = self.get(id)?;
                let said = sheet.says(self, node);
                let same = node.ruled.as_deref().unwrap_or(&[]) == said.as_slice();
                (!same).then(|| (id, (!said.is_empty()).then(|| said.into())))
            })
            .collect();
        for (id, ruled) in changes {
            if let Ok(node) = self.edit(id) {
                node.ruled = ruled;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;
    use crate::style::prop;

    fn doc(inner: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg>{inner}</svg>")).unwrap()
    }

    /// The `fill` of each node of `inner` that has one, by element.
    fn fills(inner: &str) -> Vec<(String, String)> {
        let d = doc(inner);
        d.descendants(d.root()).into_iter().filter_map(|id| d.get(id)).filter_map(|n| Some((n.name.clone(), prop(n, "fill")?.to_owned()))).collect()
    }

    fn pairs(got: &[(String, String)]) -> Vec<(&str, &str)> {
        got.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
    }

    #[test]
    fn a_rule_gives_what_it_matches_its_declarations() {
        // An Illustrator export: classes, and a comment.
        let got = fills(r##"<style type="text/css">/* made by a program */ .st0{fill:#171516;} .st1 { fill : #314A52 ; stroke:none }</style><path class="st0"/><rect class="other st1"/><circle/>"##);
        assert_eq!(pairs(&got), [("path", "#171516"), ("rect", "#314A52")]);
        // By name, by id, by all at once, and several selectors to a rule.
        let got = fills(r##"<style>circle, #a { fill: red } path.dim.small { fill: grey } * { fill: black }</style><path id="a"/><circle/><path class="small dim"/><path class="dim"/>"##);
        assert_eq!(pairs(&got), [("svg", "black"), ("style", "black"), ("path", "red"), ("circle", "red"), ("path", "grey"), ("path", "black")]);
        // In CDATA, as XML likes it; and what isn't CSS isn't read.
        assert_eq!(pairs(&fills("<style><![CDATA[ path { fill: red } ]]></style><path/>")), [("path", "red")]);
        assert_eq!(fills(r#"<style type="text/x-other">path { fill: red }</style><path/>"#), []);
    }

    #[test]
    fn nesting_is_by_a_space_or_an_angle() {
        let inner = |css: &str| format!(r#"<style>{css}</style><g class="a"><g><path/></g><rect/></g><path/>"#);
        let filled = |css: &str| fills(&inner(css)).into_iter().map(|(name, _)| name).collect::<Vec<_>>().join(" ");
        assert_eq!(filled(".a path { fill: red }"), "path", "somewhere inside");
        assert_eq!(filled(".a > path { fill: red }"), "", "not directly inside");
        assert_eq!(filled(".a>rect{fill:red}"), "rect");
        assert_eq!(filled("svg g g path { fill: red }"), "path");
        assert_eq!(filled("g > g > path, svg > path { fill: red }"), "path path");
    }

    #[test]
    fn the_rule_that_singles_out_more_wins_and_then_the_later() {
        let fill = |css: &str, attrs: &str| fills(&format!(r#"<style>{css}</style><path id="a" class="b" {attrs}/>"#)).pop().map(|(_, v)| v);
        assert_eq!(fill("#a { fill: red } .b { fill: green } path { fill: blue }", "").as_deref(), Some("red"), "an id over a class over a name");
        assert_eq!(fill("path.b { fill: red } .b { fill: green }", "").as_deref(), Some("red"));
        assert_eq!(fill(".b { fill: red } .b { fill: green }", "").as_deref(), Some("green"), "of equals, the later");
        // An attribute is outvoted by any rule; the node's own style
        // outvotes them all, but for one that says it's important.
        assert_eq!(fill("path { fill: red }", r#"fill="blue""#).as_deref(), Some("red"));
        assert_eq!(fill("#a { fill: red }", r#"style="fill: blue""#).as_deref(), Some("blue"));
        assert_eq!(fill("path { fill: red !important }", r#"style="fill: blue""#).as_deref(), Some("red"));
    }

    #[test]
    fn what_ink_does_not_read_matches_nothing_and_breaks_nothing() {
        let css = r#"@import url("a.css"); @media print { path { fill: red } } path:hover, path[fill], g + path, g ~ path { fill: red } @font-face { font-family: x } circle { fill: green } path { fill: blue"#;
        assert_eq!(pairs(&fills(&format!("<style>{css}</style><path/><circle/>"))), [("circle", "green")], "an unclosed rule at the end is no rule");
        assert!(Sheet::parse("").is_empty() && Sheet::parse("}{ nonsense").is_empty() && Sheet::parse("{ fill: red }").is_empty());
        assert_eq!(uncommented("a/* b */c/* never closed"), "ac");
    }

    #[test]
    fn rules_are_worked_out_again_after_every_change() {
        let mut d = doc(r#"<style>.on { fill: red }</style><g><path/></g>"#);
        let path = NodeId(4);
        let fill = |d: &Document| prop(d.node(path).unwrap(), "fill").map(str::to_owned);
        assert_eq!(fill(&d), None);
        let rev = d.node(path).unwrap().rev;
        // Given the class, it has the rule's fill, and is a new revision
        // of itself; a change that isn't about it leaves it alone.
        d.apply(&Command::SetAttr { node: path, name: "class".into(), value: Some("on".into()) }).unwrap();
        assert_eq!(fill(&d).as_deref(), Some("red"));
        let styled = d.node(path).unwrap().rev;
        assert!(styled > rev);
        d.apply(&Command::SetAttr { node: NodeId(3), name: "id".into(), value: Some("g".into()) }).unwrap();
        assert_eq!(d.node(path).unwrap().rev, styled);
        // The sheet gone, so are its rules; undone, they're back.
        let before = d.snapshot();
        d.apply(&Command::Delete { nodes: vec![NodeId(2)] }).unwrap();
        assert_eq!(fill(&d), None);
        d.restore(&before);
        assert_eq!(fill(&d).as_deref(), Some("red"));
        // The file is as it was written: what rules say isn't in it.
        assert_eq!(d.to_svg(), r#"<svg><style>.on { fill: red }</style><g id="g"><path class="on"/></g></svg>"#);
    }
}
