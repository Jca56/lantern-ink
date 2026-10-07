//! Where two pieces of line meet: the crossings, the touches, and the
//! ends of any stretch they share. Boolean operations cut outlines
//! here, so each piece stays the kind it is and nothing is flattened.
//!
//! Two pieces *meet* where they cross or touch, and where an end of
//! one lies on the other to within `tol` (a corner set on a side).
//! Two that only run close to each other, without crossing, don't
//! meet: they stay two lines, and what's built on this tells them
//! apart however near they are.

use lntrn_math::{Rect, Vec2};

use crate::piece::Piece;

/// A place two pieces meet: how far along each, 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meet {
    pub t: f64,
    pub u: f64,
}

/// Two outlines lie on each other in a way that can't be worked out: no
/// answer is better than a wrong one.
/// With what it was that couldn't be: for whoever looks into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tangled(pub &'static str);

/// The most looking one pair of pieces gets: two that cross a handful
/// of times take a few hundred looks.
const BUDGET: u32 = 200_000;
/// A touch is looked for within this many `tol`s of where two pieces
/// were found to come near each other.
const TOUCH: f64 = 1e6;
/// Two pieces are the same line where they're within this share of
/// `tol` of each other: what's left when one line is worked out two
/// ways, and a hundredth of what counts as meeting.
pub(crate) const SAME: f64 = 0.01;

/// Whether two boxes come within `tol` of each other (touching counts,
/// and so does a box with no width: a level line's).
pub(crate) fn boxes_near(a: &Rect, b: &Rect, tol: f64) -> bool {
    a.min.x <= b.max.x + tol && b.min.x <= a.max.x + tol && a.min.y <= b.max.y + tol && b.min.y <= a.max.y + tol
}

/// A part of a piece, and where on the whole piece it is.
struct Part {
    piece: Piece,
    t0: f64,
    t1: f64,
    bounds: Rect,
    bulge: f64,
}

impl Part {
    fn new(piece: Piece, t0: f64, t1: f64) -> Part {
        Part { piece, t0, t1, bounds: piece.bounds(), bulge: piece.bulge() }
    }

    fn of(whole: &Piece, t0: f64, t1: f64) -> Part {
        Part::new(whole.part(t0, t1), t0, t1)
    }

    fn halves(&self) -> (Part, Part) {
        let (a, b) = self.piece.split(0.5);
        let mid = (self.t0 + self.t1) * 0.5;
        (Part::new(a, self.t0, mid), Part::new(b, mid, self.t1))
    }

    /// Straight enough to stand for its chord.
    fn flat(&self, within: f64) -> bool {
        self.bulge <= within || self.t1 - self.t0 < 1e-13
    }
}

/// Where the straight lines `p0`–`p1` and `q0`–`q1` come nearest each
/// other: how far along each, and how near.
fn closest(p0: Vec2, p1: Vec2, q0: Vec2, q1: Vec2) -> (f64, f64, f64) {
    let (d1, d2, r) = (p1 - p0, q1 - q0, p0 - q0);
    let (a, e, f) = (d1.length_squared(), d2.length_squared(), d2.dot(r));
    let (s, t);
    if a <= 0.0 && e <= 0.0 {
        (s, t) = (0.0, 0.0);
    } else if a <= 0.0 {
        (s, t) = (0.0, (f / e).clamp(0.0, 1.0));
    } else {
        let c = d1.dot(r);
        if e <= 0.0 {
            (s, t) = ((-c / a).clamp(0.0, 1.0), 0.0);
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            // Parallel lines are as near at one end as anywhere.
            let s0 = if denom > 1e-18 * a * e { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let t0 = (b * s0 + f) / e;
            (s, t) = if t0 < 0.0 {
                ((-c / a).clamp(0.0, 1.0), 0.0)
            } else if t0 > 1.0 {
                (((b - c) / a).clamp(0.0, 1.0), 1.0)
            } else {
                (s0, t0)
            };
        }
    }
    (s, t, (p0 + d1 * s).distance(q0 + d2 * t))
}

/// How near the straight line `p0`–`p1` comes to the box `r`: nothing
/// if it goes through it.
fn line_to_box(p0: Vec2, p1: Vec2, r: &Rect) -> f64 {
    let inside = |p: Vec2| p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y;
    if inside(p0) || inside(p1) {
        return 0.0;
    }
    let corners = [r.min, Vec2::new(r.max.x, r.min.y), r.max, Vec2::new(r.min.x, r.max.y)];
    (0..4).map(|i| closest(p0, p1, corners[i], corners[(i + 1) % 4]).2).fold(f64::INFINITY, f64::min)
}

/// Every place the parts `a` and `b` might meet, onto `out`: each to
/// be looked at closely after.
fn search(a: &Part, b: &Part, tol: f64, out: &mut Vec<Meet>, budget: &mut u32) -> Result<(), Tangled> {
    *budget = budget.checked_sub(1).ok_or(Tangled("two pieces lie along each other without being the same line"))?;
    if !boxes_near(&a.bounds, &b.bounds, tol) {
        return Ok(());
    }
    // Each part is within its bulge of its chord, so the parts are no
    // nearer than their chords less both bulges: two curves running
    // side by side are told apart long before they're cut down to
    // straight bits.
    let (s, r, gap) = closest(a.piece.from, a.piece.to(), b.piece.from, b.piece.to());
    if gap > tol + a.bulge + b.bulge {
        return Ok(());
    }
    let (flat_a, flat_b) = (a.flat(tol * 0.25), b.flat(tol * 0.25));
    if flat_a && flat_b {
        out.push(Meet { t: a.t0 + s * (a.t1 - a.t0), u: b.t0 + r * (b.t1 - b.t0) });
        return Ok(());
    }
    // A straight part's box says little when it runs corner to corner:
    // it's the line itself the other part has to come near.
    let (line, other) = if flat_a { (a, b) } else { (b, a) };
    if (flat_a || flat_b) && line_to_box(line.piece.from, line.piece.to(), &other.bounds) > tol + line.bulge {
        return Ok(());
    }
    if !flat_a && (flat_b || a.bulge >= b.bulge) {
        let (first, second) = a.halves();
        search(&first, b, tol, out, budget)?;
        search(&second, b, tol, out, budget)
    } else {
        let (first, second) = b.halves();
        search(a, &first, tol, out, budget)?;
        search(a, &second, tol, out, budget)
    }
}

/// A place the pieces might meet, made as near a meeting as it can be:
/// where they cross, or where they come closest. With the gap left.
fn settle(a: &Piece, b: &Piece, from: Meet, tol: f64) -> (Meet, f64) {
    let gap = |m: &Meet| a.at(m.t).distance(b.at(m.u));
    let (mut at, mut best) = (from, gap(&from));
    // Downhill on the gap, each step held back as far as it needs to
    // be to go down (Levenberg and Marquardt's way): it finds a
    // crossing fast, and a touch at all.
    let mut hold = 1e-9;
    for _ in 0..60 {
        if best == 0.0 || hold > 1e9 {
            break;
        }
        let f = a.at(at.t) - b.at(at.u);
        let (da, db) = (a.heading(at.t), b.heading(at.u) * -1.0);
        let (m00, m01, m11) = (da.dot(da), da.dot(db), db.dot(db));
        let (d0, d1) = (m00 * (1.0 + hold) + 1e-300, m11 * (1.0 + hold) + 1e-300);
        let det = d0 * d1 - m01 * m01;
        if det.is_nan() || det <= 0.0 {
            break;
        }
        let (g0, g1) = (da.dot(f), db.dot(f));
        let next = Meet { t: (at.t + (g1 * m01 - g0 * d1) / det).clamp(0.0, 1.0), u: (at.u + (g0 * m01 - g1 * d0) / det).clamp(0.0, 1.0) };
        let now = gap(&next);
        if now < best {
            let done = best - now <= 1e-3 * best && now <= 1e-14 * (a.from.abs().max_element() + 1.0);
            (at, best, hold) = (next, now, (hold * 0.2).max(1e-12));
            if done {
                break;
            }
        } else if next == at {
            break;
        } else {
            hold *= 10.0;
        }
    }
    // Where they only touch, the gap is as flat as ground gets and
    // downhill stops anywhere near. The touch itself is where they
    // head the same way, square to the line between them: that is a
    // sharp place, found as a crossing is.
    let inside = |v: f64| v > 0.0 && v < 1.0;
    let (ha, hb) = (a.heading(at.t), b.heading(at.u));
    if inside(at.t) && inside(at.u) && ha.perp_dot(hb).abs() <= 1e-3 * ha.length() * hb.length() {
        let aim = |m: &Meet| {
            let (ha, hb) = (a.heading(m.t), b.heading(m.u));
            let scale = (ha.length() * hb.length()).max(1e-300);
            (ha.perp_dot(hb) / scale, (a.at(m.t) - b.at(m.u)).dot(ha) / scale)
        };
        let mut touch = at;
        for _ in 0..12 {
            const H: f64 = 1e-6;
            let (g0, g1) = aim(&touch);
            let (dt, du) = (aim(&Meet { t: touch.t + H, ..touch }), aim(&Meet { u: touch.u + H, ..touch }));
            let (j00, j10, j01, j11) = ((dt.0 - g0) / H, (dt.1 - g1) / H, (du.0 - g0) / H, (du.1 - g1) / H);
            let det = j00 * j11 - j01 * j10;
            if !(det.is_finite() && det != 0.0) {
                break;
            }
            let next = Meet { t: touch.t - (g0 * j11 - g1 * j01) / det, u: touch.u - (g1 * j00 - g0 * j10) / det };
            if !(inside(next.t) && inside(next.u)) {
                break;
            }
            let moved = (next.t - touch.t).abs() + (next.u - touch.u).abs();
            touch = next;
            if moved < 1e-15 {
                break;
            }
        }
        // Kept if it is the same meeting still: not far off, and the
        // pieces no further apart there than rounding puts them.
        let now = gap(&touch);
        if now <= best.max(tol * SAME) && a.at(touch.t).distance(a.at(at.t)) <= TOUCH * tol {
            (at, best) = (touch, now);
        }
    }
    (at, best)
}

/// The stretch `a` and `b` share, if the ends found on each other
/// (`ends`) are the ends of one: they are within `tol` of each other
/// all the way between, which is the same line as far as anything
/// here can tell (and if they cross somewhere along it, at an angle
/// too fine to see, that is neither here nor there).
fn stretch(a: &Piece, b: &Piece, ends: &[Meet], tol: f64) -> Option<(Meet, Meet)> {
    let lo = *ends.iter().min_by(|x, y| x.t.total_cmp(&y.t))?;
    let hi = *ends.iter().max_by(|x, y| x.t.total_cmp(&y.t))?;
    if a.at(lo.t).distance(a.at(hi.t)) <= 4.0 * tol || b.at(lo.u).distance(b.at(hi.u)) <= 4.0 * tol {
        return None;
    }
    let together = [0.25, 0.5, 0.75].into_iter().all(|share| {
        let p = a.at(lo.t + (hi.t - lo.t) * share);
        b.at(b.nearest(p)).distance(p) <= tol
    });
    together.then_some((lo, hi))
}

/// Where `a` and `b` meet, in order along `a`: each crossing, each
/// touch, each end of one lying on the other (to within `tol`), and
/// both ends of any stretch where they're the same line. A meet at an
/// end of either piece says exactly 0 or 1 for it.
pub fn meets(a: &Piece, b: &Piece, tol: f64) -> Result<Vec<Meet>, Tangled> {
    met(a, b, tol).map(|(meets, _)| meets)
}

/// Where two pieces meet, and the two ends of the stretch along which
/// they're the same line, if there is one.
pub(crate) type Met = (Vec<Meet>, Option<(Meet, Meet)>);

/// [`meets`], and the stretch along which the two are the same line,
/// if there is one: its two ends, the first the earlier along `a`.
pub(crate) fn met(a: &Piece, b: &Piece, tol: f64) -> Result<Met, Tangled> {
    if !boxes_near(&a.bounds(), &b.bounds(), tol) {
        return Ok((Vec::new(), None));
    }
    // An end of one lying on the other is a meet at exactly that end.
    let mut found: Vec<Meet> = Vec::new();
    for t in [0.0, 1.0] {
        let p = a.at(t);
        let u = b.nearest(p);
        if b.at(u).distance(p) <= tol {
            found.push(Meet { t, u });
        }
    }
    for u in [0.0, 1.0] {
        let p = b.at(u);
        let t = a.nearest(p);
        if a.at(t).distance(p) <= tol {
            found.push(Meet { t, u });
        }
    }
    // Where they're the same line there's no end of places to find:
    // the stretch's two ends are its meets, and only what's left of
    // each piece is looked through.
    let mut maybe = Vec::new();
    let mut budget = BUDGET;
    let shared = stretch(a, b, &found, tol);
    match shared {
        Some((lo, hi)) => {
            found.retain(|m| *m == lo || *m == hi);
            let (u0, u1) = (lo.u.min(hi.u), lo.u.max(hi.u));
            for (t0, t1) in [(0.0, lo.t), (hi.t, 1.0)] {
                for (v0, v1) in [(0.0, u0), (u1, 1.0)] {
                    if t1 > t0 && v1 > v0 {
                        search(&Part::of(a, t0, t1), &Part::of(b, v0, v1), tol, &mut maybe, &mut budget)?;
                    }
                }
            }
        }
        None => search(&Part::new(*a, 0.0, 1.0), &Part::new(*b, 0.0, 1.0), tol, &mut maybe, &mut budget)?,
    }
    // What the search turned up, looked at closely. Where the pieces
    // truly meet (they cross, or touch) is a meet. Where they only
    // come within reach of each other, they may cross somewhere along
    // that stretch at so fine an angle that there's no homing in on it:
    // that is found by which side of one the other is on at each end
    // of the stretch. A stretch they run along without crossing has no
    // meet: they're two lines there, however close.
    let near = |t: f64| {
        let p = a.at(t);
        b.at(b.nearest(p)).distance(p)
    };
    let side = |t: f64| {
        let p = a.at(t);
        let u = b.nearest(p);
        b.heading(u).perp_dot(p - b.at(u))
    };
    let mut looked: Vec<(f64, f64)> = Vec::new();
    for (m, gap) in maybe.into_iter().map(|m| settle(a, b, m, tol)) {
        if gap <= tol * SAME {
            found.push(m);
        } else if gap <= tol && !looked.iter().any(|(lo, hi)| *lo <= m.t && m.t <= *hi) {
            // As far each way as they stay within reach.
            let reach = |way: f64| {
                let mut step = 1e-6;
                while (0.0..=1.0).contains(&(m.t + way * step)) && near(m.t + way * step) <= tol {
                    step *= 2.0;
                }
                (m.t + way * step).clamp(0.0, 1.0)
            };
            let (mut lo, mut hi) = (reach(-1.0), reach(1.0));
            looked.push((lo, hi));
            let (from, to) = (side(lo), side(hi));
            if from * to < 0.0 {
                for _ in 0..64 {
                    let mid = (lo + hi) * 0.5;
                    if side(mid) * from > 0.0 { lo = mid } else { hi = mid }
                }
                let t = (lo + hi) * 0.5;
                found.push(Meet { t, u: b.nearest(a.at(t)) });
            }
        }
    }
    // A meet within reach of an end is at the end.
    for m in &mut found {
        for end in [0.0, 1.0] {
            if a.at(m.t).distance(a.at(end)) <= 4.0 * tol {
                m.t = end;
            }
            if b.at(m.u).distance(b.at(end)) <= 4.0 * tol {
                m.u = end;
            }
        }
    }
    found.sort_by(|x, y| x.t.total_cmp(&y.t).then(x.u.total_cmp(&y.u)));
    // One meet for each place: the same place found twice is the one
    // of them nearest an end (an end is a corner both outlines already
    // have), else the one where the pieces head most the same way (a
    // touch is where they run together), else where they're closest.
    let rank = |m: &Meet| {
        let at_end = |v: f64| v == 0.0 || v == 1.0;
        let (ha, hb) = (a.heading(m.t), b.heading(m.u));
        let askew = ha.perp_dot(hb).abs() / (ha.length() * hb.length()).max(1e-300);
        (!(at_end(m.t) && at_end(m.u)), !(at_end(m.t) || at_end(m.u)), if askew < 1e-3 { askew } else { 1.0 }, a.at(m.t).distance(b.at(m.u)))
    };
    // The same place: right there, or a little along a stretch where
    // the two are one line (either side of where they touch).
    let same = |x: &Meet, y: &Meet| {
        let apart = a.at(x.t).distance(a.at(y.t));
        (apart <= 4.0 * tol && b.at(x.u).distance(b.at(y.u)) <= 4.0 * tol) || (apart <= TOUCH * tol && near((x.t + y.t) * 0.5) <= tol * SAME)
    };
    let mut out: Vec<Meet> = Vec::new();
    for m in found {
        match out.iter_mut().find(|known| same(known, &m)) {
            Some(known) if rank(&m) < rank(known) => *known = m,
            Some(_) => {}
            None => out.push(m),
        }
    }
    out.sort_by(|x, y| x.t.total_cmp(&y.t).then(x.u.total_cmp(&y.u)));
    Ok((out, shared))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Path;

    /// The first piece of `d`.
    fn piece(d: &str) -> Piece {
        let path = Path::parse(d).path;
        let sub = &path.subpaths[0];
        Piece::new(sub.start, sub.segs[0])
    }

    const TOL: f64 = 1e-9;

    /// The places `a` and `b` meet, as points (on `a`).
    fn places(a: &str, b: &str) -> Vec<(f64, f64)> {
        let (a, b) = (piece(a), piece(b));
        let found = meets(&a, &b, TOL).unwrap();
        for m in &found {
            assert!(a.at(m.t).distance(b.at(m.u)) <= TOL, "{m:?}: {:?} and {:?}", a.at(m.t), b.at(m.u));
        }
        // The other way about it finds the same places.
        let back = meets(&b, &a, TOL).unwrap();
        assert_eq!(back.len(), found.len(), "{found:?} one way, {back:?} the other");
        found.iter().map(|m| a.at(m.t)).map(|p| ((p.x * 1e6).round() / 1e6, (p.y * 1e6).round() / 1e6)).collect()
    }

    #[test]
    fn lines_cross_touch_and_miss() {
        assert_eq!(places("M0 0 L10 10", "M0 10 L10 0"), [(5.0, 5.0)]);
        assert_eq!(places("M0 0 L10 0", "M5 0 L5 10"), [(5.0, 0.0)], "an end on the other");
        assert_eq!(places("M0 0 L10 0", "M10 0 L10 10"), [(10.0, 0.0)], "end to end");
        assert_eq!(places("M0 0 L10 0", "M0 1 L10 1"), [], "side by side");
        assert_eq!(places("M0 0 L10 0", "M11 -5 L11 5"), [], "past its end");
        // A meet at an end says so exactly.
        let found = meets(&piece("M0 0 L10 0"), &piece("M10 0 L10 10"), TOL).unwrap();
        assert_eq!(found, [Meet { t: 1.0, u: 0.0 }]);
    }

    #[test]
    fn lines_that_share_a_stretch_meet_at_its_ends() {
        assert_eq!(places("M0 0 L10 0", "M4 0 L16 0"), [(4.0, 0.0), (10.0, 0.0)]);
        assert_eq!(places("M0 0 L10 0", "M8 0 L2 0"), [(2.0, 0.0), (8.0, 0.0)], "one inside the other, the other way round");
        assert_eq!(places("M0 0 L10 10", "M0 0 L10 10"), [(0.0, 0.0), (10.0, 10.0)], "the same line");
        assert_eq!(places("M0 0 L10 0", "M10 0 L20 0"), [(10.0, 0.0)], "end to end in one line");
    }

    #[test]
    fn curves_cross_where_they_cross() {
        // A circle's quarter and a line through it.
        let found = places("M10 0 A10 10 0 0 1 0 10", "M0 0 L10 10");
        assert_eq!(found, [(7.071068, 7.071068)]);
        // A cubic and a line, three times.
        assert_eq!(places("M0 0 C10 20 20 -20 30 0", "M0 0 L30 0").len(), 3);
        // Two circles' arcs, twice.
        let lens = places("M-5 -10 A10 10 0 0 1 -5 10", "M5 10 A10 10 0 0 1 5 -10");
        assert_eq!(lens, [(0.0, -8.660254), (0.0, 8.660254)]);
        // A quadratic and a cubic.
        assert_eq!(places("M0 0 Q10 20 20 0", "M0 8 C5 8 15 2 20 12").len(), 2);
        assert_eq!(places("M0 0 Q10 20 20 0", "M0 11 L20 11"), [], "over the top");
    }

    #[test]
    fn a_touch_is_one_meet_at_the_place_itself() {
        // A circle standing on a line.
        assert_eq!(places("M0 5 A5 5 0 0 0 10 5", "M-10 10 L20 10"), [(5.0, 10.0)]);
        // Two circles side by side.
        assert_eq!(places("M5 -5 A5 5 0 0 1 5 5", "M15 5 A5 5 0 0 1 15 -5"), [(10.0, 0.0)]);
        // A curve's top against a line, as near as its numbers put it.
        let top = places("M0 0 C0 8 12 8 12 0", "M-5 6 L20 6");
        assert_eq!(top, [(6.0, 6.0)]);
        // A corner arc running on from a straight side: they meet where
        // one hands over to the other, once.
        assert_eq!(places("M0 0 L10 0", "M6 0 A4 4 0 0 1 10 4"), [(6.0, 0.0)]);
    }

    #[test]
    fn curves_that_share_a_stretch_meet_at_its_ends() {
        // Two arcs of one circle.
        let shared = places("M10 0 A10 10 0 0 1 -10 0", "M0 10 A10 10 0 0 1 0 -10");
        assert_eq!(shared, [(0.0, 10.0), (-10.0, 0.0)]);
        // A cubic and a part of itself.
        let whole = piece("M0 0 C0 10 20 10 20 -4");
        let part = whole.part(0.25, 0.75);
        let found = meets(&whole, &part, TOL).unwrap();
        assert_eq!(found.len(), 2);
        assert!((found[0].t - 0.25).abs() < 1e-9 && (found[1].t - 0.75).abs() < 1e-9 && found[0].u == 0.0 && found[1].u == 1.0, "{found:?}");
        // The same curve, whole.
        assert_eq!(meets(&whole, &whole, TOL).unwrap(), [Meet { t: 0.0, u: 0.0 }, Meet { t: 1.0, u: 1.0 }]);
    }
}
