//! Elements back to text. What was read and hasn't changed is written as
//! it was, byte for byte: each tag replays its own formatting, and an
//! attribute its own raw text. Only what changed is written afresh.

use crate::node::{Attr, Content, Element, Raw, Written};
use crate::xml::escape::escape_attr;

/// `<name attr="value"…`, up to and including the tag's end: `>`, or
/// `/>` when it closed itself and still has nothing in it.
pub(crate) fn open_tag(name: &str, attrs: &[Attr], written: &Written, empty: bool, out: &mut String) {
    out.push('<');
    out.push_str(name);
    for attr in attrs {
        out.push_str(&attr.lead);
        out.push_str(&attr.name);
        out.push_str(&attr.eq);
        out.push(attr.quote as char);
        match &attr.raw {
            Raw::Verbatim => out.push_str(&attr.value),
            Raw::Escaped(raw) => out.push_str(raw),
            Raw::Fresh => out.push_str(&escape_attr(&attr.value, attr.quote)),
        }
        out.push(attr.quote as char);
    }
    out.push_str(&written.tail);
    out.push_str(if written.self_closing && empty { "/>" } else { ">" });
}

/// `</name>`, unless the opening tag closed the element itself.
pub(crate) fn close_tag(name: &str, written: &Written, empty: bool, out: &mut String) {
    if !(written.self_closing && empty) {
        out.push_str("</");
        out.push_str(name);
        out.push_str(&written.close_tail);
        out.push('>');
    }
}

/// An element on its own, and everything in it, onto `out`.
pub(crate) fn element(el: &Element, out: &mut String) {
    let empty = el.children.is_empty();
    open_tag(&el.name, &el.attrs, &el.written, empty, out);
    for child in &el.children {
        match child {
            Content::Element(child) => element(child, out),
            Content::Text(raw) => out.push_str(raw),
        }
    }
    close_tag(&el.name, &el.written, empty, out);
}

impl Element {
    /// This element as markup.
    pub fn to_markup(&self) -> String {
        let mut out = String::new();
        element(self, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse::{parse, parse_fragment};

    fn round_trip(text: &str) {
        let p = parse(text).unwrap();
        let mut out = p.before.clone();
        element(&p.root, &mut out);
        out.push_str(&p.after);
        assert_eq!(out, text);
    }

    #[test]
    fn what_is_read_is_written_back_byte_for_byte() {
        round_trip("<svg/>");
        round_trip("<svg></svg>");
        round_trip("<svg\n   xmlns = 'http://www.w3.org/2000/svg'\n\tviewBox=\"0 0 24 24\"\n>\r\n  <g  fill='a&amp;b' ><path d=\"M0 0\n  L1 1\" /></g >\r\n</svg >\n");
        round_trip("\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE svg [ <!ENTITY e \"x>y\"> ]>\n<!-- hi -->\n<svg a=\"&e;\">t &lt; <![CDATA[<]]><?x?><!-- c --> ✨</svg>\n<!-- bye -->\n");
        round_trip("<svg a=\"1\" a=\"2\"><empty></empty><self /></svg>");
    }

    #[test]
    fn what_changed_is_written_afresh() {
        let mut p = parse("<svg a='it&apos;s'   b = \"1\"/>").unwrap();
        p.root.attrs[0].set("it's \"new\" & <odd>");
        p.root.attrs[1].set("2");
        assert_eq!(p.root.to_markup(), "<svg a='it&apos;s \"new\" &amp; &lt;odd>'   b = \"2\"/>", "its quotes and spacing stay; only the value is new");
        // An element that gains children stops closing itself.
        let mut g = Element::new("g").with("id", "a");
        assert_eq!(g.to_markup(), "<g id=\"a\"/>");
        g = g.child(Element::new("rect"));
        assert_eq!(g.to_markup(), "<g id=\"a\"><rect/></g>");
        assert_eq!(parse_fragment(&g.to_markup()).unwrap().len(), 1);
    }
}
