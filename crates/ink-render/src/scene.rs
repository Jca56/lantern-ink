//! A document as a list of things to draw (ARCHITECTURE §5.1): each
//! shape's outline flattened and put through its transforms into the
//! picture's px, with the paint, opacity, clip and shadows it's drawn
//! with. Plain data: built once, then drawn band by band on any thread.
//!
//! What is drawn, and how, follows `lntrn-svg` (what Lantern's apps show
//! an icon with), so the two agree; where that and the SVG spec part
//! ways, the spec is followed and the difference is listed in the
//! agreement tests.

use ink_doc::filter::Filter;
use ink_doc::gradient::{Gradient, Units};
use ink_doc::length::unit;
use ink_doc::refs::Ids;
use ink_doc::style::{Paint as Ink, Style, fill_rule, prop};
use ink_doc::{Document, Kind, Node, geometry, transform};
use ink_geom::{Affine, FillRule, Polyline, Rect, Stroke, Vec2, stroke};

use crate::filter::{self, Stage};
use crate::paint::{Paint, rgba};
use crate::coverage::Shape;

/// How close to its true curves a shape is drawn, in the picture's px.
pub(crate) const TOLERANCE: f64 = 0.05;
/// Layers above the first. A group past that draws straight into the
/// one below (a hostile file can't stack a layer per nesting level).
const MAX_LAYERS: usize = 8;
/// How deep clip paths clipped by clip paths are followed.
const MAX_CLIP_DEPTH: usize = 8;
/// What a paint server Ink can't draw yet (a pattern) is painted as: a
/// shape, not a hole.
const UNDRAWN: [f32; 4] = [0.5, 0.5, 0.5, 1.0];

/// Closed polygons in the picture's px, and the rule they fill by.
pub(crate) type Polys = (Vec<Vec<Vec2>>, FillRule);

/// One thing to draw. `S` is its outline: [`Polys`] as built, a
/// [`Shape`] once fitted to the frame it's drawn in.
pub(crate) enum Item<S> {
    Fill { shape: S, paint: Paint, alpha: f32 },
    Layer(Layer<S>),
}

/// Things drawn apart and laid on as one.
pub(crate) struct Layer<S> {
    pub items: Vec<Item<S>>,
    pub opacity: f32,
    /// The filter run on it once it's drawn: its stages, in order.
    pub filter: Vec<Stage>,
    /// Nothing of it shows outside this: a filter's region, the page.
    pub cut: Option<S>,
    pub clip: Option<Clip<S>>,
}

/// What a clip path lets through: its shapes together (each cut to its
/// own clip, if it has one), within what its own clip path lets through.
pub(crate) struct Clip<S> {
    pub shapes: Vec<(S, Option<Clip<S>>)>,
    pub outer: Option<Box<Clip<S>>>,
}

impl Clip<Polys> {
    fn fitted(self, fit: &impl Fn(Polys) -> Option<Shape>) -> Option<Clip<Shape>> {
        // A shape off the frame lets nothing of the frame through.
        let shapes: Vec<(Shape, Option<Clip<Shape>>)> = self
            .shapes
            .into_iter()
            .filter_map(|(polys, within)| {
                let shape = fit(polys)?;
                match within {
                    Some(within) => Some((shape, Some(within.fitted(fit)?))),
                    None => Some((shape, None)),
                }
            })
            .collect();
        let outer = match self.outer {
            Some(outer) => Some(Box::new(outer.fitted(fit)?)),
            None => None,
        };
        (!shapes.is_empty()).then_some(Clip { shapes, outer })
    }
}

/// `items` with every outline fitted by `fit`; what's off the frame is
/// left out.
pub(crate) fn fitted(items: Vec<Item<Polys>>, fit: &impl Fn(Polys) -> Option<Shape>) -> Vec<Item<Shape>> {
    items
        .into_iter()
        .filter_map(|item| match item {
            Item::Fill { shape, paint, alpha } => Some(Item::Fill { shape: fit(shape)?, paint, alpha }),
            Item::Layer(layer) => {
                let cut = match layer.cut {
                    Some(cut) => Some(fit(cut)?),
                    None => None,
                };
                let clip = match layer.clip {
                    Some(clip) => Some(clip.fitted(fit)?),
                    None => None,
                };
                Some(Item::Layer(Layer { items: fitted(layer.items, fit), opacity: layer.opacity, filter: layer.filter, cut, clip }))
            }
        })
        .collect()
}

/// Whether `node` is drawn at all: a group or a shape, and not
/// `display: none`. (Text, images and `<use>` aren't drawn yet.)
fn is_drawn(node: &Node) -> bool {
    (node.kind.is_group() || node.kind.is_shape()) && prop(node, "display") != Some("none")
}

/// The lines of `lines` with enough points to enclose anything, through
/// `t`.
fn polygons(lines: &[Polyline], t: &Affine) -> Vec<Vec<Vec2>> {
    lines.iter().filter(|l| l.points.len() >= 3).map(|l| l.points.iter().map(|p| t.apply(*p)).collect()).collect()
}

pub(crate) struct Builder<'a> {
    doc: &'a Document,
    ids: Ids<'a>,
    /// The size percentages are of.
    view: Vec2,
    /// The clipping left to do (see [`Builder::new`]).
    clip_budget: u32,
    /// Layers open above the first.
    layers: usize,
    /// How far the shadows of the layers now open look, px, and the
    /// furthest any chain of them has.
    reach: f64,
    pub furthest: f64,
}

impl<'a> Builder<'a> {
    pub fn new(doc: &'a Document, view: Vec2) -> Builder<'a> {
        // How much clipping the whole picture may do: a unit for each
        // clip path worked out and each shape in one. It grows with the
        // document, so honest clipping never runs out, while clip paths
        // that fan out through each other (a hundred shapes each clipped
        // by a hundred more) stop there instead of taking the afternoon.
        // What's clipped past it isn't shown.
        let clip_budget = 4096 + 8 * doc.len().min(1 << 24) as u32;
        Builder { doc, ids: Ids::of(doc), view, clip_budget, layers: 0, reach: 0.0, furthest: 0.0 }
    }

    /// Open a layer, if there's room for one more.
    fn open_layer(&mut self) -> bool {
        let room = self.layers < MAX_LAYERS;
        self.layers += usize::from(room);
        room
    }

    /// Everything in `root` (the `<svg>`), drawn through `to_px`. With
    /// `page`, cut to those corners (the page's, in the picture's px).
    pub fn root(&mut self, root: &Node, to_px: &Affine, page: Option<[Vec2; 4]>) -> Vec<Item<Polys>> {
        let doc = self.doc;
        let style = Style::default().cascade(root);
        let cut = page.filter(|_| self.open_layer());
        let mut items = Vec::new();
        for child in root.elements().filter_map(|id| doc.get(id)) {
            self.node(child, &style, 1.0, to_px, &mut items);
        }
        match cut {
            Some(edge) => vec![Item::Layer(Layer { items, opacity: 1.0, filter: Vec::new(), cut: Some((vec![edge.to_vec()], FillRule::NonZero)), clip: None })],
            None => items,
        }
    }

    /// The box around what `node` draws (strokes aside), in its own
    /// coordinates.
    fn bbox(&self, node: &Node) -> Option<Rect> {
        let mut all = geometry::path_of(node).bounds();
        for child in node.elements().filter_map(|id| self.doc.get(id)).filter(|c| is_drawn(c)) {
            let Some(b) = self.bbox(child) else { continue };
            let b = transform::of(child, self.view).map_or(b, |t| t.bounds(&b));
            if b.min.is_finite() && b.max.is_finite() {
                all = Some(all.map_or(b, |a| a.union(&b)));
            }
        }
        all
    }

    /// What `ink` paints over a shape whose box is `bbox`.
    fn paint(&self, ink: &Ink, bbox: Option<Rect>, ctm: &Affine) -> Option<Paint> {
        match ink {
            Ink::None => None,
            Ink::Color(c) => Some(Paint::Solid(rgba(*c))),
            Ink::Server { id, fallback } => match self.ids.get(id).and_then(|id| self.doc.get(id)) {
                Some(server) if matches!(server.kind, Kind::LinearGradient | Kind::RadialGradient) => Paint::fit(&Gradient::of(self.doc, &self.ids, server)?, bbox, ctm, self.view),
                Some(server) if server.kind == Kind::Pattern => Some(Paint::Solid(UNDRAWN)),
                // Nothing there to paint with: the colour given for
                // that, or nothing.
                _ => fallback.map(|c| Paint::Solid(rgba(c))),
            },
        }
    }

    /// Draw `node` and what is under it, onto `out`. `alpha` is the
    /// opacity of the groups above that had no layer of their own to be
    /// faded as one.
    fn node(&mut self, node: &Node, parent: &Style, alpha: f64, parent_ctm: &Affine, out: &mut Vec<Item<Polys>>) {
        if !is_drawn(node) {
            return;
        }
        let doc = self.doc;
        let st = parent.cascade(node);
        let ctm = transform::of(node, self.view).map_or(*parent_ctm, |t| t.then(parent_ctm));
        let opacity = prop(node, "opacity").and_then(unit).unwrap_or(1.0);
        let stretch = ctm.max_stretch();
        if opacity <= 0.0 || !(stretch.is_finite() && stretch > 0.0) {
            return;
        }
        let tol = TOLERANCE / stretch;
        let path = geometry::path_of(node);
        let lines = path.flatten(tol);
        let fills = st.visible && !lines.is_empty() && st.fill != Ink::None;
        let strokes = st.visible && !lines.is_empty() && st.stroke != Ink::None && st.line.width > 0.0;
        // A filter that can be drawn; one with no region to show in
        // shows nothing.
        let said = self.ids.target(doc, node, "filter", Kind::Filter).and_then(|f| Filter::of(doc, f));
        let fitted = match &said {
            Some(said) => match self.bbox(node).and_then(|b| filter::fit(said, b, &ctm, self.view)) {
                Some(fitted) => Some(fitted),
                None => return,
            },
            None => None,
        };
        let clip = self.ids.target(doc, node, "clip-path", Kind::ClipPath);
        // What's filtered, clipped, or faded as more than one piece, is
        // drawn apart and laid on as one.
        let apart = fitted.is_some() || clip.is_some() || (opacity < 1.0 && (node.elements().next().is_some() || (fills && strokes)));
        let layered = apart && self.open_layer();
        let alpha = if layered { alpha } else { alpha * opacity };
        let own_reach = if layered { fitted.as_ref().map_or(0.0, |f| f.reach()) } else { 0.0 };
        self.reach += own_reach;
        self.furthest = self.furthest.max(self.reach);

        let mut inside = Vec::new();
        let items = if layered { &mut inside } else { &mut *out };
        for filling in if st.stroke_first { [false, true] } else { [true, false] } {
            if filling && fills && let Some(paint) = self.paint(&st.fill, path.bounds(), &ctm) {
                items.push(Item::Fill { shape: (polygons(&lines, &ctm), st.fill_rule), paint, alpha: (st.fill_opacity * alpha) as f32 });
            }
            if !filling && strokes && let Some(paint) = self.paint(&st.stroke, path.bounds(), &ctm) {
                // Dashes are cut from the curves themselves, and the
                // line flattened as its stroke's edges need it.
                let whole = Stroke { dashes: Vec::new(), ..st.line.clone() };
                let outline: Vec<Vec<Vec2>> = stroke(&path.dashed(&st.line).flatten_to_stroke(tol, st.line.width), &whole, tol).iter().map(|poly| poly.iter().map(|p| ctm.apply(*p)).collect()).collect();
                items.push(Item::Fill { shape: (outline, FillRule::NonZero), paint, alpha: (st.stroke_opacity * alpha) as f32 });
            }
        }
        for child in node.elements().filter_map(|id| doc.get(id)) {
            self.node(child, &st, alpha, &ctm, items);
        }
        self.reach -= own_reach;
        if !layered {
            return;
        }
        self.layers -= 1;
        let clip = match clip {
            // A clip path that lets nothing through shows nothing.
            Some(clip) => match self.clip(clip, &ctm, self.bbox(node), 0) {
                Some(clip) => Some(clip),
                None => return,
            },
            None => None,
        };
        let (filter, cut) = match fitted {
            Some(f) => (f.stages, Some((vec![f.region.to_vec()], FillRule::NonZero))),
            None => (Vec::new(), None),
        };
        out.push(Item::Layer(Layer { items: inside, opacity: opacity as f32, filter, cut, clip }));
    }

    /// What `clip` (a `<clipPath>`) lets through, for an element drawn
    /// through `ctm` whose box (in its own coordinates) is `bbox`.
    /// `None`: nothing at all (no shapes, a ring of clip paths clipping
    /// each other, the budget spent).
    fn clip(&mut self, clip: &Node, ctm: &Affine, bbox: Option<Rect>, depth: usize) -> Option<Clip<Polys>> {
        if depth > MAX_CLIP_DEPTH || self.clip_budget == 0 {
            return None;
        }
        self.clip_budget -= 1;
        let doc = self.doc;
        let to_user = match clip.attr("clipPathUnits").and_then(Units::parse) {
            Some(Units::BBox) => {
                let b = bbox.filter(|b| b.width() > 0.0 && b.height() > 0.0)?;
                Affine::new(b.width(), 0.0, 0.0, b.height(), b.min.x, b.min.y)
            }
            _ => Affine::IDENTITY,
        };
        // The clip path's own clip first: if that lets nothing through,
        // there's nothing to work out.
        let outer = match self.ids.target(doc, clip, "clip-path", Kind::ClipPath) {
            Some(outer) => Some(Box::new(self.clip(outer, ctm, bbox, depth + 1)?)),
            None => None,
        };
        let base = transform::of(clip, self.view).unwrap_or(Affine::IDENTITY).then(&to_user).then(ctm);
        let inherited = prop(clip, "clip-rule").and_then(fill_rule).unwrap_or(FillRule::NonZero);
        let mut shapes = Vec::new();
        for child in clip.elements().filter_map(|id| doc.get(id)).filter(|c| is_drawn(c)) {
            if matches!(prop(child, "visibility"), Some("hidden" | "collapse")) {
                continue;
            }
            let t = transform::of(child, self.view).map_or(base, |t| t.then(&base));
            let stretch = t.max_stretch();
            if !(stretch.is_finite() && stretch > 0.0) {
                continue;
            }
            let path = geometry::path_of(child);
            let polys = polygons(&path.flatten(TOLERANCE / stretch), &t);
            if polys.is_empty() {
                continue;
            }
            let within = match self.ids.target(doc, child, "clip-path", Kind::ClipPath) {
                Some(inner) => match self.clip(inner, &t, path.bounds(), depth + 1) {
                    Some(within) => Some(within),
                    None => continue,
                },
                None => None,
            };
            if self.clip_budget == 0 {
                return None;
            }
            self.clip_budget -= 1;
            shapes.push(((polys, prop(child, "clip-rule").and_then(fill_rule).unwrap_or(inherited)), within));
        }
        (!shapes.is_empty()).then_some(Clip { shapes, outer })
    }
}
