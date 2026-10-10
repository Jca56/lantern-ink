//! What Lantern's own renderer leaves out. Lantern's apps show icons
//! with `lntrn-svg`, which draws less of SVG than Ink does: a drawing
//! that uses what it doesn't draw looks different there, and is worth
//! saying so wherever the drawing is shown "as Lantern's apps will
//! draw it" (the MCP's `renderer: "lantern"` preview, the window's
//! preview strip).

use crate::document::Document;
use crate::kind::Kind;

/// What's in `doc` that `lntrn-svg` doesn't draw, by name, each once,
/// in the order met.
pub fn misses(doc: &Document) -> Vec<&'static str> {
    let mut misses: Vec<&'static str> = Vec::new();
    for id in doc.descendants(doc.root()) {
        let Some(node) = doc.get(id) else { continue };
        let miss = match node.kind {
            Kind::Text | Kind::TSpan => "<text>",
            Kind::Use => "<use>",
            Kind::Image => "<image>",
            Kind::Mask => "<mask>",
            Kind::Pattern => "<pattern>",
            Kind::Marker => "<marker>",
            Kind::Style => "<style> rules",
            Kind::FilterPrimitive if node.local() != "feDropShadow" => "filters other than feDropShadow",
            _ => continue,
        };
        if !misses.contains(&miss) {
            misses.push(miss);
        }
    }
    misses
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    #[test]
    fn what_lanterns_renderer_leaves_out_is_named_once() {
        let doc = |inner: &str| Document::parse(DocId(1), &format!("<svg xmlns=\"http://www.w3.org/2000/svg\">{inner}</svg>")).unwrap();
        assert!(misses(&doc("<defs><filter id=\"f\"><feDropShadow dx=\"1\"/></filter></defs><rect width=\"4\" height=\"4\" filter=\"url(#f)\"/>")).is_empty());
        let rich = doc("<style>rect { fill: red }</style><text>a<tspan>b</tspan></text><text>c</text><defs><filter id=\"f\"><feGaussianBlur stdDeviation=\"1\"/></filter></defs>");
        assert_eq!(misses(&rich), ["<style> rules", "<text>", "filters other than feDropShadow"]);
    }
}
