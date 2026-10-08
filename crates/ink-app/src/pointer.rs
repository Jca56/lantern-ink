//! The Pointer (ARCHITECTURE §8): a click on the canvas picks what's
//! there (Shift adds or takes away), a drag on nothing draws a marquee,
//! a double-click goes into a group. The selection's box has handles:
//! drag inside it to move, a corner or a side to scale, just outside a
//! corner to turn (`handles.rs` has what each drag comes to).
//!
//! A drag of the box is a gesture in the core (§4.3): the real drawing
//! shows as it goes, and it lands as one step. The box itself is the
//! window's, worked out from where the drag began, so it keeps up with
//! the pointer whatever the drawing's tiles are doing.

use std::collections::HashMap;

use ink_core::{Actor, Command, DocId, Document, NodeId};
use ink_doc::hit;
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, Ui};

use crate::canvas::CanvasInput;
use crate::cursors::Cursor;
use crate::handles::{self, HIT, Handle, Keys, TURN};
use crate::ink::Ink;
use crate::overlay::Scene;
use crate::select::Click;

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
}

/// What the Pointer keeps between frames.
#[derive(Default)]
pub struct Pointer {
    drag: Option<(DocId, Drag)>,
    /// The last refusal said during this drag: said once, not at every
    /// frame.
    said: Option<String>,
}

impl Pointer {
    /// The button is down on the canvas for the Pointer: the view holds
    /// still, and no key does anything but Escape.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }
}

/// What a click at `point` (the drawing's coordinates) picks, with the
/// Pointer inside `context`: the thing on top there, as the child of
/// `context` it is or is in. And whether it's outside `context`
/// altogether: then it's picked at the drawing's top level, and the
/// Pointer comes out. Nothing locked is picked: a click goes through
/// it to what's behind. `reach`: how far from the point still counts.
pub fn pick(doc: &Document, context: NodeId, point: Vec2, reach: f64) -> Option<(NodeId, bool)> {
    // The point itself; then, for a thin line just missed, round it.
    let ring = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (-0.7, -0.7), (0.7, -0.7)];
    let root = doc.root();
    let child_of = |holder: NodeId, node: NodeId| std::iter::once(node).chain(doc.ancestors(node).map(|n| n.id)).find(|&n| doc.get(n).and_then(|n| n.parent) == Some(holder));
    for (dx, dy) in ring {
        let found = hit::at(doc, point + Vec2::new(dx, dy) * reach).into_iter().find(|h| doc.lock_over(h.node).is_none());
        let Some(found) = found else { continue };
        return match child_of(context, found.node) {
            Some(inside) => Some((inside, false)),
            None => child_of(root, found.node).map(|top| (top, true)),
        };
    }
    None
}

/// The children of `context` that `marquee` touches, back to front: a
/// shape by its box, a group by anything it holds (not by the empty
/// room between them). Nothing locked, and nothing by way of
/// something locked.
fn caught(doc: &Document, context: NodeId, boxes: &HashMap<NodeId, Rect>, marquee: Rect) -> Vec<NodeId> {
    fn touched(doc: &Document, id: NodeId, boxes: &HashMap<NodeId, Rect>, marquee: &Rect) -> bool {
        let Some((node, b)) = doc.get(id).zip(boxes.get(&id)) else { return false };
        let reaches = b.min.x <= marquee.max.x && b.max.x >= marquee.min.x && b.min.y <= marquee.max.y && b.max.y >= marquee.min.y;
        if !reaches || doc.is_locked(id) {
            return false;
        }
        !node.kind.is_group() || node.elements().any(|child| touched(doc, child, boxes, marquee))
    }
    let Some(holder) = doc.get(context) else { return Vec::new() };
    holder.elements().filter(|&id| doc.lock_over(id).is_none() && touched(doc, id, boxes, &marquee)).collect()
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
            Drag::Shaping { .. } => self.core.cancel(doc),
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

        let (mut ask, mut show, mut say) = (Vec::new(), None, None);
        let mut scene = Scene { handles: active, ..Scene::default() };
        let mut by = Affine::IDENTITY;
        let released = input.released || !input.held;
        let drag = match self.pointing.drag.take().map(|(_, drag)| drag) {
            None if active && input.pressed => {
                let on_box = zone.filter(|z| *z != Handle::Body);
                let found = if on_box.is_some() { None } else { pick(drawing, context, at, REACH * s / per_unit) };
                let press = |handle, pick| Press { at: pointer, handle, pick, shift: keys.shift };
                match found {
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
        };

        // The box: where the drag has it, or where the selection is.
        match &drag {
            Some(Drag::Shaping { was, nodes, .. }) => {
                scene.boxes = nodes.iter().map(|(_, b)| view.quad(*b, &by)).collect();
                scene.joint = Some(view.quad(*was, &by));
            }
            _ => {
                // After a pick this frame, the selection's as it is now.
                let tops: Vec<Rect> = sel.tops(drawing).into_iter().filter_map(|id| boxes.get(&id).copied()).collect();
                scene.joint = tops.iter().copied().reduce(|a, b| a.union(&b)).map(|j| view.quad(j, &Affine::IDENTITY));
                scene.boxes = tops.into_iter().map(|b| view.quad(b, &Affine::IDENTITY)).collect();
                if drag.is_none()
                    && let Some(zone) = zone
                {
                    ui.state.cursor_icon = cursor(zone);
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
        let Some(tab) = self.tabs.active() else { return };
        let doc = tab.doc;
        let Ok(drawing) = self.core.doc(doc) else { return };
        // What's drawn of it: a definition has nowhere to go.
        let nodes: Vec<NodeId> = tab.selection.tops(drawing).into_iter().filter(|&id| crate::select::is_drawn(drawing, id)).collect();
        if !nodes.is_empty() {
            self.edit(doc, &Command::Transform { nodes, by: Affine::translate(by.x, by.y) }, "Nudge");
        }
    }

    /// Escape: out of a drag; else out of the group the Pointer is in,
    /// one level, with that group picked; else nothing picked.
    pub(crate) fn escape(&mut self) {
        if self.pointing.busy() {
            return self.drop_drag();
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

#[cfg(test)]
mod tests {
    use ink_doc::DocId;

    use super::*;

    /// Back to front: a square, a group of (a dot over a locked bar),
    /// a ring with no fill.
    fn doc() -> Document {
        Document::parse(DocId(1), r##"<svg xmlns:ink="urn:lantern:ink" viewBox="0 0 48 48"><rect id="a" x="2" y="2" width="20" height="20"/><g id="g"><rect id="bar" x="10" y="26" width="30" height="6" ink:locked="true"/><circle id="dot" cx="12" cy="12" r="4"/></g><circle id="ring" cx="36" cy="12" r="6" fill="none" stroke="#000" stroke-width="0.5"/></svg>"##).unwrap()
    }

    const A: NodeId = NodeId(2);
    const G: NodeId = NodeId(3);
    const DOT: NodeId = NodeId(5);
    const RING: NodeId = NodeId(6);

    #[test]
    fn a_click_picks_the_top_thing_at_the_level_the_pointer_is_in() {
        let d = doc();
        let at = |x: f64, y: f64, context: NodeId| pick(&d, context, Vec2::new(x, y), 0.3);
        let root = d.root();
        // The dot is over the square: at the top level, its group.
        assert_eq!((at(12.0, 12.0, root), at(4.0, 4.0, root)), (Some((G, false)), Some((A, false))));
        // Inside the group, the dot itself; the square is outside it,
        // and picking it comes back out.
        assert_eq!((at(12.0, 12.0, G), at(4.0, 4.0, G)), (Some((DOT, false)), Some((A, true))));
        // What's locked isn't picked: the click goes through.
        assert_eq!(at(30.0, 29.0, root), None);
        // A thin ring: on its line, or within reach of it; not in its
        // empty middle.
        assert_eq!((at(42.0, 12.0, root), at(42.5, 12.0, root), at(36.0, 12.0, root)), (Some((RING, false)), Some((RING, false)), None));
        assert_eq!(at(46.0, 46.0, root), None);
    }

    #[test]
    fn a_marquee_catches_what_it_touches_at_that_level() {
        let d = doc();
        let boxes = ink_doc::geometry::page_bounds(&d);
        let over = |x0: f64, y0: f64, x1: f64, y1: f64, context: NodeId| caught(&d, context, &boxes, Rect::new(Vec2::new(x0, y0), Vec2::new(x1, y1)));
        // Across the whole page: everything at the top level, back to
        // front. (The group isn't locked; the bar in it is.)
        assert_eq!(over(0.0, 0.0, 48.0, 48.0, d.root()), [A, G, RING]);
        // A shape by its box (not its line); a group by what it holds
        // (not the room between them).
        assert_eq!(over(30.0, 5.0, 31.0, 8.0, d.root()), [RING]);
        assert_eq!(over(24.0, 19.0, 28.0, 24.0, d.root()), Vec::<NodeId>::new());
        // Only its locked bar touched: the group isn't caught by that.
        assert_eq!(over(30.0, 27.0, 34.0, 30.0, d.root()), Vec::<NodeId>::new());
        assert_eq!(over(44.0, 40.0, 48.0, 48.0, d.root()), Vec::<NodeId>::new());
        // Inside the group: the dot, and never the locked bar.
        assert_eq!(over(0.0, 0.0, 48.0, 48.0, G), [DOT]);
    }
}
