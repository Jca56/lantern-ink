//! Exact coverage: the true share of each pixel that lies inside a
//! shape, by either fill rule, however its outlines cross and overlap.
//!
//! A pixel row is cut into slices at every height where an edge starts,
//! ends or crosses another. Within a slice no two edges cross, so, read
//! left to right, the edges where the winding goes from outside to
//! inside and back bound trapezoids that never overlap. Each trapezoid's
//! two sides add the signed area they sweep to the cells they cross (the
//! accumulation buffer of font-rs, after FreeType and stb_truetype 2),
//! and a running sum along the row is then each pixel's coverage.
//!
//! Sweeping every edge of the shape as it comes, as LS3's rasterizer
//! does, counts the winding number rather than what's inside: where two
//! pieces of a stroke overlap in part of a pixel, that pixel gets both
//! their areas. Working out the slices first is what makes it exact.
//!
//! A row depends on nothing but itself, so its coverage is the same
//! whichever band of rows it's worked out in.

use ink_geom::{FillRule, Vec2};

/// One edge in the frame, `x` clipped to it, going down (`dir` says
/// which way it really went).
#[derive(Clone, Copy, Debug)]
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// x per y.
    slope: f64,
    dir: i32,
}

impl Edge {
    /// Where it is at height `y`, kept to its own ends.
    fn x_at(&self, y: f64) -> f64 {
        (self.x0 + (y - self.y0) * self.slope).clamp(self.x0.min(self.x1), self.x0.max(self.x1))
    }
}

/// Polygons ready to draw: their edges from the top down, and the rows
/// and columns they touch.
#[derive(Clone, Debug)]
pub(crate) struct Shape {
    edges: Vec<Edge>,
    rows: (f64, f64),
    /// The first column any edge touches, and one past the last cell one
    /// writes to (coverage returns to zero after it).
    cols: (usize, usize),
    rule: FillRule,
}

impl Shape {
    /// `polys` (closed polygons, each implicitly closed) in the px of a
    /// frame `w` × `h`. `None` when they touch none of it.
    pub fn new(polys: &[Vec<Vec2>], rule: FillRule, w: usize, h: usize) -> Option<Shape> {
        let drawn = |poly: &&Vec<Vec2>| poly.len() >= 3 && poly.iter().all(|p| p.is_finite());
        // Wholly to one side of the frame, they cover none of it: a
        // part of a big picture is spared the shapes beside it.
        let (left, right) = polys.iter().filter(drawn).flatten().fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
        if right <= 0.0 || left >= w as f64 {
            return None;
        }
        let mut edges = Vec::new();
        for poly in polys.iter().filter(drawn) {
            for (i, &a) in poly.iter().enumerate() {
                clip_x(a, poly[(i + 1) % poly.len()], w as f64, &mut edges);
            }
        }
        edges.retain(|e| e.y1 > 0.0 && e.y0 < h as f64);
        if edges.is_empty() {
            return None;
        }
        edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));
        let rows = (edges[0].y0, edges.iter().fold(f64::MIN, |hi, e| hi.max(e.y1)));
        let x_min = edges.iter().fold(f64::MAX, |m, e| m.min(e.x0).min(e.x1));
        let x_max = edges.iter().fold(f64::MIN, |m, e| m.max(e.x0).max(e.x1));
        let cols = (x_min.floor().max(0.0) as usize, (x_max.ceil() as usize + 2).min(w + 2));
        Some(Shape { edges, rows, cols, rule })
    }
}

/// `a → b` onto `out`, `x` kept to `0..=w`: a stretch left of the frame
/// becomes a vertical at 0 (what it encloses reaches every column, as it
/// should), one right of it a vertical at `w` (reaching none).
fn clip_x(a: Vec2, b: Vec2, w: f64, out: &mut Vec<Edge>) {
    if a.y == b.y {
        return;
    }
    let mut cuts = [a, b, a, a];
    let mut n = 2;
    for x in [0.0, w] {
        if (a.x < x) != (b.x < x) && a.x != x && b.x != x {
            let t = (x - a.x) / (b.x - a.x);
            cuts[n] = Vec2::new(x, a.y + (b.y - a.y) * t);
            n += 1;
        }
    }
    // Order the pieces along the edge, by y (it isn't level).
    let down = a.y < b.y;
    cuts[..n].sort_by(|p, q| if down { p.y.total_cmp(&q.y) } else { q.y.total_cmp(&p.y) });
    for pair in cuts[..n].windows(2) {
        let (p, q) = (pair[0], pair[1]);
        if p.y == q.y {
            continue;
        }
        let (px, qx) = (p.x.clamp(0.0, w), q.x.clamp(0.0, w));
        let (top, bottom, dir) = if p.y < q.y { ((px, p.y), (qx, q.y), 1) } else { ((qx, q.y), (px, p.y), -1) };
        out.push(Edge { x0: top.0, y0: top.1, x1: bottom.0, y1: bottom.1, slope: (bottom.0 - top.0) / (bottom.1 - top.1), dir });
    }
}

/// Add to a row's cells the area a trapezoid's side sweeps: the side
/// runs from `xa` at the top of a slice of the row to `xb` at its
/// bottom, and `d` is the slice's height, positive for the trapezoid's
/// left side and negative for its right. (font-rs's `draw_line`, for one
/// row.) A running sum along the cells is then the row's coverage.
fn sweep(cells: &mut [f64], xa: f64, xb: f64, d: f64) {
    let (x0, x1) = if xa < xb { (xa, xb) } else { (xb, xa) };
    let (x0floor, x1ceil) = (x0.floor(), x1.ceil());
    let (x0i, x1i) = (x0floor as usize, x1ceil as usize);
    if x1i <= x0i + 1 {
        // Within one column: split its area between it and the next.
        let xmf = 0.5 * (xa + xb) - x0floor;
        cells[x0i] += d - d * xmf;
        cells[x0i + 1] += d * xmf;
        return;
    }
    let s = 1.0 / (x1 - x0);
    let x0f = x0 - x0floor;
    let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
    let x1f = x1 - x1ceil + 1.0;
    let am = 0.5 * s * x1f * x1f;
    cells[x0i] += d * a0;
    if x1i == x0i + 2 {
        cells[x0i + 1] += d * (1.0 - a0 - am);
    } else {
        let a1 = s * (1.5 - x0f);
        cells[x0i + 1] += d * (a1 - a0);
        for cell in &mut cells[x0i + 2..x1i - 1] {
            *cell += d * s;
        }
        let a2 = a1 + (x1i - x0i - 3) as f64 * s;
        cells[x1i - 1] += d * (1.0 - a2 - am);
    }
    cells[x1i] += d * am;
}

/// An edge within a slice of a row: where it is at the slice's top and
/// at its bottom, and which way it winds.
type Crossing = (f64, f64, i32);

/// What working out a row's coverage needs, kept between rows.
#[derive(Default)]
pub(crate) struct Scratch {
    /// The areas swept in the row being worked out: `w + 2` cells.
    cells: Vec<f64>,
    /// The row's coverage.
    row: Vec<f32>,
    /// The edges that touch the row.
    active: Vec<usize>,
    /// The heights the row is cut at.
    cuts: Vec<f64>,
    slice: Vec<Crossing>,
}

/// The most times one slice is cut again at edges crossing; past it the
/// rest is taken as it lies (a hostile scribble, not a drawing).
const MAX_CROSSINGS: usize = 4096;
/// A crossing nearer a slice's top or bottom than this share of it isn't
/// cut at.
const NEAR: f64 = 1e-9;

impl Scratch {
    /// Room for rows `w` px wide.
    pub fn new(w: usize) -> Scratch {
        Scratch { cells: vec![0.0; w + 2], row: vec![0.0; w], ..Scratch::default() }
    }

    /// Sweep the trapezoids of one slice (`ya..yb`, within one row) in
    /// which no edges cross: `self.slice`, in order from the left.
    fn trapezoids(&mut self, d: f64, rule: FillRule) {
        let mut wind = 0i32;
        let inside = |wind: i32| match rule {
            FillRule::NonZero => wind != 0,
            FillRule::EvenOdd => wind & 1 == 1,
        };
        for &(xa, xb, dir) in &self.slice {
            let was = inside(wind);
            wind += if rule == FillRule::NonZero { dir } else { 1 };
            match (was, inside(wind)) {
                (false, true) => sweep(&mut self.cells, xa, xb, d),
                (true, false) => sweep(&mut self.cells, xa, xb, -d),
                _ => {}
            }
        }
    }

    /// Sweep the slice `ya..yb` of a row: `self.slice` holds the edges
    /// that run right through it. Where two cross, it's cut there and
    /// each part swept on its own.
    fn sweep_slice(&mut self, mut ya: f64, yb: f64, rule: FillRule) {
        for _ in 0..MAX_CROSSINGS {
            self.slice.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
            // Neighbours that have changed places by the bottom cross on
            // the way; the first crossing down is between two of them.
            let first = self.slice.windows(2).filter(|pair| pair[0].1 > pair[1].1).map(|pair| (pair[1].0 - pair[0].0) / ((pair[1].0 - pair[0].0) + (pair[0].1 - pair[1].1))).fold(None, |first: Option<f64>, t| Some(first.map_or(t, |f| f.min(t))));
            let Some(t) = first.filter(|t| *t > NEAR && *t < 1.0 - NEAR) else { break };
            // Down to the crossing nothing crosses.
            let ym = ya + (yb - ya) * t;
            let whole: Vec<Crossing> = std::mem::take(&mut self.slice);
            self.slice.extend(whole.iter().map(|&(xa, xb, dir)| (xa, xa + (xb - xa) * t, dir)));
            self.trapezoids(ym - ya, rule);
            self.slice.clear();
            self.slice.extend(whole.iter().map(|&(xa, xb, dir)| (xa + (xb - xa) * t, xb, dir)));
            ya = ym;
        }
        // Edges that cross within a hair of the slice's ends (or past
        // the limit) are taken by where their middles lie.
        self.slice.sort_by(|p, q| (p.0 + p.1).total_cmp(&(q.0 + q.1)));
        self.trapezoids(yb - ya, rule);
    }
}

/// Walk the rows of the band `shape` touches: `each(row, first, coverage)`
/// gets the coverage (0..1) of the pixels from column `first` on, `row`
/// counted from the band's top.
pub(crate) fn cover(scratch: &mut Scratch, w: usize, y0: usize, h: usize, shape: &Shape, mut each: impl FnMut(usize, usize, &[f32])) {
    let (top, bottom) = (y0 as f64, (y0 + h) as f64);
    if shape.rows.1 <= top || shape.rows.0 >= bottom {
        return;
    }
    let edges = &shape.edges;
    let (first, last) = (shape.rows.0.floor().max(top) as usize, shape.rows.1.ceil().min(bottom) as usize);
    let (c0, c1) = shape.cols;
    let end = c1.min(w);
    scratch.active.clear();
    let mut next = 0;
    for y in first..last {
        let (row_top, row_bottom) = (y as f64, (y + 1) as f64);
        while next < edges.len() && edges[next].y0 < row_bottom {
            scratch.active.push(next);
            next += 1;
        }
        scratch.active.retain(|&e| edges[e].y1 > row_top);
        if scratch.active.is_empty() {
            continue;
        }
        // The heights where an edge starts or ends within the row.
        scratch.cuts.clear();
        scratch.cuts.extend([row_top, row_bottom]);
        for &e in &scratch.active {
            scratch.cuts.extend([edges[e].y0, edges[e].y1].into_iter().filter(|y| *y > row_top && *y < row_bottom));
        }
        scratch.cuts.sort_by(f64::total_cmp);
        scratch.cuts.dedup();
        for k in 0..scratch.cuts.len() - 1 {
            let (ya, yb) = (scratch.cuts[k], scratch.cuts[k + 1]);
            scratch.slice.clear();
            for &e in &scratch.active {
                let edge = &edges[e];
                if edge.y0 <= ya && edge.y1 >= yb {
                    scratch.slice.push((edge.x_at(ya), edge.x_at(yb), edge.dir));
                }
            }
            scratch.sweep_slice(ya, yb, shape.rule);
        }
        let mut sum = 0f64;
        for (i, cell) in scratch.cells[c0..c1].iter_mut().enumerate() {
            sum += *cell;
            *cell = 0.0;
            if c0 + i < w {
                scratch.row[c0 + i] = sum.clamp(0.0, 1.0) as f32;
            }
        }
        if c0 < end {
            each(y - y0, c0, &scratch.row[c0..end]);
        }
    }
}
