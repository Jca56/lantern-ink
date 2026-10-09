//! Editing a path by its anchors (ARCHITECTURE §3.4): moving them,
//! giving them handles, putting one in and taking one out, bending a
//! segment, closing and breaking and joining runs. Each edit is made on
//! an [`Outline`], which keeps every anchor's id through it; a segment
//! stays the kind it is unless the edit can't be said in that kind.

use std::collections::HashMap;

use ink_geom::{ArcTo, Vec2, circle_through};

use crate::error::{DocError, invalid};
use crate::outline::{Anchor, AnchorId, Link, Outline, Run};

/// Where on a segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Along {
    /// This share of the way along it, more than 0 and less than 1.
    Share(f64),
    /// The point of it nearest this one.
    Nearest(Vec2),
}

/// One edit to a path, naming its anchors.
#[derive(Clone, Debug, PartialEq)]
pub enum PathEdit {
    /// Move anchors by `by`. Their handles go with them.
    Move { anchors: Vec<AnchorId>, by: Vec2 },
    /// Move one anchor to `to`, its handles with it.
    MoveTo { anchor: AnchorId, to: Vec2 },
    /// Set an anchor's handles, as offsets from it: where the path
    /// comes into it (`into`) and where it goes out (`out`). `None`
    /// leaves one as it is; `Some(None)` takes it off.
    Handles { anchor: AnchorId, into: Option<Option<Vec2>>, out: Option<Option<Vec2>> },
    /// Put an anchor on the segment after `after`. The path keeps its
    /// shape.
    Add { after: AnchorId, at: Along },
    /// Take anchors out. What was joined through one is joined across.
    Delete { anchors: Vec<AnchorId> },
    /// Bend the segment after `after` so its middle passes through
    /// `through`.
    Bend { after: AnchorId, through: Vec2 },
    /// Take the point `share` of the way along the segment after `after`
    /// (more than 0, less than 1) to `to`: what dragging a segment by
    /// that point does. A line or a cubic becomes the cubic whose point
    /// there is `to`, its two control points moved as little as does
    /// it; a quadratic moves its one; an arc becomes the arc of a circle
    /// through it.
    Pull { after: AnchorId, share: f64, to: Vec2 },
    /// Make the segment after `after` a straight line.
    Straighten { after: AnchorId },
    /// Give anchors handles in line with each other, so the path runs
    /// smoothly through them.
    Smooth { anchors: Vec<AnchorId> },
    /// Take anchors' handles off: the path turns a corner at each.
    Corner { anchors: Vec<AnchorId> },
    /// Go on from the loose end `from` to a new anchor at `to`, which is
    /// that run's end from then on: with a line, or, where either has a
    /// handle on the new segment (`out`: the end's, `into`: the new
    /// anchor's, each as an offset from its anchor), a cubic. What a pen
    /// does.
    Extend { from: AnchorId, to: Vec2, out: Option<Vec2>, into: Option<Vec2> },
    /// Close the run `anchor` is in.
    Close { anchor: AnchorId },
    /// Part the path at `at`: a closed run opens there, an open one
    /// becomes two.
    Break { at: AnchorId },
    /// Join two ends of open runs with a line (or, where they lie on
    /// each other, into one anchor).
    Join { a: AnchorId, b: AnchorId },
    /// Turn round the run `anchor` is in, or with `None` every run.
    Reverse { anchor: Option<AnchorId> },
}

/// A handle shorter than this (against how long its segment is) is none.
const NO_HANDLE: f64 = 1e-9;

fn same(a: Vec2, b: Vec2) -> bool {
    a.distance(b) <= NO_HANDLE * a.abs().max(b.abs()).max_element().max(1.0)
}

/// The control point a link has for the anchor it leaves (`a`), as a
/// cubic would have it. `b` is where it goes.
fn leaving(link: &Link, a: Vec2) -> Vec2 {
    match *link {
        Link::Cubic { c1, .. } => c1,
        Link::Quad { c } => a + (c - a) * (2.0 / 3.0),
        Link::Line | Link::Arc { .. } => a,
    }
}

/// The control point a link has for the anchor it comes to (`b`).
fn arriving(link: &Link, b: Vec2) -> Vec2 {
    match *link {
        Link::Cubic { c2, .. } => c2,
        Link::Quad { c } => b + (c - b) * (2.0 / 3.0),
        Link::Line | Link::Arc { .. } => b,
    }
}

/// A cubic between `a` and `b`, or the line it is when its handles are
/// none.
pub(crate) fn cubic(a: Vec2, c1: Vec2, c2: Vec2, b: Vec2) -> Link {
    if same(c1, a) && same(c2, b) { Link::Line } else { Link::Cubic { c1, c2 } }
}

/// The arc of a circle from `a` through `through` to `b`.
fn arc_through(a: Vec2, through: Vec2, b: Vec2) -> Result<Link, DocError> {
    let Some((centre, radius)) = circle_through(a, through, b) else { return invalid("those three points are in a line, and no arc goes through them: straighten the segment instead") };
    let side = |p: Vec2| (b - a).perp_dot(p - a);
    Ok(Link::Arc { arc: ArcTo { rx: radius, ry: radius, rotation: 0.0, large: side(through) * side(centre) > 0.0, sweep: (through - a).perp_dot(b - through) > 0.0 } })
}

fn reversed(link: &Link) -> Link {
    match *link {
        Link::Cubic { c1, c2 } => Link::Cubic { c1: c2, c2: c1 },
        Link::Arc { arc } => Link::Arc { arc: ArcTo { sweep: !arc.sweep, ..arc } },
        other => other,
    }
}

impl Run {
    /// Turned round: the same line from its other end. A closed run
    /// still starts at its first anchor.
    fn turned(&self) -> Run {
        let mut anchors = self.anchors.clone();
        if self.closed && !anchors.is_empty() {
            anchors[1..].reverse();
        } else {
            anchors.reverse();
        }
        Run { anchors, links: self.links.iter().rev().map(reversed).collect(), closed: self.closed }
    }

    fn is_end(&self, i: usize) -> bool {
        !self.closed && (i == 0 || i + 1 == self.anchors.len())
    }

    /// Close it. A last anchor lying on the first becomes the first.
    fn close(&mut self) {
        if self.closed {
            return;
        }
        if self.anchors.len() >= 2 && same(self.anchors[self.anchors.len() - 1].at, self.anchors[0].at) {
            self.anchors.pop();
        } else {
            self.links.push(Link::Line);
        }
        self.closed = true;
    }
}

impl Outline {
    /// The handles of the anchor `id`, where it has them: the control
    /// point of what comes into it, and of what goes out of it, as a
    /// cubic has them (a quadratic's one control point stands for both
    /// its ends, two thirds of the way to it from each). None for a
    /// line or an arc on that side, for the end of an open run, and for
    /// a handle that lies on its anchor.
    pub fn handles(&self, id: AnchorId) -> (Option<Vec2>, Option<Vec2>) {
        let Some((r, i)) = self.find(id) else { return (None, None) };
        let run = &self.runs[r];
        let at = run.anchors[i].at;
        let real = |h: Vec2| (!same(h, at)).then_some(h);
        let into = run.prev(i).and_then(|p| real(arriving(&run.links[p], at)));
        let out = run.next(i).and_then(|_| real(leaving(&run.links[i], at)));
        (into, out)
    }

    fn place(&self, id: AnchorId) -> Result<(usize, usize), DocError> {
        self.find(id).ok_or_else(|| DocError::Invalid(format!("this path has no anchor {id} (node_info lists the ones it has)")))
    }

    /// Set what joins the anchor at `(r, i)` to the next to have `h`
    /// as its handle at that anchor (`None`: no handle).
    fn set_out(&mut self, r: usize, i: usize, h: Option<Vec2>) -> Result<(), DocError> {
        let run = &mut self.runs[r];
        let (a, id) = (run.anchors[i].at, run.anchors[i].id);
        let Some(b) = run.next(i).map(|n| run.anchors[n].at) else { return invalid(format!("{id} is the end of an open run: nothing goes out of it to have a handle")) };
        let link = run.links[i];
        if let Link::Arc { .. } = link {
            return if h.is_none() { Ok(()) } else { invalid(format!("what goes out of {id} is an arc, which has radii, not handles: bend it, or make it a line first")) };
        }
        run.links[i] = cubic(a, h.map_or(a, |h| a + h), arriving(&link, b), b);
        Ok(())
    }

    /// The same for the handle of the anchor at `(r, i)` on what comes
    /// into it.
    fn set_into(&mut self, r: usize, i: usize, h: Option<Vec2>) -> Result<(), DocError> {
        let run = &mut self.runs[r];
        let (b, id) = (run.anchors[i].at, run.anchors[i].id);
        let Some(p) = run.prev(i) else { return invalid(format!("{id} is the start of an open run: nothing comes into it to have a handle")) };
        let (a, link) = (run.anchors[p].at, run.links[p]);
        if let Link::Arc { .. } = link {
            return if h.is_none() { Ok(()) } else { invalid(format!("what comes into {id} is an arc, which has radii, not handles: bend it, or make it a line first")) };
        }
        run.links[p] = cubic(a, leaving(&link, a), h.map_or(b, |h| b + h), b);
        Ok(())
    }

    /// Make `edit`. `fresh` gives the ids of the anchors it makes, which
    /// are returned.
    pub fn edit(&mut self, edit: &PathEdit, fresh: &mut dyn FnMut() -> AnchorId) -> Result<Vec<AnchorId>, DocError> {
        let mut made = Vec::new();
        match edit {
            PathEdit::Move { anchors, by } => {
                let mut moved: HashMap<(usize, usize), Vec2> = HashMap::new();
                for &id in anchors {
                    moved.insert(self.place(id)?, *by);
                }
                for (r, run) in self.runs.iter_mut().enumerate() {
                    let shift = |i: usize| moved.get(&(r, i)).copied().unwrap_or(Vec2::ZERO);
                    for i in 0..run.links.len() {
                        let Some(n) = run.next(i) else { continue };
                        match &mut run.links[i] {
                            Link::Cubic { c1, c2 } => (*c1, *c2) = (*c1 + shift(i), *c2 + shift(n)),
                            // One control point between two anchors goes
                            // half as far as each.
                            Link::Quad { c } => *c += (shift(i) + shift(n)) * 0.5,
                            Link::Line | Link::Arc { .. } => {}
                        }
                    }
                    for (i, anchor) in run.anchors.iter_mut().enumerate() {
                        anchor.at += shift(i);
                    }
                }
            }
            PathEdit::MoveTo { anchor, to } => {
                let (r, i) = self.place(*anchor)?;
                let by = *to - self.runs[r].anchors[i].at;
                return self.edit(&PathEdit::Move { anchors: vec![*anchor], by }, fresh);
            }
            PathEdit::Handles { anchor, into, out } => {
                let (r, i) = self.place(*anchor)?;
                if let Some(h) = into {
                    self.set_into(r, i, *h)?;
                }
                if let Some(h) = out {
                    self.set_out(r, i, *h)?;
                }
            }
            PathEdit::Add { after, at } => {
                let (r, i) = self.place(*after)?;
                let Some(piece) = self.runs[r].piece(i) else { return invalid(format!("{after} is the last anchor of an open run: no segment comes after it to put one on")) };
                let t = match *at {
                    Along::Share(t) if t > 0.0 && t < 1.0 => t,
                    Along::Share(t) => return invalid(format!("an anchor goes between a segment's ends: {t} of the way along isn't (give more than 0 and less than 1)")),
                    Along::Nearest(p) => piece.nearest(p).clamp(0.02, 0.98),
                };
                let (first, second) = piece.split(t);
                let id = fresh();
                let run = &mut self.runs[r];
                run.links[i] = Link::of(&first.seg);
                run.links.insert(i + 1, Link::of(&second.seg));
                run.anchors.insert(i + 1, Anchor { id, at: first.to() });
                made.push(id);
            }
            PathEdit::Delete { anchors } => {
                for &id in anchors {
                    let (r, i) = self.place(id)?;
                    let run = &mut self.runs[r];
                    let n = run.anchors.len();
                    if n == 1 {
                        self.runs.remove(r);
                        continue;
                    }
                    match (run.prev(i), run.next(i)) {
                        // Between two others: they're joined across it.
                        (Some(p), Some(next)) if n > 2 || run.closed => {
                            let (a, c) = (run.anchors[p].at, run.anchors[next].at);
                            let across = match (run.links[p], run.links[i]) {
                                (Link::Line, Link::Line) => Link::Line,
                                (first, second) => cubic(a, leaving(&first, a), arriving(&second, c), c),
                            };
                            run.links[p] = across;
                            run.links.remove(i);
                        }
                        // An end of an open run goes with what joined it.
                        (None, _) => {
                            run.links.remove(0);
                        }
                        _ => {
                            run.links.pop();
                        }
                    }
                    run.anchors.remove(i);
                    if run.closed && run.anchors.len() == 1 {
                        run.links = vec![Link::Line];
                    }
                }
            }
            PathEdit::Bend { after, through } => {
                let (r, i) = self.place(*after)?;
                let Some(piece) = self.runs[r].piece(i) else { return invalid(format!("{after} is the last anchor of an open run: no segment comes after it to bend")) };
                let (a, b) = (piece.from, piece.to());
                self.runs[r].links[i] = match self.runs[r].links[i] {
                    // The one control point that puts its middle there.
                    Link::Line | Link::Quad { .. } => Link::Quad { c: *through * 2.0 - (a + b) * 0.5 },
                    // Both control points by as much as moves its middle there.
                    Link::Cubic { c1, c2 } => {
                        let pull = (*through - piece.at(0.5)) * (4.0 / 3.0);
                        Link::Cubic { c1: c1 + pull, c2: c2 + pull }
                    }
                    // The arc of a circle through all three.
                    Link::Arc { .. } => arc_through(a, *through, b)?,
                };
            }
            PathEdit::Pull { after, share, to } => {
                let (r, i) = self.place(*after)?;
                let Some(piece) = self.runs[r].piece(i) else { return invalid(format!("{after} is the last anchor of an open run: no segment comes after it to pull")) };
                let (t, u) = (*share, 1.0 - *share);
                if !(t > 0.0 && t < 1.0) {
                    return invalid(format!("a segment is pulled by a point between its ends: {t} of the way along isn't (give more than 0 and less than 1)"));
                }
                let (a, b, by) = (piece.from, piece.to(), *to - piece.at(t));
                let link = self.runs[r].links[i];
                self.runs[r].links[i] = match link {
                    // Its one control point, by as much as takes that
                    // point of it there.
                    Link::Quad { c } => Link::Quad { c: c + by * (1.0 / (2.0 * u * t)) },
                    Link::Arc { .. } => arc_through(a, *to, b)?,
                    // Each control point by its own share of that point,
                    // the two as little as does it.
                    Link::Line | Link::Cubic { .. } => {
                        // (A line is the cubic with no handles, which
                        // goes slowly from its ends: the same point of
                        // it is this far along by that cubic's count.)
                        let t = if link == Link::Line { 0.5 - ((1.0 - 2.0 * t).asin() / 3.0).sin() } else { t };
                        let u = 1.0 - t;
                        let (b1, b2) = (3.0 * u * u * t, 3.0 * u * t * t);
                        let pull = by * (1.0 / (b1 * b1 + b2 * b2));
                        cubic(a, leaving(&link, a) + pull * b1, arriving(&link, b) + pull * b2, b)
                    }
                };
            }
            PathEdit::Straighten { after } => {
                let (r, i) = self.place(*after)?;
                if self.runs[r].next(i).is_none() {
                    return invalid(format!("{after} is the last anchor of an open run: no segment comes after it"));
                }
                self.runs[r].links[i] = Link::Line;
            }
            PathEdit::Smooth { anchors } => {
                for &id in anchors {
                    let (r, i) = self.place(id)?;
                    let run = &self.runs[r];
                    let at = run.anchors[i].at;
                    let (before, after) = (run.prev(i).map(|p| run.anchors[p].at), run.next(i).map(|n| run.anchors[n].at));
                    // Along the line from the anchor before to the one
                    // after, a third of the way to each.
                    let along = match (before, after) {
                        (Some(b), Some(a)) => a - b,
                        (Some(b), None) => at - b,
                        (None, Some(a)) => a - at,
                        (None, None) => continue,
                    };
                    let Some(way) = (along.length() > 0.0).then(|| along * (1.0 / along.length())) else { continue };
                    let arc = |link: Option<&Link>| matches!(link, Some(Link::Arc { .. }));
                    if let Some(b) = before.filter(|_| !arc(run.prev(i).and_then(|p| run.links.get(p)))) {
                        self.set_into(r, i, Some(way * (-at.distance(b) / 3.0)))?;
                    }
                    if let Some(a) = after.filter(|_| !arc(self.runs[r].links.get(i))) {
                        self.set_out(r, i, Some(way * (at.distance(a) / 3.0)))?;
                    }
                }
            }
            PathEdit::Corner { anchors } => {
                for &id in anchors {
                    let (r, i) = self.place(id)?;
                    if self.runs[r].prev(i).is_some() {
                        self.set_into(r, i, None)?;
                    }
                    if self.runs[r].next(i).is_some() {
                        self.set_out(r, i, None)?;
                    }
                }
            }
            PathEdit::Extend { from, to, out, into } => {
                let (r, i) = self.place(*from)?;
                let run = &mut self.runs[r];
                if !run.is_end(i) {
                    return invalid(format!("{from} isn't an end of an open run: a path goes on from a loose end"));
                }
                let (end, id) = (run.anchors[i].at, fresh());
                let (end_handle, new_handle) = (out.map_or(end, |h| end + h), into.map_or(*to, |h| *to + h));
                // On from its last anchor; or, from its first, back
                // before it: the new anchor is where the run starts.
                if i + 1 == run.anchors.len() {
                    run.links.push(cubic(end, end_handle, new_handle, *to));
                    run.anchors.push(Anchor { id, at: *to });
                } else {
                    run.links.insert(0, cubic(*to, new_handle, end_handle, end));
                    run.anchors.insert(0, Anchor { id, at: *to });
                }
                made.push(id);
            }
            PathEdit::Close { anchor } => {
                let (r, _) = self.place(*anchor)?;
                self.runs[r].close();
            }
            PathEdit::Break { at } => {
                let (r, i) = self.place(*at)?;
                let run = &mut self.runs[r];
                let twin = Anchor { id: fresh(), at: run.anchors[i].at };
                if run.closed {
                    // It opens there: that anchor is where it starts,
                    // and its twin where it ends.
                    run.anchors.rotate_left(i);
                    run.links.rotate_left(i);
                    run.anchors.push(twin);
                    run.closed = false;
                } else if run.is_end(i) {
                    return invalid(format!("{at} is an end of its run already: there's nothing to part there"));
                } else {
                    let rest = Run { anchors: std::iter::once(twin).chain(run.anchors.drain(i + 1..)).collect(), links: run.links.drain(i..).collect(), closed: false };
                    self.runs.insert(r + 1, rest);
                }
                made.push(twin.id);
            }
            PathEdit::Join { a, b } => {
                let ((ra, ia), (rb, ib)) = (self.place(*a)?, self.place(*b)?);
                for (id, r, i) in [(a, ra, ia), (b, rb, ib)] {
                    if !self.runs[r].is_end(i) {
                        return invalid(format!("{id} isn't an end of an open run: only two loose ends can be joined"));
                    }
                }
                if ra == rb {
                    if ia == ib {
                        return invalid(format!("{a} can't be joined to itself"));
                    }
                    self.runs[ra].close();
                } else {
                    // The first run ending at `a`, the second starting at `b`.
                    let first = if ia == 0 { self.runs[ra].turned() } else { self.runs[ra].clone() };
                    let mut second = if ib == 0 { self.runs[rb].clone() } else { self.runs[rb].turned() };
                    let mut joined = first;
                    if same(joined.anchors[joined.anchors.len() - 1].at, second.anchors[0].at) {
                        second.anchors.remove(0);
                    } else {
                        joined.links.push(Link::Line);
                    }
                    joined.anchors.append(&mut second.anchors);
                    joined.links.append(&mut second.links);
                    self.runs[ra.min(rb)] = joined;
                    self.runs.remove(ra.max(rb));
                }
            }
            PathEdit::Reverse { anchor } => {
                let only = anchor.map(|id| self.place(id)).transpose()?.map(|(r, _)| r);
                for (r, run) in self.runs.iter_mut().enumerate() {
                    if only.is_none_or(|only| only == r) {
                        *run = run.turned();
                    }
                }
            }
        }
        Ok(made)
    }
}
