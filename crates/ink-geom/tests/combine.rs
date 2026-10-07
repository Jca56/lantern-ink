//! Boolean operations, put to thousands of pairs of shapes
//! (ARCHITECTURE §10). Two things are asked of every result, neither
//! of them a picture (`ink-render`'s tests draw the same pairs):
//!
//! - **Point by point.** A few hundred places in the drawing are each
//!   in the result exactly when the operation says so of the two
//!   shapes, by counting how the outlines wind round them.
//! - **To the last digit.** The four results' areas add up as they
//!   must: union and intersection together hold what the two shapes
//!   hold, and so on. Areas are exact here, so a face of the picture
//!   kept or dropped by mistake can't hide.

mod shapes;

use ink_geom::{Combine, FillRule, Path, Seg, Vec2, combine, outline, winding};
use shapes::{Rng, pair};

const OPS: [Combine; 4] = [Combine::Union, Combine::Subtract, Combine::Intersect, Combine::Exclude];

fn within(path: &Path, rule: FillRule, p: Vec2) -> bool {
    let turns = winding(&outline(path), p, 1e-12);
    match rule {
        FillRule::NonZero => turns != 0,
        FillRule::EvenOdd => turns % 2 != 0,
    }
}

/// What `path` covers: its own outline made simple, so that each place
/// counts once.
fn covered(path: &Path, rule: FillRule) -> f64 {
    combine(&[(path, rule)], Combine::Union, 0.0).unwrap().area().abs()
}

/// How many of each kind of segment a path has: lines, quadratics,
/// cubics, arcs.
fn kinds(path: &Path) -> [usize; 4] {
    let mut n = [0; 4];
    for seg in path.subpaths.iter().flat_map(|s| &s.segs) {
        n[match seg {
            Seg::Line { .. } => 0,
            Seg::Quad { .. } => 1,
            Seg::Cubic { .. } => 2,
            Seg::Arc { .. } => 3,
        }] += 1;
    }
    n
}

fn check(round: usize, a: &Path, b: &Path, rules: (FillRule, FillRule), rng: &mut Rng) {
    let said = || format!("round {round}: {:?}\n  a: {}\n  b: {}", rules, a.to_data(6), b.to_data(6));
    let results: Vec<Path> = OPS.iter().map(|op| combine(&[(a, rules.0), (b, rules.1)], *op, 0.0).unwrap_or_else(|_| panic!("{op:?} couldn't be worked out, {}", said()))).collect();
    // Point by point. A place right on an outline is in neither camp:
    // those are passed over.
    let (edge_a, edge_b) = (outline(a), outline(b));
    let near = |p: Vec2| a.distance(p, 1e-4).is_some_and(|d| d < 1e-3) || b.distance(p, 1e-4).is_some_and(|d| d < 1e-3);
    for _ in 0..200 {
        let p = Vec2::new(rng.grid(0.0, 24.0, 0.001) + 0.000_37, rng.grid(0.0, 24.0, 0.001) + 0.000_61);
        if near(p) {
            continue;
        }
        let (in_a, in_b) = (within(a, rules.0, p), within(b, rules.1, p));
        for (op, result) in OPS.iter().zip(&results) {
            let want = match op {
                Combine::Union => in_a || in_b,
                Combine::Subtract => in_a && !in_b,
                Combine::Intersect => in_a && in_b,
                Combine::Exclude => in_a != in_b,
            };
            // Once round what it holds and not at all round the rest:
            // the same by either rule.
            let turns = winding(&outline(result), p, 1e-12);
            assert!(turns == i32::from(want), "{op:?} at {p:?} winds {turns} times, wanted {want}, {}\n  made: {}", said(), result.to_data(6));
        }
    }
    let _ = (&edge_a, &edge_b);
    // To the last digit.
    let area: Vec<f64> = results.iter().map(|r| r.area()).collect();
    let (hold_a, hold_b) = (covered(a, rules.0), covered(b, rules.1));
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-7 * (hold_a + hold_b).max(1.0);
    let [union, subtract, intersect, exclude] = area[..] else { unreachable!() };
    assert!(area.iter().all(|x| *x >= -1e-9), "a result holds less than nothing: {area:?}, {}", said());
    assert!(close(union + intersect, hold_a + hold_b), "union {union} + intersect {intersect} isn't {hold_a} + {hold_b}, {}", said());
    assert!(close(subtract, hold_a - intersect), "subtract {subtract} isn't {hold_a} - {intersect}, {}", said());
    assert!(close(exclude, union - intersect), "exclude {exclude} isn't {union} - {intersect}, {}", said());
    // The curves are the curves they were: nothing a result has is of
    // a kind neither shape had, and cutting hasn't shredded them.
    let (had_a, had_b) = (kinds(a), kinds(b));
    for (op, result) in OPS.iter().zip(&results) {
        let has = kinds(result);
        for kind in 1..4 {
            assert!(has[kind] == 0 || had_a[kind] + had_b[kind] > 0, "{op:?} made a kind of curve neither shape had: {has:?}, {}", said());
        }
        let (segs, had): (usize, usize) = (has.iter().sum(), had_a.iter().chain(&had_b).sum());
        assert!(segs <= 6 * had + 8, "{op:?} made {segs} segments of {had}, {}\n  made: {}", said(), result.to_data(6));
        assert!(result.subpaths.iter().all(|s| s.closed && !s.segs.is_empty()), "{op:?}: every loop is closed, {}", said());
    }
}

#[test]
fn every_pair_of_shapes_is_combined_right() {
    let mut rng = Rng(20261007);
    for round in 0..3000 {
        let (a, b) = pair(&mut rng);
        // One pair in five by the other rule: overlaps are holes.
        let rule = |rng: &mut Rng| if rng.below(5) == 0 { FillRule::EvenOdd } else { FillRule::NonZero };
        let rules = (rule(&mut rng), rule(&mut rng));
        check(round, &a, &b, rules, &mut rng);
    }
}

/// What an operation made goes into the next one: its outline runs
/// along the shapes it came from, curve on curve, which is where two
/// outlines are hardest to tell apart. Once as it was made, and once
/// as a file would hold it (three decimals: a hair off the shapes it
/// came from).
#[test]
fn what_was_made_is_combined_again() {
    let mut rng = Rng(7);
    let both = (FillRule::NonZero, FillRule::NonZero);
    for round in 0..1000 {
        let (a, b) = pair(&mut rng);
        let made = |op: Combine| combine(&[(&a, FillRule::NonZero), (&b, FillRule::NonZero)], op, 0.0).unwrap();
        let (union, cut, shared) = (made(Combine::Union), made(Combine::Subtract), made(Combine::Intersect));
        check(round, &union, &a, both, &mut rng);
        check(round, &cut, &b, both, &mut rng);
        check(round, &shared, &union, both, &mut rng);
        check(round, &cut, &shared, both, &mut rng);
        let written = |path: &Path| Path::parse(&path.to_data(3)).path;
        check(round, &written(&union), &a, both, &mut rng);
        check(round, &written(&cut), &written(&shared), both, &mut rng);
    }
}

/// Three shapes at once are what two operations in a row would make.
#[test]
fn three_shapes_at_once() {
    let mut rng = Rng(3);
    for round in 0..500 {
        let (a, b) = pair(&mut rng);
        let c = shapes::shape(&mut rng);
        let rule = FillRule::NonZero;
        for op in OPS {
            let at_once = combine(&[(&a, rule), (&b, rule), (&c, rule)], op, 0.0).unwrap();
            let first = combine(&[(&a, rule), (&b, rule)], op, 0.0).unwrap();
            let in_turn = combine(&[(&first, rule), (&c, rule)], op, 0.0).unwrap();
            assert!((at_once.area() - in_turn.area()).abs() <= 1e-7 * at_once.area().abs().max(1.0), "round {round}, {op:?}: {} at once, {} in turn\n  a: {}\n  b: {}\n  c: {}", at_once.area(), in_turn.area(), a.to_data(6), b.to_data(6), c.to_data(6));
        }
    }
}
