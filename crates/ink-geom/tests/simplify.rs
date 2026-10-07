//! A simplified path is the path it was, to within what it was told
//! (ARCHITECTURE §10): over shapes of every kind, as drawn and as the
//! short straight lines a tracer or a flattener would leave of them.

mod shapes;

use ink_geom::{Path, Piece, Vec2};
use shapes::{Rng, shape};

fn pieces(path: &Path) -> Vec<Piece> {
    path.subpaths
        .iter()
        .flat_map(|sub| {
            let mut all: Vec<Piece> = sub.segs.iter().scan(sub.start, |at, seg| Some(Piece::new(std::mem::replace(at, seg.to()), *seg))).collect();
            let end = all.last().map_or(sub.start, Piece::to);
            if sub.closed && end != sub.start {
                all.push(Piece::new(end, ink_geom::Seg::Line { to: sub.start }));
            }
            all
        })
        .collect()
}

/// How far the furthest point of `a` is from `b`.
fn strays(a: &[Piece], b: &[Piece]) -> f64 {
    let far = |p: Vec2| b.iter().map(|piece| piece.at(piece.nearest(p)).distance(p)).fold(f64::INFINITY, f64::min);
    a.iter().flat_map(|piece| (0..=12).map(|i| piece.at(i as f64 / 12.0))).map(far).fold(0.0, f64::max)
}

#[test]
fn a_simplified_path_stays_where_it_was() {
    let mut rng = Rng(5);
    let (mut before, mut after) = (0, 0);
    for round in 0..400 {
        let drawn = shape(&mut rng);
        // As drawn, and as short lines within a hundredth of it.
        let flat = Path { subpaths: drawn.flatten(0.01).into_iter().map(|line| Path::polyline(&line.points, line.closed).subpaths.remove(0)).collect() };
        for (path, tol) in [(&drawn, 0.02), (&flat, 0.05), (&flat, 0.2)] {
            let (simple, left) = path.simplified(tol);
            let (was, now) = (pieces(path), pieces(&simple));
            // Both ways: nothing of either strays from the other.
            let (out, back) = (strays(&was, &now), strays(&now, &was));
            assert!(out <= tol * 1.05 && back <= tol * 1.05, "round {round} at {tol}: strays {out:.4} one way, {back:.4} the other\n  was: {}\n  now: {}", path.to_data(4), simple.to_data(4));
            assert!(now.len() <= was.len(), "round {round}: {} segments of {}", now.len(), was.len());
            // The anchors left are anchors it had, in order.
            for (sub, kept) in simple.subpaths.iter().zip(&left) {
                // (Round a closed one they come in order from wherever
                // it starts now: where it started, if that one is left.)
                let least = kept.iter().position(|k| Some(k) == kept.iter().min()).unwrap_or(0);
                let mut turned = kept.clone();
                turned.rotate_left(least);
                assert!(turned.windows(2).all(|w| w[0] < w[1]) && (least == 0 || (sub.closed && !kept.contains(&0))), "round {round}: {kept:?} for {}", simple.to_data(3));
                assert_eq!(kept.len(), sub.segs.len() + usize::from(!sub.closed || matches!(sub.segs.last(), Some(seg) if seg.to() != sub.start)), "round {round}: {kept:?} for {}", simple.to_data(3));
            }
            before += was.len();
            after += now.len();
        }
    }
    // And it is worth doing: far fewer segments in all.
    assert!(after * 3 < before, "{after} segments left of {before}");
}
