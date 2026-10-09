//! The Pointer (ARCHITECTURE §8): a click on the canvas picks what's
//! there (Shift adds or takes away), a drag on nothing draws a marquee,
//! a double-click goes into a group. The selection's box has handles:
//! drag inside it to move, a corner or a side to scale, just outside a
//! corner to turn (`handles.rs` has what each drag comes to). A
//! rectangle picked alone has a handle of its own besides: the dot that
//! rounds its corners (`rounding.rs`).
//!
//! A drag of the box is a gesture in the core (§4.3): the real drawing
//! shows as it goes, and it lands as one step. The box itself is the
//! window's, worked out from where the drag began, so it keeps up with
//! the pointer whatever the drawing's tiles are doing.

use ink_core::{Actor, Command, DocId, NodeId};
use ink_doc::geometry;
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, Ui};

use crate::canvas::CanvasInput;
use crate::cursors::Cursor;
use crate::handles::{self, HIT, Handle, Keys, TURN};
use crate::ink::Ink;
use crate::overlay::Scene;
use crate::picking::{caught, pick};
use crate::rounding::{self, Rounded};
use crate::select::Click;
use crate::shapes;

/// How far the pointer may stray from a press and still have clicked,
/// and how far from the pointer a click still finds something, logical
/// px.
const SLOP: f64 = 4.0;
const REACH: f64 = 3.0;

/// How the drawing sits in the window this frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// The drawing's coordinates to window px, and back.
    pub to_window: Affine,
    pub to_doc: Affine,
    /// Window px per logical px.
    pub scale: f64,
}

impl View {
    /// `b` (in the drawing's coordinates) as it shows: its corners,
    /// from the top left clockwise, once through `by`.
    fn quad(&self, b: Rect, by: &Affine) -> [Vec2; 4] {
        handles::corners(b).map(|p| self.to_window.apply(by.apply(p)))
    }
}

/// What the button went down on.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Press {
    /// Where, window px.
    at: Vec2,
    /// The part of the selection's box a drag from here takes hold of;
    /// none on the bare canvas (a drag is a marquee).
    handle: Option<Handle>,
    /// The thing picked there, and whether it was selected already (a
    /// click then narrows the selection to it, or with Shift lets it
    /// go).
    pick: Option<(NodeId, bool)>,
    shift: bool,
    /// On this rectangle's corner dot: a drag from here rounds it.
    dot: Option<NodeId>,
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    /// Down, and not gone far: a click, unless it does.
    Pressed(Press),
    /// A marquee from here (the drawing's coordinates), over what was
    /// selected before it (kept, with Shift).
    Marquee { from: Vec2, keep: Vec<NodeId> },
    /// The selection's box, dragged by `handle`: a gesture in the core.
    Shaping {
        handle: Handle,
        /// Where the drag began, and the box as it was then.
        from: Vec2,
        was: Rect,
        /// What's dragged, back to front, and each one's box then.
        nodes: Vec<(NodeId, Rect)>,
    },
    /// A rectangle's corner dot, dragged: a gesture too. The rectangle
    /// as it was, and where the drag began in its own coordinates.
    Rounding { node: NodeId, was: Rounded, from: Vec2 },
}

/// What the Pointer keeps between frames.
#[derive(Default)]
pub struct Pointer {
    drag: Option<(DocId, Drag)>,
    /// The last refusal said during this drag: said once, not at every
    /// frame.
    said: Option<String>,
    /// A drag going on somewhere else (a number of the Box): what it
    /// has the selection going through this frame, so its box on the
    /// canvas goes along.
    pub(crate) carried: Option<(DocId, Affine)>,
    /// A right press on the canvas this frame: where its menu opens.
    pub(crate) menu_at: Option<Vec2>,
    /// The selection's box as a drag on the canvas has it this frame,
    /// in the drawing's coordinates: what the Box's numbers read while
    /// it goes.
    pub(crate) live: Option<Rect>,
}

impl Pointer {
    /// The button is down on the canvas for the Pointer: the view holds
    /// still, and no key does anything but Escape.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }
}

/// The pointer's picture over a part of the box.
fn cursor(handle: Handle) -> CursorIcon {
    match handle {
        Handle::Body => CursorIcon::Default,
        Handle::Corner(i) if i.is_multiple_of(2) => CursorIcon::NwseResize,
        Handle::Corner(_) => CursorIcon::NeswResize,
        Handle::Side(i) if i.is_multiple_of(2) => CursorIcon::NsResize,
        Handle::Side(_) => CursorIcon::EwResize,
        Handle::Turn(_) => Cursor::Rotate.icon(),
    }
}

/// What a frame of the Pointer asks of the core, once the drawing is
/// no longer being read.
enum Ask {
    Begin,
    Update(Command),
    Commit(Command, &'static str),
}

impl Ink {
    /// Give up whatever the Pointer is in the middle of: a drag of the
    /// box is as if it never began, a marquee leaves what was selected
    /// before it.
    pub(crate) fn drop_drag(&mut self) {
        let Some((doc, drag)) = self.pointing.drag.take() else { return };
        self.pointing.said = None;
        match drag {
            Drag::Shaping { .. } | Drag::Rounding { .. } => self.core.cancel(doc),
            Drag::Marquee { keep, .. } => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.doc == doc) {
                    tab.selection.active = keep.last().copied();
                    tab.selection.nodes = keep;
                }
            }
            Drag::Pressed(_) => {}
        }
    }

    /// One frame of the Pointer on `doc`, and of the selection's box
    /// whatever tool is in hand (`active`: it's the Pointer). What to
    /// draw over the canvas.
    pub(crate) fn pointer_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, active: bool) -> Scene {
        // A drag on another tab's drawing, or with another tool in
        // hand by now: given up.
        if self.pointing.drag.as_ref().is_some_and(|(on, _)| *on != doc || !active) {
            self.drop_drag();
        }
        let s = view.scale;
        let pointer = ui.state.pointer;
        let at = view.to_doc.apply(pointer);
        let keys = Keys { shift: ui.state.mods.shift(), alt: ui.state.mods.alt() };
        let scale_strokes = self.settings.scale_strokes;
        let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);

        let Ok(drawing) = self.core.doc(doc) else { return Scene::default() };
        let stamp = self.core.history(doc).map_or(0, |h| h.stamp());
        let Some(tab) = self.tabs.iter_mut().find(|t| t.doc == doc) else { return Scene::default() };
        let boxes = tab.boxes(drawing, stamp).clone();
        let sel = &mut tab.selection;
        let context = sel.context(drawing);
        // What the selection's box is round: what's drawn of it.
        let tops: Vec<(NodeId, Rect)> = sel.tops(drawing).into_iter().filter_map(|id| Some((id, *boxes.get(&id)?))).collect();
        let joint = tops.iter().map(|(_, b)| *b).reduce(|a, b| a.union(&b));
        let zone = joint.filter(|_| active && input.over).and_then(|j| handles::hit(view.to_window.bounds(&j), pointer, HIT * s, TURN * s));
        // A rectangle picked alone, that may be changed and that
        // nothing else has hold of: its corner dot, through to the
        // window from its own coordinates.
        let free = !self.core.gesturing(doc) || matches!(self.pointing.drag, Some((_, Drag::Rounding { .. })));
        let own = |id: NodeId| geometry::to_doc(drawing, id).map(|to_doc| to_doc.then(&view.to_window));
        let round = match tops.as_slice() {
            [(id, _)] if active && free && drawing.lock_over(*id).is_none() => Rounded::of(drawing, *id).zip(own(*id)).map(|(rect, own)| (*id, rect, own)),
            _ => None,
        };
        let dot = round.and_then(|(_, rect, own)| rect.dot(&own, s));
        // Its box's corners and sides come first, where they meet.
        let on_dot = input.over && zone.is_none_or(|z| z == Handle::Body) && dot.is_some_and(|d| (pointer - d).length() <= rounding::REACH * s);

        let (mut ask, mut show, mut say) = (Vec::new(), None, None);
        let mut scene = Scene { handles: active, ..Scene::default() };
        let mut by = Affine::IDENTITY;
        // A right press: on something not picked, it's picked first;
        // then the selection's menu opens there.
        if active && input.over && ui.state.right_pressed && self.pointing.drag.is_none() {
            match pick(drawing, context, at, REACH * s / per_unit) {
                Some((node, out)) if !sel.is_selected(node) => {
                    sel.click(drawing, node, Click::Plain);
                    if out {
                        sel.within = None;
                    }
                    show = Some(node);
                }
                None if zone.is_none() => sel.clear(),
                _ => {}
            }
            self.pointing.menu_at = Some(pointer);
        }
        let carried = self.pointing.carried.take().filter(|(on, _)| *on == doc).map(|(_, by)| by);
        let released = input.released || !input.held;
        let drag = match self.pointing.drag.take().map(|(_, drag)| drag) {
            None if active && input.pressed => {
                let on_box = zone.filter(|z| *z != Handle::Body);
                let found = if on_box.is_some() || on_dot { None } else { pick(drawing, context, at, REACH * s / per_unit) };
                let press = |handle, pick| Press { at: pointer, handle, pick, shift: keys.shift, dot: None };
                match found {
                    _ if on_dot => Some(Drag::Pressed(Press { dot: round.map(|(id, ..)| id), ..press(None, None) })),
                    _ if on_box.is_some() => Some(Drag::Pressed(press(on_box, None))),
                    Some((node, out)) => {
                        if out {
                            sel.within = None;
                        }
                        if input.double && !out && drawing.get(node).is_some_and(|n| n.kind.is_group()) {
                            // Into the group, to what's under the
                            // pointer in it.
                            sel.within = Some(node);
                            if let Some((inner, _)) = pick(drawing, node, at, REACH * s / per_unit) {
                                sel.select_only(inner);
                                show = Some(inner);
                            }
                            None
                        } else {
                            let was = sel.is_selected(node);
                            if !was {
                                sel.click(drawing, node, if keys.shift { Click::Toggle } else { Click::Plain });
                                show = Some(node);
                            }
                            Some(Drag::Pressed(press(Some(Handle::Body), Some((node, was)))))
                        }
                    }
                    // On nothing: inside the box a drag still moves it.
                    None => Some(Drag::Pressed(press(zone, None))),
                }
            }
            None => None,
            Some(Drag::Pressed(press)) if released => {
                // A click.
                match press.pick {
                    // On the dot: nothing's picked or let go.
                    _ if press.dot.is_some() => {}
                    Some((node, true)) if press.shift => sel.click(drawing, node, Click::Toggle),
                    Some((node, true)) => sel.select_only(node),
                    // Picked as the button went down.
                    Some((_, false)) => {}
                    None if press.handle.is_none_or(|h| h == Handle::Body) && !press.shift => {
                        sel.clear();
                        sel.within = None;
                    }
                    None => {}
                }
                None
            }
            Some(Drag::Pressed(press)) if (pointer - press.at).length() > SLOP * s => {
                let from = view.to_doc.apply(press.at);
                match (press.handle, joint) {
                    _ if press.dot.is_some() => round.filter(|(id, ..)| press.dot == Some(*id)).and_then(|(node, was, _)| {
                        let from = rounding::to_own(drawing, node)?.apply(from);
                        ask.push(Ask::Begin);
                        Some(Drag::Rounding { node, was, from })
                    }),
                    (Some(handle), Some(was)) if !tops.is_empty() => {
                        ask.push(Ask::Begin);
                        Some(Drag::Shaping { handle, from, was, nodes: tops.clone() })
                    }
                    (Some(_), _) => None,
                    (None, _) => Some(Drag::Marquee { from, keep: if press.shift { sel.nodes.clone() } else { Vec::new() } }),
                }
            }
            Some(pressed @ Drag::Pressed(_)) => Some(pressed),
            Some(Drag::Marquee { from, keep }) => {
                let marquee = Rect::new(from.min(at), from.max(at));
                let mut nodes = keep.clone();
                for id in caught(drawing, context, &boxes, marquee) {
                    if !nodes.contains(&id) {
                        nodes.push(id);
                    }
                }
                (sel.active, sel.nodes) = (nodes.last().copied(), nodes);
                if released {
                    show = sel.active;
                    None
                } else {
                    scene.marquee = Some(view.to_window.bounds(&marquee));
                    Some(Drag::Marquee { from, keep })
                }
            }
            Some(Drag::Shaping { handle, from, was, nodes }) => {
                by = handles::dragged(was, handle, from, at, keys);
                let ids: Vec<NodeId> = nodes.iter().map(|(id, _)| *id).collect();
                let (command, label) = match handle {
                    Handle::Body => (Command::Transform { nodes: ids, by }, "Move"),
                    Handle::Turn(_) => (Command::Transform { nodes: ids, by }, "Rotate"),
                    Handle::Corner(_) | Handle::Side(_) if scale_strokes => (Command::Transform { nodes: ids, by }, "Scale"),
                    Handle::Corner(_) | Handle::Side(_) => (Command::Resize { nodes: ids, by }, "Scale"),
                };
                ui.state.cursor_icon = cursor(handle);
                if released {
                    ask.push(Ask::Commit(command, label));
                    None
                } else {
                    ask.push(Ask::Update(command));
                    Some(Drag::Shaping { handle, from, was, nodes })
                }
            }
            Some(Drag::Rounding { node, was, from }) => {
                // On whole units, as new shapes land; freely with Ctrl.
                let page = ink_doc::arrange::page_box(drawing);
                let grid = if ui.state.mods.ctrl() { 0.0 } else { shapes::grid_for(page.width().max(page.height())) };
                let now = rounding::to_own(drawing, node).map_or(was, |to_own| was.dragged(from, to_own.apply(at), grid));
                let command = Command::SetGeometry { node, geometry: now.geometry() };
                ui.state.cursor_icon = CursorIcon::Pointer;
                // Where the dot is now, on the rectangle as it was put.
                scene.dot = round.and_then(|(_, _, own)| now.dot(&own, s)).map(|d| (d, true));
                if released {
                    ask.push(Ask::Commit(command, "Corners"));
                    None
                } else {
                    ask.push(Ask::Update(command));
                    Some(Drag::Rounding { node, was, from })
                }
            }
        };

        // The box: where the drag has it, or where the selection is.
        match &drag {
            Some(Drag::Shaping { was, nodes, .. }) => {
                scene.boxes = nodes.iter().map(|(_, b)| view.quad(*b, &by)).collect();
                scene.joint = Some(view.quad(*was, &by));
                self.pointing.live = Some(by.bounds(was));
            }
            _ => {
                // After a pick this frame, the selection's as it is now;
                // and where a number of the Box has it going.
                let through = carried.unwrap_or(Affine::IDENTITY);
                let tops: Vec<Rect> = sel.tops(drawing).into_iter().filter_map(|id| boxes.get(&id).copied()).collect();
                scene.joint = tops.iter().copied().reduce(|a, b| a.union(&b)).map(|j| view.quad(j, &through));
                scene.boxes = tops.into_iter().map(|b| view.quad(b, &through)).collect();
                if drag.is_none()
                    && let Some(zone) = zone
                {
                    ui.state.cursor_icon = if on_dot { CursorIcon::Pointer } else { cursor(zone) };
                }
                // The dot, where nothing is being dragged past it.
                if scene.dot.is_none() && carried.is_none() && !matches!(drag, Some(Drag::Marquee { .. })) {
                    scene.dot = dot.map(|d| (d, on_dot || matches!(drag, Some(Drag::Pressed(Press { dot: Some(_), .. })))));
                }
            }
        }
        scene.entered = sel.within.and_then(|g| boxes.get(&g)).map(|b| view.quad(*b, &Affine::IDENTITY));
        if let Some(node) = show {
            sel.reveal(drawing, node);
        }

        // The drawing is read; now what the drag asks of the core.
        for ask in ask {
            let done = match ask {
                Ask::Begin => self.core.begin(doc, Actor::Alva),
                Ask::Update(command) => self.core.update(doc, &command).map(|_| ()),
                // Where it is now is where it lands; and the gesture
                // is over, whatever the drawing says to it.
                Ask::Commit(command, label) => {
                    let shown = self.core.update(doc, &command).map(|_| ());
                    shown.and(self.core.commit(doc, label).map(|_| ()))
                }
            };
            if let Err(e) = done {
                say = Some(e.to_string());
            }
        }
        // What the drawing won't have is said once a drag, in the
        // names its rows go by.
        match say {
            Some(why) if self.pointing.said.as_ref() != Some(&why) => {
                let said = self.core.doc(doc).map_or(why.clone(), |d| crate::edits::in_row_names(d, &why));
                self.toast(said);
                self.pointing.said = Some(why);
            }
            _ => {}
        }
        if drag.is_none() {
            self.pointing.said = None;
        }
        if let Some(node) = show {
            self.tree.show(node);
        }
        self.pointing.drag = drag.map(|drag| (doc, drag));
        scene
    }

    /// The arrow keys: the selection moved `by`, in the drawing's
    /// units.
    pub(crate) fn nudge(&mut self, by: Vec2) {
        // The Node tool's anchors, where it has some picked.
        if self.anchors_in_hand() {
            return self.nudge_anchors(by);
        }
        let Some(tab) = self.tabs.active() else { return };
        let doc = tab.doc;
        let Ok(drawing) = self.core.doc(doc) else { return };
        // What's drawn of it: a definition has nowhere to go.
        let nodes: Vec<NodeId> = tab.selection.tops(drawing).into_iter().filter(|&id| crate::select::is_drawn(drawing, id)).collect();
        if !nodes.is_empty() {
            self.edit(doc, &Command::Transform { nodes, by: Affine::translate(by.x, by.y) }, "Nudge");
        }
    }

    /// Escape: out of a drag; else the Node tool's anchors let go; else
    /// out of the group the Pointer is in, one level, with that group
    /// picked; else nothing picked.
    pub(crate) fn escape(&mut self) {
        if self.shaping.is_some() {
            return self.drop_shape();
        }
        if self.pointing.busy() {
            return self.drop_drag();
        }
        if self.noding.busy() {
            return self.drop_nodes();
        }
        if self.penning.busy() {
            return self.drop_pen();
        }
        if self.grading.busy() {
            return self.drop_grade();
        }
        // A text typed into is let go of (what was typed has landed).
        if self.typing() {
            return self.type_done();
        }
        // The Node tool and the Pen let go of their anchors (and the Pen
        // of a first point not yet a path) before anything else.
        let tool = self.tools.active();
        if matches!(tool, crate::tools::Tool::Node | crate::tools::Tool::Pen) && (self.penning.forget() | self.noding.unpick()) {
            return;
        }
        let Some(tab) = self.tabs.active_mut() else { return };
        let Ok(drawing) = self.core.doc(tab.doc) else { return };
        let sel = &mut tab.selection;
        let context = sel.context(drawing);
        if context == drawing.root() {
            sel.clear();
            sel.within = None;
        } else {
            sel.within = drawing.get(context).and_then(|n| n.parent).filter(|&up| up != drawing.root());
            sel.select_only(context);
        }
    }
}
