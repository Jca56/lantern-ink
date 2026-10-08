//! Where everything goes in the one area Ink draws (ARCHITECTURE §8;
//! LS3's `layout.rs`): the toolbar and a rainbow strip down the left,
//! the tab bar over the canvas, a rule and the right panel, then a
//! rainbow strip and the status bar along the bottom. Window px, whole
//! pixels so lines stay crisp.

use lntrn_math::{Rect, Vec2};

use crate::theme::{GRIP, PANEL_MAX, PANEL_MIN, RULE, STATUS_H, STRIP, TAB_BAR_H, TOOLBAR_W};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Layout {
    pub toolbar: Rect,
    pub strip_v: Rect,
    pub tabs: Rect,
    pub canvas: Rect,
    pub rule: Rect,
    pub panel: Rect,
    /// The panel's resize grip, over the rule and the panel's left edge.
    pub grip: Rect,
    pub strip_h: Rect,
    pub status: Rect,
}

impl Layout {
    /// Cut `area` at `scale` (window px per logical px), with the panel
    /// `panel_w` logical px wide (kept to its 260–520 range, and never
    /// wider than leaves the canvas nothing).
    pub fn new(area: Rect, scale: f64, panel_w: f64) -> Layout {
        let px = |v: f64| (v * scale).round().max(1.0);
        let (status, rest) = area.take_bottom(px(STATUS_H).min(area.height()));
        let (strip_h, content) = rest.take_bottom(px(STRIP).min(rest.height()));
        let (toolbar, rest) = content.take_left(px(TOOLBAR_W).min(content.width()));
        let (strip_v, rest) = rest.take_left(px(STRIP).min(rest.width()));
        let panel_w = px(clamp_panel(panel_w)).min((rest.width() - px(RULE)).max(0.0));
        let (panel, rest) = rest.take_right(panel_w);
        let (rule, middle) = rest.take_right(px(RULE).min(rest.width()));
        let (tabs, canvas) = middle.take_top(px(TAB_BAR_H).min(middle.height()));
        let grip = Rect::from_min_size(Vec2::new(rule.min.x, rule.min.y), Vec2::new(px(GRIP), rule.height()));
        Layout { toolbar, strip_v, tabs, canvas, rule, panel, grip, strip_h, status }
    }
}

/// A panel width kept to its range, logical px.
pub fn clamp_panel(w: f64) -> f64 {
    if w.is_finite() { w.clamp(PANEL_MIN, PANEL_MAX) } else { crate::theme::PANEL_W }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_regions_tile_the_area() {
        let area = Rect::from_xywh(0.0, 45.0, 1920.0, 1035.0);
        let l = Layout::new(area, 1.0, 400.0);
        assert_eq!(l.status, Rect::from_xywh(0.0, 1032.0, 1920.0, 48.0));
        assert_eq!(l.strip_h, Rect::from_xywh(0.0, 1028.0, 1920.0, 4.0));
        assert_eq!(l.toolbar, Rect::from_xywh(0.0, 45.0, 78.0, 983.0));
        assert_eq!(l.strip_v, Rect::from_xywh(78.0, 45.0, 4.0, 983.0));
        assert_eq!(l.panel, Rect::from_xywh(1520.0, 45.0, 400.0, 983.0));
        assert_eq!(l.rule, Rect::from_xywh(1518.0, 45.0, 2.0, 983.0));
        assert_eq!(l.tabs, Rect::from_xywh(82.0, 45.0, 1436.0, 44.0));
        assert_eq!(l.canvas, Rect::from_xywh(82.0, 89.0, 1436.0, 939.0));
        assert_eq!(l.grip.min, l.rule.min);
    }

    #[test]
    fn it_scales_and_keeps_the_panel_in_range() {
        let area = Rect::from_xywh(0.0, 0.0, 2000.0, 1000.0);
        let l = Layout::new(area, 1.4, 1000.0);
        assert_eq!(l.panel.width(), (520.0f64 * 1.4).round());
        assert_eq!(l.toolbar.width(), (78.0f64 * 1.4).round());
        assert_eq!(Layout::new(area, 1.0, 10.0).panel.width(), 260.0);
        // A window too narrow for it all squeezes the canvas to nothing,
        // never below.
        let narrow = Layout::new(Rect::from_xywh(0.0, 0.0, 300.0, 600.0), 1.0, 400.0);
        assert!(narrow.canvas.width() >= 0.0);
        assert!(narrow.panel.max.x <= 300.0);
    }
}
