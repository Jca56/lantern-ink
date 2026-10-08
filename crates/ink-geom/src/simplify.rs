//! A path said with fewer segments: runs of them that one line, one arc
//! of a circle or one curve says as well, to within a tolerance, become
//! that one. Corners stay corners, and no anchor is ever moved or made:
//! what's left of a path's anchors are anchors it had.
//!
//! It is what turns a circle drawn as sixty-four short lines back into
//! four arcs, and a curve cut in pieces back into the curve.

use std::f64::consts::{PI, TAU};

use lntrn_math::Vec2;

use crate::path::{ArcTo, Path, Seg, Subpath};
use crate::piece::{Piece, circle_through};

/// A turn sharper than this (radians) between two segments is a corner,
/// and nothing is smoothed across it.
const CORNER: f64 = PI / 6.0;
/// The points looked at along each segment.
const LOOKS: usize = 8;

/// The points along `pieces` that anything standing in for them has to
/// pass by.
fn samples(pieces: &[Piece]) -> Vec<Vec2> {
    let mut points: Vec<Vec2> = pieces.iter().flat_map(|piece| (0..LOOKS).map(|i| piece.at(i as f64 / LOOKS as f64))).collect();
    points.extend(pieces.last().map(Piece::to));
    points
}

/// Whether `one` says what `pieces` say, to within `tol`: every point
/// along either is that near the other.
fn says(one: &Piece, pieces: &[Piece], along: &[Vec2], tol: f64) -> bool {
    let near_one = |p: &Vec2| one.at(one.nearest(*p)).distance(*p) <= tol;
    let near_them = |p: Vec2| pieces.iter().any(|piece| piece.at(piece.nearest(p)).distance(p) <= tol);
    along.iter().all(near_one) && (1..LOOKS * 2).all(|i| near_them(one.at(i as f64 / (LOOKS * 2) as f64)))
}

/// The arc of a circle from the first of `along` to the last that
/// passes through the middle one, if they aren't in a line.
fn arc(along: &[Vec2]) -> Option<Piece> {
    let (from, by, to) = (*along.first()?, along[along.len() / 2], *along.last()?);
    let (centre, radius) = circle_through(from, by, to)?;
    // Round from the start, by way of the middle, to the end.
    let angle = |p: Vec2| (p - centre).angle();
    let way = if (by - from).perp_dot(to - by) > 0.0 { 1.0 } else { -1.0 };
    let sweep = ((angle(to) - angle(from)) * way).rem_euclid(TAU);
    if !(sweep > 0.0 && sweep < TAU * 0.99) {
        return None;
    }
    Some(Piece::new(from, Seg::Arc { arc: ArcTo { rx: radius, ry: radius, rotation: 0.0, large: sweep > PI, sweep: way > 0.0 }, to }))
}

/// The curve from the first of `along` to the last that leaves heading
/// `out` and arrives heading `into`, with handles as long as bring it
/// nearest the points between (Schneider's fit, the least squares of
/// it).
fn curve(along: &[Vec2], out: Vec2, into: Vec2) -> Option<Piece> {
    let (from, to) = (*along.first()?, *along.last()?);
    let (out, into) = (out * (1.0 / out.length()), into * (-1.0 / into.length()));
    if !(out.is_finite() && into.is_finite()) {
        return None;
    }
    // How far along each point is, by the length of line so far.
    let mut gone = vec![0.0];
    for pair in along.windows(2) {
        gone.push(gone[gone.len() - 1] + pair[0].distance(pair[1]));
    }
    let whole = *gone.last()?;
    if whole <= 0.0 {
        return None;
    }
    let mut us: Vec<f64> = gone.iter().map(|g| g / whole).collect();
    let chord = from.distance(to);
    let mut fitted: Option<(Piece, f64)> = None;
    for _ in 0..40 {
        let (mut c00, mut c01, mut c11, mut x0, mut x1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (p, u) in along.iter().zip(&us) {
            let v = 1.0 - u;
            let (b0, b1, b2, b3) = (v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u);
            let (a0, a1) = (out * b1, into * b2);
            let rest = *p - from * (b0 + b1) - to * (b2 + b3);
            c00 += a0.dot(a0);
            c01 += a0.dot(a1);
            c11 += a1.dot(a1);
            x0 += a0.dot(rest);
            x1 += a1.dot(rest);
        }
        let det = c00 * c11 - c01 * c01;
        let (mut a, mut b) = if det.abs() > 1e-12 * c00 * c11 { ((x0 * c11 - x1 * c01) / det, (c00 * x1 - c01 * x0) / det) } else { (chord / 3.0, chord / 3.0) };
        // Handles that point the wrong way, or are no length at all,
        // are a fit gone astray: a third of the way each, then.
        if !(a > 1e-6 * chord && b > 1e-6 * chord) {
            (a, b) = (chord / 3.0, chord / 3.0);
        }
        let piece = Piece::new(from, Seg::Cubic { c1: from + out * a, c2: to + into * b, to });
        // Each point's place along the curve, found again on the curve:
        // the next fit is to those. Until it's no nearer for it.
        us = along.iter().zip(&us).map(|(p, u)| piece.foot(*p, *u)).collect();
        (us[0], us[along.len() - 1]) = (0.0, 1.0);
        let off = along.iter().zip(&us).map(|(p, u)| piece.at(*u).distance(*p)).fold(0.0, f64::max);
        match fitted {
            Some((_, best)) if off >= best * 0.999 => break,
            _ => fitted = Some((piece, off)),
        }
    }
    // Handles that pass each other pinch the curve into a bend as
    // tight as the corner it was meant to smooth: no fit at all.
    fitted.map(|(piece, _)| piece).filter(|piece| !matches!(piece.seg, Seg::Cubic { c1, c2, to } if (c2 - c1).dot(to - piece.from) < 0.0))
}

/// The heading a join with no corner at it is passed through with:
/// half way between the one it's reached with and the one it's left
/// with. Where the two are one already (a curve cut in pieces), that
/// one; where they differ a little (short lines round a curve), what
/// the curve they were drawn round had there.
fn through(reached: Vec2, left: Vec2) -> Vec2 {
    let (reached, left) = (reached * (1.0 / reached.length()), left * (1.0 / left.length()));
    let between = reached + left;
    if between.is_finite() && between.length() > 1e-9 { between } else if left.is_finite() { left } else { reached }
}

/// `pieces` (which run on from each other, no corner between) said
/// with as few as will do, onto `out`; and which of their starts are
/// still starts, onto `kept` (`first` is the first one's number).
/// `ends` are the headings a curve standing in for them leaves and
/// arrives with: what's fitted either side of a join shares its
/// heading there, so the join stays as smooth as it was.
fn fewer(pieces: &[Piece], first: usize, ends: (Vec2, Vec2), tol: f64, out: &mut Vec<Seg>, kept: &mut Vec<usize>) {
    let Some(start) = pieces.first() else { return };
    let along = samples(pieces);
    let to = pieces[pieces.len() - 1].to();
    // A line, an arc or a curve, whichever is plainest and says it.
    let line = Piece::new(start.from, Seg::Line { to });
    let plain = |piece: &Piece| matches!(piece.seg, Seg::Line { .. }) || piece.round().is_some_and(|(_, rx, ry, _)| (rx - ry).abs() <= 1e-9 * rx.max(ry));
    let one = if pieces.len() == 1 && plain(start) {
        Some(*start)
    } else {
        [Some(line), arc(&along), curve(&along, ends.0, ends.1)].into_iter().flatten().find(|one| says(one, pieces, &along, tol))
    };
    match one {
        Some(one) => {
            kept.push(first);
            out.push(one.seg);
        }
        None if pieces.len() == 1 => {
            kept.push(first);
            out.push(start.seg);
        }
        // In two, at the join nearest the middle: each half again.
        None => {
            let half = pieces.len() / 2;
            let join = through(pieces[half - 1].heading(1.0), pieces[half].heading(0.0));
            fewer(&pieces[..half], first, (ends.0, join), tol, out, kept);
            fewer(&pieces[half..], first + half, (join, ends.1), tol, out, kept);
        }
    }
}

impl Path {
    /// The path with as few segments as say the same outline to within
    /// `tol`: each stays that near where it was, both ways. And, for
    /// each subpath, which of its anchors are left (an anchor is where
    /// a segment starts; an open subpath's last is where it ends).
    pub fn simplified(&self, tol: f64) -> (Path, Vec<Vec<usize>>) {
        let mut path = Path::new();
        let mut left = Vec::new();
        for sub in &self.subpaths {
            let mut pieces: Vec<Piece> = Vec::new();
            let mut at = sub.start;
            for seg in &sub.segs {
                pieces.push(Piece::new(at, *seg));
                at = seg.to();
            }
            if sub.closed && at != sub.start {
                pieces.push(Piece::new(at, Seg::Line { to: sub.start }));
            }
            if pieces.is_empty() || tol.is_nan() || tol <= 0.0 {
                // As it is: every anchor it has (a lone point has one).
                path.subpaths.push(sub.clone());
                left.push((0..if pieces.is_empty() { 1 } else { pieces.len() + usize::from(!sub.closed) }).collect());
                continue;
            }
            // Where it turns a corner: nothing is smoothed across one.
            let n = pieces.len();
            let turn = |i: usize| {
                let (a, b) = (pieces[(i + n - 1) % n].heading(1.0), pieces[i].heading(0.0));
                a.perp_dot(b).atan2(a.dot(b)).abs()
            };
            let mut corners: Vec<usize> = (0..n).filter(|i| (*i > 0 || sub.closed) && turn(*i) > CORNER).collect();
            // A closed run with no corner anywhere starts and ends at a
            // join like any other: it's passed through, not left.
            let round = sub.closed && corners.is_empty();
            if !sub.closed || corners.is_empty() {
                corners.insert(0, 0);
            }
            // From each corner to the next (round to the first, closed).
            let (mut segs, mut kept) = (Vec::new(), Vec::new());
            for (k, &from) in corners.iter().enumerate() {
                let to = if k + 1 < corners.len() { corners[k + 1] } else if sub.closed { corners[0] + n } else { n };
                let stretch: Vec<Piece> = (from..to).map(|i| pieces[i % n]).collect();
                let (leaves, arrives) = (stretch[0].heading(0.0), stretch[stretch.len() - 1].heading(1.0));
                let ends = if round { (through(arrives, leaves), through(arrives, leaves)) } else { (leaves, arrives) };
                let mut found = Vec::new();
                fewer(&stretch, from, ends, tol, &mut segs, &mut found);
                kept.extend(found.into_iter().map(|i| i % n));
            }
            // It starts where it started, if that anchor is left, and
            // at the first that is if not.
            if let Some(first) = kept.iter().position(|k| *k == 0) {
                kept.rotate_left(first);
                segs.rotate_left(first);
            }
            let start = pieces[kept[0]].from;
            if sub.closed && segs.len() > 1 && matches!(segs.last(), Some(Seg::Line { .. })) {
                segs.pop();
            }
            if !sub.closed {
                kept.push(n);
            }
            path.subpaths.push(Subpath { start, segs, closed: sub.closed });
            left.push(kept);
        }
        (path, left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple(d: &str, tol: f64) -> (String, Vec<Vec<usize>>) {
        let (path, kept) = Path::parse(d).path.simplified(tol);
        (path.to_data(3), kept)
    }

    #[test]
    fn lines_in_a_line_are_one_and_corners_stay() {
        assert_eq!(simple("M0 0 H3 H7 H10 V4 V10 H0 Z", 0.01), ("M0 0 H10 V10 H0 Z".to_owned(), vec![vec![0, 3, 5, 6]]));
        // A wobble under the tolerance is a line; one over it stays.
        assert_eq!(simple("M0 0 L5 0.004 L10 0", 0.01).0, "M0 0 H10");
        assert_eq!(simple("M0 0 L5 0.5 L10 0", 0.01).0, "M0 0 L5 0.5 L10 0");
        // An open path keeps both its ends.
        assert_eq!(simple("M0 0 H5 H10", 0.01), ("M0 0 H10".to_owned(), vec![vec![0, 2]]));
        // No tolerance, no change.
        assert_eq!(simple("M0 0 H5 H10", 0.0).0, "M0 0 H5 H10");
    }

    #[test]
    fn a_circle_in_short_lines_is_arcs_again() {
        // Sixty-four sides round a circle of radius 10.
        let points: Vec<Vec2> = (0..64).map(|i| Vec2::new(12.0, 12.0) + Vec2::from_angle(TAU * i as f64 / 64.0) * 10.0).collect();
        let (round, kept) = Path::polyline(&points, true).simplified(0.05);
        assert!(round.subpaths[0].segs.len() <= 4 && round.subpaths[0].segs.iter().all(|s| matches!(s, Seg::Arc { arc, .. } if (arc.rx - 10.0).abs() < 0.05)), "{}", round.to_data(3));
        assert!(kept[0].len() == round.subpaths[0].segs.len() && kept[0][0] == 0);
        // Twelve sides are a twelve-sided shape: its corners are corners.
        let dozen: Vec<Vec2> = (0..12).map(|i| Vec2::from_angle(TAU * i as f64 / 12.0) * 10.0).collect();
        assert_eq!(Path::polyline(&dozen, true).simplified(0.05).0.subpaths[0].segs.len(), 11);
    }

    #[test]
    fn what_is_fitted_either_side_of_a_join_meets_there_smoothly() {
        // A wave in short lines, more than one curve can say.
        let wave: Vec<Vec2> = (0..=25).map(|i| Vec2::new(95.0 + 4.6 * i as f64, 190.0 + 14.0 * (i as f64 / 25.0 * 1.5 * TAU).sin())).collect();
        let pieces = |path: &Path| {
            let mut at = path.subpaths[0].start;
            path.subpaths[0].segs.iter().map(|seg| Piece::new(std::mem::replace(&mut at, seg.to()), *seg)).collect::<Vec<Piece>>()
        };
        let (loose, _) = Path::polyline(&wave, false).simplified(1.0);
        let curves = pieces(&loose);
        assert!((2..=4).contains(&curves.len()) && curves.iter().all(|c| matches!(c.seg, Seg::Cubic { .. })), "{}", loose.to_data(3));
        for pair in curves.windows(2) {
            let (reached, left) = (pair[0].heading(1.0), pair[1].heading(0.0));
            assert!(reached.perp_dot(left).atan2(reached.dot(left)).abs() < 1e-6, "a kink at {:?}: {}", pair[1].from, loose.to_data(3));
        }
        // Held closer than its lines are long, some of it stays lines;
        // but no curve is pinched into a corner (handles past each other).
        let (tight, _) = Path::polyline(&wave, false).simplified(0.23);
        assert!(tight.subpaths[0].segs.len() < 25, "{}", tight.to_data(3));
        for piece in pieces(&tight) {
            assert!(!matches!(piece.seg, Seg::Cubic { c1, c2, to } if (c2 - c1).dot(to - piece.from) < 0.0), "pinched: {}", tight.to_data(3));
        }
        // A ring of short lines has no corner to start from: it's
        // smooth all the way round, where it starts too.
        let ring: Vec<Vec2> = (0..48).map(|i| Vec2::new(30.0 * (TAU * i as f64 / 48.0).cos(), 18.0 * (TAU * i as f64 / 48.0).sin())).collect();
        let (oval, _) = Path::polyline(&ring, true).simplified(0.3);
        let round = pieces(&oval);
        assert!(round.len() <= 8, "{}", oval.to_data(3));
        let (reached, left) = (round[round.len() - 1].heading(1.0), round[0].heading(0.0));
        assert!(reached.perp_dot(left).atan2(reached.dot(left)).abs() < 0.05, "{}", oval.to_data(3));
    }

    #[test]
    fn a_curve_cut_in_pieces_is_the_curve_again() {
        let whole = Piece::new(Vec2::new(0.0, 0.0), Seg::Cubic { c1: Vec2::new(0.0, 10.0), c2: Vec2::new(20.0, 10.0), to: Vec2::new(20.0, -4.0) });
        let mut cut = Path::new();
        cut.move_to(whole.from);
        for (a, b) in [(0.0, 0.2), (0.2, 0.5), (0.5, 0.6), (0.6, 1.0)] {
            cut.push(whole.part(a, b).seg);
        }
        let (again, kept) = cut.simplified(0.001);
        assert_eq!((again.subpaths[0].segs.len(), &kept[0]), (1, &vec![0, 4]));
        let fitted = Piece::new(again.subpaths[0].start, again.subpaths[0].segs[0]);
        for i in 0..=20 {
            let p = whole.at(i as f64 / 20.0);
            assert!(fitted.at(fitted.nearest(p)).distance(p) < 0.001, "{p:?}");
        }
        // Four cubics that draw a circle are four arcs.
        let k = 0.552_284_749_831 * 5.0;
        let circle = format!("M5 0 C5 {k} {k} 5 0 5 C-{k} 5 -5 {k} -5 0 C-5 -{k} -{k} -5 0 -5 C{k} -5 5 -{k} 5 0 Z");
        let (arcs, _) = Path::parse(&circle).path.simplified(0.01);
        assert!(arcs.subpaths[0].segs.iter().all(|s| matches!(s, Seg::Arc { .. })) && arcs.subpaths[0].segs.len() <= 4, "{}", arcs.to_data(3));
        // And a line, an arc and a curve that are already one each stay.
        assert_eq!(simple("M0 0 L10 0 A5 5 0 0 1 10 10 C8 12 4 12 2 10", 0.01).0, "M0 0 H10 A5 5 0 0 1 10 10 C8 12 4 12 2 10");
    }
}
