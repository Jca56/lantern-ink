//! Dashes: a line cut into the shorter lines its pattern leaves on.

use lntrn_math::Vec2;

use crate::flatten::Polyline;
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
