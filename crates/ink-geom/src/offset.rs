//! A stroke's outline as a shape of its own: what "outline stroke"
//! makes of a line. Each piece of the path gives a ribbon (the line
//! half the width to its left, the same to its right, joined across
//! its ends), each corner a join and each open end a cap, and the lot
//! is made one outline ([`combine`]).
//!
//! As far as a stroke's edge has an exact shape it keeps it: beside a
//! straight line it is a straight line, beside a circle's arc an arc,
//! round a round join or cap an arc. Beside any other curve it is a
//! curve of its own kind that no path can write, so cubics are fitted
//! to it, to within what the caller says is fine.

use lntrn_math::Vec2;

use crate::FillRule;
use crate::combine::{Combine, combine_giving};
use crate::dash::dashed_pieces;
use crate::meet::Tangled;
use crate::path::{ArcTo, Path, Seg, Subpath};
use crate::piece::Piece;
use crate::stroke::{Cap, Join, Stroke};

/// How many times a curve is halved to fit its offset, at most.
const MAX_DEPTH: u32 = 10;
/// The most pieces (ribbons, joins, caps) one outline is made of.
const MAX_PIECES: usize = 20_000;

/// The way `piece` heads at `t`, one unit long: from a little way in
/// where a handle of no length leaves it heading nowhere.
fn heading(piece: &Piece, t: f64) -> Vec2 {
    let inward = if t < 0.5 { t + 1e-4 } else { t - 1e-4 };
    let (h, near) = (piece.heading(t), piece.heading(inward));
    let h = if h.length_squared() > 0.0 { h } else if near.length_squared() > 0.0 { near } else { piece.to() - piece.from };
    if h.length_squared() > 0.0 { h * (1.0 / h.length()) } else { Vec2::X }
}

/// The point `d` to the left of `piece` at `t`.
fn beside(piece: &Piece, t: f64, d: f64) -> Vec2 {
    piece.at(t) + heading(piece, t).perp() * d
}

/// How fast the line `d` to the left of `piece` goes at `t`, for each
/// unit `piece` itself does: less on the inside of a bend, nothing
/// where the bend is as tight as `d`, backwards where it's tighter.
fn pace(piece: &Piece, t: f64, d: f64) -> f64 {
    const H: f64 = 1e-5;
    let (a, b) = ((t - H).max(0.0), (t + H).min(1.0));
    let (way, turning) = (piece.heading(t), (piece.heading(b) - piece.heading(a)) * (1.0 / (b - a)));
    let speed = way.length();
    if speed > 0.0 { 1.0 - d * way.perp_dot(turning) / (speed * speed * speed) } else { 1.0 }
}

/// Cubics for the line `d` to the left of the curve `piece`, onto
/// `out`: halved until each is within `tol` of it.
fn fit(piece: &Piece, d: f64, tol: f64, depth: u32, out: &mut Vec<Seg>) {
    let (from, to) = (beside(piece, 0.0, d), beside(piece, 1.0, d));
    // A curve's handles are a third of how fast it sets out and
    // arrives; the offset does both at its own pace.
    let c1 = from + piece.heading(0.0) * (pace(piece, 0.0, d) / 3.0);
    let c2 = to - piece.heading(1.0) * (pace(piece, 1.0, d) / 3.0);
    let fitted = Piece::new(from, Seg::Cubic { c1, c2, to });
    let near = |t: f64| {
        let p = beside(piece, t, d);
        fitted.at(fitted.nearest(p)).distance(p) <= tol
    };
    if depth >= MAX_DEPTH || [0.2, 0.4, 0.5, 0.6, 0.8].into_iter().all(near) {
        out.push(fitted.seg);
    } else {
        let (first, second) = piece.split(0.5);
        fit(&first, d, tol, depth + 1, out);
        fit(&second, d, tol, depth + 1, out);
    }
}

/// The line `d` to the left of `piece`, onto `out`, from
/// `beside(piece, 0.0, d)`.
fn offset(piece: &Piece, d: f64, tol: f64, out: &mut Vec<Seg>) {
    match (piece.seg, piece.round()) {
        (Seg::Line { .. }, _) => out.push(Seg::Line { to: beside(piece, 1.0, d) }),
        // Beside a circle's arc is an arc about the same middle: a
        // smaller one inside the bend, and one on the far side of the
        // middle where the bend is tighter than `d`.
        (Seg::Arc { arc, to }, Some((centre, rx, ry, sweep))) if (rx - ry).abs() <= 1e-9 * rx.max(ry) => {
            let radius = rx - d * sweep.signum();
            let to = centre + (to - centre) * (radius / rx);
            out.push(if radius.abs() > 1e-12 * rx { Seg::Arc { arc: ArcTo { rx: radius.abs(), ry: radius.abs(), rotation: 0.0, ..arc }, to } } else { Seg::Line { to } });
        }
        _ => fit(piece, d, tol, 0, out),
    }
}

fn closed(start: Vec2, segs: Vec<Seg>) -> Path {
    Path { subpaths: vec![Subpath { start, segs, closed: true }] }
}

/// The ways a stretch of line is squared off across its two ends:
/// which way it's taken to head at each. Its own way, but for where
/// it runs on into the next piece so nearly straight that they share
/// one: then both are squared off along the very same line, and not
/// along two a hair apart.
#[derive(Clone, Copy)]
struct Ends {
    start: Vec2,
    end: Vec2,
}

impl Ends {
    fn of(piece: &Piece) -> Ends {
        Ends { start: heading(piece, 0.0), end: heading(piece, 1.0) }
    }
}

/// What a stroke of half-width `h` covers along `piece`: between the
/// lines either side of it, squared off across its ends as `ends` says.
fn ribbon(piece: &Piece, h: f64, tol: f64, ends: Ends) -> Path {
    let back = piece.reversed();
    let (start, end) = (ends.start.perp() * h, ends.end.perp() * h);
    let mut segs = Vec::new();
    offset(piece, h, tol, &mut segs);
    arrive(&mut segs, piece.to() + end);
    segs.push(Seg::Line { to: piece.to() - end });
    offset(&back, h, tol, &mut segs);
    arrive(&mut segs, piece.from - start);
    closed(piece.from + start, segs)
}

/// Have the last of `segs` end at `to` (it ends a hair from it).
fn arrive(segs: &mut [Seg], to: Vec2) {
    if let Some(Seg::Line { to: end } | Seg::Quad { to: end, .. } | Seg::Cubic { to: end, .. } | Seg::Arc { to: end, .. }) = segs.last_mut() {
        *end = to;
    }
}

/// Whether a stroke of half-width `h` folds over itself somewhere
/// along `piece`: the bend is as tight as the stroke is wide, so the
/// stroke's inner edge runs backwards. A circle's arc doesn't count:
/// it folds everywhere alike, and its ribbon is right as it stands.
fn folds(piece: &Piece, h: f64) -> bool {
    match (piece.seg, piece.round()) {
        (Seg::Line { .. }, _) => false,
        (Seg::Arc { .. }, Some((_, rx, ry, _))) if (rx - ry).abs() <= 1e-9 * rx.max(ry) => false,
        (Seg::Arc { .. }, None) => false,
        _ => (0..=16).any(|i| {
            let t = i as f64 / 16.0;
            pace(piece, t, h).min(pace(piece, t, -h)) < 0.05
        }),
    }
}

/// What a stroke of half-width `h` covers along `piece`, as shapes
/// onto `out`.
///
/// A ribbon, where the stroke doesn't fold over itself. Where it does,
/// no one ribbon's outline says what's covered (places reached from
/// both sides of the fold count for nothing), so that stretch is cut
/// into slices short enough that each is four straight sides, from one
/// of the stroke's edges across to the other: together they cover what
/// the stroke does, to within `tol`.
fn along(piece: &Piece, h: f64, tol: f64, ends: Ends, depth: u32, out: &mut Vec<Path>) {
    if !folds(piece, h) {
        out.push(ribbon(piece, h, tol, ends));
        return;
    }
    let (start, end) = (ends.start.perp(), ends.end.perp());
    // Straight enough: each edge's middle is where a straight side's is.
    let straight = [h, -h].into_iter().all(|d| ((piece.from + start * d + piece.to() + end * d) * 0.5).distance(beside(piece, 0.5, d)) <= tol);
    if straight || depth >= 12 {
        out.push(closed(piece.from + start * h, vec![Seg::Line { to: piece.to() + end * h }, Seg::Line { to: piece.to() - end * h }, Seg::Line { to: piece.from - start * h }]));
        return;
    }
    // The two halves are squared off along one line where they meet.
    let (first, second) = piece.split(0.5);
    let middle = heading(&first, 1.0);
    along(&first, h, tol, Ends { end: middle, ..ends }, depth + 1, out);
    along(&second, h, tol, Ends { start: middle, ..ends }, depth + 1, out);
}

/// Shapes made one, all at once. `give` is how short an edge may be
/// that can't be told from its neighbour, and so is taken for a point.
fn all_at_once(shapes: &[Path], fine: f64, give: f64) -> Result<Path, Tangled> {
    let all: Vec<(&Path, FillRule)> = shapes.iter().map(|shape| (shape, FillRule::NonZero)).collect();
    combine_giving(&all, Combine::Union, fine, give)
}

/// A row of slices made one, a few at a time: neighbours first, then
/// what those made, so that a long row of slices that all overlap
/// (round a fold they do) is never cut against itself all at once.
fn unite(slices: &[Path], give: f64) -> Result<Path, Tangled> {
    if slices.len() <= 6 {
        return all_at_once(slices, 0.0, give);
    }
    let (first, second) = slices.split_at(slices.len() / 2);
    all_at_once(&[unite(first, give)?, unite(second, give)?], 0.0, give)
}

/// An arc of radius `h` about `centre` from `from` to `to`, less than
/// half way round, the way `turn` says (positive: x towards y).
fn round(h: f64, to: Vec2, turn: f64) -> Seg {
    Seg::Arc { arc: ArcTo { rx: h, ry: h, rotation: 0.0, large: false, sweep: turn > 0.0 }, to }
}

/// What fills the corner at `at` where a stroke heading `a` turns to
/// head `b`: nothing where it doesn't turn, or by so little that the
/// gap it leaves is under `tol`.
fn join(at: Vec2, a: Vec2, b: Vec2, h: f64, st: &Stroke, tol: f64) -> Option<Path> {
    let (turn, same) = (a.perp_dot(b), a.dot(b));
    if turn.abs() * h <= tol.max(1e-9 * h) && same > 0.0 {
        return None;
    }
    // Straight back on itself has no outside: round, it's a half disc
    // about the end, and nothing otherwise.
    if turn.abs() <= 1e-9 {
        let across = a.perp() * h;
        return (st.join == Join::Round).then(|| closed(at + across, vec![round(h, at + a * h, -1.0), round(h, at - across, -1.0)]));
    }
    // The outside of the bend is on the right of a left turn.
    let out = if turn > 0.0 { -h } else { h };
    let (from, to) = (at + a.perp() * out, at + b.perp() * out);
    let corner = match st.join {
        Join::Round => vec![Seg::Line { to: from }, round(h, to, turn)],
        Join::Bevel => vec![Seg::Line { to: from }, Seg::Line { to }],
        Join::Miter => {
            // As long as the stroke is wide, over the sine of half
            // the angle between the two lines.
            let level = (0.5 * (1.0 + same)).max(0.0).sqrt();
            if level > 0.0 && 1.0 / level <= st.miter_limit {
                vec![Seg::Line { to: from }, Seg::Line { to: at + (a.perp() + b.perp()) * (out / (1.0 + same)) }, Seg::Line { to }]
            } else {
                vec![Seg::Line { to: from }, Seg::Line { to }]
            }
        }
    };
    Some(closed(at, corner))
}

/// What caps an open end at `at`, the line heading `way` as it leaves.
fn cap(at: Vec2, way: Vec2, h: f64, st: &Stroke) -> Option<Path> {
    let across = way.perp() * h;
    match st.cap {
        Cap::Butt => None,
        Cap::Square => Some(closed(at + across, vec![Seg::Line { to: at + across + way * h }, Seg::Line { to: at - across + way * h }, Seg::Line { to: at - across }])),
        Cap::Round => Some(closed(at + across, vec![round(h, at + way * h, -1.0), round(h, at - across, -1.0)])),
    }
}

/// A line of no length under a cap: a dot, or a square.
fn dot(at: Vec2, h: f64, st: &Stroke) -> Option<Path> {
    match st.cap {
        Cap::Butt => None,
        Cap::Square => Some(Path::rect(at.x - h, at.y - h, 2.0 * h, 2.0 * h, 0.0, 0.0)),
        Cap::Round => Some(Path::ellipse(at, h, h)),
    }
}

/// The outline of `path` stroked with `st`: a path that fills (by
/// either rule) what the stroke covers, dashes and all. Its edges are
/// lines and arcs where a stroke's are, and cubics within `near` of
/// them elsewhere (the nearer, the more of them); what's thinner than
/// `fine` is left out.
pub fn outline_stroke(path: &Path, st: &Stroke, near: f64, fine: f64) -> Result<Path, Tangled> {
    let h = st.width * 0.5;
    if !(h > 0.0 && h.is_finite()) {
        return Ok(Path::new());
    }
    let tol = if near > 0.0 { near } else { h * 1e-4 };
    // What each unbroken stretch of line (a subpath, or one dash of
    // it) covers, worked out on its own first: its pieces, joins and
    // caps share sides and corners with each other by the way they're
    // made, and with nothing of any other stretch.
    let mut stretches: Vec<Path> = Vec::new();
    let mut made = 0;
    for sub in &path.subpaths {
        // The subpath's pieces that go somewhere, and its closing line.
        let mut pieces: Vec<Piece> = Vec::new();
        let mut at = sub.start;
        for seg in &sub.segs {
            let piece = Piece::new(at, *seg);
            at = seg.to();
            if piece.from != at || !matches!(piece.seg, Seg::Line { .. } | Seg::Arc { .. }) {
                pieces.push(piece);
            }
        }
        if sub.closed && at != sub.start {
            pieces.push(Piece::new(at, Seg::Line { to: sub.start }));
        }
        if pieces.is_empty() {
            // A point that was drawn to, or closed: a dot.
            if sub.closed || !sub.segs.is_empty() {
                stretches.extend(dot(sub.start, h, st));
            }
            continue;
        }
        let whole = vec![(pieces.clone(), sub.closed)];
        for (run, closed) in dashed_pieces(&pieces, sub.closed, st).unwrap_or(whole) {
            // What's too short to see has no direction worth joining to.
            let seen = |p: &Piece| p.length() > h * 1e-7;
            let run: Vec<Piece> = if !run.iter().any(seen) {
                stretches.extend(run.first().and_then(|p| dot(p.from, h, st)));
                continue;
            } else {
                run.into_iter().filter(seen).collect()
            };
            // Where one piece runs on into the next (or an open line's
            // end into its start) too nearly straight for a join to
            // show, the two are squared off along one line.
            let mut ends: Vec<Ends> = run.iter().map(Ends::of).collect();
            let meets_itself = run[run.len() - 1].to().distance(run[0].from) <= tol;
            for i in 0..run.len() {
                let next = (i + 1) % run.len();
                let (a, b) = (ends[i].end, ends[next].start);
                if (i + 1 < run.len() || closed || meets_itself) && a.perp_dot(b).abs() * h <= tol.max(1e-9 * h) && a.dot(b) > 0.0 {
                    let one = (a + b) * (1.0 / (a + b).length());
                    (ends[i].end, ends[next].start) = (one, one);
                }
            }
            let mut shapes: Vec<Path> = Vec::new();
            for (i, piece) in run.iter().enumerate() {
                let mut slices = Vec::new();
                along(piece, h, tol, ends[i], 0, &mut slices);
                made += slices.len();
                shapes.push(if slices.len() == 1 { slices.remove(0) } else { unite(&slices, tol)? });
                if i + 1 < run.len() || closed {
                    shapes.extend(join(piece.to(), ends[i].end, ends[(i + 1) % run.len()].start, h, st, tol));
                }
            }
            if !closed {
                shapes.extend(cap(run[0].from, ends[0].start * -1.0, h, st));
                shapes.extend(cap(run[run.len() - 1].to(), ends[run.len() - 1].end, h, st));
            }
            made += shapes.len();
            if made > MAX_PIECES {
                return Err(Tangled("the stroke is too many pieces to outline"));
            }
            stretches.push(all_at_once(&shapes, 0.0, tol)?);
        }
    }
    all_at_once(&stretches, fine, tol)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;

    fn outlined(d: &str, st: &Stroke) -> String {
        outline_stroke(&Path::parse(d).path, st, 0.0, 0.0).unwrap().to_data(3)
    }

    fn pen(width: f64, cap: Cap, join: Join) -> Stroke {
        Stroke { width, cap, join, ..Stroke::default() }
    }

    #[test]
    fn a_straight_line_is_a_box_with_its_caps() {
        assert_eq!(outlined("M2 5 H12", &pen(2.0, Cap::Butt, Join::Miter)), "M12 6 H2 V4 H12 Z");
        assert_eq!(outlined("M2 5 H12", &pen(2.0, Cap::Square, Join::Miter)), "M1 6 V4 H13 V6 Z", "half the width past each end, and one box");
        assert_eq!(outlined("M2 5 H12", &pen(2.0, Cap::Round, Join::Miter)), "M12 6 H2 A1 1 0 0 1 1 5 A1 1 0 0 1 2 4 H12 A1 1 0 0 1 13 5 A1 1 0 0 1 12 6 Z");
        // No width, no outline; a dot is a dot only under a cap.
        assert_eq!(outlined("M2 5 H12", &pen(0.0, Cap::Round, Join::Round)), "");
        assert_eq!(outlined("M5 5 Z", &pen(2.0, Cap::Butt, Join::Round)), "");
        assert_eq!(outlined("M5 5 Z", &pen(2.0, Cap::Round, Join::Round)), "M6 5 A1 1 0 0 1 5 6 A1 1 0 0 1 4 5 A1 1 0 0 1 5 4 A1 1 0 0 1 6 5 Z");
    }

    #[test]
    fn corners_are_joined_as_asked() {
        let corner = "M2 2 H10 V10";
        assert_eq!(outlined(corner, &pen(2.0, Cap::Butt, Join::Miter)), "M9 3 H2 V1 H11 V10 H9 Z");
        assert_eq!(outlined(corner, &pen(2.0, Cap::Butt, Join::Bevel)), "M9 3 H2 V1 H10 L11 2 V10 H9 Z");
        assert_eq!(outlined(corner, &pen(2.0, Cap::Butt, Join::Round)), "M9 3 H2 V1 H10 A1 1 0 0 1 11 2 V10 H9 Z");
        // Too sharp for its limit, a mitre is a bevel; within it, a point.
        let sharp = "M0 0 L10 1 L0 2";
        assert_eq!(outlined(sharp, &pen(1.0, Cap::Butt, Join::Miter)), "M4.975 1 L-0.05 0.498 L0.05 -0.498 L10.05 0.502 V1.498 L0.05 2.498 L-0.05 1.502 Z");
        assert_eq!(outlined(sharp, &Stroke { miter_limit: 100.0, ..pen(1.0, Cap::Butt, Join::Miter) }), "M4.975 1 L-0.05 0.498 L0.05 -0.498 L15.025 1 L0.05 2.498 L-0.05 1.502 Z");
        // A closed square: a frame, its hole drawn the other way.
        assert_eq!(outlined("M2 2 H10 V10 H2 Z", &pen(2.0, Cap::Butt, Join::Miter)), "M9 3 H3 V9 H9 Z M11 1 V11 H1 V1 Z");
    }

    #[test]
    fn beside_a_circle_is_a_circle() {
        // A ring: two circles, all arcs.
        let ring = outlined("M15 10 A5 5 0 0 1 5 10 A5 5 0 0 1 15 10 Z", &pen(2.0, Cap::Butt, Join::Round));
        assert_eq!(ring, "M6 10 A4 4 0 0 0 14 10 A4 4 0 0 0 6 10 Z M16 10 A6 6 0 0 1 4 10 A6 6 0 0 1 16 10 Z");
        // Half of one, butt ended.
        assert_eq!(outlined("M15 10 A5 5 0 0 1 5 10", &pen(2.0, Cap::Butt, Join::Round)), "M6 10 A4 4 0 0 0 14 10 H16 A6 6 0 0 1 4 10 Z");
        // A bend tighter than the stroke is wide: the far side of the
        // stroke swings round behind the bend's middle.
        assert_eq!(outlined("M3 10 A1 1 0 0 1 5 10", &pen(4.0, Cap::Butt, Join::Round)), "M5 10 A1 1 0 0 1 3 10 H1 A3 3 0 0 1 7 10 Z");
    }

    #[test]
    fn beside_a_curve_is_fitted_to_within_what_is_fine() {
        let curve = Path::parse("M0 0 C0 10 20 10 20 0").path;
        let st = pen(2.0, Cap::Butt, Join::Round);
        let made = outline_stroke(&curve, &st, 0.00025, 0.001).unwrap();
        assert!(made.subpaths.len() == 1 && made.subpaths[0].segs.iter().filter(|s| matches!(s, Seg::Cubic { .. })).count() >= 2, "{}", made.to_data(3));
        // Every point of its outline is half the width from the curve.
        let source = Piece::new(curve.subpaths[0].start, curve.subpaths[0].segs[0]);
        let mut at = made.subpaths[0].start;
        for seg in &made.subpaths[0].segs {
            let piece = Piece::new(at, *seg);
            at = seg.to();
            if matches!(seg, Seg::Cubic { .. }) {
                for t in [0.1, 0.3, 0.5, 0.7, 0.9] {
                    let p = piece.at(t);
                    let far = source.at(source.nearest(p)).distance(p);
                    assert!((far - 1.0).abs() <= 0.001, "{far} from the curve at {p:?}");
                }
            }
        }
        // Told it needn't be so near, it's fewer curves.
        let loose = outline_stroke(&curve, &st, 0.02, 0.001).unwrap();
        assert!(loose.subpaths[0].segs.len() < made.subpaths[0].segs.len(), "{} against {}", loose.subpaths[0].segs.len(), made.subpaths[0].segs.len());
        // And it holds about what a ribbon that long and wide would.
        assert!((made.area().abs() - 2.0 * source.length()).abs() < 0.05, "{} against {}", made.area().abs(), 2.0 * source.length());
    }

    #[test]
    fn dashes_are_outlined_one_by_one() {
        let dashed = Stroke { dashes: vec![4.0, 2.0], ..pen(2.0, Cap::Butt, Join::Miter) };
        assert_eq!(outlined("M0 5 H16", &dashed), "M4 6 H0 V4 H4 Z M10 6 H6 V4 H10 Z M16 6 H12 V4 H16 Z");
        // Round a circle of radius 5: each dash a bent bar, its long
        // sides arcs.
        let round = outline_stroke(&Path::parse("M15 10 A5 5 0 0 1 5 10 A5 5 0 0 1 15 10 Z").path, &Stroke { dashes: vec![PI * 2.5, PI * 2.5], ..pen(2.0, Cap::Butt, Join::Miter) }, 0.0, 0.0).unwrap();
        assert_eq!(round.subpaths.len(), 2, "{}", round.to_data(3));
        assert!(round.subpaths.iter().all(|s| s.segs.iter().filter(|seg| matches!(seg, Seg::Arc { .. })).count() == 2), "{}", round.to_data(3));
        // Dots.
        let dots = Stroke { dashes: vec![0.0, 5.0], ..pen(2.0, Cap::Round, Join::Miter) };
        assert_eq!(outline_stroke(&Path::parse("M0 5 H16").path, &dots, 0.0, 0.0).unwrap().subpaths.len(), 4);
    }
}
