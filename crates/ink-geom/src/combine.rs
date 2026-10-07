//! Boolean operations on filled outlines: union, subtract, intersect,
//! exclude. The outlines are cut where they meet ([`meets`]), each cut
//! piece is kept or dropped by what is filled on either side of it
//! ([`winding`]), and the kept ones are joined up into the new outline.
//! Nothing is flattened: a piece of an arc is an arc of the same
//! ellipse, a piece of a curve the same curve (ARCHITECTURE §3.4).

use std::collections::HashMap;

use lntrn_math::Vec2;

use crate::FillRule;
use crate::meet::{SAME, Tangled, boxes_near, meets};
use crate::path::{Path, Seg, Subpath};
use crate::piece::Piece;
use crate::wind::{outline, winding};

/// How shapes are made one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combine {
    /// What any of them covers.
    Union,
    /// The first, less what any of the others covers.
    Subtract,
    /// What all of them cover.
    Intersect,
    /// What an odd number of them cover: two shapes, less where they
    /// overlap.
    Exclude,
}

impl Combine {
    /// Whether a place is in the result, given which shapes it's in.
    fn has(self, within: &[bool]) -> bool {
        match self {
            Combine::Union => within.iter().any(|w| *w),
            Combine::Intersect => within.iter().all(|w| *w),
            Combine::Subtract => within[0] && !within[1..].iter().any(|w| *w),
            Combine::Exclude => within.iter().filter(|w| **w).count() % 2 == 1,
        }
    }
}

/// How close counts as the same place, as a share of the shapes' size:
/// far below anything a file can say, far above what the arithmetic
/// loses.
const TOL: f64 = 1e-9;

/// A cut piece of an outline: the part of piece `source` from `t0` to
/// `t1`, between two corners.
#[derive(Clone, Copy, Debug)]
struct Edge {
    piece: Piece,
    source: usize,
    t0: f64,
    t1: f64,
    from: usize,
    to: usize,
}

impl Edge {
    /// The same edge walked the other way.
    fn turned(&self) -> Edge {
        Edge { piece: self.piece.reversed(), source: self.source, t0: self.t1, t1: self.t0, from: self.to, to: self.from }
    }
}

/// A shape's outline, ready to cut: a cubic that might cross itself is
/// two that can only cross each other.
fn prepared(path: &Path) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for piece in outline(path) {
        let loops = match piece.seg {
            // Its first handle crosses its last.
            Seg::Cubic { c1, c2, to } => {
                let (a, b) = (c1 - piece.from, to - c2);
                let (s, t) = ((c2 - piece.from).perp_dot(b), (c2 - piece.from).perp_dot(a));
                let det = a.perp_dot(b);
                det != 0.0 && (0.0..=1.0).contains(&(s / det)) && (0.0..=1.0).contains(&(t / det))
            }
            _ => false,
        };
        if loops {
            let (first, second) = piece.split(0.5);
            pieces.extend([first, second]);
        } else {
            pieces.push(piece);
        }
    }
    pieces
}

/// Which corner each of `points` is: points within `within` of each
/// other (or of one that is) are one corner. And where each corner is:
/// at one of its `exact` points if it has any.
fn weld(points: &[Vec2], exact: &[bool], within: f64) -> (Vec<usize>, Vec<Vec2>) {
    let mut group: Vec<usize> = (0..points.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by(|a, b| points[*a].x.total_cmp(&points[*b].x));
    for (n, &i) in order.iter().enumerate() {
        for &j in order[n + 1..].iter().take_while(|j| points[**j].x - points[i].x <= within) {
            if points[i].distance(points[j]) <= within {
                let (a, b) = (root(&mut group, i), root(&mut group, j));
                group[a.max(b)] = a.min(b);
            }
        }
    }
    let (mut corner, mut at, mut is_exact) = (vec![0; points.len()], Vec::new(), Vec::new());
    let mut named: HashMap<usize, usize> = HashMap::new();
    for i in 0..points.len() {
        let r = root(&mut group, i);
        let id = *named.entry(r).or_insert_with(|| {
            at.push(points[i]);
            is_exact.push(exact[i]);
            at.len() - 1
        });
        if exact[i] && !is_exact[id] {
            (at[id], is_exact[id]) = (points[i], true);
        }
        corner[i] = id;
    }
    (corner, at)
}

/// Whether two edges between the same corners are the same line.
fn alike(a: &Edge, b: &Edge, within: f64) -> bool {
    [0.25, 0.5, 0.75].into_iter().all(|share| {
        let p = a.piece.at(share);
        b.piece.at(b.piece.nearest(p)).distance(p) <= within
    })
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
fn loops(edges: &[Edge], corners: usize) -> Result<Vec<Vec<Edge>>, Tangled> {
    let mut leaving: Vec<Vec<usize>> = vec![Vec::new(); corners];
    let mut balance = vec![0i32; corners];
    for (i, e) in edges.iter().enumerate() {
        leaving[e.from].push(i);
        balance[e.from] += 1;
        balance[e.to] -= 1;
    }
    // An outline goes on from every corner it comes to.
    if balance.iter().any(|b| *b != 0) {
        return Err(Tangled);
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
                return Err(Tangled);
            }
        }
    }
    Ok(out)
}

/// A loop's edges with the cuts that didn't end up mattering taken out
/// again: two parts of one piece that follow on are that part of it,
/// and straight lines in one line are one.
fn tidy(mut edges: Vec<Edge>, sources: &[Piece]) -> Vec<Edge> {
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

/// The shapes made one: each a path and how it's filled. `fine` is the
/// narrowest thing worth keeping (what a file can't write is nothing):
/// a loop of the result thinner than that is left out.
///
/// The result's loops each have what's filled on one side all the way
/// round, and none crosses another, so it fills the same by either
/// rule. Its segments are the shapes' own, cut where they meet.
pub fn combine(shapes: &[(&Path, FillRule)], how: Combine, fine: f64) -> Result<Path, Tangled> {
    let outlines: Vec<Vec<Piece>> = shapes.iter().map(|(path, _)| prepared(path)).collect();
    let sources: Vec<Piece> = outlines.iter().flatten().copied().collect();
    let boxes: Vec<_> = sources.iter().map(Piece::bounds).collect();
    let Some(all) = boxes.iter().copied().reduce(|a, b| a.union(&b)) else { return Ok(Path::new()) };
    let size = all.width().max(all.height()).max(all.min.abs().max_element()).max(all.max.abs().max_element());
    if !(size > 0.0 && size.is_finite()) {
        return Ok(Path::new());
    }
    let tol = TOL * size;

    // Where each piece is cut: wherever it meets another.
    let mut cuts: Vec<Vec<f64>> = vec![Vec::new(); sources.len()];
    for i in 0..sources.len() {
        for j in i + 1..sources.len() {
            if boxes_near(&boxes[i], &boxes[j], tol) {
                for meet in meets(&sources[i], &sources[j], tol)? {
                    cuts[i].push(meet.t);
                    cuts[j].push(meet.u);
                }
            }
        }
    }
    // The cut pieces, and the corners they run between.
    let mut edges: Vec<Edge> = Vec::new();
    for (source, (piece, cuts)) in sources.iter().zip(&mut cuts).enumerate() {
        cuts.extend([0.0, 1.0]);
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|b, a| piece.at(*a).distance(piece.at(*b)) <= 4.0 * tol && *b != 1.0);
        if let [.., before, last] = cuts[..] && last == 1.0 && piece.at(before).distance(piece.to()) <= 4.0 * tol && before != 0.0 {
            cuts.remove(cuts.len() - 2);
        }
        for pair in cuts.windows(2) {
            edges.push(Edge { piece: piece.part(pair[0], pair[1]), source, t0: pair[0], t1: pair[1], from: 0, to: 0 });
        }
    }
    let ends: Vec<Vec2> = edges.iter().flat_map(|e| [e.piece.from, e.piece.to()]).collect();
    let exact: Vec<bool> = edges.iter().flat_map(|e| [e.t0 == 0.0, e.t1 == 1.0]).collect();
    let (corner, at) = weld(&ends, &exact, 8.0 * tol);
    for (i, edge) in edges.iter_mut().enumerate() {
        (edge.from, edge.to) = (corner[2 * i], corner[2 * i + 1]);
    }
    // An edge that comes to nothing is nothing.
    edges.retain(|e| {
        let b = e.piece.bounds();
        e.from != e.to || b.width().max(b.height()) > 16.0 * tol
    });
    // Edges that are the same line (two shapes' shared side) are one.
    // The same to what's left over when one line is worked out twice:
    // anything further apart than that is two lines, and is told apart.
    let floor = 4.0 * tol * SAME;
    let mut single: Vec<Edge> = Vec::new();
    let mut between: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for edge in edges {
        let key = (edge.from.min(edge.to), edge.from.max(edge.to));
        let known = between.entry(key).or_default();
        if !known.iter().any(|k| alike(&single[*k], &edge, floor)) {
            known.push(single.len());
            single.push(edge);
        }
    }

    // Kept: the edges with the result on one side and not the other,
    // each walked so that the result is on its left.
    let rules: Vec<FillRule> = shapes.iter().map(|(_, rule)| *rule).collect();
    let filled = |p: Vec2, fine: f64| {
        let within: Vec<bool> = outlines
            .iter()
            .zip(&rules)
            .map(|(pieces, rule)| {
                let turns = winding(pieces, p, fine);
                match rule {
                    FillRule::NonZero => turns != 0,
                    FillRule::EvenOdd => turns % 2 != 0,
                }
            })
            .collect();
        how.has(&within)
    };
    let reach: Vec<_> = single.iter().map(|e| e.piece.bounds()).collect();
    let mut kept: Vec<Edge> = Vec::new();
    for (i, edge) in single.iter().enumerate() {
        // A look to either side of it, from the place along it with
        // the most room: a quarter of the way to whatever else is
        // nearest there, so that nothing is stepped over, however thin
        // the gap.
        let look = [0.5, 0.25, 0.75, 0.375, 0.625, 0.125, 0.875]
            .into_iter()
            .filter_map(|share| {
                let (mid, way) = (edge.piece.at(share), edge.piece.heading(share));
                let near = |(j, other): (usize, &Edge)| (j != i && boxes_near(&reach[j], &lntrn_math::Rect::new(mid, mid), tol)).then(|| other.piece.at(other.piece.nearest(mid)).distance(mid));
                let room = single.iter().enumerate().filter_map(near).fold(tol, f64::min);
                (way.length_squared() > 0.0 && room > floor).then(|| (room, mid, way.perp() * (0.25 * room / way.length())))
            })
            .reduce(|best, next| if next.0 > best.0 { next } else { best })
            .map(|(room, mid, aside)| (mid, aside, room * 0.0025));
        let Some((mid, aside, fine)) = look else { return Err(Tangled) };
        match (filled(mid + aside, fine), filled(mid - aside, fine)) {
            (true, false) => kept.push(*edge),
            (false, true) => kept.push(edge.turned()),
            _ => {}
        }
    }

    let mut path = Path::new();
    for edges in loops(&kept, at.len())? {
        let edges = tidy(edges, &sources);
        let Some(first) = edges.first() else { continue };
        let mut sub = Subpath { start: at[first.from], segs: Vec::with_capacity(edges.len()), closed: true };
        for edge in &edges {
            let to = at[edge.to];
            sub.segs.push(match edge.piece.seg {
                Seg::Line { .. } => Seg::Line { to },
                Seg::Quad { c, .. } => Seg::Quad { c, to },
                Seg::Cubic { c1, c2, .. } => Seg::Cubic { c1, c2, to },
                Seg::Arc { arc, .. } => Seg::Arc { arc, to },
            });
        }
        // The line home is the one closing draws.
        if sub.segs.len() > 1 && matches!(sub.segs.last(), Some(Seg::Line { .. })) {
            sub.segs.pop();
        }
        // Thinner than can be written, it's a seam, not a shape.
        let one = Path { subpaths: vec![sub] };
        let round: f64 = one.flatten(size * 1e-4).iter().map(|line| (0..line.points.len()).map(|i| line.points[i].distance(line.points[(i + 1) % line.points.len()])).sum::<f64>()).sum();
        if one.area().abs() > fine.max(0.0) * round * 0.5 {
            path.subpaths.extend(one.subpaths);
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(d: &str) -> Path {
        Path::parse(d).path
    }

    /// `how` of the paths `ds`, as path data to one decimal.
    fn made(how: Combine, ds: &[&str]) -> String {
        let paths: Vec<Path> = ds.iter().map(|d| path(d)).collect();
        let shapes: Vec<(&Path, FillRule)> = paths.iter().map(|p| (p, FillRule::NonZero)).collect();
        combine(&shapes, how, 0.0).unwrap().to_data(1)
    }

    const SQUARE: &str = "M0 0 H10 V10 H0 Z";

    #[test]
    fn squares_that_overlap() {
        let other = "M5 5 H15 V15 H5 Z";
        assert_eq!(made(Combine::Union, &[SQUARE, other]), "M0 0 H10 V5 H15 V15 H5 V10 H0 Z");
        assert_eq!(made(Combine::Intersect, &[SQUARE, other]), "M10 5 V10 H5 V5 Z");
        assert_eq!(made(Combine::Subtract, &[SQUARE, other]), "M0 0 H10 V5 H5 V10 H0 Z");
        assert_eq!(made(Combine::Exclude, &[SQUARE, other]), "M0 0 H10 V5 H5 V10 H0 Z M10 10 V5 H15 V15 H5 V10 Z");
    }

    #[test]
    fn squares_side_by_side_share_a_side() {
        let next = "M10 0 H20 V10 H10 Z";
        assert_eq!(made(Combine::Union, &[SQUARE, next]), "M0 0 H20 V10 H0 Z", "the shared side is gone, and the top is one line");
        assert_eq!(made(Combine::Intersect, &[SQUARE, next]), "");
        assert_eq!(made(Combine::Subtract, &[SQUARE, next]), "M0 0 H10 V10 H0 Z");
        // The same square twice.
        assert_eq!(made(Combine::Union, &[SQUARE, SQUARE]), "M0 0 H10 V10 H0 Z");
        assert_eq!(made(Combine::Intersect, &[SQUARE, SQUARE]), "M0 0 H10 V10 H0 Z");
        assert_eq!(made(Combine::Subtract, &[SQUARE, SQUARE]), "");
        assert_eq!(made(Combine::Exclude, &[SQUARE, SQUARE]), "");
    }

    #[test]
    fn one_inside_another_and_apart() {
        let inner = "M3 3 H7 V7 H3 Z";
        assert_eq!(made(Combine::Union, &[SQUARE, inner]), "M0 0 H10 V10 H0 Z");
        assert_eq!(made(Combine::Intersect, &[SQUARE, inner]), "M3 3 H7 V7 H3 Z");
        assert_eq!(made(Combine::Subtract, &[SQUARE, inner]), "M0 0 H10 V10 H0 Z M7 3 H3 V7 H7 Z", "a hole, drawn the other way round");
        let apart = "M20 0 H30 V10 H20 Z";
        assert_eq!(made(Combine::Union, &[SQUARE, apart]), "M0 0 H10 V10 H0 Z M20 0 H30 V10 H20 Z");
        assert_eq!(made(Combine::Intersect, &[SQUARE, apart]), "");
        // Corner to corner: two squares still.
        assert_eq!(made(Combine::Union, &[SQUARE, "M10 10 H20 V20 H10 Z"]), "M0 0 H10 V10 H0 Z M10 10 H20 V20 H10 Z");
    }

    #[test]
    fn curves_stay_the_curves_they_were() {
        // A circle of radius 5 on a square's corner.
        let circle = "M15 10 A5 5 0 0 1 5 10 A5 5 0 0 1 15 10 Z";
        assert_eq!(made(Combine::Intersect, &[SQUARE, circle]), "M10 5 V10 H5 A5 5 0 0 1 10 5 Z", "a quarter of it, its arc an arc");
        assert_eq!(made(Combine::Subtract, &[SQUARE, circle]), "M0 0 H10 V5 A5 5 0 0 0 5 10 H0 Z");
        assert_eq!(made(Combine::Union, &[SQUARE, circle]), "M0 0 H10 V5 A5 5 0 0 1 15 10 A5 5 0 0 1 5 10 H0 Z");
        // A circle inside a square it touches on all four sides.
        let snug = "M10 5 A5 5 0 0 1 0 5 A5 5 0 0 1 10 5 Z";
        assert_eq!(made(Combine::Union, &[SQUARE, snug]), "M0 0 H10 V10 H0 Z");
        assert_eq!(made(Combine::Intersect, &[SQUARE, snug]), snug, "the circle as it was: cut where it touches, and joined again");
        // A leaf of two cubics, halved by a line.
        let leaf = "M0 0 C0 8 12 8 12 0 C12 -8 0 -8 0 0 Z";
        assert_eq!(made(Combine::Intersect, &[leaf, "M6 -10 H20 V10 H6 Z"]), "M12 0 C12 4 9 6 6 6 V-6 C9 -6 12 -4 12 0 Z", "half of each curve: curves still");
    }

    #[test]
    fn a_shape_is_what_its_rule_fills() {
        // A square with a square drawn the same way inside it: solid by
        // one rule, a ring by the other.
        let both = path("M0 0 H10 V10 H0 Z M3 3 H7 V7 H3 Z");
        let band = path("M-5 4 H15 V6 H-5 Z");
        let solid = combine(&[(&both, FillRule::NonZero), (&band, FillRule::NonZero)], Combine::Intersect, 0.0).unwrap();
        assert_eq!(solid.to_data(1), "M10 4 V6 H0 V4 Z");
        let ring = combine(&[(&both, FillRule::EvenOdd), (&band, FillRule::NonZero)], Combine::Intersect, 0.0).unwrap();
        assert_eq!(ring.to_data(1), "M10 4 V6 H7 V4 Z M0 6 V4 H3 V6 Z");
        // One shape alone: where it crosses itself, it's one outline.
        let bow = path("M0 0 L10 10 V0 L0 10 Z");
        assert_eq!(combine(&[(&bow, FillRule::NonZero)], Combine::Union, 0.0).unwrap().to_data(1), "M0 0 L5 5 L0 10 Z M10 10 L5 5 L10 0 Z");
        // Three at once.
        assert_eq!(made(Combine::Intersect, &[SQUARE, "M5 -5 H20 V20 H5 Z", "M-5 5 H20 V20 H-5 Z"]), "M10 5 V10 H5 V5 Z");
        assert_eq!(made(Combine::Subtract, &[SQUARE, "M5 -5 H20 V5 H5 Z", "M-5 5 H5 V20 H-5 Z"]), "M0 0 H5 V5 H0 Z M10 5 V10 H5 V5 Z");
    }

    #[test]
    fn what_is_too_thin_to_write_is_left_out() {
        let sliver = "M9.9999 0 H20 V10 H9.9999 Z";
        let (a, b) = (path(SQUARE), path(sliver));
        let all = combine(&[(&a, FillRule::NonZero), (&b, FillRule::NonZero)], Combine::Intersect, 0.0).unwrap();
        assert_eq!(all.to_data(4), "M9.9999 0 H10 V10 H9.9999 Z", "all of it, asked for all of it");
        let kept = combine(&[(&a, FillRule::NonZero), (&b, FillRule::NonZero)], Combine::Intersect, 0.0005).unwrap();
        assert!(kept.is_empty(), "{}", kept.to_data(4));
        // Nothing at all is nothing.
        assert!(combine(&[], Combine::Union, 0.0).unwrap().is_empty());
        assert!(combine(&[(&Path::new(), FillRule::NonZero), (&a, FillRule::NonZero)], Combine::Intersect, 0.0).unwrap().is_empty());
    }
}
