//! The canvas's picture (ARCHITECTURE §8): the drawing as tiles, each
//! drawn by Ink's own renderer on the job pool and shown as an LUI2
//! image, pixel for pixel. A frame never waits for a tile.
//!
//! A tab's tiles are a [`Level`]: the drawing in one state, at one
//! zoom. When either changes, a new level is drawn behind the one that
//! shows, which stays up (stretched, if the zoom moved) until every
//! tile in view has landed; then they change places. So the canvas
//! never goes blank for a zoom or an edit, and never shows half of one
//! state beside half of another.
//!
//! The pool lays the drawing out once for a level ([`Plan`]) and draws
//! its tiles from that. Tiles nearest the middle of the view go first,
//! and only as many at once as the pool has threads and a budget of
//! pixels allows: what's still waiting when the view moves on is never
//! begun.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use ink_core::{DocId, Document};
use ink_geom::Affine;
use ink_render::Plan;
use lntrn_app::Waker;
use lntrn_app::lntrn_render::{Gpu, ImageId, Images};
use lntrn_core::jobs::Pool;
use lntrn_image::Image;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{ImageHandle, Ui};

/// How many tiles past the canvas's edges are drawn ahead of a pan.
const AHEAD: i32 = 1;
/// Tiles further than this past the edges are let go.
const KEPT: i32 = 4;
/// The px being drawn at once, margins and all, across every tile on
/// its way: a blur that reaches far makes a tile many times its size
/// to draw, and a pool full of those would be gigabytes.
const BUDGET_PX: u64 = 24 << 20;
/// How many tabs keep their tiles: the ones shown most lately.
const SHEETS: usize = 4;

/// A tile's side for a drawing whose shadows reach `reach` px past it:
/// bigger where the room around a tile would outweigh the tile.
fn side_for(reach: u32) -> u32 {
    match reach {
        0..=64 => 256,
        65..=256 => 512,
        _ => 1024,
    }
}

/// Where tiles' pictures are kept: the GPU's images, for the window.
pub trait Store {
    fn add(&mut self, image: &Image) -> ImageHandle;
    fn remove(&mut self, id: ImageId);
}

/// LUI2's images, on the window's GPU.
pub struct OnGpu<'a>(pub &'a Gpu, pub &'a mut Images);

impl Store for OnGpu<'_> {
    fn add(&mut self, image: &Image) -> ImageHandle {
        self.1.add(self.0, image)
    }

    fn remove(&mut self, id: ImageId) {
        self.1.remove(id);
    }
}

/// A tile's place: across and down, in tiles, from the page's corner.
type At = (i32, i32);

enum Tile {
    /// On the pool.
    Asked,
    /// Drawn, and nothing's there.
    Clear,
    Drawn(ImageHandle),
}

/// A level's drawing laid out: what its tiles are drawn from.
struct Laid {
    plan: Arc<Plan>,
    /// A tile's side, px.
    side: u32,
    /// The tiles anything is painted in: first and last, each way.
    painted: Option<(At, At)>,
}

struct Level {
    id: u64,
    /// Window px per px of the page.
    zoom: f64,
    /// The state of the drawing it shows (`History::stamp`).
    stamp: u64,
    /// None until the pool has laid the drawing out.
    laid: Option<Laid>,
    tiles: HashMap<At, Tile>,
    /// Off once the level is let go: what's still queued for it on the
    /// pool isn't drawn.
    alive: Arc<AtomicBool>,
}

impl Level {
    /// Every picture it holds, to be freed.
    fn images(&self) -> impl Iterator<Item = ImageId> + '_ {
        self.tiles.values().filter_map(|t| if let Tile::Drawn(h) = t { Some(h.id) } else { None })
    }
}

impl Drop for Level {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct Sheet {
    /// What shows.
    front: Option<Level>,
    /// What's being drawn to take its place: another zoom, or the
    /// drawing after an edit.
    back: Option<Level>,
    /// The drawing as it was at a stamp, shared with the pool.
    snapshot: Option<(u64, Arc<Document>)>,
    /// When it was last the one in view.
    shown: u64,
}

enum Done {
    Laid { doc: DocId, level: u64, plan: Arc<Plan> },
    /// A tile came back: its picture, or none (clear, or let go).
    Tile { doc: DocId, level: u64, at: At, cost: u64, image: Option<Image> },
}

pub struct Tiles {
    sheets: HashMap<DocId, Sheet>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    waker: Option<Waker>,
    /// Tiles on the pool, and the px they're drawing between them.
    flying: usize,
    flying_px: u64,
    /// Pictures let go, to free when the GPU is next in reach.
    dead: Vec<ImageId>,
    next_level: u64,
    clock: u64,
}

impl Default for Tiles {
    fn default() -> Tiles {
        let (tx, rx) = channel();
        Tiles { sheets: HashMap::new(), tx, rx, waker: None, flying: 0, flying_px: 0, dead: Vec::new(), next_level: 1, clock: 0 }
    }
}

/// The tiles `side` px square that `r` (the picture's px) touches:
/// first and last, each way.
fn span(r: Rect, side: u32) -> (At, At) {
    let s = side as f64;
    let first = |v: f64| (v / s).floor() as i32;
    // A tile is touched where px of it are: up to the edge, not on it.
    let last = |lo: f64, hi: f64| first(lo).max((hi / s).ceil() as i32 - 1);
    ((first(r.min.x), first(r.min.y)), (last(r.min.x, r.max.x), last(r.min.y, r.max.y)))
}

fn within(at: At, ((i0, j0), (i1, j1)): (At, At), by: i32) -> bool {
    at.0 >= i0 - by && at.0 <= i1 + by && at.1 >= j0 - by && at.1 <= j1 + by
}

impl Tiles {
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    fn sender(&self) -> impl Fn(Done) + Send + 'static {
        let (tx, waker) = (self.tx.clone(), self.waker.clone());
        move |done| {
            // Only fails once the window is gone: nothing to tell then.
            let _ = tx.send(done);
            if let Some(w) = &waker {
                w.wake();
            }
        }
    }

    /// A tab closed: its tiles go.
    pub fn forget(&mut self, doc: DocId) {
        if let Some(sheet) = self.sheets.remove(&doc) {
            self.dead.extend(sheet.front.iter().chain(&sheet.back).flat_map(Level::images));
        }
    }

    /// A new level of `doc`, its layout begun on the pool.
    fn begin(&mut self, doc: DocId, zoom: f64, stamp: u64, drawing: Arc<Document>) -> Level {
        let id = self.next_level;
        self.next_level += 1;
        let alive = Arc::new(AtomicBool::new(true));
        let (send, live) = (self.sender(), alive.clone());
        Pool::global().spawn(move || {
            if live.load(Ordering::Relaxed) {
                send(Done::Laid { doc, level: id, plan: Arc::new(Plan::new(&drawing, &Affine::scale(zoom, zoom), false)) });
            }
        });
        Level { id, zoom, stamp, laid: None, tiles: HashMap::new(), alive }
    }

    /// Have `doc` (the tab in view; `drawing` in the state `stamp`)
    /// shown at `zoom`, where `view` is the part of its picture (px
    /// from the page's corner) that the canvas shows: begin what isn't
    /// drawn yet, bring forward what's ready, let go of what's left
    /// behind. Once a frame.
    pub fn want(&mut self, doc: DocId, drawing: &Document, stamp: u64, zoom: f64, view: Rect) {
        self.clock += 1;
        let mut sheet = self.sheets.remove(&doc).unwrap_or_default();
        sheet.shown = self.clock;
        let is = |level: &Option<Level>| level.as_ref().is_some_and(|l| l.zoom == zoom && l.stamp == stamp);
        if is(&sheet.front) {
            // Back where it was (a zoom undone before it landed).
            self.dead.extend(sheet.back.take().iter().flat_map(Level::images));
        } else if !is(&sheet.back) {
            if sheet.snapshot.as_ref().is_none_or(|(at, _)| *at != stamp) {
                sheet.snapshot = Some((stamp, Arc::new(drawing.clone())));
            }
            let level = sheet.snapshot.as_ref().map(|(_, drawing)| self.begin(doc, zoom, stamp, drawing.clone()));
            self.dead.extend(std::mem::replace(&mut sheet.back, level).iter().flat_map(Level::images));
        }
        let behind = sheet.back.is_some();
        if let Some(level) = if behind { sheet.back.as_mut() } else { sheet.front.as_mut() }
            && let Some(laid) = &level.laid
        {
            let seen = span(view, laid.side);
            self.ask(doc, level.id, &level.alive, laid, &mut level.tiles, seen);
            // Far behind a pan, a tile is let go (one on the pool stays
            // until it's back).
            let Tiles { dead, .. } = self;
            level.tiles.retain(|&at, tile| {
                let keep = within(at, seen, KEPT) || matches!(tile, Tile::Asked);
                if let (false, Tile::Drawn(h)) = (keep, &*tile) {
                    dead.push(h.id);
                }
                keep
            });
            // All of the view is drawn: it's what shows now.
            let ready = (seen.0.0..=seen.1.0).all(|i| (seen.0.1..=seen.1.1).all(|j| matches!(level.tiles.get(&(i, j)), Some(Tile::Clear | Tile::Drawn(_)))));
            if behind && ready {
                let old = std::mem::replace(&mut sheet.front, sheet.back.take());
                self.dead.extend(old.iter().flat_map(Level::images));
            }
        }
        self.sheets.insert(doc, sheet);
        // The tabs longest out of view give their tiles up.
        while self.sheets.len() > SHEETS {
            let Some(oldest) = self.sheets.iter().min_by_key(|(_, s)| s.shown).map(|(&id, _)| id) else { break };
            self.forget(oldest);
        }
    }

    /// Send for the tiles of `seen` (and a ring around it) that aren't
    /// there yet, the nearest its middle first, while there's room on
    /// the pool. One that nothing is painted in is clear at once.
    fn ask(&mut self, doc: DocId, level: u64, alive: &Arc<AtomicBool>, laid: &Laid, tiles: &mut HashMap<At, Tile>, seen: (At, At)) {
        let ((i0, j0), (i1, j1)) = seen;
        let middle = ((i0 + i1) as f64 / 2.0, (j0 + j1) as f64 / 2.0);
        let mut missing: Vec<At> = (i0 - AHEAD..=i1 + AHEAD).flat_map(|i| (j0 - AHEAD..=j1 + AHEAD).map(move |j| (i, j))).filter(|at| !tiles.contains_key(at)).collect();
        let far = |&(i, j): &At| (i as f64 - middle.0).powi(2) + (j as f64 - middle.1).powi(2);
        missing.sort_by(|a, b| far(a).total_cmp(&far(b)));
        let frame = laid.side as u64 + 2 * laid.plan.reach() as u64;
        let cost = frame * frame;
        // A thread is kept free for the next level's layout.
        let threads = Pool::global().threads().saturating_sub(1).max(1);
        for at in missing {
            if !laid.painted.is_some_and(|painted| within(at, painted, 0)) {
                tiles.insert(at, Tile::Clear);
                continue;
            }
            if self.flying >= threads || (self.flying > 0 && self.flying_px + cost > BUDGET_PX) {
                break;
            }
            self.flying += 1;
            self.flying_px += cost;
            tiles.insert(at, Tile::Asked);
            let (send, live, plan, side) = (self.sender(), alive.clone(), laid.plan.clone(), laid.side);
            Pool::global().spawn(move || {
                // Let go while it waited its turn: not drawn.
                let image = live.load(Ordering::Relaxed).then(|| plan.part(at.0 as i64 * side as i64, at.1 as i64 * side as i64, side, side).ok()).flatten();
                send(Done::Tile { doc, level, at, cost, image });
            });
        }
    }

    /// Take in what the pool finished: layouts, and tiles (their
    /// pictures put in `store`, here, where the GPU is in reach).
    /// Whether anything landed, so the frame is built again to show it.
    pub fn finished(&mut self, store: &mut impl Store) -> bool {
        let mut landed = false;
        let done: Vec<Done> = self.rx.try_iter().collect();
        for done in done {
            let (doc, id) = match &done {
                Done::Laid { doc, level, .. } | Done::Tile { doc, level, .. } => (*doc, *level),
            };
            if let Done::Tile { cost, .. } = &done {
                self.flying = self.flying.saturating_sub(1);
                self.flying_px = self.flying_px.saturating_sub(*cost);
                // Room on the pool for the next: the frame asks.
                landed = true;
            }
            let level = self.sheets.get_mut(&doc).and_then(|s| s.front.iter_mut().chain(&mut s.back).find(|l| l.id == id));
            let Some(level) = level else { continue };
            match done {
                Done::Laid { plan, .. } => {
                    let side = side_for(plan.reach());
                    level.laid = Some(Laid { painted: plan.bounds().map(|b| span(b, side)), plan, side });
                }
                Done::Tile { at, image, .. } => {
                    let inked = image.filter(|i| i.rgba.chunks_exact(4).any(|px| px[3] != 0));
                    level.tiles.insert(at, inked.map_or(Tile::Clear, |i| Tile::Drawn(store.add(&i))));
                }
            }
            landed = true;
        }
        for id in self.dead.drain(..) {
            store.remove(id);
        }
        landed
    }

    /// What shows of `doc`: the zoom its tiles were drawn for, and how
    /// many of them hold a picture.
    #[cfg(test)]
    pub(crate) fn showing(&self, doc: DocId) -> Option<(f64, usize)> {
        let level = self.sheets.get(&doc)?.front.as_ref()?;
        Some((level.zoom, level.images().count()))
    }

    /// Draw what shows of `doc` in `area`: its tiles from `corner` (the
    /// page's, window px), at `zoom`. Tiles made for this zoom land
    /// pixel for pixel; ones made for another are stretched until its
    /// own are ready.
    pub fn draw(&self, ui: &mut Ui, doc: DocId, area: Rect, corner: Vec2, zoom: f64) {
        let Some(level) = self.sheets.get(&doc).and_then(|s| s.front.as_ref()) else { return };
        let Some(laid) = &level.laid else { return };
        let side = laid.side as f64 * zoom / level.zoom;
        // Edges on whole pixels, shared by neighbours: no seam between
        // two stretched tiles.
        let edge = |from: f64, n: i32| (from + n as f64 * side).round();
        ui.draw.push_clip(area);
        for (&(i, j), tile) in &level.tiles {
            let Tile::Drawn(image) = tile else { continue };
            let r = Rect::new(Vec2::new(edge(corner.x, i), edge(corner.y, j)), Vec2::new(edge(corner.x, i + 1), edge(corner.y, j + 1)));
            if r.intersects(&area) {
                ui.draw.image(r, *image, 0.0, Color::WHITE);
            }
        }
        ui.draw.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_view_touches_the_tiles_it_has_pixels_of() {
        let r = |x0: f64, y0: f64, x1: f64, y1: f64| Rect::new(Vec2::new(x0, y0), Vec2::new(x1, y1));
        assert_eq!(span(r(0.0, 0.0, 256.0, 256.0), 256), ((0, 0), (0, 0)), "up to the next tile's edge, not onto it");
        assert_eq!(span(r(0.0, 0.0, 257.0, 256.0), 256), ((0, 0), (1, 0)));
        assert_eq!(span(r(-1.0, -300.0, 600.0, 10.0), 256), ((-1, -2), (2, 0)));
        assert_eq!(span(r(100.0, 100.0, 100.0, 100.0), 256), ((0, 0), (0, 0)), "an empty view still has a place");
        assert_eq!(span(r(-700.5, 0.0, -600.0, 1.0), 512), ((-2, 0), (-2, 0)));
        assert!(within((3, -1), ((0, 0), (2, 2)), 1) && !within((4, -1), ((0, 0), (2, 2)), 1) && !within((3, -1), ((0, 0), (2, 2)), 0));
    }

    #[test]
    fn tiles_grow_with_how_far_shadows_reach() {
        assert_eq!((side_for(0), side_for(64), side_for(65), side_for(256), side_for(257), side_for(ink_render::MAX_REACH)), (256, 256, 512, 512, 1024, 1024));
        // The worst tile there can be fits the budget on its own, with
        // room for a few beside it.
        let worst = (1024 + 2 * ink_render::MAX_REACH as u64).pow(2);
        assert!(worst * 2 <= BUDGET_PX, "{worst} px");
    }
}
