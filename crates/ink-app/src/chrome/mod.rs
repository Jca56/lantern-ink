//! The window's chrome, drawn by Ink inside its one area, in LS3's look
//! (its `chrome/`, copied: D12): surfaces with the sheen, 2 px rules,
//! the rainbow strips, the logo in the title bar. The toolbar, tab bar,
//! status bar and panel frame have files of their own.

pub mod panel;
pub mod status;
pub mod tabs;
pub mod toolbar;

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, ImageHandle, Response, Sense, Ui};

use crate::icons::LOGO_ICON;
use crate::theme::{self, ACCENT, BORDER, BUTTON, FONT_3XL, FONT_BASE, RAINBOW, TEXT};

/// A surface with the sheen and a `BORDER` outline `border` logical px
/// wide (0 for none).
pub fn surface(ui: &mut Ui, r: Rect, base: Color, border: f64) {
    let g = theme::sheen(base);
    ui.draw.rect_gradient(r, g.top, g.bottom);
    if border > 0.0 {
        ui.draw.stroke_rect(r, (border * ui.m.scale).round().max(1.0), 0.0, BORDER);
    }
}

/// The rainbow along `r`: down it when it's tall, across when wide.
pub fn rainbow(ui: &mut Ui, r: Rect) {
    let steps = RAINBOW.len() - 1;
    for (i, pair) in RAINBOW.windows(2).enumerate() {
        let (a, b) = (i as f64 / steps as f64, (i + 1) as f64 / steps as f64);
        if r.height() > r.width() {
            let seg = Rect::new(Vec2::new(r.min.x, lerp(r.min.y, r.max.y, a)), Vec2::new(r.max.x, lerp(r.min.y, r.max.y, b)));
            ui.draw.rect_gradient(seg, pair[0], pair[1]);
        } else {
            let seg = Rect::new(Vec2::new(lerp(r.min.x, r.max.x, a), r.min.y), Vec2::new(lerp(r.min.x, r.max.x, b), r.max.y));
            ui.draw.rect_gradient_h(seg, pair[0], pair[1]);
        }
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// The icon and `L A N T E R N   I N K` at the right of the title bar.
/// Returns the width taken.
pub fn logo(ui: &mut Ui, space: Rect, icon: Option<ImageHandle>) -> f64 {
    let s = ui.m.scale;
    let style = theme::text(ui, FONT_3XL).bold();
    let label = "L A N T E R N   I N K";
    let text_w = ui.measure(label, &style);
    let icon_side = (LOGO_ICON * s).round();
    let (gap, pad) = ((10.0 * s).round(), (12.0 * s).round());
    let width = pad + icon_side + gap + text_w + pad;
    if width > space.width() {
        return 0.0;
    }
    let right = space.max.x - pad;
    let text_rect = Rect::new(Vec2::new(right - text_w, space.min.y), Vec2::new(right, space.max.y));
    ui.text_in_rect(label, &style, text_rect, ACCENT);
    if let Some(icon) = icon {
        // Drawn for this scale: pixel for pixel.
        let at = Vec2::new((text_rect.min.x - gap - icon_side).round(), (space.center().y - icon_side / 2.0).round());
        ui.draw.image(Rect::from_min_size(at, Vec2::splat(icon_side)), icon, 0.0, Color::WHITE);
    }
    width
}

/// How a flat text button looks: its ink, and its ink and ground while
/// the pointer's on it.
pub struct Look {
    pub size: f64,
    pub ink: Color,
    pub hover_ink: Color,
    pub hover: Color,
    pub radius: f64,
}

/// A flat text button.
pub fn text_button(ui: &mut Ui, id: &str, r: Rect, label: &str, look: &Look) -> Response {
    let resp = ui.interact(ui.id(id), r, Sense::CLICK);
    if resp.hovered {
        ui.draw.rounded_rect(r, look.radius * ui.m.scale, look.hover);
        ui.state.cursor_icon = CursorIcon::Pointer;
    }
    let style = theme::text(ui, look.size);
    ui.text_centered(label, &style, r, if resp.hovered { look.hover_ink } else { look.ink });
    resp
}

/// The draw layer of Ink's own tooltips: over its flyouts and the Box
/// (LUI2's menus are on 1 and 2, its tooltips on 3).
pub const TOOLTIP_LAYER: usize = 3;

/// `text` in a box to the right of `anchor`, over everything, kept
/// inside `area`.
pub fn tooltip(ui: &mut Ui, area: Rect, anchor: Rect, text: &str) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let style = theme::text(ui, FONT_BASE);
    let w = ui.measure(text, &style).ceil() + px(12.0) * 2.0;
    let h = (style.line_height() as f64).ceil() + px(6.0) * 2.0;
    let at = Vec2::new(anchor.max.x + px(6.0), (anchor.center().y - h / 2.0).round());
    let at = Vec2::new(at.x.min(area.max.x - w).max(area.min.x), at.y.min(area.max.y - h).max(area.min.y));
    let rect = Rect::from_min_size(at, Vec2::new(w, h));
    let layer = ui.draw.layer();
    ui.draw.set_layer(TOOLTIP_LAYER);
    ui.draw.push_clip_absolute(area);
    ui.draw.rounded_rect(rect, px(6.0), BUTTON);
    ui.draw.stroke_rect(rect, px(2.0).max(1.0), px(6.0), ACCENT);
    ui.text_centered(text, &style, rect, TEXT);
    ui.draw.pop_clip();
    ui.draw.set_layer(layer);
}
