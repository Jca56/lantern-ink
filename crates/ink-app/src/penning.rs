//! The Pen on the canvas (ARCHITECTURE §8, D17; LS3's `penning.rs`). A
//! press on bare canvas places an anchor, and dragging on pulls its
//! handles out, one each way: on the path whose loose end is the one
//! anchor picked, else as the first point of a new path, on top of the
//! level the Pointer is in. A press on that path's other end closes it.
//! Anchors land on whole units of the drawing (Ctrl frees them); a
//! handle pulled out is free (Shift: at a multiple of 45°).
//!
//! Everything else is the Node tool's, which runs under the Pen
//! (`noding.rs`): anchors, handles and segments are dragged, and a
//! picked end of any open path is where the Pen goes on from. Enter or
//! Escape lets go of the path, so the next press begins another;
//! Delete takes the end back off.
//!
//! **A path begins with its second point.** The first is the tool's
//! own until then (an SVG path of one point draws nothing, and would
//! be left in the file by a press and a change of mind). So is the
//! handle pulled out of the path's end: it has no segment to be in till
//! the next point is placed.

use ink_core::{Actor, Command, DocId, Document, NodeId, Place};
use ink_doc::outline::AnchorId;
use ink_doc::pathedit::PathEdit;
use ink_doc::{Kind, Precision, elements, geometry};
use ink_geom::{Affine, Path, Seg, Subpath};
use lntrn_math::Vec2;
use lntrn_ui::{CursorIcon, Ui};

use crate::anchors;
use crate::canvas::CanvasInput;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::nodeops::NodeOp;
use crate::nodes::{self, Hit};
use crate::overlay::Scene;
use crate::pointer::View;
use crate::shapes;
use crate::tools::Tool;

/// How far the pointer goes from a new anchor before its handles are
/// pulled out, and how big the other end is for a press that closes
/// the path, logical px (LS3's).
const PULL_MIN: f64 = 3.0;
pub const CLOSE: f64 = 44.0;

/// Where the Pen goes on from: a loose end of an open path.
struct Tip {
    node: NodeId,
    id: AnchorId,
    /// Where it is, in the path's own coordinates, which are the
    /// drawing's through `to_doc`.
    at: Vec2,
    to_doc: Affine,
    /// It's its run's last anchor (else its first).
    last: bool,
    /// The run's other end, where it has one: a press there closes it.
    /// Its place, and its handle on the run as it is, from it.
    other: Option<(AnchorId, Vec2, Option<Vec2>)>,
}

impl Tip {
    /// The anchor `id` of `node`, if the Pen can go on from it: the
    /// path may be changed, and it's an end of an open run.
    fn of(drawing: &Document, node: NodeId, id: AnchorId) -> Option<Tip> {
        if drawing.get(node)?.kind != Kind::Path || drawing.lock_over(node).is_some() {
            return None;
        }
        let outline = drawing.outline(node)?;
        let (r, i) = outline.find(id).filter(|_| anchors::is_end(&outline, id))?;
        let run = &outline.runs[r];
        let last = i + 1 == run.anchors.len();
        let other = (run.anchors.len() >= 2).then(|| {
            let end = run.anchors[if last { 0 } else { run.anchors.len() - 1 }];
            let (into, out) = outline.handles(end.id);
            (end.id, end.at, if last { out } else { into }.map(|h| h - end.at))
        });
        Some(Tip { node, id, at: run.anchors[i].at, to_doc: geometry::to_doc(drawing, node)?, last, other })
    }

    /// What closes the run: a line between its ends, or a curve where
    /// either has a handle for it (`out`: the one pulled out of this
    /// end; the other end's is its own, turned round).
    fn closed(&self, out: Option<Vec2>) -> Command {
        let handle = |anchor: AnchorId, h: Vec2, leaving: bool| if leaving { PathEdit::Handles { anchor, into: None, out: Some(Some(h)) } } else { PathEdit::Handles { anchor, into: Some(Some(h)), out: None } };
        let mut edits = vec![PathEdit::Close { anchor: self.id }];
        // The closing segment leaves the run's last anchor for its first.
        edits.extend(out.map(|h| handle(self.id, h, self.last)));
        edits.extend(self.other.and_then(|(id, _, h)| Some(handle(id, h? * -1.0, !self.last))));
        Command::EditPath { node: self.node, edits }
    }
}

/// A point being placed, the button still down: at `at` (the drawing's
/// coordinates), where a press at `press` put it. Its handles are
/// pulled out by as far as the pointer goes from the press.
enum Drag {
    /// The first of a new path.
    Start { at: Vec2, press: Vec2 },
    /// The second: the path is made, from `from` (with the handle
    /// pulled out of it) to `at`, in the group `into`.
    Begin { from: Vec2, out: Option<Vec2>, at: Vec2, press: Vec2, into: NodeId },
    /// One more, on from the end `from` of `node`, whose own
    /// coordinates the drawing's are through `to_own`.
    Extend { node: NodeId, from: AnchorId, out: Option<Vec2>, at: Vec2, press: Vec2, to_own: Affine },
}

/// What a commit leaves the Pen with: the new end, and the handle
/// pulled out of it (the drawing's coordinates).
enum After {
    Began(Option<Vec2>),
    Extended(NodeId, Option<Vec2>),
    Closed,
}

enum Ask {
    Begin,
    Update(Command),
    Commit(Command, After),
    Edit(Command, &'static str, After),
}

/// What the Pen keeps between frames.
#[derive(Default)]
pub struct Penning {
    /// A first point put down, of a path not begun yet: in which
    /// drawing, where, and the handle pulled out of it (both the
    /// drawing's coordinates).
    start: Option<(DocId, Vec2, Option<Vec2>)>,
    /// The handle pulled out of a path's end as it was placed, in the
    /// path's own coordinates: it goes with the next segment.
    out: Option<(NodeId, AnchorId, Vec2)>,
    drag: Option<(DocId, Drag)>,
    said: Option<String>,
}

impl Penning {
    /// The button is down on the canvas for the Pen.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }

    /// Let go of a first point not yet a path, and of the end's handle.
    /// Whether there was a first point.
    pub fn forget(&mut self) -> bool {
        self.out = None;
        self.start.take().is_some()
    }

    #[cfg(test)]
    pub(crate) fn start(&self) -> Option<Vec2> {
        self.start.map(|(_, at, _)| at)
    }
}

/// `v` turned to the nearest multiple of 45°, as long as it was.
fn octant(v: Vec2) -> Vec2 {
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (v.y.atan2(v.x) / step).round() * step;
    Vec2::new(angle.cos(), angle.sin()) * v.length()
}

/// The path from `from` to `to` (handles `out` and `into`, from each),
/// as a Command that makes it: on top of `group`, in whose coordinates
/// the drawing's are through `into_group`.
fn begun(drawing: &Document, group: NodeId, into_group: &Affine, (from, out): (Vec2, Option<Vec2>), (to, into): (Vec2, Option<Vec2>), paints: &crate::paint::Paints) -> Command {
    let place = |p: Vec2, h: Option<Vec2>| (into_group.apply(p), h.map(|h| into_group.linear(h)));
    let ((a, out), (b, into)) = (place(from, out), place(to, into));
    let seg = if out.is_none() && into.is_none() { Seg::Line { to: b } } else { Seg::Cubic { c1: out.map_or(a, |h| a + h), c2: into.map_or(b, |h| b + h), to: b } };
    let d = Precision::of(drawing).path(&Path { subpaths: vec![Subpath { start: a, segs: vec![seg], closed: false }] });
    match elements(&shapes::path(&d, paints)) {
        Ok(elements) => Command::Insert { place: Place::LastIn(group), elements },
        Err(_) => Command::Batch(Vec::new()),
    }
}

impl Ink {
    /// Give up a point being placed: as if the button never went down.
    pub(crate) fn drop_pen(&mut self) {
        let Some((doc, drag)) = self.penning.drag.take() else { return };
        self.penning.said = None;
        match drag {
            Drag::Start { .. } => self.penning.start = None,
            Drag::Begin { .. } | Drag::Extend { .. } => self.core.cancel(doc),
        }
    }

    /// Enter: the Pen lets go of its path, so the next press begins
    /// another.
    pub(crate) fn pen_end(&mut self) {
        if self.tools.active() == Tool::Pen && !self.penning.busy() {
            self.penning.forget();
            self.noding.unpick();
        }
    }

    /// Delete, with the Pen: a first point not yet a path is forgotten;
    /// else the anchors picked go, and where that was the path's end,
    /// the one before it is the end to go on from.
    pub(crate) fn pen_back(&mut self) {
        if self.penning.forget() {
            return;
        }
        let before = match self.noding.picked.as_slice() {
            [(node, id)] => self.noding.of.and_then(|doc| self.core.doc(doc).ok()).and_then(|drawing| {
                let outline = drawing.outline(*node)?;
                let (r, i) = outline.find(*id).filter(|_| anchors::is_end(&outline, *id))?;
                let run = &outline.runs[r];
                run.anchors.get(if i == 0 { 1 } else { i - 1 }).map(|a| (*node, a.id))
            }),
            _ => None,
        };
        self.node_op(NodeOp::Delete);
        if let Some(before) = before
            && self.noding.picked.is_empty()
        {
            self.noding.picked = vec![before];
        }
    }

    /// One frame of the Pen on `doc` (`active`: it's in hand). Whether
    /// the pointer is its own this frame: else it's the Node tool's.
    pub(crate) fn pen_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, active: bool, scene: &mut Scene) -> bool {
        if self.penning.drag.as_ref().is_some_and(|(on, _)| *on != doc || !active) {
            self.drop_pen();
        }
        if !active || self.penning.start.is_some_and(|(on, ..)| on != doc) {
            self.penning.forget();
        }
        // The Node tool in the middle of something has the pointer.
        if !active || self.noding.busy() {
            return false;
        }
        let (s, pointer, mods) = (view.scale, ui.state.pointer, ui.state.mods);
        let at = view.to_doc.apply(pointer);
        let Ok(drawing) = self.core.doc(doc) else { return false };
        let Ok((looks, _)) = self.core.shown(doc) else { return false };
        let Some(tab) = self.tabs.iter().find(|t| t.doc == doc) else { return false };
        let group = tab.selection.context(drawing);
        // What the Node tool would take: an anchor, a handle or a
        // segment of what's selected.
        let shown = anchors::shown(drawing, looks, &self.noding.would_be, &anchors::editable(drawing, &tab.selection.nodes), &view.to_window);
        let hover = if input.over && self.penning.drag.is_none() { nodes::under(&shown, &self.noding.picked, pointer, s) } else { None };
        // Where the path goes on from, and the handle pulled out of it.
        let tip = match self.noding.picked.as_slice() {
            [(node, id)] if self.noding.of == Some(doc) => Tip::of(drawing, *node, *id),
            _ => None,
        };
        self.penning.out = self.penning.out.filter(|(node, id, _)| tip.as_ref().is_some_and(|t| t.node == *node && t.id == *id));
        let out = self.penning.out.map(|(_, _, out)| out);
        let window = |p: Vec2| view.to_window.apply(p);
        let closes = tip.as_ref().and_then(|t| t.other.map(|(_, end, _)| window(t.to_doc.apply(end)))).filter(|end| input.over && self.penning.drag.is_none() && !matches!(hover, Some((_, Hit::Handle { .. }))) && end.distance(pointer) <= CLOSE * s / 2.0);
        // A new point: on whole units of the drawing, unless Ctrl.
        let grid = if mods.ctrl() {
            0.0
        } else {
            let page = ink_doc::arrange::page_box(drawing);
            shapes::grid_for(page.width().max(page.height()))
        };
        let place = shapes::on_grid(at, grid);
        // The handle pulled out of a point by a press at `from`: as far
        // as the pointer has gone from there.
        let pulled = |from: Vec2| (window(from).distance(pointer) >= PULL_MIN * s).then(|| if mods.shift() { octant(at - from) } else { at - from });

        let (mut ask, mut say) = (Vec::new(), None);
        let released = input.released || !input.held;
        let mut taken = self.penning.drag.is_some();
        let drag = match self.penning.drag.take().map(|(_, drag)| drag) {
            None if input.pressed => match (&tip, self.penning.start) {
                (Some(tip), _) if closes.is_some() => {
                    taken = true;
                    ask.push(Ask::Edit(tip.closed(out), "Close Path", After::Closed));
                    None
                }
                // On an anchor, a handle or a segment: the Node tool's.
                _ if hover.is_some() => None,
                (Some(tip), _) => {
                    taken = true;
                    // (Not twice on one spot: a double click isn't two
                    // points.)
                    (tip.to_doc.apply(tip.at).distance(place) > 1e-9).then_some(()).and_then(|()| tip.to_doc.inverse()).map(|to_own| {
                        ask.push(Ask::Begin);
                        Drag::Extend { node: tip.node, from: tip.id, out, at: place, press: at, to_own }
                    })
                }
                (None, Some((_, from, out))) => {
                    taken = true;
                    (from.distance(place) > 1e-9).then(|| {
                        ask.push(Ask::Begin);
                        Drag::Begin { from, out, at: place, press: at, into: group }
                    })
                }
                (None, None) => {
                    taken = true;
                    self.penning.start = Some((doc, place, None));
                    self.noding.picked.clear();
                    Some(Drag::Start { at: place, press: at })
                }
            },
            None => None,
            Some(Drag::Start { at: placed, press }) => {
                self.penning.start = Some((doc, placed, pulled(press)));
                (!released).then_some(Drag::Start { at: placed, press })
            }
            Some(Drag::Begin { from, out, at: placed, press, into }) => {
                let pull = pulled(press);
                let into_group = geometry::to_doc(drawing, into).and_then(|t| t.inverse()).unwrap_or(Affine::IDENTITY);
                let command = begun(drawing, into, &into_group, (from, out), (placed, pull.map(|p| p * -1.0)), &self.paints);
                scene.levers.extend(pull.into_iter().flat_map(|p| [(window(placed), window(placed + p)), (window(placed), window(placed - p))]));
                scene.anchors.extend([(window(from), false), (window(placed), true)]);
                if released {
                    ask.push(Ask::Commit(command, After::Began(pull)));
                    None
                } else {
                    ask.push(Ask::Update(command));
                    Some(Drag::Begin { from, out, at: placed, press, into })
                }
            }
            Some(Drag::Extend { node, from, out, at: placed, press, to_own }) => {
                let pull = pulled(press);
                let own = |p: Vec2| to_own.linear(p);
                let command = Command::EditPath { node, edits: vec![PathEdit::Extend { from, to: to_own.apply(placed), out, into: pull.map(|p| own(p) * -1.0) }] };
                scene.levers.extend(pull.into_iter().flat_map(|p| [(window(placed), window(placed + p)), (window(placed), window(placed - p))]));
                scene.anchors.push((window(placed), true));
                if released {
                    ask.push(Ask::Commit(command, After::Extended(node, pull)));
                    None
                } else {
                    ask.push(Ask::Update(command));
                    Some(Drag::Extend { node, from, out, at: placed, press, to_own })
                }
            }
        };

        // Over the canvas: a first point not yet a path, the handle
        // pulled out of the end, and a ring round the end a press would
        // close the path on.
        if let Some((_, from, pull)) = self.penning.start.filter(|_| !matches!(drag, Some(Drag::Begin { .. }))) {
            scene.levers.extend(pull.map(|p| (window(from), window(from + p))));
            scene.anchors.push((window(from), true));
        }
        if let (Some(tip), Some(out), None) = (&tip, out, &drag) {
            scene.levers.push((window(tip.to_doc.apply(tip.at)), window(tip.to_doc.apply(tip.at + out))));
        }
        scene.ring = closes;
        if closes.is_some() {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }

        // The drawing is read; now what the Pen asks of the core.
        for ask in ask {
            let done = match ask {
                Ask::Begin => self.core.begin(doc, Actor::Alva).map(|()| None),
                Ask::Update(command) => self.core.update(doc, &command).map(|_| None),
                Ask::Commit(command, after) => self.core.update(doc, &command).and_then(|_| self.core.commit(doc, "Pen")).map(|applied| Some((applied, after))),
                Ask::Edit(command, label, after) => Ok(self.edit(doc, &command, label).map(|applied| (applied, after))),
            };
            match done {
                Ok(Some((applied, after))) => self.pen_landed(doc, &applied, after),
                Ok(None) => {}
                Err(e) => say = Some(e.to_string()),
            }
        }
        match say {
            Some(why) if self.penning.said.as_ref() != Some(&why) => {
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
                self.penning.said = Some(why);
            }
            _ => {}
        }
        if drag.is_none() {
            self.penning.said = None;
        }
        self.penning.drag = drag.map(|drag| (doc, drag));
        taken
    }

    /// A point landed: the path's new end is the one anchor picked, with
    /// the handle that was pulled out of it kept for the next segment.
    fn pen_landed(&mut self, doc: DocId, applied: &ink_core::Applied, after: After) {
        self.penning.start = None;
        self.penning.out = None;
        let Ok(drawing) = self.core.doc(doc) else { return };
        let end = match after {
            After::Closed => None,
            // The path just made: it's what's selected, and its second
            // anchor is its end.
            After::Began(pull) => applied.created.first().and_then(|&node| Some((node, drawing.outline(node)?.anchors().last()?.id, pull))),
            After::Extended(node, pull) => applied.anchors.first().map(|&made| (node, made, pull)),
        };
        let Some((node, id, pull)) = end else {
            self.noding.picked.clear();
            return;
        };
        let to_own = geometry::to_doc(drawing, node).and_then(|t| t.inverse()).unwrap_or(Affine::IDENTITY);
        self.penning.out = pull.map(|p| (node, id, to_own.linear(p)));
        if let (After::Began(_), Some(tab)) = (&after, self.tabs.iter_mut().find(|t| t.doc == doc)) {
            tab.selection.select_only(node);
            tab.selection.reveal(drawing, node);
            self.tree.show(node);
        }
        (self.noding.picked, self.noding.of) = (vec![(node, id)], Some(doc));
    }
}
