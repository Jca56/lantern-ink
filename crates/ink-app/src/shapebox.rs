//! A shape's own rows in the Box (M4c; Alva's rule from LS3: the Box
//! holds every selected object's settings as well as every tool's): a
//! rectangle's corners; a polygon's sides, whether it's a star, and how
//! deep its points go. Under a shape tool they're the next shape's
//! (`shapes::Settings`). Under the Pointer they're the selected shape's,
//! read off the drawing (`rounding.rs`, `polygons.rs`), and what's
//! changed on the one in hand is changed on every selected shape of its
//! kind (as in LS3).
//!
//! Dragged, a number is a gesture in the core: the drawing shows it as
//! it goes, and it lands as one step. Typed or ticked, it's one step at
//! once.

use ink_core::{Actor, Command, DocId, Document, NodeId};
use ink_doc::{Geometry, Precision};
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{FILL, Ui};

use crate::controls;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::polygons::Regular;
use crate::rounding::Rounded;
use crate::shapes::{self, Kind, Settings};

/// Where each row was put, by name: what the window's tests press.
pub type Laid = Vec<(&'static str, Rect)>;

/// One of a shape's settings, set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tune {
    /// A rectangle's corners' rounding.
    Radius(f64),
    Sides(usize),
    Star(bool),
    Depth(f64),
}

impl Tune {
    /// What the step that sets it is called.
    fn label(self) -> &'static str {
        match self {
            Tune::Radius(_) => "Corners",
            Tune::Sides(_) => "Sides",
            Tune::Star(_) => "Star",
            Tune::Depth(_) => "Depth",
        }
    }

    /// What `was` changed to `now` set, of a shape of `kind`.
    fn between(kind: Kind, was: &Settings, now: &Settings) -> Option<Tune> {
        match kind {
            Kind::Rect => (now.radius != was.radius).then_some(Tune::Radius(now.radius)),
            Kind::Polygon if now.sides.round() != was.sides.round() => Some(Tune::Sides(now.sides.round() as usize)),
            Kind::Polygon if now.star != was.star => Some(Tune::Star(now.star)),
            Kind::Polygon if now.star && now.depth != was.depth => Some(Tune::Depth(now.depth)),
            _ => None,
        }
    }

    /// The sides and the depth of the polygon `was` with this set
    /// (`depth`: how deep one made a star is).
    fn of(self, was: &Regular, depth: f64) -> Option<(usize, Option<f64>)> {
        match self {
            Tune::Sides(sides) => Some((sides, was.star)),
            // (One that's a star already stays as deep as it is.)
            Tune::Star(star) => Some((was.sides, if star { was.star.or(Some(depth)) } else { None })),
            Tune::Depth(depth) => Some((was.sides, was.star.map(|_| depth))),
            Tune::Radius(_) => None,
        }
    }
}

/// A row of the selected shape's being dragged along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tuning {
    doc: DocId,
    label: &'static str,
}

/// One in the last place `doc` writes: how finely a polygon's corners
/// say where they are.
fn unit(doc: &Document) -> f64 {
    Precision::of(doc).within() * 2.0
}

/// The polygon `node`, as the circle it's drawn on.
fn polygon(doc: &Document, node: NodeId) -> Option<Regular> {
    match Geometry::of(doc.get(node)?)? {
        Geometry::Poly { points, closed: true } => Regular::read(&points, unit(doc)),
        _ => None,
    }
}

/// What the Box shows of `node`: the kind of shape it is, its settings
/// as a tool's are said, and the roundest its corners can be. None for
/// what has no rows: an ellipse, a line, a path, a polygon that isn't a
/// regular one's corners. `depth`: how deep a polygon made a star is.
pub fn read(doc: &Document, node: NodeId, depth: f64) -> Option<(Kind, Settings, f64)> {
    if let Some(rect) = Rounded::of(doc, node) {
        return Some((Kind::Rect, Settings { radius: rect.radius(), ..Settings::default() }, rect.most()));
    }
    let regular = polygon(doc, node)?;
    Some((Kind::Polygon, Settings { sides: regular.sides as f64, star: regular.star.is_some(), depth: regular.star.unwrap_or(depth), ..Settings::default() }, 0.0))
}

/// What makes each of `nodes` that `tune` is about (and that isn't
/// locked) what it says: a rectangle's corners, a polygon's corners on
/// its own circle. `depth`: how deep a polygon made a star is.
pub fn command(doc: &Document, nodes: &[NodeId], tune: Tune, depth: f64) -> Option<Command> {
    let one = |node: NodeId| -> Option<Command> {
        let geometry = match tune {
            Tune::Radius(radius) => Rounded::of(doc, node)?.with(radius).geometry(),
            _ => {
                let was = polygon(doc, node)?;
                // What it is already it's left as, to the byte.
                let (sides, star) = tune.of(&was, depth).filter(|now| *now != (was.sides, was.star))?;
                Geometry::Poly { points: was.points(sides, star), closed: true }
            }
        };
        Some(Command::SetGeometry { node, geometry })
    };
    let mut edits: Vec<Command> = nodes.iter().filter(|&&id| doc.lock_over(id).is_none()).filter_map(|&id| one(id)).collect();
    if edits.len() > 1 { Some(Command::Batch(edits)) } else { edits.pop() }
}

/// A number on a row of its own in the Box. Where it was put.
fn number_row(ui: &mut Ui, name: &str, value: &mut f64, step: f64, range: (f64, f64), decimals: usize) -> Rect {
    let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    controls::number_in(ui, ui.id(name), row, name, value, step, Some(range), decimals);
    row
}

/// The rows of a shape of `kind`, showing `s` and changing it: a
/// rectangle's corners (`step` a pixel along, no rounder than `most`);
/// a polygon's sides, whether it's a star, and how deep its points go.
pub fn rows(ui: &mut Ui, kind: Kind, s: &mut Settings, step: f64, most: f64, laid: &mut Laid) {
    match kind {
        Kind::Rect => laid.push(("Corners", number_row(ui, "Corners", &mut s.radius, step, (0.0, most), 3))),
        Kind::Polygon => {
            // A side for every twenty pixels along.
            laid.push(("Sides", number_row(ui, if s.star { "Points" } else { "Sides" }, &mut s.sides, 0.05, shapes::SIDES, 0)));
            laid.push(("Star", Rect::from_min_size(ui.cursor(), Vec2::new(ui.avail_width(), ui.m.widget_h))));
            controls::toggle(ui, "Star", &mut s.star);
            if s.star {
                let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
                let style = ui.text_style();
                let name_w = ui.measure("Depth", &style).ceil() + ui.m.gap * 2.0;
                ui.text_in_rect("Depth", &style, row, crate::theme::TEXT);
                let mut percent = (s.depth * 100.0).round();
                let rail = Rect::new(Vec2::new(row.min.x + name_w, row.min.y), row.max);
                laid.push(("Depth", rail));
                if controls::Slider::new(5.0, 95.0, 1.0).unit("%").rest(50.0).in_row(ui, ui.id("Depth"), rail, &mut percent) {
                    s.depth = percent / 100.0;
                }
            }
        }
        Kind::Ellipse | Kind::Line => {}
    }
}

impl Ink {
    /// What the Box shows of the selection `tops` of `doc` under the
    /// Pointer: the rows of the one in hand (the last picked), as the
    /// drawing shows it now. None where that has none, or is locked.
    pub(crate) fn own_shown(&self, doc: DocId, tops: &[NodeId]) -> Option<(Kind, Settings, f64)> {
        let (drawing, _) = self.core.shown(doc).ok()?;
        let active = self.tabs.active().and_then(|tab| tab.selection.active);
        let in_hand = active.filter(|a| tops.contains(a)).or(tops.last().copied())?;
        if drawing.lock_over(in_hand).is_some() {
            return None;
        }
        read(drawing, in_hand, self.shape_settings.depth)
    }

    /// The selected shape's rows, in the Box: `was`, as they were read.
    /// What one of them was set to.
    pub(crate) fn own_rows(ui: &mut Ui, (kind, was, most): &(Kind, Settings, f64), step: f64, laid: &mut Laid) -> Option<Tune> {
        let mut now = was.clone();
        rows(ui, *kind, &mut now, step, *most, laid);
        Tune::between(*kind, was, &now)
    }

    /// Set `tune` on the shapes of `tops` it's about. `held`: the button
    /// is down on the row that chose it, so it's shown and not yet done
    /// ([`Ink::tune_settled`] does it). A tick is done at once.
    pub(crate) fn tune(&mut self, doc: DocId, tops: &[NodeId], tune: Tune, held: bool) {
        let Ok(drawing) = self.core.doc(doc) else { return };
        let Some(command) = command(drawing, tops, tune, self.shape_settings.depth) else { return };
        self.box_set(doc, &command, tune.label(), held && !matches!(tune, Tune::Star(_)));
    }

    /// Do `command` from a row of the Box: at once, as the step `label`;
    /// or, `held` (the button is down on the row that chose it), shown
    /// and not yet done ([`Ink::tune_settled`] does it).
    pub(crate) fn box_set(&mut self, doc: DocId, command: &Command, label: &'static str, held: bool) {
        if self.tuning.is_none() && !held {
            self.edit(doc, command, label);
            return;
        }
        if self.tuning.is_none() {
            if self.core.begin(doc, Actor::Alva).is_err() {
                return;
            }
            self.tuning = Some(Tuning { doc, label });
        }
        // A refusal waits for the button to come up to be said.
        let _ = self.core.update(doc, command);
    }

    /// The button came up: what a row was dragged to lands, as one step.
    pub(crate) fn tune_settled(&mut self) {
        let Some(tuning) = self.tuning.take() else { return };
        if let Err(e) = self.core.commit(tuning.doc, tuning.label) {
            let why = e.to_string();
            let said = self.core.doc(tuning.doc).map_or(why.clone(), |d| in_row_names(d, &why));
            self.toast(said);
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId as Id;

    use super::*;

    /// A square with rounded corners, a pentagon as the tool draws one
    /// (10 across its circle, about 12,12), a star of four, an arrow,
    /// and a locked rectangle.
    fn doc() -> Document {
        Document::parse(Id(1), r##"<svg xmlns:ink="urn:lantern:ink" viewBox="0 0 48 48"><rect x="2" y="2" width="20" height="10" rx="2"/><polygon points="12,2 21.511,8.91 17.878,20.09 6.122,20.09 2.489,8.91"/><polygon points="30,20 33.536,26.464 40,30 33.536,33.536 30,40 26.464,33.536 20,30 26.464,26.464"/><polygon points="0,0 6,0 6,2 2,2 2,6 0,6"/><rect width="8" height="8" ink:locked="true"/><circle r="4"/></svg>"##).unwrap()
    }

    const RECT: NodeId = NodeId(2);
    const FIVE: NodeId = NodeId(3);
    const STAR: NodeId = NodeId(4);
    const ARROW: NodeId = NodeId(5);
    const LOCKED: NodeId = NodeId(6);
    const CIRCLE: NodeId = NodeId(7);

    fn points(d: &Document, node: NodeId) -> String {
        d.node(node).unwrap().attr("points").unwrap().to_owned()
    }

    /// Whether `node`'s corners are these, to the file's last decimal.
    fn at(d: &Document, node: NodeId, corners: &[(f64, f64)]) -> bool {
        let Some(Geometry::Poly { points, .. }) = Geometry::of(d.node(node).unwrap()) else { return false };
        points.len() == corners.len() && points.iter().zip(corners).all(|(p, c)| (p.x - c.0).abs() < 0.0015 && (p.y - c.1).abs() < 0.0015)
    }

    #[test]
    fn a_shape_is_read_as_its_tool_would_set_it() {
        let d = doc();
        let shown = |node| read(&d, node, 0.3).map(|(kind, s, most)| (kind, s.radius, s.sides, s.star, (s.depth * 100.0).round(), most));
        assert_eq!(shown(RECT), Some((Kind::Rect, 2.0, 5.0, false, 50.0, 5.0)));
        // One made a star would be as deep as the tool makes them.
        assert_eq!(shown(FIVE), Some((Kind::Polygon, 0.0, 5.0, false, 30.0, 0.0)));
        assert_eq!(shown(STAR), Some((Kind::Polygon, 0.0, 4.0, true, 50.0, 0.0)));
        // A polygon that's no regular one's corners, and a circle: no
        // rows.
        assert_eq!((shown(ARROW), shown(CIRCLE)), (None, None));
    }

    #[test]
    fn a_row_set_is_set_on_every_shape_its_about() {
        let mut d = doc();
        let all = [RECT, FIVE, STAR, ARROW, LOCKED, CIRCLE];
        let set = |d: &mut Document, tune: Tune| d.apply(&command(d, &all, tune, 0.3).expect("something to set")).unwrap().changed;
        // Corners: the rectangle, and not the locked one.
        assert_eq!(set(&mut d, Tune::Radius(4.0)), [RECT]);
        assert_eq!(d.node(RECT).unwrap().attr("rx"), Some("4"));
        // Sides: both polygons that are regular ones, each on its own
        // circle; the star stays a star.
        assert_eq!(set(&mut d, Tune::Sides(3)), [FIVE, STAR]);
        assert!(at(&d, FIVE, &[(12.0, 2.0), (20.66, 17.0), (3.34, 17.0)]), "{}", points(&d, FIVE));
        assert_eq!(read(&d, STAR, 0.3).map(|(_, s, _)| (s.sides, s.star)), Some((3.0, true)));
        // A star made of the triangle, as deep as the tool's; deeper;
        // and no star again.
        assert_eq!(set(&mut d, Tune::Star(true)), [FIVE]);
        assert_eq!(read(&d, FIVE, 0.9).map(|(_, s, _)| (s.sides, s.star, (s.depth * 100.0).round())), Some((3.0, true, 30.0)));
        assert_eq!(set(&mut d, Tune::Depth(0.6)), [FIVE, STAR]);
        assert_eq!(set(&mut d, Tune::Star(false)), [FIVE, STAR]);
        assert!(at(&d, FIVE, &[(12.0, 2.0), (20.66, 17.0), (3.34, 17.0)]) && at(&d, STAR, &[(30.0, 20.0), (38.66, 35.0), (21.34, 35.0)]), "{} / {}", points(&d, FIVE), points(&d, STAR));
        // Back to five: where it began, to the file's last decimal.
        assert_eq!(set(&mut d, Tune::Sides(5)), [FIVE, STAR]);
        assert!(at(&d, FIVE, &[(12.0, 2.0), (21.511, 8.91), (17.878, 20.09), (6.122, 20.09), (2.489, 8.91)]), "{}", points(&d, FIVE));
        // And set to what it is already, it's left alone to the byte.
        let now = points(&d, FIVE);
        assert_eq!(command(&d, &[FIVE], Tune::Sides(5), 0.3), None);
        assert_eq!(command(&d, &[FIVE], Tune::Star(false), 0.3), None);
        assert_eq!(points(&d, FIVE), now);
        // Nothing it's about: nothing to do.
        assert_eq!(command(&d, &[ARROW, CIRCLE, LOCKED], Tune::Sides(6), 0.3), None);
        assert_eq!(command(&d, &[CIRCLE], Tune::Radius(1.0), 0.3), None);
    }

    #[test]
    fn what_changed_between_two_settings_is_what_was_set() {
        let was = Settings { radius: 1.0, sides: 5.0, star: false, depth: 0.5 };
        let with = |f: fn(&mut Settings)| {
            let mut now = was.clone();
            f(&mut now);
            now
        };
        assert_eq!(Tune::between(Kind::Rect, &was, &with(|s| s.radius = 2.5)), Some(Tune::Radius(2.5)));
        assert_eq!(Tune::between(Kind::Polygon, &was, &with(|s| s.sides = 7.4)), Some(Tune::Sides(7)));
        assert_eq!(Tune::between(Kind::Polygon, &was, &with(|s| s.star = true)), Some(Tune::Star(true)));
        // A depth is a star's; and a number dragged less than a side
        // along is no change yet.
        assert_eq!(Tune::between(Kind::Polygon, &was, &with(|s| s.depth = 0.2)), None);
        assert_eq!(Tune::between(Kind::Polygon, &was, &with(|s| s.sides = 5.3)), None);
        let star = Settings { star: true, ..was.clone() };
        assert_eq!(Tune::between(Kind::Polygon, &star, &Settings { depth: 0.2, ..star.clone() }), Some(Tune::Depth(0.2)));
        assert_eq!((Tune::between(Kind::Rect, &was, &was), Tune::between(Kind::Ellipse, &was, &with(|s| s.radius = 9.0))), (None, None));
    }
}
