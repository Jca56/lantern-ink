//! The toolbar down the left (LS3's, copied: D12): a tile per slot, the
//! tool in hand ringed in gold, a gold chevron on a group's tile. A
//! right-click on a group opens a flyout of its tools beside it; a tile
//! under the pointer shows its name and key. The column scrolls when
//! the window is too short for it.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Key, Sense, Ui};

use crate::chrome::{surface, tooltip};
use crate::icons::Icons;
use crate::theme::{self, ACCENT, BORDER, FONT_SM, PANEL, STRIP, TEXT_DIM, TOOL_BUTTON, TOOL_BUTTON_HOVER, TOOL_GAP, TOOL_TILE};
use crate::tools::{Group, LAYOUT, Slot, Tool, Tools};

/// The flyout's draw layer, over the area's own.
const FLYOUT_LAYER: usize = 1;

/// What the toolbar keeps between frames.
#[derive(Default)]
pub struct Toolbar {
    /// How far down the column is scrolled, logical px.
    scroll: f64,
    /// The group whose flyout is open, and where it was drawn.
    flyout: Option<(Group, Rect)>,
    /// The tool that was in hand last frame: a change (a key) brings its
    /// tile into view.
    seen: Option<Tool>,
}

/// A tile's ground and outline: the tool in hand in gold.
fn face(ui: &mut Ui, r: Rect, in_hand: bool, hovered: bool) {
    let s = ui.m.scale;
    let (fill, edge, width) = match (in_hand, hovered) {
        (true, _) => (TOOL_BUTTON_HOVER, ACCENT, 3.0),
        (false, true) => (TOOL_BUTTON_HOVER, BORDER, 2.0),
        (false, false) => (TOOL_BUTTON, BORDER, 2.0),
    };
    let radius = (6.0 * s).round();
    ui.draw.rounded_rect(r, radius, fill);
    ui.draw.stroke_rect(r, (width * s).round().max(1.0), radius, edge);
}

/// `tool`'s icon in the middle of `r`, pixel for pixel.
fn icon(ui: &mut Ui, r: Rect, icons: &Icons, tool: Tool) {
    let Some(image) = icons.tool(tool) else { return };
    let size = Vec2::new(image.width as f64, image.height as f64);
    let at = Vec2::new((r.center().x - size.x / 2.0).round(), (r.center().y - size.y / 2.0).round());
    ui.draw.image(Rect::from_min_size(at, size), image, 0.0, Color::WHITE);
}

pub fn draw(ui: &mut Ui, bar: Rect, tools: &mut Tools, icons: &Icons, st: &mut Toolbar) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let area = ui.clip();
    surface(ui, bar, PANEL, 0.0);
    let (tile, gap, pad, rule) = (px(TOOL_TILE), px(TOOL_GAP), px(8.0), px(2.0).max(1.0));

    // Where each slot starts down the column.
    let mut tops = [0.0; LAYOUT.len()];
    let mut content = pad;
    for (top, slot) in tops.iter_mut().zip(LAYOUT) {
        *top = content;
        content += if slot == Slot::Separator { rule } else { tile } + gap;
    }
    content += pad - gap;
    let max_scroll = (content - bar.height()).max(0.0);

    // The flyout's ways out, before anything can take the press. The
    // press that closes it does nothing else.
    if let Some((_, rect)) = st.flyout {
        let escape = ui.state.take_key(|k| k.key == Key::Escape).is_some();
        let outside = ui.state.pressed && !rect.contains(ui.state.press_pos);
        if outside {
            ui.state.press_claimed = true;
        }
        if escape || outside || (ui.state.right_pressed && !rect.contains(ui.state.right_press_pos)) {
            st.flyout = None;
        }
    }

    let mut scroll = (st.scroll * s).clamp(0.0, max_scroll);
    if ui.state.pointer_in_window && bar.contains(ui.state.pointer) && ui.state.wheel.y != 0.0 {
        scroll = (scroll - ui.state.wheel.y).clamp(0.0, max_scroll);
        ui.state.wheel = Vec2::ZERO;
    }
    if st.seen != Some(tools.active()) {
        st.seen = Some(tools.active());
        // Another tool in hand, by key or by press: the flyout goes.
        st.flyout = None;
        if let Some(i) = LAYOUT.iter().position(|slot| tools.in_slot(*slot) == st.seen) {
            scroll = scroll.min(tops[i] - pad).max(tops[i] + tile + pad - bar.height()).clamp(0.0, max_scroll);
        }
    }
    let scroll = scroll.round();
    st.scroll = scroll / s;

    ui.draw.push_clip(bar);
    let x = bar.min.x + pad;
    let mut tip = None;
    let mut anchor = None;
    for (i, slot) in LAYOUT.into_iter().enumerate() {
        let y = bar.min.y + tops[i] - scroll;
        let Some(tool) = tools.in_slot(slot) else {
            ui.draw.rect(Rect::from_xywh(x, y, bar.width() - pad - px(2.0), rule), BORDER);
            continue;
        };
        let r = Rect::from_xywh(x, y, tile, tile);
        // Only what shows of it can be pressed: the rest is under the
        // status bar or the title.
        let showing = r.intersection(&bar);
        if showing.is_empty() {
            continue;
        }
        ui.push_index(i);
        let resp = ui.interact(ui.id("tool"), showing, Sense::CLICK);
        ui.pop_id();
        face(ui, r, tools.active() == tool, resp.hovered);
        icon(ui, r, icons, tool);
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
            tip = Some((r, tool));
        }
        if resp.clicked {
            tools.select(tool);
            st.flyout = None;
        }
        if let Slot::Group(group) = slot {
            // The chevron: more behind this one.
            let (w, h) = (px(7.0), px(10.0));
            let corner = Vec2::new(r.max.x - px(5.0) - w, r.max.y - px(6.0) - h);
            ui.draw.triangle(corner, Vec2::new(corner.x, corner.y + h), Vec2::new(corner.x + w, corner.y + h / 2.0), ACCENT);
            if resp.hovered && ui.state.right_pressed && showing.contains(ui.state.right_press_pos) {
                st.flyout = Some((group, Rect::default()));
            }
            if st.flyout.is_some_and(|(open, _)| open == group) {
                anchor = Some(r);
            }
        }
    }
    if max_scroll > 0.0 {
        // Where the column is, in the gutter beside the tiles.
        let (top, room) = (bar.min.y + pad, bar.height() - pad * 2.0);
        let thumb = (room * bar.height() / content).max(px(24.0)).min(room);
        let y = top + (room - thumb) * scroll / max_scroll;
        ui.draw.rounded_rect(Rect::from_xywh(bar.max.x - px(6.0), y, px(4.0), thumb), px(2.0), TEXT_DIM);
    }
    ui.draw.pop_clip();

    match (st.flyout, anchor) {
        (Some((group, _)), Some(anchor)) => {
            let (rect, picked) = flyout(ui, area, bar, anchor, group, tools, icons);
            st.flyout = Some((group, rect));
            if let Some(tool) = picked {
                tools.select(tool);
                st.flyout = None;
            }
        }
        // Its tile scrolled out of sight.
        (Some(_), None) => st.flyout = None,
        (None, _) => {
            if let Some((r, tool)) = tip {
                tooltip(ui, area, r, &tool.tooltip());
            }
        }
    }
}

/// The flyout of `group`'s tools beside its tile at `anchor`: each with
/// its icon and name. Returns where it is, and a tool picked from it.
fn flyout(ui: &mut Ui, area: Rect, bar: Rect, anchor: Rect, group: Group, tools: &Tools, icons: &Icons) -> (Rect, Option<Tool>) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let style = theme::text(ui, FONT_SM);
    let line = (style.line_height() as f64).ceil();
    let (pad, gap, tile) = (px(10.0), px(6.0), px(TOOL_TILE));
    let members = group.members();
    let widths: Vec<f64> = members.iter().map(|t| ui.measure(t.label(), &style).ceil().max(tile) + px(20.0)).collect();
    let button_h = px(6.0) + tile + px(4.0) + line + px(6.0);
    let w = pad * 2.0 + widths.iter().sum::<f64>() + gap * (members.len() - 1) as f64;
    let h = pad * 2.0 + button_h;
    // Past the rainbow strip, a little above its tile, kept in the window.
    let y = (anchor.min.y - px(6.0)).min(area.max.y - h - px(6.0)).max(area.min.y + px(6.0));
    let rect = Rect::from_xywh(bar.max.x + px(STRIP), y, w, h);
    let mut picked = None;
    ui.child(rect, FLYOUT_LAYER, |ui| {
        let (ground, radius) = (theme::sheen(PANEL), px(8.0));
        ui.draw.rounded_rect_gradient(rect, radius, ground.top, ground.bottom);
        ui.draw.stroke_rect(rect, px(2.0).max(1.0), radius, ACCENT);
        let mut x = rect.min.x + pad;
        for (i, (&tool, &bw)) in members.iter().zip(&widths).enumerate() {
            let b = Rect::from_xywh(x, rect.min.y + pad, bw, button_h);
            ui.push_index(i);
            let resp = ui.interact(ui.id("member"), b, Sense::CLICK);
            ui.pop_id();
            face(ui, b, tools.active() == tool, resp.hovered);
            let top = b.min.y + px(6.0);
            icon(ui, Rect::from_xywh(b.min.x, top, bw, tile), icons, tool);
            ui.text_centered(tool.label(), &style, Rect::from_xywh(b.min.x, top + tile + px(4.0), bw, line), TEXT_DIM);
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            if resp.clicked {
                picked = Some(tool);
            }
            x += bw + gap;
        }
    });
    (rect, picked)
}

#[cfg(test)]
mod tests {
    use lntrn_ui::testing::Harness;

    use super::*;

    /// A toolbar `height` tall in an 800 px wide window, at scale 1.
    struct Bench {
        h: Harness,
        bar: Rect,
        tools: Tools,
        st: Toolbar,
        /// The wheel as the frame left it for what comes after the toolbar.
        wheel_left: f64,
        /// Whether something after the toolbar could take the frame's press.
        press_left: bool,
    }

    impl Bench {
        fn new(height: f64) -> Bench {
            let mut b = Bench { h: Harness::new(800.0, height), bar: Rect::from_xywh(0.0, 0.0, 78.0, height), tools: Tools::default(), st: Toolbar::default(), wheel_left: 0.0, press_left: false };
            b.frame();
            b
        }

        fn frame(&mut self) {
            let Bench { h, bar, tools, st, wheel_left, press_left } = self;
            h.frame(|ui| {
                draw(ui, *bar, tools, &Icons::default(), st);
                *wheel_left = ui.state.wheel.y;
                *press_left = ui.state.pressed && !ui.state.press_claimed;
            });
        }

        fn click(&mut self, x: f64, y: f64) {
            self.h.move_to(Vec2::new(x, y));
            self.frame();
            self.h.press();
            self.frame();
            self.h.release();
            self.frame();
            self.frame();
        }

        fn right_click(&mut self, x: f64, y: f64) {
            self.h.move_to(Vec2::new(x, y));
            self.frame();
            self.h.right_press();
            self.frame();
            self.frame();
        }
    }

    // Where the tiles are at scale 1, unscrolled: 8 px in, 52 px tiles
    // 8 px apart, a 2 px rule after the 2nd, the 5th and the 7th.
    const POINTER: (f64, f64) = (34.0, 34.0);
    const NODE: (f64, f64) = (34.0, 94.0);
    const SHAPES: (f64, f64) = (34.0, 224.0);
    const ZOOM: (f64, f64) = (34.0, 544.0);
    /// All of it: 9 tiles, 3 rules, the gaps between and 8 px at each end.
    const COLUMN: f64 = 578.0;

    #[test]
    fn a_click_takes_the_tool_and_a_group_gives_the_one_it_shows() {
        let mut b = Bench::new(900.0);
        assert_eq!(b.tools.active(), Tool::Pointer);
        b.click(NODE.0, NODE.1);
        assert_eq!(b.tools.active(), Tool::Node);
        b.click(SHAPES.0, SHAPES.1);
        assert_eq!(b.tools.active(), Tool::Rect);
        b.click(ZOOM.0, ZOOM.1);
        assert_eq!(b.tools.active(), Tool::Zoom);
        // Between tiles, and on a rule, nothing.
        b.click(34.0, 64.0);
        b.click(34.0, 129.0);
        assert_eq!(b.tools.active(), Tool::Zoom);
        // A right-click on a tool that's alone opens nothing.
        b.right_click(POINTER.0, POINTER.1);
        assert!(b.st.flyout.is_none());
    }

    #[test]
    fn a_right_click_on_a_group_opens_its_flyout() {
        let mut b = Bench::new(900.0);
        b.right_click(SHAPES.0, SHAPES.1);
        let (group, rect) = b.st.flyout.expect("open");
        assert_eq!(group, Group::Shape);
        assert_eq!(rect.min, Vec2::new(82.0, 192.0), "past the strip, 6 px above its tile");
        assert_eq!(b.tools.active(), Tool::Pointer, "opening it picks nothing");
        // Picking from it takes the tool, shows it in the slot, and
        // closes. The last of its four buttons ends 10 px in.
        b.click(rect.max.x - 10.0 - 30.0, rect.min.y + 40.0);
        assert_eq!((b.tools.active(), b.tools.shown(Group::Shape)), (Tool::Polygon, Tool::Polygon));
        assert!(b.st.flyout.is_none());
        b.click(POINTER.0, POINTER.1);
        b.click(SHAPES.0, SHAPES.1);
        assert_eq!(b.tools.active(), Tool::Polygon, "the slot gives what it shows");
    }

    #[test]
    fn the_flyout_closes_on_a_press_outside_which_does_nothing_else() {
        let mut b = Bench::new(900.0);
        b.right_click(SHAPES.0, SHAPES.1);
        // A press on another tile: the flyout goes, the tile isn't taken.
        b.h.move_to(Vec2::new(NODE.0, NODE.1));
        b.frame();
        b.h.press();
        b.frame();
        assert!(b.st.flyout.is_none());
        assert!(!b.press_left, "nothing after the toolbar gets the press either");
        b.h.release();
        b.frame();
        assert_eq!(b.tools.active(), Tool::Pointer);
        // The next click is an ordinary one.
        b.click(NODE.0, NODE.1);
        assert_eq!(b.tools.active(), Tool::Node);
        // A press inside the flyout but off its buttons keeps it open and
        // goes nowhere.
        b.right_click(SHAPES.0, SHAPES.1);
        b.click(86.0, 196.0);
        assert!(b.st.flyout.is_some());
        // Escape closes it; so does a right-click elsewhere.
        b.h.key(Key::Escape);
        b.frame();
        assert!(b.st.flyout.is_none());
        b.right_click(SHAPES.0, SHAPES.1);
        assert!(b.st.flyout.is_some());
        b.right_click(400.0, 400.0);
        assert!(b.st.flyout.is_none());
    }

    #[test]
    fn a_tool_taken_by_key_closes_the_flyout() {
        let mut b = Bench::new(900.0);
        b.right_click(SHAPES.0, SHAPES.1);
        assert!(b.st.flyout.is_some());
        assert!(b.tools.press('h'));
        b.frame();
        assert!(b.st.flyout.is_none());
    }

    #[test]
    fn a_short_window_scrolls_the_column() {
        let mut b = Bench::new(400.0);
        b.h.move_to(Vec2::new(34.0, 200.0));
        b.h.wheel_pixels(Vec2::new(0.0, -100.0));
        b.frame();
        assert_eq!((b.st.scroll, b.wheel_left), (100.0, 0.0), "the wheel over the toolbar is the toolbar's");
        // What was at 164 is now at 64: the Pen.
        b.click(34.0, 64.0);
        assert_eq!(b.tools.active(), Tool::Pen);
        // It stops at its ends.
        b.h.wheel_pixels(Vec2::new(0.0, -5000.0));
        b.frame();
        assert_eq!(b.st.scroll, COLUMN - 400.0);
        b.click(34.0, 400.0 - 8.0 - 26.0);
        assert_eq!(b.tools.active(), Tool::Zoom, "the last tile, 8 px off the bottom");
        b.h.wheel_pixels(Vec2::new(0.0, 5000.0));
        b.frame();
        assert_eq!(b.st.scroll, 0.0);
        // Elsewhere the wheel is left alone.
        b.h.move_to(Vec2::new(400.0, 200.0));
        b.h.wheel_pixels(Vec2::new(0.0, -100.0));
        b.frame();
        assert_eq!((b.st.scroll, b.wheel_left), (0.0, -100.0));
        // A tool taken by its key comes into view (the Zoom tool is in
        // hand; the Pointer is at the top, the Hand near the bottom).
        b.tools.select(Tool::Hand);
        b.frame();
        assert_eq!(b.st.scroll, COLUMN - 60.0 - 400.0, "just far enough to show it and its 8 px of room");
        b.tools.select(Tool::Pointer);
        b.frame();
        assert_eq!(b.st.scroll, 0.0);
        // A tall window doesn't scroll at all.
        let mut tall = Bench::new(900.0);
        tall.h.move_to(Vec2::new(34.0, 200.0));
        tall.h.wheel_pixels(Vec2::new(0.0, -100.0));
        tall.frame();
        assert_eq!(tall.st.scroll, 0.0);
    }
}
