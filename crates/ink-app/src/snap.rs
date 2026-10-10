//! Snapping (ARCHITECTURE §8; LS3's `snap.rs`, with a grid under it):
//! what's dragged lands on the grid (whole units of the drawing, and
//! half ones once those are far enough apart on the screen to tell),
//! or on a line: the page's edges and middle, another shape's, an
//! anchor's, a guide. Each way by itself, and whichever is nearest
//! wins: a line draws things to it from `REACH` px away, and the grid
//! from wherever they are. A line landed on is shown; the grid isn't
//! (the pixel grid shows it).
//!
//! Apart from the window: the lines, and the ways something lands (a
//! point, a box's side, its corner along a diagonal, a box moved
//! whole, and each handle of the selection's box).

use lntrn_math::{Rect, Vec2};

use crate::handles::{self, Handle, Keys};

/// How near, logical screen px, something is drawn to a line (LS3's).
pub const REACH: f64 = 11.0;
/// How far apart half units are on the screen, logical px, before
/// things land on them too.
pub const HALF_MIN: f64 = 6.0;
/// As near as makes two places one.
const SAME: f64 = 1e-9;

/// What things land on, in the drawing's coordinates: `xs` run down
/// the page, `ys` across it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    /// How far apart the grid's lines are; 0 for no grid.
    pub step: f64,
    /// How near a line draws something to it.
    pub reach: f64,
}

/// The lines something landed on this frame: one down, one across at
/// most.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Landed {
    pub x: Option<f64>,
    pub y: Option<f64>,
}

/// How far apart the grid's lines are when a `whole` step of it (a
/// unit, on any page but a tiny one) shows `per_unit` window px a unit,
/// at the display's `scale`: half steps once they're far enough apart.
pub fn step_for(whole: f64, per_unit: f64, scale: f64) -> f64 {
    if whole / 2.0 * per_unit >= HALF_MIN * scale { whole / 2.0 } else { whole }
}

/// The line nearest `v` within `reach`, if there's one.
fn nearest(lines: &[f64], v: f64, reach: f64) -> Option<f64> {
    lines.iter().copied().filter(|t| (t - v).abs() <= reach).min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
}

impl Targets {
    /// A box's edges and middle.
    pub fn add_box(&mut self, r: Rect) {
        self.xs.extend([r.min.x, (r.min.x + r.max.x) / 2.0, r.max.x]);
        self.ys.extend([r.min.y, (r.min.y + r.max.y) / 2.0, r.max.y]);
    }

    /// A point: the line down through it, and the one across.
    pub fn add_point(&mut self, p: Vec2) {
        self.xs.push(p.x);
        self.ys.push(p.y);
    }

    /// In order, each line once: so that which of two lines as near
    /// wins doesn't hang on what order they came in.
    pub fn settle(&mut self) {
        for lines in [&mut self.xs, &mut self.ys] {
            lines.retain(|v| v.is_finite());
            lines.sort_by(f64::total_cmp);
            lines.dedup_by(|a, b| (*a - *b).abs() <= SAME);
        }
    }

    fn on_grid(&self, v: f64) -> Option<f64> {
        (self.step > 0.0).then(|| (v / self.step).round() * self.step)
    }

    /// Where `v` lands among `lines` and the grid, and the line it's
    /// on there, if it's a line: the nearer of the two, the line where
    /// they're as near.
    fn land(&self, lines: &[f64], v: f64) -> Option<(f64, Option<f64>)> {
        match (nearest(lines, v, self.reach), self.on_grid(v)) {
            (Some(line), Some(grid)) if (line - v).abs() > (grid - v).abs() + SAME => Some((grid, None)),
            (Some(line), _) => Some((line, Some(line))),
            (None, Some(grid)) => Some((grid, None)),
            (None, None) => None,
        }
    }

    /// A point: each of its coordinates to where it lands.
    pub fn point(&self, p: Vec2) -> (Vec2, Landed) {
        let (x, y) = (self.land(&self.xs, p.x), self.land(&self.ys, p.y));
        (Vec2::new(x.map_or(p.x, |l| l.0), y.map_or(p.y, |l| l.0)), Landed { x: x.and_then(|l| l.1), y: y.and_then(|l| l.1) })
    }

    /// A box's side dragged to `v` (its x when `across`, else its y):
    /// where it lands that way. Its other coordinate isn't the side's
    /// to move.
    pub fn edge(&self, across: bool, v: f64) -> (f64, Landed) {
        let landed = self.land(if across { &self.xs } else { &self.ys }, v);
        let line = landed.and_then(|l| l.1);
        (landed.map_or(v, |l| l.0), if across { Landed { x: line, y: None } } else { Landed { x: None, y: line } })
    }

    /// A box's corner dragged to `p` along the diagonal from `anchor`
    /// (the box keeps its shape): on along the diagonal to where one
    /// of the corner's two edges lands, whichever has less far to go.
    pub fn diagonal(&self, anchor: Vec2, p: Vec2) -> (Vec2, Landed) {
        let d = p - anchor;
        // How far along the diagonal puts an edge where it lands, and
        // how far that edge is from there now.
        let x = self.land(&self.xs, p.x).filter(|_| d.x.abs() > SAME).map(|(to, line)| ((to - anchor.x) / d.x, (to - p.x).abs(), Landed { x: line, y: None }));
        let y = self.land(&self.ys, p.y).filter(|_| d.y.abs() > SAME).map(|(to, line)| ((to - anchor.y) / d.y, (to - p.y).abs(), Landed { x: None, y: line }));
        match [x, y].into_iter().flatten().min_by(|a, b| a.1.total_cmp(&b.1)) {
            Some((along, _, landed)) => (anchor + d * along, landed),
            None => (p, Landed::default()),
        }
    }

    /// A box moved whole to `r`: how much further to move it so that
    /// its left, middle or right (and top, middle or bottom) is on a
    /// line, or an edge of it on the grid: the least there is to move,
    /// each way.
    pub fn moved(&self, r: Rect) -> (Vec2, Landed) {
        let axis = |lines: &[f64], lo: f64, hi: f64| -> (f64, Option<f64>) {
            let on_lines = [lo, (lo + hi) / 2.0, hi].into_iter().filter_map(|v| nearest(lines, v, self.reach).map(|line| (line - v, Some(line))));
            let on_grid = [lo, hi].into_iter().filter_map(|v| self.on_grid(v).map(|grid| (grid - v, None)));
            // The lines first: one as near as the grid wins.
            on_lines.chain(on_grid).fold(None, |best: Option<(f64, Option<f64>)>, next| match best {
                Some(best) if next.0.abs() >= best.0.abs() - SAME => Some(best),
                _ => Some(next),
            })
            .unwrap_or((0.0, None))
        };
        let ((dx, x), (dy, y)) = (axis(&self.xs, r.min.x, r.max.x), axis(&self.ys, r.min.y, r.max.y));
        (Vec2::new(dx, dy), Landed { x, y })
    }

    /// The selection's box `was`, dragged by `handle` from `from` to
    /// `to`: where to take the pointer to be instead, so that what the
    /// drag moves lands. The box moved whole lands as `moved` has it
    /// (with Shift, only along the way it goes); a side lands the one
    /// way it goes; a corner lands as a point, or with Shift (the shape
    /// kept) along its diagonal. A turn isn't snapped.
    pub fn handle(&self, was: Rect, handle: Handle, from: Vec2, to: Vec2, keys: Keys) -> (Vec2, Landed) {
        let d = to - from;
        let quad = handles::corners(was);
        match handle {
            Handle::Body => {
                let by = handles::dragged(was, handle, from, to, keys).apply(Vec2::ZERO);
                let (mut more, mut landed) = self.moved(Rect::new(was.min + by, was.max + by));
                if keys.shift && by.x == 0.0 {
                    (more.x, landed.x) = (0.0, None);
                }
                if keys.shift && by.y == 0.0 {
                    (more.y, landed.y) = (0.0, None);
                }
                (from + by + more, landed)
            }
            Handle::Side(i) => {
                let side = handles::sides(quad)[i % 4];
                // The top and bottom go up and down; the others, across.
                if i.is_multiple_of(2) {
                    let (y, landed) = self.edge(false, side.y + d.y);
                    (Vec2::new(to.x, from.y + y - side.y), landed)
                } else {
                    let (x, landed) = self.edge(true, side.x + d.x);
                    (Vec2::new(from.x + x - side.x, to.y), landed)
                }
            }
            Handle::Corner(i) => {
                let corner = quad[i % 4];
                let (at, landed) = if keys.shift {
                    let pivot = if keys.alt { was.center() } else { quad[(i + 2) % 4] };
                    self.diagonal(pivot, handles::dragged(was, handle, from, to, keys).apply(corner))
                } else {
                    self.point(corner + d)
                };
                (from + (at - corner), landed)
            }
            Handle::Turn(_) => (to, Landed::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    /// A 400 × 300 page with a shape at (100, 50), 100 × 80; no grid.
    fn lines() -> Targets {
        let mut t = Targets { reach: 5.0, ..Targets::default() };
        t.add_box(Rect::from_xywh(0.0, 0.0, 400.0, 300.0));
        t.add_box(Rect::from_xywh(100.0, 50.0, 100.0, 80.0));
        t.settle();
        t
    }

    /// The same over a grid of tens.
    fn gridded() -> Targets {
        Targets { step: 10.0, ..lines() }
    }

    #[test]
    fn half_units_come_in_once_they_can_be_told_apart() {
        // An icon filling a screen: halves. A 512-unit page fitted: whole
        // units. And by the display's scale.
        assert_eq!((step_for(1.0, 37.0, 1.0), step_for(1.0, 1.7, 1.0), step_for(1.0, 12.0, 1.0), step_for(1.0, 11.9, 1.0)), (0.5, 1.0, 0.5, 1.0));
        assert_eq!((step_for(1.0, 14.0, 1.25), step_for(0.1, 200.0, 1.0)), (1.0, 0.05));
    }

    #[test]
    fn a_point_goes_to_the_nearest_line_each_way() {
        let t = lines();
        assert_eq!(t.point(v(104.0, 200.0)), (v(100.0, 200.0), Landed { x: Some(100.0), y: None }));
        assert_eq!(t.point(v(197.0, 133.0)), (v(200.0, 130.0), Landed { x: Some(200.0), y: Some(130.0) }));
        assert_eq!(t.point(v(110.0, 200.0)), (v(110.0, 200.0), Landed::default()), "out of reach");
        // Two lines in reach: the nearer.
        assert_eq!(Targets { reach: 60.0, ..lines() }.point(v(152.0, 148.0)).0, v(150.0, 150.0));
        // Lines come in any order, each once.
        assert_eq!((t.xs.as_slice(), t.ys.len()), ([0.0, 100.0, 150.0, 200.0, 400.0].as_slice(), 6));
    }

    #[test]
    fn the_grid_takes_what_no_line_is_nearer_to() {
        let mut t = gridded();
        t.add_point(v(123.0, 77.0));
        // Nothing near: the grid, and nothing to show.
        assert_eq!(t.point(v(254.0, 216.0)), (v(250.0, 220.0), Landed::default()));
        // A line that's on the grid is what's landed on, and shown.
        assert_eq!(t.point(v(101.0, 49.0)), (v(100.0, 50.0), Landed { x: Some(100.0), y: Some(50.0) }));
        // A line off the grid takes what's nearer it than the grid is:
        // 123 between 120 and 130.
        assert_eq!((t.point(v(122.0, 0.0)).0.x, t.point(v(121.0, 0.0)).0.x, t.point(v(126.0, 0.0)).0.x, t.point(v(127.0, 0.0)).0.x), (123.0, 120.0, 123.0, 130.0));
        assert_eq!(t.point(v(124.0, 76.0)).1, Landed { x: Some(123.0), y: Some(77.0) });
        // No grid and no lines: where it is.
        assert_eq!(Targets::default().point(v(1.3, 2.7)), (v(1.3, 2.7), Landed::default()));
    }

    #[test]
    fn a_moved_box_lands_by_an_edge_or_its_middle() {
        let t = lines();
        let boxed = |x: f64, y: f64, x2: f64, y2: f64| Rect::new(v(x, y), v(x2, y2));
        // Its left on the shape's right.
        assert_eq!(t.moved(boxed(203.0, 10.0, 243.0, 30.0)), (v(-3.0, 0.0), Landed { x: Some(200.0), y: None }));
        // Its middle on the page's middle, both ways.
        assert_eq!(t.moved(boxed(182.0, 141.0, 222.0, 161.0)), (v(-2.0, -1.0), Landed { x: Some(200.0), y: Some(150.0) }));
        // Three of its lines in reach: the one with least far to go.
        let (by, landed) = t.moved(boxed(96.0, 10.0, 203.0, 30.0));
        assert_eq!((by.x, landed.x), (0.5, Some(150.0)));
        assert_eq!(t.moved(boxed(30.0, 20.0, 60.0, 40.0)), (v(0.0, 0.0), Landed::default()));
        // Over a grid, an edge goes to it: the left here (2 to go), the
        // bottom there (1 to go, where the top has 4). Never the middle.
        let g = gridded();
        assert_eq!(g.moved(boxed(32.0, 224.0, 57.0, 239.0)), (v(-2.0, 1.0), Landed::default()));
        assert_eq!(g.moved(boxed(31.0, 220.0, 44.0, 240.0)).0.x, -1.0);
        // A line as near as the grid is what it's on.
        assert_eq!(g.moved(boxed(203.0, 220.0, 243.0, 240.0)), (v(-3.0, 0.0), Landed { x: Some(200.0), y: None }));
    }

    #[test]
    fn a_side_lands_one_way_and_a_corner_along_its_diagonal() {
        let t = lines();
        assert_eq!(t.edge(true, 397.0), (400.0, Landed { x: Some(400.0), y: None }));
        assert_eq!(t.edge(false, 152.0), (150.0, Landed { x: None, y: Some(150.0) }));
        assert_eq!(t.edge(true, 250.0), (250.0, Landed::default()), "nothing near: where it's dragged");
        assert_eq!(gridded().edge(true, 254.0), (250.0, Landed::default()));
        // A corner going out from (100, 50) along a diagonal of 5 to 4,
        // let go just short of (400, 290): its right edge is near the
        // page's, so it goes on along the diagonal to there.
        let anchor = v(100.0, 50.0);
        let out = |along: f64| anchor + v(300.0, 240.0) * along;
        let (to, landed) = t.diagonal(anchor, out(0.99));
        assert!((to - out(1.0)).length() < 1e-9 && landed == Landed { x: Some(400.0), y: None }, "{to:?} {landed:?}");
        // The box keeps its shape: the corner stays on the diagonal.
        let (to, _) = t.diagonal(anchor, out(0.335));
        assert!(((to.x - 100.0) / 300.0 - (to.y - 50.0) / 240.0).abs() < 1e-9 && (to.x == 200.0 || to.y == 130.0), "{to:?}");
        assert_eq!(t.diagonal(anchor, out(0.6)), (out(0.6), Landed::default()));
        // Over a grid, one of its edges is on it, and it's on the
        // diagonal still.
        let (to, _) = gridded().diagonal(anchor, out(0.6));
        assert!(((to.x - 100.0) / 300.0 - (to.y - 50.0) / 240.0).abs() < 1e-9 && ((to.x / 10.0).fract().abs() < 1e-9 || (to.y / 10.0).fract().abs() < 1e-9), "{to:?}");
    }

    #[test]
    fn each_handle_of_the_box_lands_what_it_moves() {
        let g = gridded();
        let was = Rect::from_xywh(213.0, 163.0, 40.0, 20.0);
        let (from, keys) = (v(230.0, 170.0), Keys::default());
        let lands = |handle: Handle, to: Vec2, keys: Keys| {
            let (to, landed) = g.handle(was, handle, from, to, keys);
            (handles::dragged(was, handle, from, to, keys).bounds(&was), landed)
        };
        // Moved whole: an edge each way on the grid (the left 3 back,
        // the top 3 back).
        assert_eq!(lands(Handle::Body, v(230.4, 169.8), keys).0, Rect::from_xywh(210.0, 160.0, 40.0, 20.0));
        // With Shift it goes one way, and lands that way only.
        let (b, _) = lands(Handle::Body, v(261.0, 172.0), Keys { shift: true, alt: false });
        assert_eq!((b.min.x, b.min.y), (240.0, 163.0));
        // The right side: across, to the grid; nothing else moves.
        assert_eq!(lands(Handle::Side(1), v(241.0, 199.0), keys).0, Rect::new(v(213.0, 163.0), v(260.0, 183.0)));
        // The bottom, to the shape's line... there's none near: the grid.
        assert_eq!(lands(Handle::Side(2), v(230.0, 176.0), keys).0.max.y, 190.0);
        // The bottom right corner: a point, each way by itself.
        assert_eq!(lands(Handle::Corner(2), v(239.0, 174.0), keys).0, Rect::new(v(213.0, 163.0), v(260.0, 190.0)));
        // With Shift the shape is kept: 2 to 1, one edge landed.
        let (b, _) = lands(Handle::Corner(2), v(245.0, 171.0), Keys { shift: true, alt: false });
        assert!((b.width() / b.height() - 2.0).abs() < 1e-9 && (b.max.x == 270.0 || (b.max.y / 10.0).fract() == 0.0) && b.min == was.min, "{b:?}");
        // A turn goes where it's dragged.
        assert_eq!(g.handle(was, Handle::Turn(0), from, v(244.4, 171.2), keys), (v(244.4, 171.2), Landed::default()));
        // The top left corner onto the page's middle: shown.
        let (b, landed) = lands(Handle::Corner(0), v(218.0, 157.5), keys);
        assert_eq!((b.min, landed), (v(200.0, 150.0), Landed { x: Some(200.0), y: Some(150.0) }));
    }
}
