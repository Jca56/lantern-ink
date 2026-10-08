//! The paint section's face: a row each for Fill and Stroke (a swatch
//! that opens the picker, the colour's hex beside it, and what kind of
//! paint it is: none or a colour), the palette grid under them (a
//! press takes a colour as the fill, with Shift as the stroke; a drag
//! reorders; `+` adds the fill), and the button that opens the
//! palettes. It draws what it's shown and changes nothing: a paint it
//! wants set comes back for the window to set.

use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Sense, Ui, WidgetId};

use super::drawer::{self, Drawer};
use super::palettes::{Library, to_hex};
use super::picker::{Picker, swatch_face};
use crate::chrome::{Look, text_button};
use crate::icons::Icons;
use crate::paint::{Paint, Paints, Which};
use crate::theme::{self, ACCENT, BORDER, FONT_BASE, FONT_LG, FONT_MD, FONT_SM, INPUT_BG, LAYER_ROW_BORDER, TEXT, TEXT_DIM, TOOL_BUTTON, TOOL_BUTTON_HOVER};

/// The grid's columns, the least a swatch is, and the room between
/// them, logical px (LS3's).
pub const COLUMNS: usize = 8;
const SWATCH_MIN: f64 = 14.0;
const SWATCH_MAX: f64 = 36.0;
const SWATCH_GAP: f64 = 4.0;
/// The heading, a paint's row, its name's room, its swatch and one of
/// its kind buttons.
const HEAD: f64 = 40.0;
const ROW: f64 = 44.0;
const LABEL: f64 = 76.0;
const SWATCH: (f64, f64) = (58.0, 34.0);
const KIND: f64 = 38.0;
const PAD: f64 = 10.0;
/// How far a pressed swatch goes before it's being dragged.
const DRAG_FROM: f64 = 10.0;

/// What the section keeps between frames.
pub struct Section {
    pub drawer: Drawer,
    /// The palette swatch pressed, and whether it's being dragged.
    drag: Option<(usize, bool)>,
    /// The colour each paint last was: what "a colour" means for one
    /// that's none now.
    last: [Color; 2],
    /// Where its parts were last drawn, by name.
    #[cfg(test)]
    pub(crate) laid: Vec<(String, Rect)>,
}

impl Default for Section {
    fn default() -> Section {
        Section {
            drawer: Drawer::default(),
            drag: None,
            last: [Color::hex(0xF3B700), Color::BLACK],
            #[cfg(test)]
            laid: Vec::new(),
        }
    }
}

/// What a frame of the section came to.
#[derive(Default)]
pub struct Out {
    /// How much of the panel it took, window px.
    pub height: f64,
    /// The palettes changed: write them.
    pub changed: bool,
    /// A right press on swatch `.0` of the grid, at `.1`.
    pub menu: Option<(usize, Vec2)>,
    /// A paint to set.
    pub set: Option<(Which, Paint)>,
}

/// The grid's swatch size for `width` px of room, at `scale`: eight
/// across, no bigger than leaves the object tree its room under a full
/// palette.
pub fn swatch_size(width: f64, scale: f64) -> f64 {
    let gap = (SWATCH_GAP * scale).round();
    ((width - gap * (COLUMNS - 1) as f64) / COLUMNS as f64).floor().clamp((SWATCH_MIN * scale).round(), (SWATCH_MAX * scale).round())
}

/// A paint's swatch: a colour as a fill is, a stroke as a ring of it,
/// nothing as a white square struck through in red, a gradient as two
/// tones (its own colours come with the gradient's slice).
fn face(ui: &mut Ui, r: Rect, which: Which, paint: &Paint, lit: bool, icons: &Icons) {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round().max(1.0);
    let edge = if lit { ACCENT } else { LAYER_ROW_BORDER };
    match paint {
        Paint::Color(c) if which == Which::Fill => swatch_face(ui, r, *c, lit, icons),
        Paint::Color(c) => {
            // A ring: a line's colour round an empty middle.
            swatch_face(ui, r, *c, lit, icons);
            let hole = r.shrink(px(9.0));
            if !hole.is_empty() {
                ui.draw.rect(hole, INPUT_BG);
                ui.draw.stroke_rect(hole, px(1.0), 0.0, Color::rgba(0.0, 0.0, 0.0, 0.5));
            }
        }
        Paint::None => {
            ui.draw.rect(r, Color::WHITE);
            ui.draw.push_clip(r);
            ui.draw.line(Vec2::new(r.min.x, r.max.y), Vec2::new(r.max.x, r.min.y), px(3.0), Color::hex(0xE8122A));
            ui.draw.pop_clip();
            ui.draw.stroke_rect(r, px(2.0), 0.0, edge);
        }
        Paint::Server(_) => {
            ui.draw.rect_gradient_h(r, Color::hex(0xE8DCC8), Color::hex(0x4A4038));
            ui.draw.stroke_rect(r, px(2.0), 0.0, edge);
        }
    }
}

/// What a paint's row says beside its swatch.
fn said(paint: &Paint) -> String {
    match paint {
        Paint::None => "None".to_owned(),
        Paint::Color(c) if c.a < 1.0 - 1e-9 => format!("{} {:.0}%", to_hex(*c).to_uppercase(), c.a * 100.0),
        Paint::Color(c) => to_hex(*c).to_uppercase(),
        Paint::Server(_) => "Gradient".to_owned(),
    }
}

pub fn draw(ui: &mut Ui, panel: Rect, st: &mut Section, shown: &Paints, lib: &mut Library, picker: &mut Picker, icons: &Icons) -> Out {
    let s = ui.m.scale;
    let px = |v: f64| (v * s).round();
    let mut out = Out::default();
    let pad = px(PAD);
    let inner = Rect::new(Vec2::new(panel.min.x + pad, panel.min.y), Vec2::new(panel.max.x - pad, panel.max.y));
    if inner.width() <= 0.0 {
        return out;
    }
    ui.push_id("paint");
    #[cfg(test)]
    st.laid.clear();
    let head = Rect::from_min_size(inner.min, Vec2::new(inner.width(), px(HEAD)));
    ui.text_in_rect("Paint", &theme::text(ui, FONT_MD), head, TEXT_DIM);
    let rule = px(2.0).max(1.0);
    ui.draw.rect(Rect::from_min_size(Vec2::new(panel.min.x, head.max.y), Vec2::new(panel.width(), rule)), BORDER);
    let mut y = head.max.y + rule + px(8.0);

    // A row each: its name, its swatch and what it says, and its kind
    // at the right.
    let mut anchors: [(WidgetId, Rect); 2] = [(ui.id("Fill"), Rect::default()), (ui.id("Stroke"), Rect::default())];
    for (k, which) in Which::BOTH.into_iter().enumerate() {
        let row = Rect::from_xywh(inner.min.x, y, inner.width(), px(ROW));
        y += px(ROW) + px(4.0);
        let paint = shown.get(which);
        if let Paint::Color(c) = paint {
            st.last[k] = *c;
        }
        let id = anchors[k].0;
        ui.push_id(which.label());
        ui.text_in_rect(which.label(), &theme::text(ui, FONT_BASE), Rect::from_min_size(row.min, Vec2::new(px(LABEL), row.height())), TEXT);

        // What kind of paint, from the right: a colour, then none.
        let kind = |i: usize| Rect::from_xywh(row.max.x - px(KIND) * (i + 1) as f64 - px(4.0) * i as f64, (row.center().y - px(SWATCH.1) / 2.0).round(), px(KIND), px(SWATCH.1));
        let kinds = [(kind(0), "colour", Paint::Color(st.last[k]), matches!(paint, Paint::Color(_))), (kind(1), "none", Paint::None, *paint == Paint::None)];
        for (r, name, to, on) in kinds {
            #[cfg(test)]
            st.laid.push((format!("{} {name}", which.label()), r));
            let resp = ui.interact(ui.id(name), r, Sense::CLICK);
            let inside = r.shrink(px(5.0));
            face(ui, inside, which, &to, false, icons);
            ui.draw.stroke_rect(r, px(if on { 3.0 } else { 2.0 }).max(1.0), px(4.0), if on || resp.hovered { ACCENT } else { BORDER });
            if resp.hovered {
                ui.state.cursor_icon = CursorIcon::Pointer;
            }
            if resp.clicked && !on {
                out.set = Some((which, to));
            }
        }

        // The swatch, and the colour in letters: a press on either
        // opens the picker for it.
        let swatch = Rect::from_xywh(row.min.x + px(LABEL), (row.center().y - px(SWATCH.1) / 2.0).round(), px(SWATCH.0), px(SWATCH.1));
        let words = Rect::new(Vec2::new(swatch.max.x + px(10.0), row.min.y), Vec2::new(kind(1).min.x - px(8.0), row.max.y));
        let target = if words.width() > px(40.0) { swatch.union(&words) } else { swatch };
        let resp = ui.interact(id, target, Sense::CLICK);
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        if resp.clicked {
            picker.toggle(id);
            // A colour to pick needs there to be one.
            if picker.is_open_for(id) && !matches!(paint, Paint::Color(_)) {
                out.set = Some((which, Paint::Color(st.last[k])));
            }
            ui.state.request_rebuild = true;
        }
        face(ui, swatch, which, paint, picker.is_open_for(id) || resp.hovered, icons);
        if words.width() > px(40.0) {
            ui.draw.push_clip(words);
            ui.text_in_rect(&said(paint), &theme::text(ui, FONT_BASE), words, if *paint == Paint::None { TEXT_DIM } else { TEXT });
            ui.draw.pop_clip();
        }
        anchors[k].1 = swatch;
        #[cfg(test)]
        st.laid.push((which.label().to_owned(), swatch));
        ui.pop_id();
    }
    y += px(6.0);

    // The grid.
    let gap = px(SWATCH_GAP);
    let size = swatch_size(inner.width(), s);
    let colors = lib.shown().colors.clone();
    let cell = |i: usize| Rect::from_xywh(inner.min.x + (size + gap) * (i % COLUMNS) as f64, y + (size + gap) * (i / COLUMNS) as f64, size, size);
    let plus = lib.can_add().then_some(colors.len());
    let cells = colors.len() + usize::from(plus.is_some());
    let grid_bottom = if cells == 0 { y } else { cell(cells - 1).max.y };
    let dragging = st.drag.is_some_and(|(_, live)| live);
    for (i, &color) in colors.iter().enumerate() {
        let r = cell(i);
        #[cfg(test)]
        st.laid.push((format!("swatch {i}"), r));
        ui.push_index(i);
        let resp = ui.interact(ui.id("swatch"), r, Sense::CLICK);
        ui.pop_id();
        ui.draw.rect(r, color);
        ui.draw.stroke_rect(r, px(1.0).max(1.0), 0.0, if resp.hovered && !dragging { ACCENT } else { Color::rgba(0.0, 0.0, 0.0, 0.5) });
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        if resp.pressed {
            // The fill; with Shift, the stroke. As see-through as it was.
            let which = if ui.state.mods.shift() { Which::Stroke } else { Which::Fill };
            let alpha = match shown.get(which) {
                Paint::Color(c) => c.a,
                _ => 1.0,
            };
            out.set = Some((which, Paint::Color(color.with_alpha(alpha))));
            st.drag = Some((i, false));
        }
        let state = &ui.state;
        if state.right_pressed && r.contains(state.right_press_pos) && !state.shielded(ui.layer(), state.right_press_pos) {
            out.menu = Some((i, state.right_press_pos));
        }
    }
    if let Some(i) = plus {
        let r = cell(i);
        let resp = ui.interact(ui.id("add"), r, Sense::CLICK);
        ui.draw.rect(r, if resp.hovered { TOOL_BUTTON_HOVER } else { TOOL_BUTTON });
        ui.draw.stroke_rect(r, px(2.0).max(1.0), 0.0, ACCENT);
        ui.text_centered("+", &theme::text(ui, FONT_LG.min(size / s)).bold(), r, ACCENT);
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        // The fill, when it's a colour, joins the palette.
        if resp.clicked
            && let Paint::Color(c) = shown.fill
        {
            lib.add_color(c);
            out.changed = true;
        }
    }
    // A pressed swatch, dragged: a gold mark on the place it would take.
    if let Some((from, live)) = st.drag.as_mut() {
        let held = ui.state.down;
        *live |= held && ui.state.pointer.distance(ui.state.press_pos) > px(DRAG_FROM);
        let to = (0..colors.len()).find(|&i| cell(i).expand(gap / 2.0).contains(ui.state.pointer));
        if *live && held {
            ui.state.cursor_icon = CursorIcon::Grabbing;
            if let Some(to) = to.filter(|t| t != from) {
                ui.draw.stroke_rect(cell(to).expand(px(1.0)), px(3.0).max(1.0), 0.0, ACCENT);
            }
        }
        if !held {
            if let (true, Some(to)) = (*live, to.filter(|t| t != from)) {
                lib.move_color(*from, to);
                out.changed = true;
            }
            st.drag = None;
        }
    }

    // The palettes: their button, and under it the list when it's open.
    let label = if st.drawer.open { "Palettes \u{25b4}" } else { "Palettes \u{25be}" };
    let style = theme::text(ui, FONT_SM);
    let toggle = Rect::from_xywh(inner.min.x, grid_bottom + px(8.0), ui.measure(label, &style).ceil() + px(20.0), px(34.0));
    ui.draw.rounded_rect(toggle, px(4.0), theme::BUTTON);
    let look = Look { size: FONT_SM, ink: if st.drawer.open { ACCENT } else { TEXT }, hover_ink: ACCENT, hover: theme::BUTTON_HOVER, radius: 4.0 };
    if text_button(ui, "palettes", toggle, label, &look).clicked {
        st.drawer.open = !st.drawer.open;
        st.drawer.renaming = None;
        ui.state.request_rebuild = true;
    }
    let mut bottom = toggle.max.y;
    if st.drawer.open {
        let top = bottom + px(10.0);
        let d = drawer::draw(ui, Rect::new(Vec2::new(inner.min.x, top), inner.max), &mut st.drawer, lib);
        out.changed |= d.changed;
        bottom = top + d.height;
    }

    // The pickers last: over everything else of the section.
    for (k, which) in Which::BOTH.into_iter().enumerate() {
        let (id, anchor) = anchors[k];
        if let Paint::Color(mut color) = shown.get(which).clone()
            && picker.popup(ui, id, anchor, which.label(), &mut color, icons)
        {
            out.set = Some((which, Paint::Color(color)));
        }
    }
    ui.pop_id();
    out.height = bottom + px(10.0) - panel.min.y;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swatches_fill_the_width_eight_across() {
        // 240 px of a narrow panel at scale 1: eight of 26 with seven
        // gaps of 4.
        assert_eq!(swatch_size(240.0, 1.0), 26.0);
        assert_eq!((swatch_size(100.0, 1.0), swatch_size(380.0, 1.0)), (14.0, 36.0), "never under 14, nor over 36");
        assert_eq!(swatch_size(380.0 * 1.25, 1.25), 45.0);
    }

    #[test]
    fn a_row_says_its_paint_in_letters() {
        assert_eq!(said(&Paint::Color(Color::hex(0xf3b700))), "#F3B700");
        assert_eq!(said(&Paint::Color(Color::hex(0x102030).with_alpha(0.4))), "#102030 40%");
        assert_eq!((said(&Paint::None), said(&Paint::Server("sky".into()))), ("None".to_owned(), "Gradient".to_owned()));
    }
}
