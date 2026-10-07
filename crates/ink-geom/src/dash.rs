//! Dashes: a line cut into the shorter lines its pattern leaves on.

use lntrn_math::Vec2;

use crate::flatten::Polyline;
use crate::path::{Path, Seg, Subpath};
use crate::piece::Piece;
use crate::stroke::Stroke;

/// The most dashes one line is cut into; a finer pattern is drawn solid,
/// as it would look.
const MAX_DASHES: f64 = 10_000.0;

/// The dash pattern in use: an even count of lengths, none negative,
/// some more than nothing. `None` for a solid line.
fn pattern(dashes: &[f64]) -> Option<Vec<f64>> {
    if dashes.is_empty() || dashes.iter().any(|d| !d.is_finite() || *d < 0.0) || dashes.iter().sum::<f64>() <= 0.0 {
        return None;
    }
    // An odd list repeats to make its on and off lengths.
    Some(if dashes.len() % 2 == 1 { [dashes, dashes].concat() } else { dashes.to_vec() })
}

/// `pts` cut into its dashes, or `None` when it's stroked whole. A closed
/// line whose last dash runs into its first has them as one.
pub(crate) fn dashed(pts: &[Vec2], closed: bool, st: &Stroke) -> Option<Vec<Polyline>> {
    let pat = pattern(&st.dashes)?;
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let sum: f64 = pat.iter().sum();
    let segs = if closed { n } else { n - 1 };
    let total: f64 = (0..segs).map(|i| pts[(i + 1) % n].distance(pts[i])).sum();
    if !total.is_finite() || !sum.is_finite() || total / sum * pat.len() as f64 > MAX_DASHES {
        return None;
    }
    // Where in the pattern the line starts.
    let mut phase = if st.dash_offset.is_finite() { st.dash_offset.rem_euclid(sum) } else { 0.0 };
    let mut idx = 0;
    while phase > 0.0 && phase >= pat[idx] {
        phase -= pat[idx];
        idx = (idx + 1) % pat.len();
    }
    let mut left = pat[idx] - phase;
    let mut on = idx % 2 == 0;
    let starts_on = on;
    let mut pieces: Vec<Vec<Vec2>> = Vec::new();
    let mut cur: Vec<Vec2> = if on { vec![pts[0]] } else { Vec::new() };
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = b.distance(a);
        let mut at = 0.0;
        while len - at > left {
            at += left;
            let p = a + (b - a) * (at / len);
            if on {
                cur.push(p);
                pieces.push(std::mem::take(&mut cur));
            }
            idx = (idx + 1) % pat.len();
            left = pat[idx];
            on = !on;
            if on {
                cur.push(p);
            }
        }
        left -= len - at;
        if on {
            cur.push(b);
        }
    }
    if on && closed && starts_on {
        match pieces.first_mut() {
            // One dash all the way round: the line, closed as it was.
            None => return Some(vec![Polyline { points: pts.to_vec(), closed: true }]),
            Some(first) => {
                cur.extend_from_slice(&first[1..]);
                *first = cur;
            }
        }
    } else if on {
        pieces.push(cur);
    }
    Some(pieces.into_iter().map(|points| Polyline { points, closed: false }).collect())
}

impl Path {
    /// The path as `st` dashes it: each dash a subpath of its own, cut
    /// from the curves themselves, so a dash is as long along a curve
    /// as it is along a line and ends square to it. The path as it is
    /// when `st` has no dashes (or too many to draw).
    pub fn dashed(&self, st: &Stroke) -> Path {
        if pattern(&st.dashes).is_none() {
            return self.clone();
        }
        let mut out = Path::new();
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
            match dashed_pieces(&pieces, sub.closed, st) {
                Some(dashes) => out.subpaths.extend(dashes.into_iter().filter_map(|(run, closed)| Some(Subpath { start: run.first()?.from, segs: run.iter().map(|p| p.seg).collect(), closed }))),
                None => out.subpaths.push(sub.clone()),
            }
        }
        out
    }
}

/// A run of pieces cut into its dashes, curves staying curves: each
/// dash a run of its own, open (or the whole run, closed, when one
/// dash goes all the way round). `None` when it's stroked whole. A dash
/// of no length is one piece of no length: a dot, under a round cap.
pub(crate) fn dashed_pieces(run: &[Piece], closed: bool, st: &Stroke) -> Option<Vec<(Vec<Piece>, bool)>> {
    let pat = pattern(&st.dashes)?;
    let lengths: Vec<f64> = run.iter().map(Piece::length).collect();
    let (sum, total): (f64, f64) = (pat.iter().sum(), lengths.iter().sum());
    if run.is_empty() || total.is_nan() || total <= 0.0 || !total.is_finite() || !sum.is_finite() || total / sum * pat.len() as f64 > MAX_DASHES {
        return None;
    }
    let mut phase = if st.dash_offset.is_finite() { st.dash_offset.rem_euclid(sum) } else { 0.0 };
    let mut idx = 0;
    while phase > 0.0 && phase >= pat[idx] {
        phase -= pat[idx];
        idx = (idx + 1) % pat.len();
    }
    let mut left = pat[idx] - phase;
    let mut on = idx % 2 == 0;
    let starts_on = on;
    let mut dashes: Vec<Vec<Piece>> = Vec::new();
    let mut cur: Vec<Piece> = Vec::new();
    // A cut that lands on a corner to within rounding is at the corner:
    // not a sliver of the next piece, heading wherever rounding says.
    let hair = 1e-12 * total;
    for (piece, &len) in run.iter().zip(&lengths) {
        // How much of this piece is behind the cut, and where that is.
        let (mut gone, mut from) = (0.0, 0.0);
        if left <= hair && left > 0.0 {
            left = 0.0;
        }
        while len - gone > left + hair {
            gone += left;
            let to = piece.along(gone);
            if on {
                cur.push(piece.part(from, to));
                dashes.push(std::mem::take(&mut cur));
            }
            from = to;
            idx = (idx + 1) % pat.len();
            left = pat[idx];
            on = !on;
        }
        left = (left - (len - gone)).max(0.0);
        if on && from < 1.0 {
            cur.push(piece.part(from, 1.0));
        }
    }
    if on && closed && starts_on {
        match dashes.first_mut() {
            None => return Some(vec![(run.to_vec(), true)]),
            Some(first) => {
                cur.append(first);
                *first = cur;
            }
        }
    } else if on && !cur.is_empty() {
        dashes.push(cur);
    }
    Some(dashes.into_iter().map(|pieces| (pieces, false)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stroke::tests::{covers, line};
    use crate::stroke::{Cap, stroke};

    const TOL: f64 = 0.05;

    fn dashes(dashes: &[f64], dash_offset: f64) -> Stroke {
        Stroke { width: 2.0, dashes: dashes.to_vec(), dash_offset, ..Stroke::default() }
    }

    #[test]
    fn dashes_cut_the_line() {
        let l = line(&[(0.0, 0.0), (100.0, 0.0)], false);
        let o = stroke(&l, &dashes(&[10.0, 5.0], 0.0), TOL);
        assert_eq!(o.len(), 7, "0-10, 15-25, ... 90-100");
        assert!(covers(&o, 5.0, 0.0) && !covers(&o, 12.0, 0.0) && covers(&o, 95.0, 0.0));
        let o = stroke(&l, &dashes(&[10.0, 5.0], 10.0), TOL);
        assert!(!covers(&o, 2.0, 0.0) && covers(&o, 7.0, 0.0), "the offset starts the pattern later");
        let o = stroke(&l, &dashes(&[10.0, 5.0], -5.0), TOL);
        assert!(!covers(&o, 2.0, 0.0) && covers(&o, 7.0, 0.0), "a negative offset wraps");
        // An odd list repeats: 4 on, 4 off.
        let o = stroke(&l, &dashes(&[4.0], 0.0), TOL);
        assert!(covers(&o, 2.0, 0.0) && !covers(&o, 6.0, 0.0) && covers(&o, 10.0, 0.0));
        // Dashes of no length are dots under round caps.
        let dots = stroke(&l, &Stroke { width: 4.0, cap: Cap::Round, ..dashes(&[0.0, 10.0], 0.0) }, TOL);
        assert!(covers(&dots, 10.0, 1.5) && !covers(&dots, 15.0, 0.0));
    }

    #[test]
    fn dashes_wrap_round_a_closed_line() {
        // Round a square, the last dash runs through the start corner
        // into the first: one piece, joined there.
        let sq = &line(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)], true)[0];
        let pieces = dashed(&sq.points, true, &dashes(&[30.0, 10.0], 15.0)).unwrap();
        assert_eq!(pieces.len(), 4);
        assert_eq!(pieces[0].points, vec![Vec2::new(0.0, 15.0), Vec2::new(0.0, 0.0), Vec2::new(15.0, 0.0)]);
        // A dash longer than the line leaves it closed.
        assert_eq!(dashed(&sq.points, true, &dashes(&[1000.0, 1.0], 0.0)).unwrap(), vec![sq.clone()]);
    }

    #[test]
    fn bad_patterns_draw_solid() {
        let l = line(&[(0.0, 0.0), (100.0, 0.0)], false);
        for bad in [vec![0.0, 0.0], vec![5.0, -1.0], vec![f64::NAN, 2.0], vec![1e-6, 1e-6]] {
            assert_eq!(stroke(&l, &dashes(&bad, 0.0), TOL).len(), 1, "{bad:?} draws solid");
        }
        assert!(dashed(&[Vec2::ZERO], true, &dashes(&[1.0], 0.0)).is_none(), "a dot has nothing to cut");
    }
}
