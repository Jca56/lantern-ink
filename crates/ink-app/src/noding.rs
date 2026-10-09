//! The Node tool on the canvas (ARCHITECTURE §8): the selected shapes
//! show their anchors, and a press takes one (`nodes.rs` has what's
//! under the pointer). A click on an anchor picks it (Shift adds or
//! lets go); a drag moves every anchor picked, onto whole units of the
//! drawing unless Ctrl frees it (Alva's choice, as shapes land). A
//! picked anchor shows its handles: one dragged takes the other round
//! while they're in line, and goes alone with Alt. A segment dragged
//! bends, the point taken following the pointer. A click on a shape
//! picks that shape, itself, whatever group it's in; a drag on nothing
//! is a marquee over anchors.
//!
//! Every drag is a gesture in the core (§4.3): the real drawing shows
//! as it goes, and it lands as one step. A shape that isn't a path yet
//! becomes one with the first change to its anchors (`anchors.rs`).
//!
//! A double click on a segment puts an anchor there; on an anchor, it
//! turns a corner smooth and back (LS3's pen's way). A right press
//! opens the menu of what's done to anchors (`nodeops.rs` does it).

use ink_core::{Actor, Command, DocId, NodeId};
use ink_doc::outline::AnchorId;
use ink_doc::pathedit::PathEdit;
use ink_doc::Kind;
use ink_geom::Affine;
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{CursorIcon, Ui};

use crate::anchors::{self, Picked, WouldBe};
use crate::canvas::CanvasInput;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::nodeops::NodeOp;
use crate::nodes::{self, Held, Hit};
use crate::overlay::Scene;
use crate::picking::top_at;
use crate::pointer::View;
use crate::select::Click;
use crate::shapes;

/// How far the pointer may stray from a press and still have clicked,
/// and how far from the pointer a click still finds a shape, logical
/// px. How far off its curves a path's line may be drawn, window px.
const SLOP: f64 = 4.0;
const REACH: f64 = 3.0;
const FINE: f64 = 0.25;

/// What the button went down on.
#[derive(Clone, Copy)]
struct Press {
    /// Where, window px.
    at: Vec2,
    what: Option<(NodeId, Hit)>,
    /// The anchor pressed was picked already: a click then narrows the
    /// pick to it, or with Shift lets it go.
    was: bool,
    shift: bool,
    /// Not on an anchor, but on a shape: a click lets go of anchors,
    /// and not of the shape.
    on_shape: bool,
}

enum Drag {
    /// Down, and not gone far: a click, unless it does.
    Pressed(Press),
    /// A marquee from here (window px), over the anchors picked before
    /// it (kept, with Shift).
    Marquee { from: Vec2, keep: Vec<Picked> },
    /// The anchors picked, moved: from where the drag began (the
    /// drawing's coordinates), led by the anchor pressed, which was at
    /// `lead`. Each shape's anchors, and how the drawing's coordinates
    /// are its own. `before`: the pick as it was called.
    Anchors { from: Vec2, lead: Vec2, moved: Vec<(NodeId, Vec<AnchorId>, Affine)>, first: Vec<NodeId>, before: Vec<Picked> },
    Handle { node: NodeId, held: Held, from: Vec2, to_own: Affine },
    /// A segment, taken at `share` along it, where the point `taken`
    /// (its own coordinates) was.
    Segment { node: NodeId, after: AnchorId, share: f64, taken: Vec2, from: Vec2, to_own: Affine, first: Vec<NodeId> },
}

/// What the Node tool keeps between frames.
#[derive(Default)]
pub struct Noding {
    /// The anchors picked, in the order picked, and whose drawing's
    /// they are.
    pub(crate) picked: Vec<Picked>,
    pub(crate) of: Option<DocId>,
    drag: Option<(DocId, Drag)>,
    /// The last refusal said during this drag: said once.
    said: Option<String>,
    pub(crate) would_be: WouldBe,
    /// A right press on the canvas this frame, with something of the
    /// tool's to offer: where its menu opens. And the segment it was
    /// on, for a new anchor there: which shape's, after which anchor,
    /// how far along.
    pub(crate) menu_at: Option<Vec2>,
    pub(crate) target: Option<(NodeId, AnchorId, f64)>,
}

impl Noding {
    /// The button is down on the canvas for the Node tool.
    pub fn busy(&self) -> bool {
        self.drag.is_some()
    }

    /// Let go of the anchors picked. Whether there were any.
    pub fn unpick(&mut self) -> bool {
        !std::mem::take(&mut self.picked).is_empty()
    }

    #[cfg(test)]
    pub(crate) fn picked(&self) -> &[Picked] {
        &self.picked
    }
}

/// What a frame of the tool asks of the core, once the drawing is no
/// longer being read.
enum Ask {
    Begin,
    Update(Command),
    Commit(Command, &'static str),
    Cancel,
}

impl Ink {
    /// Give up whatever the Node tool is in the middle of: a drag is as
    /// if it never began, a marquee leaves what was picked before it.
    pub(crate) fn drop_nodes(&mut self) {
        let Some((doc, drag)) = self.noding.drag.take() else { return };
        self.noding.said = None;
        match drag {
            Drag::Anchors { before, .. } => {
                self.core.cancel(doc);
                self.noding.picked = before;
            }
            Drag::Handle { .. } | Drag::Segment { .. } => self.core.cancel(doc),
            Drag::Marquee { keep, .. } => self.noding.picked = keep,
            Drag::Pressed(_) => {}
        }
    }

    /// One frame of the Node tool on `doc` (`active`: it's in hand):
    /// what it draws over the canvas goes into `scene`.
    pub(crate) fn node_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, active: bool, scene: &mut Scene) {
        // A drag on another tab's drawing, or with another tool in hand
        // by now: given up.
        if self.noding.drag.as_ref().is_some_and(|(on, _)| *on != doc || !active) {
            self.drop_nodes();
        }
        if !active {
            return;
        }
        if self.noding.of != Some(doc) {
            self.noding.picked.clear();
            self.noding.of = Some(doc);
        }
        let (s, pointer, mods) = (view.scale, ui.state.pointer, ui.state.mods);
        let at = view.to_doc.apply(pointer);
        let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);

        let Ok(drawing) = self.core.doc(doc) else { return };
        // As a drag under way has it.
        let Ok((looks, _)) = self.core.shown(doc) else { return };
        let stamp = self.core.history(doc).map_or(0, |h| h.stamp());
        let Some(tab) = self.tabs.iter_mut().find(|t| t.doc == doc) else { return };
        // The shapes whose anchors show: what's selected of them.
        let sel = &mut tab.selection;
        let editable = anchors::editable(drawing, &sel.nodes);
        let noding = &mut self.noding;
        let to_be: Vec<NodeId> = editable.iter().copied().filter(|id| drawing.get(*id).is_some_and(|n| n.kind != Kind::Path)).collect();
        noding.would_be.refresh(doc, stamp, drawing, &to_be, &mut noding.picked);
        let shown = anchors::shown(drawing, looks, &noding.would_be, &editable, &view.to_window);
        let find = |node: NodeId| shown.iter().find(|sh| sh.node == node);
        let dragging = matches!(noding.drag, Some((_, Drag::Anchors { .. } | Drag::Handle { .. } | Drag::Segment { .. })));
        let mut picked = std::mem::take(&mut noding.picked);
        // (Not mid-drag: for a frame the drawing may not have caught up
        // with what its anchors are called.)
        if !dragging {
            picked.retain(|(node, id)| find(*node).is_some_and(|sh| sh.outline.find(*id).is_some()));
        }
        let hover = (input.over && noding.drag.is_none()).then(|| nodes::under(&shown, &picked, pointer, s)).flatten();
        let grid = if mods.ctrl() {
            0.0
        } else {
            let page = ink_doc::arrange::page_box(drawing);
            shapes::grid_for(page.width().max(page.height()))
        };

        let (mut ask, mut show, mut say, mut op) = (Vec::new(), None, None, None);
        // A right press: an anchor not picked is picked first; then the
        // menu of what's done to anchors opens there (the selection's
        // own, where there's nothing of the tool's to offer).
        if input.over && ui.state.right_pressed && noding.drag.is_none() {
            noding.target = None;
            match hover {
                Some((node, Hit::Anchor(id))) if !picked.contains(&(node, id)) => picked = vec![(node, id)],
                Some((node, Hit::Segment { after, share })) => noding.target = Some((node, after, share)),
                _ => {}
            }
            if picked.is_empty() && noding.target.is_none() {
                self.pointing.menu_at = Some(pointer);
            } else {
                noding.menu_at = Some(pointer);
            }
        }
        let released = input.released || !input.held;
        let press = |what, was, on_shape| Drag::Pressed(Press { at: pointer, what, was, shift: mods.shift(), on_shape });
        let drag = match noding.drag.take().map(|(_, drag)| drag) {
            None if input.pressed => match hover {
                // A second press in a moment: a new anchor on a segment;
                // an anchor from a corner to smooth, or back.
                Some((node, Hit::Segment { after, share })) if input.double => {
                    op = Some(NodeOp::Add(node, after, share));
                    None
                }
                Some((node, Hit::Anchor(id))) if input.double => {
                    let has = find(node).is_some_and(|sh| sh.outline.handles(id) != (None, None));
                    picked = vec![(node, id)];
                    op = Some(if has { NodeOp::Corner } else { NodeOp::Smooth });
                    None
                }
                Some((node, Hit::Anchor(id))) => {
                    let was = picked.contains(&(node, id));
                    if !was {
                        if !mods.shift() {
                            picked.clear();
                        }
                        picked.push((node, id));
                    }
                    Some(press(hover, was, false))
                }
                Some(_) => Some(press(hover, false, false)),
                // Not on an anchor's or a segment's: the shape under the
                // pointer, itself, is what's picked.
                None => match top_at(drawing, at, REACH * s / per_unit) {
                    Some(shape) if !sel.is_selected(shape) => {
                        if mods.shift() {
                            sel.click(drawing, shape, Click::Toggle);
                        } else {
                            sel.select_only(shape);
                            picked.clear();
                        }
                        // The Pointer is at that shape's level, then.
                        sel.within = drawing.get(shape).and_then(|n| n.parent).filter(|&up| up != drawing.root() && drawing.get(up).is_some_and(|n| n.kind.is_group()));
                        show = Some(shape);
                        Some(press(None, false, true))
                    }
                    under => Some(press(None, false, under.is_some())),
                },
            },
            None => None,
            Some(Drag::Pressed(press)) if released => {
                // A click.
                match press.what {
                    Some((node, Hit::Anchor(id))) if press.was && press.shift => picked.retain(|one| *one != (node, id)),
                    Some((node, Hit::Anchor(id))) if press.was => picked = vec![(node, id)],
                    // A segment: its two ends, so their handles show.
                    Some((node, Hit::Segment { after, .. })) => {
                        let ends = find(node).and_then(|sh| {
                            let (r, i) = sh.outline.find(after)?;
                            let run = &sh.outline.runs[r];
                            Some([Some(after), run.next(i).map(|n| run.anchors[n].id)])
                        });
                        if !press.shift {
                            picked.clear();
                        }
                        for id in ends.into_iter().flatten().flatten() {
                            if !picked.contains(&(node, id)) {
                                picked.push((node, id));
                            }
                        }
                    }
                    Some(_) => {}
                    None if press.shift => {}
                    None if press.on_shape || !picked.is_empty() => picked.clear(),
                    None => {
                        sel.clear();
                        sel.within = None;
                    }
                }
                None
            }
            Some(Drag::Pressed(press)) if (pointer - press.at).length() > SLOP * s => {
                let from = view.to_doc.apply(press.at);
                match press.what {
                    Some((node, Hit::Anchor(lead))) => {
                        let nodes: Vec<NodeId> = picked.iter().map(|(node, _)| *node).collect();
                        let (first, names) = anchors::firsts(drawing, &noding.would_be, &nodes);
                        find(node).and_then(|sh| Some(sh.to_doc.apply(sh.outline.anchors().find(|a| a.id == lead)?.at))).map(|lead| {
                            let before = picked.clone();
                            anchors::renamed(&mut picked, &names);
                            let moved = anchors::grouped(&picked).into_iter().filter_map(|(node, ids)| Some((node, ids, find(node)?.to_doc.inverse()?))).collect();
                            ask.push(Ask::Begin);
                            Drag::Anchors { from, lead, moved, first, before }
                        })
                    }
                    Some((node, Hit::Handle { anchor, out })) => find(node).and_then(|sh| Some((Held::of(&sh.outline, anchor, out)?, sh.to_doc.inverse()?))).map(|(held, to_own)| {
                        ask.push(Ask::Begin);
                        Drag::Handle { node, held, from, to_own }
                    }),
                    Some((node, Hit::Segment { after, share })) => find(node)
                        .and_then(|sh| {
                            let (r, i) = sh.outline.find(after)?;
                            Some((sh.outline.runs[r].piece(i)?.at(share), sh.to_doc.inverse()?))
                        })
                        .map(|(taken, to_own)| {
                            let first = if to_be.contains(&node) { vec![node] } else { Vec::new() };
                            ask.push(Ask::Begin);
                            Drag::Segment { node, after, share, taken, from, to_own, first }
                        }),
                    None => Some(Drag::Marquee { from: press.at, keep: if press.shift { picked.clone() } else { Vec::new() } }),
                }
            }
            Some(pressed @ Drag::Pressed(_)) => Some(pressed),
            Some(Drag::Marquee { from, keep }) => {
                let marquee = Rect::new(from.min(pointer), from.max(pointer));
                picked = keep.clone();
                for sh in &shown {
                    for id in nodes::caught(&sh.outline, &sh.to_window, &marquee) {
                        if !picked.contains(&(sh.node, id)) {
                            picked.push((sh.node, id));
                        }
                    }
                }
                if released {
                    None
                } else {
                    scene.marquee = Some(marquee);
                    Some(Drag::Marquee { from, keep })
                }
            }
            Some(Drag::Anchors { from, lead, moved, first, before }) => {
                // The anchor pressed lands on the grid; with Shift, the
                // drag keeps to the way it has gone furthest.
                let mut by = shapes::on_grid(lead + (at - from), grid) - lead;
                if mods.shift() {
                    if by.x.abs() >= by.y.abs() { by.y = 0.0 } else { by.x = 0.0 }
                }
                let edits: Vec<(NodeId, Vec<PathEdit>)> = if by == Vec2::ZERO { Vec::new() } else { moved.iter().map(|(node, ids, to_own)| (*node, vec![PathEdit::Move { anchors: ids.clone(), by: to_own.linear(by) }])).collect() };
                let nowhere = edits.is_empty();
                let command = anchors::command(&first, edits);
                ui.state.cursor_icon = CursorIcon::Grabbing;
                if !released {
                    ask.push(Ask::Update(command));
                    Some(Drag::Anchors { from, lead, moved, first, before })
                } else if nowhere {
                    // Back where it began: nothing was done, and nothing
                    // is called otherwise.
                    ask.push(Ask::Cancel);
                    picked = before;
                    None
                } else {
                    ask.push(Ask::Commit(command, if moved.iter().map(|(_, ids, _)| ids.len()).sum::<usize>() == 1 { "Move Anchor" } else { "Move Anchors" }));
                    None
                }
            }
            Some(Drag::Handle { node, held, from, to_own }) => {
                let command = anchors::command(&[], vec![(node, vec![held.dragged(to_own.linear(at - from), mods.shift(), mods.alt())])]);
                ui.state.cursor_icon = CursorIcon::Grabbing;
                if released {
                    ask.push(Ask::Commit(command, "Handle"));
                    None
                } else {
                    ask.push(Ask::Update(command));
                    Some(Drag::Handle { node, held, from, to_own })
                }
            }
            Some(Drag::Segment { node, after, share, taken, from, to_own, first }) => {
                let by = to_own.linear(at - from);
                let edits = if by == Vec2::ZERO { Vec::new() } else { vec![(node, vec![PathEdit::Pull { after, share, to: taken + by }])] };
                let nowhere = edits.is_empty();
                let command = anchors::command(&first, edits);
                ui.state.cursor_icon = CursorIcon::Grabbing;
                if !released {
                    ask.push(Ask::Update(command));
                    Some(Drag::Segment { node, after, share, taken, from, to_own, first })
                } else {
                    ask.push(if nowhere { Ask::Cancel } else { Ask::Commit(command, "Bend") });
                    None
                }
            }
        };
        if drag.is_none() && hover.is_some() {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }

        // Over the canvas: each shape's line and anchors in place of the
        // selection's box, and the handles of the anchors picked.
        if !shown.is_empty() {
            scene.joint = None;
            scene.boxes.clear();
        }
        for sh in &shown {
            scene.paths.extend(nodes::lines(&sh.outline, &sh.to_window, FINE));
            for anchor in sh.outline.anchors() {
                let (on, place) = (picked.contains(&(sh.node, anchor.id)), sh.to_window.apply(anchor.at));
                if on {
                    let (into, out) = sh.outline.handles(anchor.id);
                    scene.levers.extend([into, out].into_iter().flatten().map(|h| (place, sh.to_window.apply(h))));
                }
                scene.anchors.push((place, on));
            }
        }
        if let Some(node) = show {
            sel.reveal(drawing, node);
        }
        noding.picked = picked;

        // The drawing is read; now what the drag asks of the core.
        for ask in ask {
            let done = match ask {
                Ask::Begin => self.core.begin(doc, Actor::Alva),
                Ask::Update(command) => self.core.update(doc, &command).map(|_| ()),
                Ask::Commit(command, label) => {
                    let shown = self.core.update(doc, &command).map(|_| ());
                    shown.and(self.core.commit(doc, label).map(|_| ()))
                }
                Ask::Cancel => {
                    self.core.cancel(doc);
                    Ok(())
                }
            };
            if let Err(e) = done {
                say = Some(e.to_string());
            }
        }
        // What the drawing won't have is said once a drag.
        match say {
            Some(why) if self.noding.said.as_ref() != Some(&why) => {
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
                self.noding.said = Some(why);
            }
            _ => {}
        }
        if drag.is_none() {
            self.noding.said = None;
        }
        if let Some(node) = show {
            self.tree.show(node);
        }
        self.noding.drag = drag.map(|drag| (doc, drag));
        if let Some(op) = op {
            self.node_op(op);
        }
    }
}
