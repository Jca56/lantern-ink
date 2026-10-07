//! A text made into paths (ARCHITECTURE §5.5): `Command::TextToPath`
//! writes out the outlines a `<text>` is drawn as, so it looks the same
//! on a machine without its fonts, and in Lantern's apps, which draw no
//! text at all.
//!
//! A text lettered and painted one way becomes one `<path>`, in its
//! place, with its id and its paint. One whose `<tspan>`s paint for
//! themselves becomes a `<g>` of paths, one for each stretch, each with
//! the paint its span gave it. What only letters read (where the text
//! starts, its font, its spacing) goes with the letters.

use ink_geom::{FillRule, Path};

use crate::document::Document;
use crate::edit::Place;
use crate::error::{DocError, invalid};
use crate::id::NodeId;
use crate::kind::Kind;
use crate::node::{Attr, Element, prefix};
use crate::style::prop;
use crate::text;
use crate::value::Precision;
use crate::viewport::Viewport;

/// Attributes that say where letters go.
const PLACED: [&str; 8] = ["x", "y", "dx", "dy", "rotate", "textLength", "lengthAdjust", "xml:space"];
/// Properties only letters read.
const LETTERED: [&str; 22] = [
    "font",
    "font-family",
    "font-size",
    "font-size-adjust",
    "font-stretch",
    "font-style",
    "font-variant",
    "font-weight",
    "font-kerning",
    "font-feature-settings",
    "letter-spacing",
    "word-spacing",
    "text-anchor",
    "dominant-baseline",
    "alignment-baseline",
    "baseline-shift",
    "white-space",
    "text-decoration",
    "text-rendering",
    "direction",
    "unicode-bidi",
    "writing-mode",
];
/// What a `<tspan>` can paint its own letters with.
const PAINT: [&str; 12] = ["fill", "fill-opacity", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit", "stroke-dasharray", "stroke-dashoffset", "stroke-opacity", "paint-order", "visibility"];

impl Document {
    /// What the elements from the text `text` down to `run` (one in it)
    /// say of their letters' paint: the nearest to say each wins.
    fn span_paint(&self, text: NodeId, run: NodeId) -> Vec<(&'static str, String)> {
        if run == text {
            return Vec::new();
        }
        let chain: Vec<NodeId> = std::iter::once(run).chain(self.ancestors(run).take_while(|n| n.id != text).map(|n| n.id)).collect();
        PAINT.into_iter().filter_map(|name| chain.iter().filter_map(|id| self.get(*id)).find_map(|n| prop(n, name)).map(|value| (name, value.to_owned()))).collect()
    }

    /// Make the text `id` paths that draw what it draws. Unless
    /// `as_drawn`, a text set in another font than it asks for is
    /// refused: its paths would be the other font's for good. Returns
    /// the nodes made (the paths of a text that became a group) and
    /// those taken out (its spans).
    pub(crate) fn text_to_path(&mut self, id: NodeId, as_drawn: bool) -> Result<(Vec<NodeId>, Vec<NodeId>), DocError> {
        let node = self.node(id)?;
        if node.kind != Kind::Text {
            return invalid(format!("{id} is a <{}>: only a <text> has letters to make paths of (a shape is made a path by its own tool)", node.name));
        }
        let view = self.get(self.root()).map_or(ink_geom::Vec2::ZERO, |root| Viewport::of(root).view);
        let laid = match text::lay(self, node, view) {
            Ok(laid) => laid,
            Err(why) => return invalid(format!("{id} can't be made into paths: {why}, which Ink doesn't set")),
        };
        if laid.runs.is_empty() {
            return invalid(format!("{id} says nothing that draws, so there are no paths to make of it"));
        }
        let fonts = text::lettered(self, node);
        let mut missing: Vec<&str> = fonts.iter().flat_map(|font| font.missing.iter().map(String::as_str)).collect();
        missing.dedup();
        if !as_drawn && !missing.is_empty() {
            let used: Vec<&str> = fonts.iter().filter(|font| !font.missing.is_empty()).map(|font| font.family.as_str()).collect();
            return invalid(format!("{id} asks for {}, which {} installed here, so it's drawn in {} instead: paths made of it would be that font's for good. Say so to make them anyway", missing.join(" and "), if missing.len() == 1 { "isn't" } else { "aren't" }, used.first().copied().unwrap_or("another font")));
        }
        let precision = Precision::of(self);
        let pieces: Vec<(Path, Vec<(&'static str, String)>)> = laid.runs.iter().map(|run| (run.outline.clone(), self.span_paint(id, run.node))).collect();
        // Letters fill one way, whatever the text was told.
        let evenodd = self.style_of(id)?.fill_rule == FillRule::EvenOdd;
        let one = pieces.iter().all(|(_, paint)| paint.is_empty());
        let name = |local: &str| prefix(&self.nodes[&id].name).map_or(local.to_owned(), |p| format!("{p}:{local}"));
        let (path_name, group_name) = (name("path"), name("g"));

        let gone: Vec<NodeId> = node.elements().collect();
        for child in &gone {
            self.remove(*child)?;
        }
        let node = self.edit(id)?;
        node.children.clear();
        let first = node.attrs.iter().position(|a| PLACED.contains(&a.name.as_str()));
        let written = first.map(|i| (node.attrs[i].lead.clone(), node.attrs[i].eq.clone(), node.attrs[i].quote));
        node.attrs.retain(|a| !PLACED.contains(&a.name.as_str()));
        let mut made = Vec::new();
        if one {
            let all = Path { subpaths: pieces.into_iter().flat_map(|(path, _)| path.subpaths).collect() };
            // Its path data goes where the first of its numbers was,
            // written as that was.
            let mut data = Attr::new("d", precision.path(&all));
            if let Some((lead, eq, quote)) = written {
                (data.lead, data.eq, data.quote) = (lead, eq, quote);
            }
            node.attrs.insert(first.unwrap_or(0).min(node.attrs.len()), data);
            (node.name, node.kind) = (path_name, Kind::Path);
            node.written.self_closing = true;
        } else {
            (node.name, node.kind) = (group_name, Kind::G);
            for (path, paint) in pieces {
                let piece = paint.iter().fold(Element::new(path_name.clone()).with("d", precision.path(&path)), |el, (name, value)| el.with(name, value.as_str()));
                let new = self.insert(Place::LastIn(id), piece)?;
                self.lay_out(new)?;
                made.push(new);
            }
        }
        for name in LETTERED {
            self.set_prop(id, name, None)?;
        }
        if evenodd {
            self.set_prop(id, "fill-rule", Some("nonzero"))?;
        }
        self.reanchor(id);
        Ok((made, gone))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;

    /// The drawing's insides once `N2` (a text in the tests' font, size
    /// 10) has been made paths.
    fn made(element: &str, as_drawn: bool) -> Result<String, DocError> {
        crate::text::tests::test_fonts();
        let open = r#"<svg viewBox="0 0 100 100" font-family="Ink Test" font-size="10">"#;
        let mut d = Document::parse(DocId(1), &format!("{open}{element}</svg>")).unwrap();
        d.apply(&Command::TextToPath { nodes: vec![NodeId(2)], as_drawn })?;
        let out = d.to_svg();
        Ok(out[open.len()..out.len() - "</svg>".len()].to_owned())
    }

    #[test]
    fn a_text_lettered_one_way_is_one_path_in_its_place() {
        // A capital is a box 4 by 7 a unit in from its pen; an i one 1 by 7.
        let text = r##"<text id="t" x="20" y="50" fill="#c33" font-weight="normal" transform="rotate(5)" style="white-space: pre; opacity: 0.5" letter-spacing="1">H<tspan dx="2">i</tspan></text>"##;
        assert_eq!(made(text, false).unwrap(), r##"<path id="t" d="M21 50 V43 H25 V50 Z M30 50 V43 H31 V50 Z" fill="#c33" transform="rotate(5)" style="opacity: 0.5"/>"##, "what only letters read went with them");
        assert_eq!(made("<text>H</text>", false).unwrap(), r#"<path d="M1 0 V-7 H5 V0 Z"/>"#);
        assert_eq!(made(r#"<text fill-rule="evenodd">H</text>"#, false).unwrap(), r#"<path d="M1 0 V-7 H5 V0 Z" fill-rule="nonzero"/>"#, "letters fill one way");
        // Several lines, painted alike, are still one path (an em is of
        // the span that says it: here 5).
        assert_eq!(made(r#"<text x="0" y="10">H<tspan x="0" dy="1.2em" font-size="5">H</tspan></text>"#, false).unwrap(), r#"<path d="M1 10 V3 H5 V10 Z M0.5 16 V12.5 H2.5 V16 Z"/>"#);
    }

    #[test]
    fn spans_that_paint_for_themselves_make_a_group_of_paths() {
        let text = r##"<text id="t" x="0" y="10" fill="#223" stroke-width="2">H<tspan fill="red"><tspan stroke="blue" style="fill: lime">H</tspan>H</tspan>H</text>"##;
        let group = made(text, false).unwrap();
        assert_eq!(
            group,
            r##"<g id="t" fill="#223" stroke-width="2"><path d="M1 10 V3 H5 V10 Z"/><path d="M7 10 V3 H11 V10 Z" fill="lime" stroke="blue"/><path d="M13 10 V3 H17 V10 Z" fill="red"/><path d="M19 10 V3 H23 V10 Z"/></g>"##,
            "each stretch with what its span gave it, the nearest to say it winning"
        );
    }

    #[test]
    fn what_cannot_be_made_paths_says_why() {
        let refused = |element: &str, as_drawn: bool| made(element, as_drawn).unwrap_err().to_string();
        assert!(refused("<rect/>", false).starts_with("N2 is a <rect>: only a <text> has letters"));
        assert!(refused(r#"<text rotate="20">H</text>"#, true).starts_with("N2 can't be made into paths: its characters are placed or turned one by one"));
        assert!(refused("<text> </text>", true).starts_with("N2 says nothing that draws"));
        // A font that isn't here would be another's outlines for good.
        let other = r#"<text font-family="No Such Font, Ink Test">H</text>"#;
        assert_eq!(refused(other, false), "N2 asks for No Such Font, which isn't installed here, so it's drawn in Ink Test instead: paths made of it would be that font's for good. Say so to make them anyway");
        assert_eq!(made(other, true).unwrap(), r#"<path d="M1 0 V-7 H5 V0 Z"/>"#);
    }
}
