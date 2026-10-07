//! The cut edges of outlines, and joining the kept ones up into the
//! loops of the result ([`super::combine`]).

use lntrn_math::Vec2;

use crate::meet::Tangled;
use crate::path::{Seg, Subpath};
use crate::piece::Piece;

/// A cut piece of an outline: the part of piece `source` from `t0` to
/// `t1`, between two corners.
#[derive(Clone, Copy, Debug)]
pub(super) struct Edge {
    pub piece: Piece,
    pub source: usize,
    pub t0: f64,
    pub t1: f64,
    pub from: usize,
    pub to: usize,
}

impl Edge {
    /// The same edge walked the other way.
    pub fn turned(&self) -> Edge {
        Edge { piece: self.piece.reversed(), source: self.source, t0: self.t1, t1: self.t0, from: self.to, to: self.from }
    }
}

/// The way an edge is heading where it starts (`end` false) or ends.
fn heading(edge: &Edge, end: bool) -> Vec2 {
    let (at, inward) = if end { (1.0, 0.999) } else { (0.0, 0.001) };
    let h = edge.piece.heading(at);
    // A handle of no length leaves it heading nowhere: the way it has
    // gone a little further in, then.
    if h.length_squared() > 0.0 { h } else if end { edge.piece.to() - edge.piece.at(inward) } else { edge.piece.at(inward) - edge.piece.from }
}

/// `edges`, each with what's filled on its left, joined into loops:
/// at each corner on to the edge that keeps the same filled place on
/// the left. A loop never passes one corner twice.
pub(super) fn loops(edges: &[Edge], corners: usize) -> Result<Vec<Vec<Edge>>, Tangled> {
    let mut leaving: Vec<Vec<usize>> = vec![Vec::new(); corners];
    let mut balance = vec![0i32; corners];
    for (i, e) in edges.iter().enumerate() {
        leaving[e.from].push(i);
        balance[e.from] += 1;
        balance[e.to] -= 1;
    }
    // An outline goes on from every corner it comes to.
    if balance.iter().any(|b| *b != 0) {
        return Err(Tangled("an outline comes to a corner and doesn't go on from it"));
    }
    let mut used = vec![false; edges.len()];
    let mut out = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut walk: Vec<usize> = Vec::new();
        let mut next = Some(start);
        while let Some(i) = next {
            used[i] = true;
            walk.push(i);
            let corner = edges[i].to;
            // Back at a corner the walk has left before: that much of
            // it is a loop.
            if let Some(since) = walk.iter().position(|w| edges[*w].from == corner) {
                out.push(walk.drain(since..).map(|w| edges[w]).collect());
            }
            let Some(&last) = walk.last() else { break };
            // The first edge clockwise from the way back.
            let back = (heading(&edges[last], true) * -1.0).angle();
            let turn = |e: &usize| {
                let turn = (back - heading(&edges[*e], false).angle()).rem_euclid(std::f64::consts::TAU);
                if turn == 0.0 { std::f64::consts::TAU } else { turn }
            };
            next = leaving[corner].iter().copied().filter(|e| !used[*e]).min_by(|a, b| turn(a).total_cmp(&turn(b)));
            if next.is_none() {
                return Err(Tangled("an outline has nowhere to go on to"));
            }
        }
    }
    Ok(out)
}

/// A loop's edges with the cuts that didn't end up mattering taken out
/// again: two parts of one piece that follow on are that part of it,
/// and straight lines in one line are one.
pub(super) fn tidy(mut edges: Vec<Edge>, sources: &[Piece]) -> Vec<Edge> {
    let joins = |a: &Edge, b: &Edge| -> Option<Edge> {
        if a.source == b.source && a.source < sources.len() && a.t1 == b.t0 && (a.t1 > a.t0) == (b.t1 > b.t0) && a.t0 != b.t1 {
            let whole = sources[a.source].part(a.t0.min(b.t1), a.t0.max(b.t1));
            let piece = if b.t1 > a.t0 { whole } else { whole.reversed() };
            return Some(Edge { piece, source: a.source, t0: a.t0, t1: b.t1, from: a.from, to: b.to });
        }
        let (da, db) = (a.piece.to() - a.piece.from, b.piece.to() - b.piece.from);
        let straight = matches!((a.piece.seg, b.piece.seg), (Seg::Line { .. }, Seg::Line { .. }));
        (straight && da.dot(db) > 0.0 && da.perp_dot(db).abs() <= 1e-12 * da.length() * db.length())
            .then(|| Edge { piece: Piece::new(a.piece.from, Seg::Line { to: b.piece.to() }), source: usize::MAX, t0: 0.0, t1: 1.0, from: a.from, to: b.to })
    };
    // Start at a real corner, so that nothing joins across the start.
    if let Some(corner) = (0..edges.len()).find(|i| joins(&edges[(i + edges.len() - 1) % edges.len()], &edges[*i]).is_none()) {
        edges.rotate_left(corner);
    }
    let mut out: Vec<Edge> = Vec::with_capacity(edges.len());
    for edge in edges {
        match out.last().and_then(|last| joins(last, &edge)) {
            Some(joined) => *out.last_mut().expect("there is a last") = joined,
            None => out.push(edge),
        }
    }
    out
}

/// A loop without what's too small to write: an edge shorter than
/// `fine` is left out (the next sets off from where the last one
/// ended, less than `fine` away), and a corner between two straight
/// lines that stands less than half of `fine` proud of the line
/// between its neighbours is that line. Nothing moves by more than a
/// file's numbers could say.
pub(super) fn neat(sub: &mut Subpath, fine: f64) {
    if fine.is_nan() || fine <= 0.0 {
        return;
    }
    let mut at = sub.start;
    sub.segs.retain(|seg| {
        let reach = Piece::new(at, *seg).bounds();
        let keep = reach.width().max(reach.height()) >= fine;
        if keep {
            at = seg.to();
        }
        keep
    });
    let mut segs: Vec<Seg> = Vec::with_capacity(sub.segs.len());
    for seg in sub.segs.drain(..) {
        if let (Some(&Seg::Line { to: corner }), Seg::Line { to }) = (segs.last(), seg) {
            let before = if segs.len() >= 2 { segs[segs.len() - 2].to() } else { sub.start };
            let (along, out) = (to - before, corner - before);
            if along.length() > 0.0 && out.perp_dot(along).abs() < 0.5 * fine * along.length() && out.dot(to - corner) > 0.0 {
                segs.pop();
            }
        }
        segs.push(seg);
    }
    sub.segs = segs;
}
