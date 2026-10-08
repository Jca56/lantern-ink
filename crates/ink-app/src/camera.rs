//! How a drawing sits in the canvas area (LS3's camera, in a drawing's
//! terms). Zoom is window px per px of the page, 1.5 %–25 600 %: an
//! icon is worked on many times its size. A fitted camera keeps fitting
//! as the area changes size, until it's panned or zoomed. Each tab has
//! its own.
//!
//! The page's corner always shows on a whole window pixel, so the tiles
//! the canvas is drawn in (`tiles.rs`) land pixel for pixel.

use lntrn_math::{Rect, Vec2};

pub const MIN_ZOOM: f64 = 1.0 / 64.0;
pub const MAX_ZOOM: f64 = 256.0;
/// A fit leaves a tenth of the area around the page.
const FIT_PAD: f64 = 0.9;
/// The View menu's and the status bar's zoom steps.
pub const ZOOM_STEP: f64 = 1.1;
/// The Zoom tool's: a click doubles, or halves.
pub const CLICK_STEP: f64 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub zoom: f64,
    /// Where the page's top-left has been slid to, window px from the
    /// area's. It's shown at the nearest whole pixel ([`Camera::corner`]):
    /// a slow drag adds up here until it's a pixel's worth.
    origin: Vec2,
    /// Still fitted: follows the area's size until the view is touched.
    pub fitted: bool,
}

impl Camera {
    /// The page (`w` × `h` px) centred in `area` at 90 % of the room.
    pub fn fit(area: Rect, page: Vec2) -> Camera {
        let (w, h) = (page.x.max(1e-6), page.y.max(1e-6));
        let zoom = (area.width() * FIT_PAD / w).min(area.height() * FIT_PAD / h).clamp(MIN_ZOOM, MAX_ZOOM);
        Camera { zoom, origin: Vec2::new((area.width() - w * zoom) / 2.0, (area.height() - h * zoom) / 2.0), fitted: true }
    }

    /// At `zoom` with the page centred in `area`: its actual size, at 1.
    pub fn centred(area: Rect, page: Vec2, zoom: f64) -> Camera {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        Camera { zoom, origin: Vec2::new((area.width() - page.x * zoom) / 2.0, (area.height() - page.y * zoom) / 2.0), fitted: false }
    }

    /// Refit if still fitted: the window or the panel changed size.
    pub fn follow(&mut self, area: Rect, page: Vec2) {
        if self.fitted {
            *self = Camera::fit(area, page);
        }
    }

    /// Where the page's top-left shows, window px from the area's: a
    /// whole pixel.
    pub fn corner(&self) -> Vec2 {
        Vec2::new(self.origin.x.round(), self.origin.y.round())
    }

    /// Window px (in `area`) to the page's px.
    pub fn page_at(&self, area: Rect, p: Vec2) -> Vec2 {
        (p - area.min - self.corner()) / self.zoom
    }

    /// The page's px to window px (in `area`).
    pub fn window_at(&self, area: Rect, p: Vec2) -> Vec2 {
        area.min + self.corner() + p * self.zoom
    }

    /// Slide the page by `d` window px.
    pub fn pan(&mut self, d: Vec2) {
        if d != Vec2::ZERO {
            self.origin += d;
            self.fitted = false;
        }
    }

    /// Zoom by `factor`, keeping the point of the page under `pivot`
    /// (window px) where it is.
    pub fn zoom_about(&mut self, area: Rect, factor: f64, pivot: Vec2) {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        if zoom == self.zoom || !zoom.is_finite() {
            return;
        }
        let under = (pivot - area.min - self.origin) / self.zoom;
        self.zoom = zoom;
        self.origin = pivot - area.min - under * zoom;
        self.fitted = false;
    }

    /// One step in or out about the area's centre.
    pub fn step(&mut self, area: Rect, steps: i32) {
        self.zoom_about(area, ZOOM_STEP.powi(steps), area.center());
    }
}

/// The zoom factor for a Ctrl+wheel turn (LS3's: 0.3 per notch, or
/// 1/200 per logical pixel from a touchpad, at most ×1.5 per turn).
/// `notches` or `pixels` is the wheel's y, away from the user positive.
pub fn wheel_zoom(notches: f64, logical_px: f64) -> f64 {
    let amount = notches * 0.3 + logical_px / 200.0;
    let step = 1.0 + amount.abs().min(0.5);
    if amount >= 0.0 { step } else { 1.0 / step }
}

/// The zoom factor for a touchpad pinch whose fingers went `spread`
/// times as far apart: twice as fast as the fingers go, the same
/// pinching in as out, and at most ×1.5 a frame, as the wheel's.
pub fn pinch_zoom(spread: f64) -> f64 {
    if !(spread.is_finite() && spread > 0.0) {
        return 1.0;
    }
    (spread * spread).clamp(1.0 / 1.5, 1.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::from_min_size(Vec2::new(100.0, 50.0), Vec2::new(1000.0, 600.0))
    }

    #[test]
    fn a_fit_centres_the_page_at_ninety_percent() {
        let c = Camera::fit(area(), Vec2::new(1600.0, 1080.0));
        // Height-bound: 600 × 0.9 / 1080 (width would allow 0.5625).
        assert!((c.zoom - 0.5).abs() < 1e-12);
        assert_eq!(c.corner(), Vec2::new((1000.0 - 800.0) / 2.0, (600.0 - 540.0) / 2.0));
        assert!(c.fitted);
        // An icon is fitted many times its size.
        let icon = Camera::fit(area(), Vec2::new(24.0, 24.0));
        assert!((icon.zoom - 22.5).abs() < 1e-12);
        assert_eq!(Camera::fit(area(), Vec2::new(0.5, 0.5)).zoom, MAX_ZOOM, "clamped");
        // At its actual size, in the middle.
        let actual = Camera::centred(area(), Vec2::new(24.0, 24.0), 1.0);
        assert_eq!((actual.zoom, actual.corner(), actual.fitted), (1.0, Vec2::new(488.0, 288.0), false));
    }

    #[test]
    fn the_pages_corner_shows_on_a_whole_pixel() {
        let mut c = Camera::fit(area(), Vec2::new(24.0, 17.0));
        let on_grid = |c: &Camera| c.corner().x.fract() == 0.0 && c.corner().y.fract() == 0.0;
        assert!(on_grid(&c));
        c.zoom_about(area(), 1.37, Vec2::new(433.3, 211.9));
        assert!(on_grid(&c));
        // A slow drag adds up to a pixel's worth.
        let before = c.corner();
        for _ in 0..4 {
            c.pan(Vec2::new(0.25, 0.0));
            assert!(on_grid(&c));
        }
        assert_eq!(c.corner(), before + Vec2::new(1.0, 0.0));
        // There and back.
        let p = Vec2::new(3.25, 9.5);
        assert!((c.page_at(area(), c.window_at(area(), p)) - p).length() < 1e-9);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let mut c = Camera::fit(area(), Vec2::new(1600.0, 1080.0));
        let pivot = Vec2::new(400.0, 300.0);
        let before = c.page_at(area(), pivot);
        c.zoom_about(area(), 2.0, pivot);
        assert!((c.zoom - 1.0).abs() < 1e-12);
        // To within the pixel the corner shows on.
        assert!((c.page_at(area(), pivot) - before).length() < 1.0);
        assert!(!c.fitted, "a zoomed view stops following the window");
        // In and out again, many times over, it doesn't creep.
        for _ in 0..200 {
            c.zoom_about(area(), 1.3, pivot);
            c.zoom_about(area(), 1.0 / 1.3, pivot);
        }
        assert!((c.page_at(area(), pivot) - before).length() < 1.0);
        c.zoom_about(area(), 1e9, pivot);
        assert_eq!(c.zoom, MAX_ZOOM);
        c.zoom_about(area(), 1e-12, pivot);
        assert_eq!(c.zoom, MIN_ZOOM);
    }

    #[test]
    fn a_touched_view_stops_following() {
        let page = Vec2::new(800.0, 600.0);
        let mut c = Camera::fit(area(), page);
        let bigger = Rect::from_min_size(area().min, Vec2::new(2000.0, 1200.0));
        c.follow(bigger, page);
        assert_eq!(c, Camera::fit(bigger, page));
        c.pan(Vec2::new(5.0, -3.0));
        let panned = c;
        c.follow(area(), page);
        assert_eq!(c, panned);
    }

    #[test]
    fn wheels_and_pinches_zoom_by_at_most_half() {
        assert!((wheel_zoom(1.0, 0.0) - 1.3).abs() < 1e-12);
        assert!((wheel_zoom(-1.0, 0.0) - 1.0 / 1.3).abs() < 1e-12);
        assert_eq!(wheel_zoom(10.0, 0.0), 1.5);
        assert!((wheel_zoom(0.0, 40.0) - 1.2).abs() < 1e-12);
        assert!((pinch_zoom(1.1) - 1.21).abs() < 1e-12 && (pinch_zoom(1.0 / 1.1) - 1.0 / 1.21).abs() < 1e-12, "the same in as out");
        assert_eq!((pinch_zoom(1.0), pinch_zoom(3.0), pinch_zoom(0.1)), (1.0, 1.5, 1.0 / 1.5));
        assert_eq!((pinch_zoom(0.0), pinch_zoom(f64::NAN), pinch_zoom(-2.0)), (1.0, 1.0, 1.0));
    }
}
