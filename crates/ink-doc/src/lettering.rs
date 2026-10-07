//! What a text says, written (ARCHITECTURE §5.5): `Command::SetText`
//! gives a `<text>` new words. SVG has no line breaks, so each line
//! after the first is a `<tspan>` that starts back at the text's `x`, a
//! line further down (`dy`, in ems, so it keeps up with the font's
//! size); a stretch with lettering or paint of its own is a `<tspan>`
//! saying so.

use ink_geom::number;

use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::{Child, Content, Element};
use crate::style::prop;
use crate::styling;

/// How far apart lines are set, in ems, unless told.
pub const LEADING: f64 = 1.2;

/// A stretch of a line, with what it sets for itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    /// Properties of its own (`fill`, `font-weight`, …), written as
    /// its attributes.
    pub set: Vec<(String, String)>,
}

impl Span {
    /// A stretch that says `text` as the text around it does.
    pub fn plain(text: impl Into<String>) -> Span {
        Span { text: text.into(), set: Vec::new() }
    }
}

/// Character data as a file writes it.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Put `text` after what's there: more of the same character data, or
/// a new run of it.
fn say(out: &mut Vec<Content>, text: &str) {
    match out.last_mut() {
        Some(Content::Text(said)) => said.push_str(&escaped(text)),
        _ => out.push(Content::Text(escaped(text))),
    }
}

/// White space that only shows if it's kept as written: at either end
/// of a line, or more than one in a row.
fn needs_keeping(lines: &[Vec<Span>]) -> bool {
    lines.iter().any(|line| {
        let said: String = line.iter().map(|span| span.text.as_str()).collect();
        said.starts_with(' ') || said.ends_with(' ') || said.contains("  ") || said.contains('\t')
    })
}

/// What goes inside a `<text>` whose `x` is `x` for it to say `lines`,
/// set `leading` ems apart. A line with nothing on it is the space
/// before the next one: an empty `<tspan>` would move nothing.
pub fn content(lines: &[Vec<Span>], x: &str, leading: f64) -> Vec<Content> {
    let mut out = Vec::new();
    let (mut written, mut skipped) = (0, 0);
    for line in lines {
        let mut inside = Vec::new();
        for span in line.iter().filter(|span| !span.text.is_empty()) {
            if span.set.is_empty() {
                say(&mut inside, &span.text);
            } else {
                let mut own = Element::new("tspan");
                for (name, value) in &span.set {
                    own = own.with(name, value.as_str());
                }
                say(&mut own.children, &span.text);
                inside.push(Content::Element(own));
            }
        }
        if inside.is_empty() {
            skipped += 1;
            continue;
        }
        let down = skipped + usize::from(written > 0);
        if down == 0 {
            out.extend(inside);
        } else {
            let mut row = Element::new("tspan").with("x", x).with("dy", format!("{}em", number::format(leading * down as f64, 3)));
            row.children = inside;
            out.push(Content::Element(row));
        }
        (written, skipped) = (written + 1, 0);
    }
    out
}

/// A new `<text>` that says `lines`, `leading` ems apart, with `attrs`
/// as its attributes (its `x` among them, where it has one).
pub fn element(attrs: &[(String, String)], lines: &[Vec<Span>], leading: f64) -> Result<Element, DocError> {
    check(lines)?;
    if !(leading.is_finite() && leading > 0.0) {
        return invalid("lines are set some way apart: give a line height over 0 (1.2 is usual)");
    }
    let mut text = attrs.iter().fold(Element::new("text"), |text, (name, value)| text.with(name, value.as_str()));
    if needs_keeping(lines) {
        text = text.with("xml:space", "preserve");
    }
    text.children = content(lines, text.attr("x").unwrap_or("0"), leading);
    Ok(text)
}

/// Whether `lines` can be written: each stretch's characters are ones
/// a file can hold, and its properties are SVG's.
pub fn check(lines: &[Vec<Span>]) -> Result<(), DocError> {
    for span in lines.iter().flatten() {
        if let Some(c) = span.text.chars().find(|c| c.is_control() && *c != '\t') {
            return invalid(format!("a line of text can't hold the control character U+{:04X} (a line break goes between lines, not in one)", c as u32));
        }
        for (name, value) in &span.set {
            styling::check(name, value).map_err(DocError::Invalid)?;
        }
    }
    Ok(())
}

impl Document {
    /// Make the text `id` say `lines`, `leading` ems apart. Everything
    /// that was in it goes. Returns the elements now in it, and the ones
    /// taken out.
    pub(crate) fn set_text(&mut self, id: NodeId, lines: &[Vec<Span>], leading: f64) -> Result<(Vec<NodeId>, Vec<NodeId>), DocError> {
        let node = self.node(id)?;
        if node.kind != Kind::Text {
            return invalid(format!("{id} is a <{}>: only a <text> has words to set", node.name));
        }
        if !(leading.is_finite() && leading > 0.0) {
            return invalid("lines are set some way apart: give a line height over 0 (1.2 is usual)");
        }
        check(lines)?;
        // A line starts where the text does: its first x, as written.
        let x = node.attr("x").and_then(|x| x.split(|c: char| c.is_whitespace() || c == ',').find(|part| !part.is_empty())).unwrap_or("0").to_owned();
        let kept_already = node.attr("xml:space") == Some("preserve") || matches!(prop(node, "white-space"), Some("pre" | "pre-wrap" | "break-spaces"));
        let gone: Vec<NodeId> = node.elements().collect();
        for child in &gone {
            self.remove(*child)?;
        }
        let (mut children, mut made) = (Vec::new(), Vec::new());
        for part in content(lines, &x, leading) {
            children.push(match part {
                Content::Text(raw) => Child::Text(raw),
                Content::Element(el) => {
                    let new = self.graft(el, Some(id))?;
                    made.push(new);
                    Child::Node(new)
                }
            });
        }
        self.edit(id)?.children = children;
        if needs_keeping(lines) && !kept_already {
            self.set_attr(id, "xml:space", Some("preserve"))?;
        }
        Ok((made, gone))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;
    use crate::text;

    /// Lines as a test writes them: each a row of words and what they
    /// set for themselves.
    type Lines<'a> = &'a [&'a [(&'a str, &'a [(&'a str, &'a str)])]];

    fn spans(lines: Lines) -> Vec<Vec<Span>> {
        lines.iter().map(|line| line.iter().map(|(text, set)| Span { text: (*text).to_owned(), set: set.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect() }).collect()).collect()
    }

    /// The text that is `N2`, after it's made to say `lines`.
    fn after(element: &str, lines: Lines, leading: Option<f64>) -> Result<String, DocError> {
        let mut d = Document::parse(DocId(1), &format!("<svg>{element}</svg>")).unwrap();
        d.apply(&Command::SetText { node: NodeId(2), lines: spans(lines), leading })?;
        d.markup(NodeId(2))
    }

    #[test]
    fn a_text_is_given_its_words() {
        assert_eq!(after(r#"<text x="4" y="9" fill="red">old <tspan>words</tspan></text>"#, &[&[("Hop on", &[])]], None).unwrap(), r#"<text x="4" y="9" fill="red">Hop on</text>"#);
        assert_eq!(after("<text/>", &[&[("a < b & c", &[])]], None).unwrap(), "<text>a &lt; b &amp; c</text>");
        assert_eq!(after("<text>gone</text>", &[], None).unwrap(), "<text></text>");
        // A stretch with paint of its own is a span; plain ones beside
        // it are the text's own words.
        assert_eq!(after("<text/>", &[&[("Hi ", &[]), ("pop", &[("fill", "#c33"), ("font-weight", "bold")]), (" on", &[]), ("", &[("fill", "red")])]], None).unwrap(), "<text>Hi <tspan fill=\"#c33\" font-weight=\"bold\">pop</tspan> on</text>");
    }

    #[test]
    fn each_line_after_the_first_starts_back_at_x_a_line_down() {
        let two = after(r#"<text x="4 5" y="9">old</text>"#, &[&[("one", &[])], &[("two ", &[]), ("red", &[("fill", "red")])], &[], &[], &[("five", &[])]], None).unwrap();
        assert_eq!(two, "<text x=\"4 5\" y=\"9\">one<tspan x=\"4\" dy=\"1.2em\">two <tspan fill=\"red\">red</tspan></tspan><tspan x=\"4\" dy=\"3.6em\">five</tspan></text>", "an empty line is the space before the next");
        assert_eq!(after("<text/>", &[&[], &[("low", &[])]], Some(1.5)).unwrap(), "<text><tspan x=\"0\" dy=\"1.5em\">low</tspan></text>");
        // What it says reads back as it was given.
        let d = Document::parse(DocId(1), &format!("<svg>{two}</svg>")).unwrap();
        assert_eq!(text::said(&d, d.node(NodeId(2)).unwrap()), "onetwo redfive", "a line break isn't a character: the lines are set apart, not spaced");
    }

    #[test]
    fn white_space_that_would_collapse_is_kept() {
        assert_eq!(after("<text/>", &[&[(" a  b", &[])]], None).unwrap(), "<text xml:space=\"preserve\"> a  b</text>");
        assert_eq!(after("<text style=\"white-space: pre\"/>", &[&[("a ", &[])]], None).unwrap(), "<text style=\"white-space: pre\">a </text>", "kept already");
        assert_eq!(after("<text/>", &[&[("a b", &[])]], None).unwrap(), "<text>a b</text>");
    }

    #[test]
    fn what_cannot_be_written_is_refused() {
        let refused = |element: &str, lines: Lines, leading: Option<f64>| after(element, lines, leading).unwrap_err().to_string();
        assert!(refused("<rect/>", &[&[("a", &[])]], None).contains("N2 is a <rect>: only a <text> has words to set"));
        assert!(refused("<text/>", &[&[("a\nb", &[])]], None).contains("U+000A"));
        assert!(refused("<text/>", &[&[("a", &[("fil", "red")])]], None).contains("fill"), "a near miss names what it was near");
        assert!(refused("<text/>", &[&[("a", &[])]], Some(0.0)).contains("line height"));
        // The same words again change nothing.
        let mut d = Document::parse(DocId(1), "<svg><text>same</text></svg>").unwrap();
        assert!(d.apply(&Command::SetText { node: NodeId(2), lines: vec![vec![Span::plain("same")]], leading: None }).unwrap().is_nothing());
    }
}
