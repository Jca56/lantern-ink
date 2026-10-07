//! What a node says about how it looks: its presentation attributes and
//! its `style=""`, and the properties a shape inherits from the groups
//! above it.

use ink_geom::{Cap, FillRule, Join, Stroke};
use lntrn_math::Color;

use crate::color;
use crate::length::{number, numbers, unit};
use crate::node::Node;

/// One `name: value` of a `style` attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Decl<'a> {
    pub name: &'a str,
    /// What it's set to, any `!important` left off.
    pub value: &'a str,
    /// Where in the style the declaration is written (what's between
    /// two `;`), and where in it that value is.
    pub at: std::ops::Range<usize>,
    pub value_at: std::ops::Range<usize>,
}

/// The declarations of a `style` attribute, in order. They part at each
/// `;` that isn't inside quotes or brackets (a `url("data:…;base64,…")`
/// is one value); what has no `:` isn't one.
pub(crate) fn declarations(style: &str) -> impl Iterator<Item = Decl<'_>> {
    let bytes = style.as_bytes();
    let mut from = 0;
    std::iter::from_fn(move || {
        while from < bytes.len() {
            let (mut end, mut quote, mut depth) = (from, 0u8, 0usize);
            while end < bytes.len() {
                match bytes[end] {
                    b if quote != 0 => quote = if b == quote { 0 } else { quote },
                    q @ (b'"' | b'\'') => quote = q,
                    b'(' => depth += 1,
                    b')' => depth = depth.saturating_sub(1),
                    b';' if depth == 0 => break,
                    _ => {}
                }
                end += 1;
            }
            let at = from..end;
            from = end + 1;
            let chunk = &style[at.clone()];
            let Some(colon) = chunk.find(':') else { continue };
            let said = chunk[colon + 1..].trim_end();
            let said = said.strip_suffix("!important").map_or(said, str::trim_end);
            let lead = said.len() - said.trim_start().len();
            let start = at.start + colon + 1 + lead;
            return Some(Decl { name: chunk[..colon].trim(), value: &said[lead..], value_at: start..start + said.len() - lead, at });
        }
        None
    })
}

/// A property of `node`: from its `style` attribute, which wins, or the
/// attribute of that name. Of the same property twice in a `style`, the
/// later one counts.
pub fn prop<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    let styled = node.attr("style").and_then(|style| declarations(style).filter(|d| d.name == name).last().map(|d| d.value));
    styled.or_else(|| node.attr(name).map(str::trim))
}

/// The id a `url(#id)` names, and what follows it.
pub fn url_id(s: &str) -> Option<(&str, &str)> {
    let rest = s.trim().strip_prefix("url(")?;
    let close = rest.find(')')?;
    Some((rest[..close].trim().trim_matches(['"', '\'']).trim_start_matches('#'), rest[close + 1..].trim()))
}

/// What a fill or a stroke is painted with.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    None,
    Color(Color),
    /// A gradient (or a pattern) by its `id`, and the colour to use if
    /// there's no such thing (`None`: no paint).
    Server { id: String, fallback: Option<Color> },
}

impl Paint {
    /// A `fill` or `stroke` value; `None` when it can't be read (the
    /// property is then as if it weren't there).
    pub fn parse(s: &str) -> Option<Paint> {
        if let Some((id, rest)) = url_id(s) {
            return Some(Paint::Server { id: id.to_owned(), fallback: color::parse(rest) });
        }
        if s.trim().eq_ignore_ascii_case("none") {
            return Some(Paint::None);
        }
        color::parse(s).map(Paint::Color)
    }
}

/// What a shape inherits from the groups above it.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub fill: Paint,
    pub stroke: Paint,
    pub fill_opacity: f64,
    pub stroke_opacity: f64,
    pub fill_rule: FillRule,
    /// The stroke's width, ends, corners and dashes.
    pub line: Stroke,
    /// `paint-order` puts the stroke under the fill.
    pub stroke_first: bool,
    /// Not `visibility: hidden`. What's under a hidden group can show
    /// itself again.
    pub visible: bool,
}

impl Default for Style {
    /// What a document starts from: a black fill and no stroke.
    fn default() -> Style {
        Style { fill: Paint::Color(Color::BLACK), stroke: Paint::None, fill_opacity: 1.0, stroke_opacity: 1.0, fill_rule: FillRule::NonZero, line: Stroke::default(), stroke_first: false, visible: true }
    }
}

/// `fill-rule` and `clip-rule` values.
pub fn fill_rule(s: &str) -> Option<FillRule> {
    match s {
        "nonzero" => Some(FillRule::NonZero),
        "evenodd" => Some(FillRule::EvenOdd),
        _ => None,
    }
}

impl Style {
    /// This, with what `node` says for itself.
    pub fn cascade(&self, node: &Node) -> Style {
        let mut st = self.clone();
        if let Some(fill) = prop(node, "fill").and_then(Paint::parse) {
            st.fill = fill;
        }
        if let Some(stroke) = prop(node, "stroke").and_then(Paint::parse) {
            st.stroke = stroke;
        }
        if let Some(o) = prop(node, "fill-opacity").and_then(unit) {
            st.fill_opacity = o;
        }
        if let Some(o) = prop(node, "stroke-opacity").and_then(unit) {
            st.stroke_opacity = o;
        }
        if let Some(rule) = prop(node, "fill-rule").and_then(fill_rule) {
            st.fill_rule = rule;
        }
        if let Some(w) = prop(node, "stroke-width").and_then(number) {
            st.line.width = w.max(0.0);
        }
        match prop(node, "stroke-linecap") {
            Some("butt") => st.line.cap = Cap::Butt,
            Some("round") => st.line.cap = Cap::Round,
            Some("square") => st.line.cap = Cap::Square,
            _ => {}
        }
        match prop(node, "stroke-linejoin") {
            Some("miter") => st.line.join = Join::Miter,
            Some("round") => st.line.join = Join::Round,
            Some("bevel") => st.line.join = Join::Bevel,
            _ => {}
        }
        if let Some(m) = prop(node, "stroke-miterlimit").and_then(number).filter(|m| *m >= 1.0) {
            st.line.miter_limit = m;
        }
        if let Some(d) = prop(node, "stroke-dasharray") {
            // `none`, and a list that can't be read, are a solid line.
            st.line.dashes = numbers(d).unwrap_or_default();
        }
        if let Some(o) = prop(node, "stroke-dashoffset").and_then(number) {
            st.line.dash_offset = o;
        }
        match prop(node, "visibility") {
            Some("hidden" | "collapse") => st.visible = false,
            Some("visible") => st.visible = true,
            _ => {}
        }
        if let Some(order) = prop(node, "paint-order") {
            // What's named is painted first, in that order; the rest
            // follow as usual (fill, then stroke).
            let at = |name: &str| order.split_whitespace().position(|w| w == name);
            st.stroke_first = at("stroke").is_some_and(|s| at("fill").is_none_or(|f| s < f));
        }
        st
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::id::{DocId, NodeId};

    fn doc(inner: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg>{inner}</svg>")).unwrap()
    }

    #[test]
    fn a_property_comes_from_the_style_attribute_first() {
        let d = doc(r##"<path fill="red" stroke=" blue " style="fill: green; stroke-width : 2px ;fill:#abc !important;; nonsense"/>"##);
        let n = d.node(NodeId(2)).unwrap();
        assert_eq!(prop(n, "fill"), Some("#abc"), "the later of two wins, its !important left off");
        assert_eq!(prop(n, "stroke-width"), Some("2px"));
        assert_eq!(prop(n, "stroke"), Some("blue"), "the attribute, when the style doesn't say");
        assert_eq!(prop(n, "opacity"), None);
    }

    #[test]
    fn a_style_parts_at_its_semicolons_but_not_inside_a_value() {
        let style = "fill: url(\"data:image/png;base64,AA==\") ; stroke :red!important;;font-family: 'a;b' ; nonsense;x:";
        let all: Vec<Decl> = declarations(style).collect();
        assert_eq!(all.iter().map(|d| (d.name, d.value)).collect::<Vec<_>>(), [("fill", "url(\"data:image/png;base64,AA==\")"), ("stroke", "red"), ("font-family", "'a;b'"), ("x", "")]);
        assert_eq!((&style[all[1].at.clone()], &style[all[1].value_at.clone()]), (" stroke :red!important", "red"));
        assert_eq!(&style[all[0].value_at.clone()], all[0].value);
        assert_eq!(declarations("").count(), 0);
    }

    #[test]
    fn reads_paints() {
        assert_eq!(Paint::parse("none"), Some(Paint::None));
        assert_eq!(Paint::parse(" #f00 "), Some(Paint::Color(Color::RED)));
        assert_eq!(Paint::parse(r##"url("#gradient-1")"##), Some(Paint::Server { id: "gradient-1".into(), fallback: None }));
        assert_eq!(Paint::parse("url(#a) red"), Some(Paint::Server { id: "a".into(), fallback: Some(Color::RED) }));
        assert_eq!(Paint::parse("url(#a) none"), Some(Paint::Server { id: "a".into(), fallback: None }));
        assert_eq!(Paint::parse("blurple"), None);
        assert_eq!(url_id("url( '#x' )"), Some(("x", "")));
        assert_eq!(url_id("#x"), None);
    }

    #[test]
    fn a_shape_inherits_and_overrides() {
        let d = doc(r##"<g fill="none" stroke="#ff0000" stroke-width="3" stroke-linecap="round" stroke-dasharray="4 2"><path style="stroke: url(&quot;#g&quot;); stroke-width: 10px; stroke-opacity: 0.7" stroke-width="1" stroke-dasharray="none" fill="blurple"/></g>"##);
        let group = Style::default().cascade(d.node(NodeId(2)).unwrap());
        assert_eq!((&group.fill, &group.stroke), (&Paint::None, &Paint::Color(Color::RED)));
        assert_eq!((group.line.width, group.line.cap, group.line.dashes.clone()), (3.0, Cap::Round, vec![4.0, 2.0]));
        let path = group.cascade(d.node(NodeId(3)).unwrap());
        assert_eq!(path.stroke, Paint::Server { id: "g".into(), fallback: None });
        assert_eq!(path.line.width, 10.0, "the style attribute wins");
        assert_eq!((path.line.cap, path.stroke_opacity), (Cap::Round, 0.7));
        assert!(path.line.dashes.is_empty(), "none is a solid line");
        assert_eq!(path.fill, Paint::None, "a fill that can't be read is as if it weren't said: inherited");
    }

    #[test]
    fn the_stroke_goes_under_only_when_named_ahead_of_the_fill() {
        for (order, first) in [("stroke", true), ("stroke fill", true), ("markers stroke", true), ("fill", false), ("fill stroke", false), ("markers", false), ("normal", false)] {
            let d = doc(&format!(r#"<path paint-order="{order}" visibility="hidden" fill-rule="evenodd" stroke-miterlimit="0.5"/>"#));
            let st = Style::default().cascade(d.node(NodeId(2)).unwrap());
            assert_eq!(st.stroke_first, first, "{order}");
            assert!(!st.visible && st.fill_rule == FillRule::EvenOdd);
            assert_eq!(st.line.miter_limit, 4.0, "a limit under 1 isn't one");
        }
    }
}
