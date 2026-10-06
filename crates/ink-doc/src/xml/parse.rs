//! Text to elements, losing nothing: every byte read ends up in the tree
//! (in a name, an attribute, a piece of formatting, or the raw text
//! between elements), so the writer can put the same bytes back.
//!
//! Strict where it matters: text that isn't well-formed is refused,
//! saying where and why, rather than "repaired" into something the file
//! never said.

use crate::error::DocError;
use crate::node::{Attr, Content, Element, Written, keep};

/// The deepest elements may nest. Real drawings are well under 32.
pub(crate) const MAX_DEPTH: usize = 256;
/// The most elements one text may hold.
pub(crate) const MAX_NODES: usize = 1_000_000;

/// A whole document: its root element, and the text around it (the XML
/// declaration, a DOCTYPE, comments, the final newline).
#[derive(Debug, PartialEq)]
pub(crate) struct Parsed {
    pub before: String,
    pub root: Element,
    pub after: String,
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    i: usize,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

impl<'a> Reader<'a> {
    fn new(text: &'a str) -> Reader<'a> {
        Reader { text, bytes: text.as_bytes(), i: 0 }
    }

    /// A syntax error at byte `at`, as a line and a column.
    fn err(&self, at: usize, message: impl Into<String>) -> DocError {
        let before = &self.text[..at.min(self.text.len())];
        let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        DocError::Syntax { line, column, message: message.into() }
    }

    fn rest(&self) -> &'a str {
        &self.text[self.i..]
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    /// The whitespace here, stepped over.
    fn space(&mut self) -> &'a str {
        let from = self.i;
        while self.peek().is_some_and(is_space) {
            self.i += 1;
        }
        &self.text[from..self.i]
    }

    /// Step past the next `end`; `what` names the thing left open if
    /// there is none.
    fn past(&mut self, end: &str, what: &str) -> Result<(), DocError> {
        match self.rest().find(end) {
            Some(k) => {
                self.i += k + end.len();
                Ok(())
            }
            None => Err(self.err(self.i, format!("{what} is never closed (no \"{end}\")"))),
        }
    }

    /// A name: everything up to whitespace or a character a tag gives
    /// meaning to.
    fn name(&mut self) -> &'a str {
        let from = self.i;
        while self.peek().is_some_and(|b| !is_space(b) && !matches!(b, b'/' | b'>' | b'=' | b'<' | b'"' | b'\'')) {
            self.i += 1;
        }
        &self.text[from..self.i]
    }

    /// A `<!DOCTYPE …>`, stepped over: quotes and an internal subset in
    /// brackets may hold `>`.
    fn doctype(&mut self) -> Result<(), DocError> {
        let start = self.i;
        let mut depth = 0usize;
        while let Some(b) = self.peek() {
            self.i += 1;
            match b {
                b'"' | b'\'' => match self.rest().find(b as char) {
                    Some(k) => self.i += k + 1,
                    None => break,
                },
                b'[' => depth += 1,
                b']' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => return Ok(()),
                _ => {}
            }
        }
        Err(self.err(start, "this DOCTYPE is never closed"))
    }

    /// Comments, processing instructions and whitespace, stepped over.
    /// With `doctype`, a DOCTYPE too (before the root).
    fn misc(&mut self, doctype: bool) -> Result<(), DocError> {
        loop {
            self.space();
            let rest = self.rest();
            if rest.starts_with("<!--") {
                self.past("-->", "this comment")?;
            } else if rest.starts_with("<?") {
                self.past("?>", "this processing instruction")?;
            } else if doctype && rest.get(..9).is_some_and(|s| s.eq_ignore_ascii_case("<!DOCTYPE")) {
                self.doctype()?;
            } else {
                return Ok(());
            }
        }
    }

    /// The opening tag at `self.i` (which is at its `<`): the element,
    /// childless so far, and whether the tag closed it (`/>`).
    fn open_tag(&mut self) -> Result<(Element, bool), DocError> {
        let start = self.i;
        self.i += 1;
        let name = self.name();
        if name.is_empty() {
            return Err(self.err(start, "a \"<\" that starts no tag (a literal one is written \"&lt;\")"));
        }
        let mut attrs = Vec::new();
        loop {
            let lead = self.space();
            let closing = match self.peek() {
                Some(b'>') => Some((false, 1)),
                Some(b'/') if self.bytes.get(self.i + 1) == Some(&b'>') => Some((true, 2)),
                None => return Err(self.err(start, format!("the tag <{name} is never closed"))),
                _ => None,
            };
            if let Some((self_closing, len)) = closing {
                self.i += len;
                let written = Written { tail: keep(lead), self_closing, close_tail: keep("") };
                return Ok((Element { name: name.to_owned(), attrs, children: Vec::new(), written }, self_closing));
            }
            let at = self.i;
            let attr = self.name();
            if attr.is_empty() {
                return Err(self.err(at, format!("unexpected \"{}\" in the tag <{name}", self.rest().chars().next().unwrap_or(' '))));
            }
            // The `=`, with whatever space the file has around it.
            let eq_from = self.i;
            self.space();
            if self.peek() != Some(b'=') {
                return Err(self.err(at, format!("the attribute \"{attr}\" has no value (it needs =\"…\")")));
            }
            self.i += 1;
            self.space();
            let eq = &self.text[eq_from..self.i];
            let quote = match self.peek() {
                Some(q @ (b'"' | b'\'')) => q,
                _ => return Err(self.err(self.i, format!("the value of \"{attr}\" isn't in quotes"))),
            };
            self.i += 1;
            let value_from = self.i;
            let Some(len) = self.rest().find(quote as char) else {
                return Err(self.err(value_from - 1, format!("the value of \"{attr}\" is never closed")));
            };
            self.i += len + 1;
            attrs.push(Attr::read(attr, &self.text[value_from..value_from + len], lead, eq, quote));
        }
    }

    /// The tree from here. With `fragment`, the text is the inside of an
    /// element that isn't there (any number of elements, text between
    /// them), returned as that element's children; without, `self.i` is
    /// at the root's `<` and reading ends with its closing tag.
    fn tree(&mut self, fragment: bool) -> Result<Element, DocError> {
        // The open elements, each with where its tag started.
        let mut stack: Vec<(Element, usize)> = Vec::new();
        if fragment {
            stack.push((Element::new(""), 0));
        }
        let mut count = 0usize;
        loop {
            // Raw text, up to the next tag.
            if !stack.is_empty() {
                let text_from = self.i;
                loop {
                    let Some(lt) = self.rest().find('<') else {
                        self.i = self.text.len();
                        break;
                    };
                    self.i += lt;
                    let rest = self.rest();
                    if rest.starts_with("<!--") {
                        self.past("-->", "this comment")?;
                    } else if rest.starts_with("<![CDATA[") {
                        self.past("]]>", "this CDATA section")?;
                    } else if rest.starts_with("<?") {
                        self.past("?>", "this processing instruction")?;
                    } else {
                        break;
                    }
                }
                if self.i > text_from
                    && let Some((open, _)) = stack.last_mut()
                {
                    open.children.push(Content::Text(self.text[text_from..self.i].to_owned()));
                }
            }
            if self.i >= self.text.len() {
                return match stack.pop() {
                    Some((root, _)) if fragment && stack.is_empty() => Ok(root),
                    Some((open, at)) => Err(self.err(at, format!("<{}> is never closed", open.name))),
                    None => Err(self.err(self.i, "there is no element here")),
                };
            }
            let at = self.i;
            if self.rest().starts_with("</") {
                self.i += 2;
                let name = self.name();
                let close_tail = self.space();
                if self.peek() != Some(b'>') {
                    return Err(self.err(at, format!("the closing tag </{name} is never closed")));
                }
                self.i += 1;
                // A fragment's own stand-in root is never the one closed.
                let Some((mut done, _)) = (stack.len() > usize::from(fragment)).then(|| stack.pop()).flatten() else {
                    return Err(self.err(at, format!("</{name}> closes nothing that's open")));
                };
                if done.name != name {
                    return Err(self.err(at, format!("</{name}> closes <{}>: the names don't match", done.name)));
                }
                done.written.close_tail = keep(close_tail);
                match stack.last_mut() {
                    Some((parent, _)) => parent.children.push(Content::Element(done)),
                    None => return Ok(done),
                }
                continue;
            }
            if self.rest().starts_with("<!") {
                return Err(self.err(at, "a declaration (\"<!…\") can't be inside an element"));
            }
            let (element, closed) = self.open_tag()?;
            count += 1;
            if count > MAX_NODES {
                return Err(DocError::TooBig(format!("more than {MAX_NODES} elements")));
            }
            if !closed {
                if stack.len() >= MAX_DEPTH + usize::from(fragment) {
                    return Err(DocError::TooBig(format!("elements nested more than {MAX_DEPTH} deep")));
                }
                stack.push((element, at));
            } else {
                match stack.last_mut() {
                    Some((parent, _)) => parent.children.push(Content::Element(element)),
                    None => return Ok(element),
                }
            }
        }
    }
}

/// Read a whole document.
pub(crate) fn parse(text: &str) -> Result<Parsed, DocError> {
    let mut r = Reader::new(text);
    // A byte-order mark stays in what's kept before the root.
    r.i = if text.starts_with('\u{feff}') { 3 } else { 0 };
    r.misc(true)?;
    let root_at = r.i;
    match r.peek() {
        Some(b'<') => {}
        Some(_) => return Err(r.err(r.i, "text before the first element")),
        None => return Err(DocError::NotSvg("there is nothing in it".into())),
    }
    let root = r.tree(false)?;
    let after_at = r.i;
    r.misc(false)?;
    if r.i < text.len() {
        return Err(r.err(r.i, "something after the root element's end (a document has one root)"));
    }
    Ok(Parsed { before: text[..root_at].to_owned(), root, after: text[after_at..].to_owned() })
}

/// Read markup that goes inside an element: any number of elements, with
/// whatever text is between them.
pub(crate) fn parse_fragment(text: &str) -> Result<Vec<Content>, DocError> {
    Ok(Reader::new(text).tree(true)?.children)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syntax(text: &str) -> (usize, usize, String) {
        match parse(text) {
            Err(DocError::Syntax { line, column, message }) => (line, column, message),
            other => panic!("{text:?} gave {other:?}"),
        }
    }

    #[test]
    fn reads_elements_attributes_and_what_is_between() {
        let p = parse("<?xml version=\"1.0\"?>\n<!-- c -->\n<svg viewBox='0 0 16 16'>\n  <g fill = \"#f00\"><path d=\"M0 0h1\" /><circle r=\"2\"></circle ></g>\n</svg>\n").unwrap();
        assert_eq!(p.before, "<?xml version=\"1.0\"?>\n<!-- c -->\n");
        assert_eq!(p.after, "\n");
        assert_eq!((p.root.name.as_str(), p.root.attr("viewBox")), ("svg", Some("0 0 16 16")));
        assert_eq!(p.root.attrs[0].quote, b'\'');
        let [Content::Text(a), Content::Element(g), Content::Text(b)] = p.root.children.as_slice() else { panic!("{:?}", p.root.children) };
        assert_eq!((a.as_str(), b.as_str()), ("\n  ", "\n"));
        assert_eq!((g.attr("fill"), g.attrs[0].eq.as_ref()), (Some("#f00"), " = "));
        let [Content::Element(path), Content::Element(circle)] = g.children.as_slice() else { panic!() };
        assert!(path.written.self_closing && path.written.tail == " ");
        assert!(!circle.written.self_closing && circle.written.close_tail == " " && circle.children.is_empty());
    }

    #[test]
    fn comments_cdata_and_instructions_are_raw_text() {
        let p = parse("<svg>a<!-- <not a=\"tag\"> -->b<![CDATA[ </svg> ]]>c<?pi <x> ?>d<g/>e</svg>").unwrap();
        let [Content::Text(before), Content::Element(_), Content::Text(after)] = p.root.children.as_slice() else { panic!("{:?}", p.root.children) };
        assert_eq!(before, "a<!-- <not a=\"tag\"> -->b<![CDATA[ </svg> ]]>c<?pi <x> ?>d");
        assert_eq!(after, "e");
    }

    #[test]
    fn a_doctype_with_a_subset_is_stepped_over() {
        let text = "\u{feff}<?xml version=\"1.0\"?>\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [\n  <!ENTITY ns \"http://a/>b\">\n]>\n<svg a=\"&ns;\"/>";
        let p = parse(text).unwrap();
        assert!(p.before.starts_with('\u{feff}') && p.before.ends_with("]>\n"));
        assert_eq!(p.root.attr("a"), Some("&ns;"), "an entity only the DOCTYPE knows stays as written");
        assert!(p.after.is_empty());
    }

    #[test]
    fn what_is_not_well_formed_is_refused_with_its_place() {
        assert_eq!(syntax("<svg>\n  <g>\n</svg>"), (3, 1, "</svg> closes <g>: the names don't match".into()));
        assert_eq!(syntax("<svg>\n  <g>"), (2, 3, "<g> is never closed".into()));
        assert_eq!(syntax("<svg a=1/>").2, "the value of \"a\" isn't in quotes");
        assert_eq!(syntax("<svg hidden/>").2, "the attribute \"hidden\" has no value (it needs =\"…\")");
        assert_eq!(syntax("<svg a=\"1/>").2, "the value of \"a\" is never closed");
        assert_eq!(syntax("<svg>1 < 2</svg>").2, "a \"<\" that starts no tag (a literal one is written \"&lt;\")");
        assert_eq!(syntax("<svg><!-- oops</svg>").2, "this comment is never closed (no \"-->\")");
        assert_eq!(syntax("<svg/><svg/>").2, "something after the root element's end (a document has one root)");
        assert_eq!(syntax("hello <svg/>").2, "text before the first element");
        assert_eq!(syntax("<svg></svg></g>").2, "something after the root element's end (a document has one root)");
        assert_eq!(syntax("<svg a=\"1\"").2, "the tag <svg is never closed");
        assert_eq!(syntax("<!-- é -->\n<svg é=\"1\" \"2\"/>"), (2, 12, "unexpected \"\"\" in the tag <svg".into()), "columns count characters, not bytes");
        assert_eq!(parse(""), Err(DocError::NotSvg("there is nothing in it".into())));
        assert_eq!(parse("  <!-- only a comment -->  "), Err(DocError::NotSvg("there is nothing in it".into())));
    }

    #[test]
    fn hostile_nesting_and_counts_are_refused() {
        let deep = format!("{}{}", "<g>".repeat(MAX_DEPTH + 1), "</g>".repeat(MAX_DEPTH + 1));
        assert!(matches!(parse(&deep), Err(DocError::TooBig(_))));
        let ok = format!("{}{}", "<g>".repeat(MAX_DEPTH), "</g>".repeat(MAX_DEPTH));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn a_fragment_is_any_number_of_elements() {
        let f = parse_fragment(" <rect x=\"1\"/>\n<g><circle/></g> tail").unwrap();
        assert_eq!(f.len(), 5);
        assert!(matches!(&f[1], Content::Element(e) if e.name == "rect"));
        assert!(matches!(&f[4], Content::Text(t) if t == " tail"));
        assert!(parse_fragment("").unwrap().is_empty());
        assert!(matches!(parse_fragment("<g>"), Err(DocError::Syntax { message, .. }) if message == "<g> is never closed"));
        assert!(matches!(parse_fragment("<g/></g>"), Err(DocError::Syntax { message, .. }) if message == "</g> closes nothing that's open"));
    }
}
