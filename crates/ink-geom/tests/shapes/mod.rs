//! Shapes to try boolean operations on: the kinds icons are made of,
//! set on a grid so that they share sides, corners and touches far
//! more often than chance would have it. `ink-render`'s tests draw the
//! same ones.
#![allow(dead_code)]

use ink_geom::{Affine, Path, Vec2};

/// A small random number generator: the same shapes every run.
pub struct Rng(pub u64);

impl Rng {
    /// One of `n` choices.
    pub fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) % n
    }

    /// A number from `lo` to `hi`, on a grid of `step`.
    pub fn grid(&mut self, lo: f64, hi: f64, step: f64) -> f64 {
        lo + step * self.below(((hi - lo) / step).round() as u64 + 1) as f64
    }
}

/// The kinds of shape [`shape`] makes.
pub const KINDS: u64 = 8;

/// A shape of `kind`, somewhere in a box 24 across.
pub fn shape_of(kind: u64, rng: &mut Rng) -> Path {
    let centre = |rng: &mut Rng| Vec2::new(rng.grid(7.0, 17.0, 0.5), rng.grid(7.0, 17.0, 0.5));
    match kind {
        // A rect, and one with round corners.
        0 | 1 => {
            let (w, h) = (rng.grid(3.0, 10.0, 0.5), rng.grid(3.0, 10.0, 0.5));
            let r = if kind == 1 { rng.grid(0.5, 1.5, 0.5) } else { 0.0 };
            Path::rect(rng.grid(2.0, 12.0, 0.5), rng.grid(2.0, 12.0, 0.5), w, h, r, r)
        }
        // A circle and an ellipse.
        2 => {
            let r = rng.grid(2.0, 6.0, 0.5);
            Path::ellipse(centre(rng), r, r)
        }
        3 => Path::ellipse(centre(rng), rng.grid(2.0, 7.0, 0.5), rng.grid(2.0, 5.0, 0.5)),
        // A polygon through grid points: it may well cross itself.
        4 => {
            let points: Vec<Vec2> = (0..3 + rng.below(4)).map(|_| Vec2::new(rng.grid(2.0, 22.0, 1.0), rng.grid(2.0, 22.0, 1.0))).collect();
            Path::polyline(&points, true)
        }
        // A blob: one smooth curve round a middle.
        5 => {
            let (middle, n) = (centre(rng), 4 + rng.below(3) as usize);
            let points: Vec<Vec2> = (0..n).map(|i| middle + Vec2::from_angle(std::f64::consts::TAU * (i as f64 + rng.grid(-0.3, 0.3, 0.1)) / n as f64) * rng.grid(3.0, 6.0, 0.5)).collect();
            let mut path = Path::new();
            path.move_to(points[0]);
            for i in 0..n {
                let at = |k: usize| points[(i + n + k - 1) % n];
                path.cubic_to(at(1) + (at(2) - at(0)) * (1.0 / 6.0), at(2) - (at(3) - at(1)) * (1.0 / 6.0), at(2));
            }
            path.close();
            path
        }
        // A rect or an ellipse, turned.
        6 => {
            let (middle, turn) = (centre(rng), Affine::rotate((15.0 * rng.grid(1.0, 11.0, 1.0)).to_radians()));
            let (w, h) = (rng.grid(3.0, 10.0, 1.0), rng.grid(2.0, 8.0, 1.0));
            let flat = if rng.below(2) == 0 { Path::rect(middle.x - w * 0.5, middle.y - h * 0.5, w, h, 0.0, 0.0) } else { Path::ellipse(middle, w * 0.5, h * 0.5) };
            flat.transformed(&turn.about(middle))
        }
        // A leaf of two quadratics.
        _ => {
            let (a, b) = (centre(rng), centre(rng) + Vec2::new(0.5, 0.0));
            let bow = (b - a).perp() * rng.grid(0.2, 0.6, 0.1);
            let mut path = Path::new();
            path.move_to(a).quad_to((a + b) * 0.5 + bow, b).quad_to((a + b) * 0.5 - bow, a).close();
            path
        }
    }
}

/// A shape of any kind.
pub fn shape(rng: &mut Rng) -> Path {
    let kind = rng.below(KINDS);
    shape_of(kind, rng)
}

/// Two shapes to put together: as often as not the second is the first
/// again, moved, copied, shrunk, set beside it or turned, since that is
/// where outlines lie on each other.
pub fn pair(rng: &mut Rng) -> (Path, Path) {
    let a = shape(rng);
    let about = |t: Affine, a: &Path| t.about(a.bounds().map_or(Vec2::ZERO, |b| b.center()));
    let b = match rng.below(24) {
        0..=9 => shape(rng),
        10..=12 => a.transformed(&Affine::translate(rng.grid(-6.0, 6.0, 1.0), rng.grid(-6.0, 6.0, 1.0))),
        13 | 14 => a.clone(),
        15 | 16 => {
            let by = [0.5, 0.75, 1.25][rng.below(3) as usize];
            a.transformed(&about(Affine::scale(by, by), &a))
        }
        17 | 18 => {
            let size = a.bounds().map_or(Vec2::ZERO, |b| b.size());
            a.transformed(&if rng.below(2) == 0 { Affine::translate(size.x, 0.0) } else { Affine::translate(0.0, size.y) })
        }
        19 => a.transformed(&about(Affine::rotate(std::f64::consts::FRAC_PI_2), &a)),
        // What just fits in the first's box, and the box itself: they
        // touch it without crossing.
        20 | 21 => a.bounds().map_or_else(Path::new, |b| Path::ellipse(b.center(), b.width() * 0.5, b.height() * 0.5)),
        _ => a.bounds().map_or_else(Path::new, |b| Path::rect(b.min.x, b.min.y, b.width(), b.height(), 0.0, 0.0)),
    };
    (a, b)
}
