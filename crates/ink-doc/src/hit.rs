//! What's at a point (ARCHITECTURE §4.1): the shapes and the texts
//! drawn there, front to back, as the renderer would draw them. Through
//! every transform above each, inside its fill by its fill rule (a
//! text's: inside one of its glyphs) or within its stroke's reach, and
//! not where a clip path cuts it away.

use ink_geom::{Affine, FillRule, Path, Vec2};

use crate::document::Document;
use crate::geometry::{outline_of, path_of};
use crate::gradient::Units;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::length::unit;
use crate::node::Node;
use crate::refs::Ids;
use crate::style::{Paint, Style, fill_rule, prop};
use crate::text;
use crate::transform;
use crate::viewport::Viewport;

/// How near a curve its flattening stays, in the picture's units.
const TOLERANCE: f64 = 0.01;
/// How deep clip paths clipped by clip paths are followed.
const MAX_CLIP_DEPTH: usize = 8;

/// Which part of a shape is at the point: the one on top there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Fill,
    Stroke,
}

/// A shape at a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub node: NodeId,
    pub part: Part,
}

struct Seeker<'a> {
    doc: &'a Document,
    ids: Ids<'a>,
    view: Vec2,
    point: Vec2,
    /// What's there, back to front.
    found: Vec<Hit>,
}

/// Something drawn where it stands, and not `display: none`.
fn is_drawn(node: &Node) -> bool {
    (node.kind.is_group() || node.kind.is_shape() || node.kind == Kind::Text) && prop(node, "display") != Some("none")
}

impl Seeker<'_> {
    /// Whether `paint` puts anything down: a colour, a gradient or a
    /// pattern that's there, or the colour given for when it isn't.
    fn paints(&self, paint: &Paint) -> bool {
        match paint {
            Paint::None => false,
            Paint::Color(_) => true,
            Paint::Server { id, fallback } => fallback.is_some() || self.ids.get(id).and_then(|id| self.doc.get(id)).is_some_and(|s| matches!(s.kind, Kind::LinearGradient | Kind::RadialGradient | Kind::Pattern)),
        }
    }

    /// Whether `clip` (a `<clipPath>`) lets `local` through: a point in
    /// the coordinates of what it clips.
    fn lets_through(&self, clip: &Node, local: Vec2, depth: usize) -> bool {
        // One measured against its element's box takes the box to work
        // out: taken as letting everything through.
        if clip.attr("clipPathUnits").and_then(Units::parse) == Some(Units::BBox) {
            return true;
        }
        if depth > MAX_CLIP_DEPTH {
            return false;
        }
        if let Some(outer) = self.ids.target(self.doc, clip, "clip-path", Kind::ClipPath)
            && !self.lets_through(outer, local, depth + 1)
        {
            return false;
        }
        let base = transform::of(clip, self.view).unwrap_or(Affine::IDENTITY);
        let inherited = prop(clip, "clip-rule").and_then(fill_rule).unwrap_or_default();
        clip.elements().filter_map(|id| self.doc.get(id)).filter(|c| is_drawn(c) && !matches!(prop(c, "visibility"), Some("hidden" | "collapse"))).any(|shape| {
            let to_clipped = transform::of(shape, self.view).map_or(base, |t| t.then(&base));
            let Some(back) = to_clipped.inverse() else { return false };
            let rule = if shape.kind == Kind::Text { FillRule::NonZero } else { prop(shape, "clip-rule").and_then(fill_rule).unwrap_or(inherited) };
            outline_of(self.doc, shape, self.view).contains(back.apply(local), rule, TOLERANCE / to_clipped.max_stretch().max(f64::MIN_POSITIVE))
        })
    }

    /// Which part of `path`, painted as `style` says and filled by
    /// `rule`, is at `local`: the one on top there.
    fn part(&self, path: &Path, style: &Style, rule: FillRule, local: Vec2, tol: f64) -> Option<Part> {
        if !style.visible {
            return None;
        }
        let filled = self.paints(&style.fill) && path.contains(local, rule, tol);
        let stroked = self.paints(&style.stroke) && style.line.width > 0.0 && path.distance(local, tol).is_some_and(|d| d <= style.line.width / 2.0);
        // The stroke is painted over the fill, unless the shape says
        // the other way round.
        match (filled, stroked) {
            (true, true) => Some(if style.stroke_first { Part::Fill } else { Part::Stroke }),
            (true, false) => Some(Part::Fill),
            (false, true) => Some(Part::Stroke),
            (false, false) => None,
        }
    }

    fn walk(&mut self, node: &Node, parent: &Style, parent_ctm: &Affine) {
        if !is_drawn(node) || prop(node, "opacity").and_then(unit) == Some(0.0) {
            return;
        }
        let style = parent.cascade(node);
        // The root's own transform isn't one.
        let ctm = if node.parent.is_none() { *parent_ctm } else { transform::of(node, self.view).map_or(*parent_ctm, |t| t.then(parent_ctm)) };
        let Some(local) = ctm.inverse().map(|back| back.apply(self.point)) else { return };
        if let Some(clip) = self.ids.target(self.doc, node, "clip-path", Kind::ClipPath)
            && !self.lets_through(clip, local, 0)
        {
            return;
        }
        let tol = TOLERANCE / ctm.max_stretch().max(f64::MIN_POSITIVE);
        let doc = self.doc;
        if node.kind == Kind::Text {
            // The text is what's there, whichever of its elements' glyphs
            // are: the ones painted last are on top.
            let runs = text::lay(doc, node, self.view).map(|laid| laid.runs).unwrap_or_default();
            let part = runs.iter().rev().find_map(|run| self.part(&run.outline, &text::style_of(doc, node, &style, run.node), FillRule::NonZero, local, tol));
            self.found.extend(part.map(|part| Hit { node: node.id, part }));
            return;
        }
        if node.kind.is_shape()
            && let Some(part) = self.part(&path_of(node), &style, style.fill_rule, local, tol)
        {
            self.found.push(Hit { node: node.id, part });
        }
        for child in node.elements().filter_map(|id| doc.get(id)) {
            self.walk(child, &style, &ctm);
        }
    }
}

/// The shapes drawn at `point` (in the document's coordinates), the one
/// on top first.
pub fn at(doc: &Document, point: Vec2) -> Vec<Hit> {
    let Some(root) = doc.get(doc.root()) else { return Vec::new() };
    let mut seeker = Seeker { doc, ids: Ids::of(doc), view: Viewport::of(root).view, point, found: Vec::new() };
    seeker.walk(root, &Style::default(), &Affine::IDENTITY);
    seeker.found.reverse();
    seeker.found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    fn hits(inner: &str, x: f64, y: f64) -> Vec<(u64, Part)> {
        let d = Document::parse(DocId(1), &format!("<svg viewBox=\"0 0 24 24\">{inner}</svg>")).unwrap();
        at(&d, Vec2::new(x, y)).into_iter().map(|h| (h.node.0, h.part)).collect()
    }

    #[test]
    fn what_is_at_a_point_comes_front_to_back() {
        let inner = r##"<rect width="10" height="10"/><g transform="translate(4 4)"><circle r="3" fill="red" stroke="#000" stroke-width="2"/></g><path d="M0 20 H10" fill="none" stroke="#000" stroke-width="2"/>"##;
        assert_eq!(hits(inner, 4.0, 4.0), [(4, Part::Fill), (2, Part::Fill)], "the circle, through its group's move, over the rect");
        assert_eq!(hits(inner, 7.5, 4.0), [(4, Part::Stroke), (2, Part::Fill)], "its stroke reaches a unit past its edge");
        assert_eq!(hits(inner, 9.0, 9.0), [(2, Part::Fill)]);
        assert_eq!(hits(inner, 5.0, 20.9), [(5, Part::Stroke)], "a line is hit within half its width");
        assert_eq!(hits(inner, 5.0, 21.1), []);
        assert_eq!(hits(inner, 20.0, 2.0), []);
    }

    #[test]
    fn only_what_shows_is_there() {
        // No fill is no fill to hit; a fill behind its stroke is still
        // under it.
        assert_eq!(hits(r#"<rect width="10" height="10" fill="none"/>"#, 5.0, 5.0), []);
        assert_eq!(hits(r##"<rect width="10" height="10" stroke="#000" stroke-width="4" paint-order="stroke"/>"##, 1.0, 5.0), [(2, Part::Fill)]);
        // A gradient that isn't there paints nothing, unless it names a
        // colour to use instead.
        let painted = |fill: &str| hits(&format!(r##"<linearGradient id="g"/><rect width="10" height="10" fill="{fill}"/>"##), 5.0, 5.0).len();
        assert_eq!((painted("url(#g)"), painted("url(#gone)"), painted("url(#gone) red")), (1, 0, 1));
        // Hidden, not displayed, or faded away entirely.
        for gone in [r#"visibility="hidden""#, r#"display="none""#, r#"opacity="0""#, r#"style="display: none""#] {
            assert_eq!(hits(&format!(r#"<g {gone}><rect width="10" height="10"/></g>"#), 5.0, 5.0), [], "{gone}");
        }
        assert_eq!(hits(r#"<g visibility="hidden"><rect width="10" height="10" visibility="visible"/></g>"#, 5.0, 5.0), [(3, Part::Fill)], "what a hidden group holds can show itself again");
        // A hole by one rule, solid by the other.
        let ring = |rule: &str| hits(&format!(r#"<path d="M0 0 H10 V10 H0 Z M3 3 H7 V7 H3 Z" fill-rule="{rule}"/>"#), 5.0, 5.0);
        assert_eq!((ring("evenodd"), ring("nonzero")), (vec![], vec![(2, Part::Fill)]));
        // Definitions stand nowhere.
        assert_eq!(hits(r#"<defs><rect width="10" height="10"/></defs>"#, 5.0, 5.0), []);
    }

    #[test]
    fn a_text_is_there_where_its_glyphs_are() {
        crate::text::tests::test_fonts();
        // At size 10 a capital is a box 1..5 across, 7 tall, standing on
        // y; the next one starts 6 on.
        let inner = r##"<g font-family="Ink Test" font-size="10"><text x="2" y="12">H<tspan fill="none" stroke="#000" stroke-width="2">H</tspan></text></g><clipPath id="word"><text x="2" y="12" font-family="Ink Test" font-size="10">H</text></clipPath><rect width="20" height="20" clip-path="url(#word)" transform="translate(0 9)"/>"##;
        assert_eq!(hits(inner, 5.0, 8.0), [(3, Part::Fill)], "in the first glyph: the text, not a span of it");
        assert_eq!(hits(inner, 7.5, 8.0), [], "between two glyphs is nowhere");
        assert_eq!(hits(inner, 11.0, 8.0), [], "a span with no fill has none to hit");
        assert_eq!(hits(inner, 13.5, 8.0), [(3, Part::Stroke)], "but its stroke is there, a unit either side of its outline");
        // A text in a clip path lets through what its glyphs cover
        // (here 9 further down, with what it clips).
        assert_eq!(hits(inner, 5.0, 17.0), [(7, Part::Fill)]);
        assert_eq!(hits(inner, 8.0, 17.0), []);
    }

    #[test]
    fn a_clip_path_cuts_away_what_it_does_not_let_through() {
        let inner = r##"<clipPath id="left"><rect width="5" height="10"/></clipPath><g clip-path="url(#left)" transform="translate(10 0)"><rect width="10" height="10"/></g>"##;
        assert_eq!(hits(inner, 12.0, 5.0), [(5, Part::Fill)], "the clip moves with what it clips");
        assert_eq!(hits(inner, 17.0, 5.0), []);
        // A clip path with nothing in it lets nothing through; one that
        // isn't there clips nothing.
        assert_eq!(hits(r##"<clipPath id="none"/><rect width="10" height="10" clip-path="url(#none)"/>"##, 5.0, 5.0), []);
        assert_eq!(hits(r##"<rect width="10" height="10" clip-path="url(#nowhere)"/>"##, 5.0, 5.0), [(2, Part::Fill)]);
    }
}
