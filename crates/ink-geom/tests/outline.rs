//! A stroke's outline covers what the stroke covers (ARCHITECTURE
//! §10), asked of the stroke itself and of nothing that draws:
//!
//! - **Round all over** (round caps, round joins), a stroke is every
//!   place within half its width of the line. That is a distance, and
//!   says of any place whether it's in.
//! - **One piece, cut off square** (butt caps), a stroke is every place
//!   a square-on line from the piece reaches within half the width:
//!   the places `p` with a point of the piece nearer than that where
//!   the way to `p` is square to the way the piece is heading. That
//!   holds however tight the piece bends, where the stroke folds over
//!   itself.
//!
//! `ink-render`'s tests draw the same strokes beside the renderer's.

mod shapes;

use ink_geom::{Cap, Join, Path, Piece, Seg, Stroke, Subpath, Vec2, outline, outline_stroke, winding};
use shapes::{Rng, line};

const WIDTHS: [f64; 7] = [0.3, 0.75, 1.0, 1.5, 2.0, 3.0, 5.0];
/// A place this near a stroke's edge is in neither camp.
const EDGE: f64 = 4e-3;

fn inside(made: &Path, p: Vec2) -> bool {
    winding(&outline(made), p, 1e-12) != 0
}

#[test]
fn round_all_over_it_is_everywhere_near_the_line() {
    let mut rng = Rng(11);
    let mut looked = 0;
    for round in 0..600 {
        let path = line(&mut rng);
        let st = Stroke { width: WIDTHS[rng.below(7) as usize], cap: Cap::Round, join: Join::Round, ..Stroke::default() };
        let made = outline_stroke(&path, &st, 0.000125, 0.0005).unwrap_or_else(|_| panic!("round {round}: {} at {} wide couldn't be worked out", path.to_data(6), st.width));
        for _ in 0..150 {
            let p = Vec2::new(rng.grid(0.0, 24.0, 0.001) + 0.000_37, rng.grid(0.0, 24.0, 0.001) + 0.000_61);
            let Some(far) = path.distance(p, 1e-6) else { continue };
            if (far - st.width * 0.5).abs() < EDGE {
                continue;
            }
            looked += 1;
            assert!(inside(&made, p) == (far < st.width * 0.5), "round {round}: {p:?} is {far} from the line, {} wide\n  line: {}\n  made: {}", st.width, path.to_data(6), made.to_data(6));
        }
    }
    assert!(looked > 60_000, "{looked} places looked at");
}

/// How near `piece` comes to `p` square on: the least of the distances
/// from `p` to the points of the piece where the way to `p` is square
/// to the way the piece is heading. `None` when there's no such point.
fn square_on(piece: &Piece, p: Vec2) -> Option<f64> {
    const STEPS: usize = 4000;
    let lean = |t: f64| (p - piece.at(t)).dot(piece.heading(t));
    let mut nearest: Option<f64> = None;
    let mut before = lean(0.0);
    for i in 1..=STEPS {
        let (a, b) = ((i - 1) as f64 / STEPS as f64, i as f64 / STEPS as f64);
        let after = lean(b);
        if before == 0.0 || before * after < 0.0 {
            let (mut lo, mut hi) = (a, b);
            for _ in 0..50 {
                let mid = (lo + hi) * 0.5;
                if lean(mid) * before > 0.0 { lo = mid } else { hi = mid }
            }
            let far = piece.at((lo + hi) * 0.5).distance(p);
            nearest = Some(nearest.map_or(far, |n| n.min(far)));
        }
        before = after;
    }
    nearest
}

#[test]
fn one_piece_cut_off_square_is_what_its_square_on_lines_reach() {
    let mut rng = Rng(12);
    let mut looked = 0;
    for round in 0..900 {
        // One curve of a shape, on its own.
        let shape = line(&mut rng);
        let pieces: Vec<Piece> = shape.subpaths.iter().flat_map(|sub| sub.segs.iter().scan(sub.start, |at, seg| Some(Piece::new(std::mem::replace(at, seg.to()), *seg)))).filter(|p| p.length() > 0.0).collect();
        let curves: Vec<&Piece> = pieces.iter().filter(|p| !matches!(p.seg, Seg::Line { .. })).collect();
        let Some(piece) = (if curves.is_empty() { pieces.first() } else { Some(curves[rng.below(curves.len() as u64) as usize]) }) else { continue };
        let path = Path { subpaths: vec![Subpath { start: piece.from, segs: vec![piece.seg], closed: false }] };
        let half = WIDTHS[rng.below(7) as usize] * 0.5;
        let st = Stroke { width: half * 2.0, cap: Cap::Butt, ..Stroke::default() };
        let made = outline_stroke(&path, &st, 0.000125, 0.0005).unwrap_or_else(|_| panic!("round {round}: {} at {} wide couldn't be worked out", path.to_data(6), st.width));
        let reach = piece.bounds().expand(half + 0.5);
        for _ in 0..150 {
            let p = Vec2::new(reach.min.x + reach.width() * rng.below(10_000) as f64 / 10_000.0, reach.min.y + reach.height() * rng.below(10_000) as f64 / 10_000.0);
            let far = square_on(piece, p);
            // Near the stroke's edge, or its ends, is in neither camp.
            let by_an_end = [0.0, 1.0].into_iter().any(|t| (p - piece.at(t)).dot(piece.heading(t)).abs() < EDGE * piece.heading(t).length() && p.distance(piece.at(t)) < half + EDGE);
            if far.is_some_and(|far| (far - half).abs() < EDGE) || by_an_end {
                continue;
            }
            looked += 1;
            let want = far.is_some_and(|far| far < half);
            assert!(inside(&made, p) == want, "round {round}: {p:?} is {far:?} square on from the piece, {} wide\n  piece: {}\n  made: {}", st.width, path.to_data(6), made.to_data(6));
        }
    }
    assert!(looked > 80_000, "{looked} places looked at");
}
