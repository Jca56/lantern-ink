//! The status bar along the bottom (LS3's, copied: D12): the version,
//! the drawing's name, where the pointer is in the drawing's own units,
//! the page's size, then the tool in hand with its key, or the latest
//! result in gold in its place; and on the right the zoom buttons, with
//! the live bridge's switch before them: a CLAUDE pill, an outline
//! while Claude is kept out (as it is until M5).

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::Ui;

use crate::chrome::{Look, surface, text_button};
use crate::theme::{self, ACCENT, BORDER, FONT_BASE, PANEL, TEXT, TEXT_DIM};

pub struct Status<'a> {
    pub name: &'a str,
    /// Where the pointer is in the drawing's own units, while it's over
    /// the canvas, and how many decimals of it are worth reading.
    pub pointer: Option<Vec2>,
    pub places: usize,
    /// The page's size, in the drawing's units.
    pub page: Option<Vec2>,
    /// A result to show for a few seconds (a save, a failure).
    pub toast: Option<&'a str>,
    pub zoom: Option<f64>,
    /// The tool in hand: its name and key.
    pub tool: &'a str,
}

/// What was pressed on the bar.
pub enum Click {
    Out,
    In,
    Fit,
    /// The live bridge's switch.
    Claude,
}

/// A number as the bar shows it: `places` decimals at most, no zeros
/// trailing.
pub fn written(v: f64, places: usize) -> String {
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s.as_str() };
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

/// How finely the pointer's place is worth reading where a unit of the
/// drawing is `per_unit` px of the screen: a pixel is about the last
/// place shown.
pub fn places(per_unit: f64) -> usize {
    if per_unit >= 50.0 {
        2
    } else if per_unit >= 5.0 {
        1
    } else {
        0
    }
}

/// The zoom as a percentage: `2250%`, and `1.6%` under ten.
pub fn percent(zoom: f64) -> String {
    let p = zoom * 100.0;
    if p < 10.0 { format!("{}%", written(p, 1)) } else { format!("{p:.0}%") }
}

pub fn draw(ui: &mut Ui, bar: Rect, st: &Status) -> Option<Click> {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    surface(ui, bar, PANEL, 0.0);
    let style = theme::text(ui, FONT_BASE);
    let (spacing, row) = (px(8.0), Rect::new(Vec2::new(bar.min.x + px(12.0), bar.min.y + px(8.0)), Vec2::new(bar.max.x - px(12.0), bar.max.y - px(8.0))));

    // The zoom buttons, right to left, so the left side knows its room.
    let mut right = row.max.x;
    let mut click = None;
    let look = Look { size: FONT_BASE, ink: TEXT, hover_ink: TEXT, hover: Color::rgba(1.0, 1.0, 1.0, 0.08), radius: 4.0 };
    if let Some(zoom) = st.zoom {
        let label = percent(zoom);
        for (id, text, pad, what) in [("fit", "Fit", 6.0, 3), ("in", "+", 4.0, 2), ("zoom", label.as_str(), 0.0, 0), ("out", "\u{2212}", 4.0, 1)] {
            // The percentage keeps the room of its widest, so the
            // buttons beside it hold still as it changes.
            let w = if what == 0 { ui.measure(text, &style).max(ui.measure("25600%", &style)) } else { ui.measure(text, &style) + px(pad) * 2.0 };
            let r = Rect::new(Vec2::new(right - w, row.min.y), Vec2::new(right, row.max.y));
            right -= w + spacing;
            if what == 0 {
                ui.text_centered(text, &style, r, TEXT_DIM);
            } else if text_button(ui, id, r, text, &look).clicked {
                click = Some(match what {
                    1 => Click::Out,
                    2 => Click::In,
                    _ => Click::Fit,
                });
            }
        }
    }
    // The live bridge's switch, left of them.
    {
        let (w, pad) = (ui.measure("CLAUDE", &style), px(10.0));
        let pill = Rect::new(Vec2::new(right - w - pad * 2.0, row.min.y + px(1.0)), Vec2::new(right, row.max.y - px(1.0)));
        right = pill.min.x - spacing;
        let resp = ui.interact(ui.id("claude"), pill, lntrn_ui::Sense::CLICK);
        if resp.hovered {
            ui.state.cursor_icon = lntrn_ui::CursorIcon::Pointer;
        }
        let fill = if resp.hovered { Color::rgba(1.0, 1.0, 1.0, 0.08) } else { Color::TRANSPARENT };
        ui.draw.rounded_rect(pill, px(4.0), fill);
        ui.draw.stroke_rect(pill, px(2.0).max(1.0), px(4.0), BORDER);
        ui.text_centered("CLAUDE", &style, pill, TEXT_DIM);
        if resp.clicked {
            click = Some(Click::Claude);
        }
    }

    let mut x = row.min.x;
    let mut put = |ui: &mut Ui, text: &str, ink: Color| {
        let w = ui.measure(text, &style);
        if x + w <= right {
            ui.text_in_rect(text, &style, Rect::new(Vec2::new(x, row.min.y), Vec2::new(x + w, row.max.y)), ink);
        }
        x += w + spacing;
    };
    put(ui, concat!("v", env!("CARGO_PKG_VERSION")), TEXT_DIM);
    put(ui, "|", BORDER);
    put(ui, st.name, TEXT);
    put(ui, "|", BORDER);
    if let Some(p) = st.pointer {
        put(ui, &format!("{}, {}", written(p.x, st.places), written(p.y, st.places)), TEXT_DIM);
    }
    if let Some(page) = st.page {
        put(ui, &format!("{} \u{00d7} {}", written(page.x, 2), written(page.y, 2)), TEXT_DIM);
    }
    // The tool in hand; a result takes its place while it shows.
    put(ui, "|", BORDER);
    match st.toast {
        Some(toast) => put(ui, toast, ACCENT),
        None => put(ui, st.tool, TEXT_DIM),
    }
    click
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_as_a_person_writes_them() {
        assert_eq!((written(24.0, 2), written(12.5, 2), written(0.004, 2), written(-0.001, 2), written(7.26, 1)), ("24".to_owned(), "12.5".to_owned(), "0".to_owned(), "0".to_owned(), "7.3".to_owned()));
        assert_eq!((places(0.5), places(22.5), places(64.0)), (0, 1, 2));
        assert_eq!((percent(1.0), percent(22.5), percent(0.015625), percent(256.0)), ("100%".to_owned(), "2250%".to_owned(), "1.6%".to_owned(), "25600%".to_owned()));
    }
}
