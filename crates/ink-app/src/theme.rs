//! Ink's look: LS3's tokens verbatim (ARCHITECTURE §8, D12), its type
//! scale with nothing under 18 px, the panel sheen, the rainbow, and the
//! LUI2 theme made from them for what LUI2 draws (the title bar, menus,
//! dialogs). Sizes here are logical px; `Ui::m.scale` makes them physical.

use lntrn_math::Color;
use lntrn_props::Gradient;
use lntrn_text::TextStyle;
use lntrn_ui::{Theme, Ui};

pub const BG: Color = Color::hex(0x12100E);
/// The ground round the page: LS3's light tan (its present shader's
/// `SURROUND`).
pub const GROUND: Color = Color::hex(0xAAA295);
pub const PANEL: Color = Color::hex(0x241A0F);
pub const TEXT: Color = Color::hex(0xE8DCC8);
pub const TEXT_DIM: Color = Color::hex(0x8A7D6A);
pub const ACCENT: Color = Color::hex(0xFFC800);
/// A raised box on a panel: the tooltips' ground.
pub const BUTTON: Color = Color::hex(0x2A2218);
pub const BUTTON_HOVER: Color = Color::hex(0x3D3225);
/// A toolbar tile, and one under the pointer or in hand.
pub const TOOL_BUTTON: Color = Color::hex(0x3F2F1C);
pub const TOOL_BUTTON_HOVER: Color = Color::hex(0x5C4832);
pub const BORDER: Color = Color::hex(0x302820);
pub const INPUT_BG: Color = Color::hex(0x151210);
pub const ACTIVE: Color = Color::hex(0x4A4038);
pub const TAB_ACTIVE: Color = Color::hex(0x4A3810);
pub const TAB_INACTIVE: Color = Color::hex(0x3A2F1F);
pub const TAB_INACTIVE_HOVER: Color = Color::hex(0x4D3F2B);
/// A row of the object tree (LS3's layer rows): at rest, under the
/// pointer, and its outline.
pub const LAYER_ROW: Color = Color::hex(0x140A00);
pub const LAYER_ROW_HOVER: Color = Color::hex(0x241304);
pub const LAYER_ROW_BORDER: Color = Color::hex(0x735A32);
/// The title bar's close button, hovered.
pub const CLOSE: Color = Color::hex(0xE8122A);
/// Near-black ink on gold.
pub const ON_ACCENT: Color = Color::rgb(0.1, 0.1, 0.1);
/// A menu row under the pointer: gold at 28 % over the panel.
pub const DROPDOWN_HOVER: Color = Color::hex(0x614B0B);

/// Rose, gold, lime, sky, purple, pink: the strips beside the toolbar and
/// above the status bar.
pub const RAINBOW: [Color; 6] = [Color::hex(0xE94560), Color::hex(0xF0C040), Color::hex(0xA8E72E), Color::hex(0x29ADFF), Color::hex(0x83769C), Color::hex(0xFF77A8)];

/// How much darker a surface's bottom edge is than its top, per channel.
pub const SHEEN_DROP: f64 = 0.06;

// The type scale in use (LS3's: nothing goes under 18 here).
pub const FONT_SM: f64 = 18.0;
pub const FONT_BASE: f64 = 20.0;
pub const FONT_MD: f64 = 22.0;
pub const FONT_LG: f64 = 24.0;
pub const FONT_2XL: f64 = 30.0;
/// The right panel's own text: its rows' names and what they say, and
/// the smaller words on its buttons. (LS3's panels are at 20; Alva had
/// Ink's made bigger, 2026-10-09: beside LUI2's 25 px they read small.)
pub const FONT_PANEL: f64 = 24.0;
pub const FONT_PANEL_SM: f64 = 20.0;
pub const FONT_3XL: f64 = 34.0;

// Chrome sizes (LS3's).
pub const TOOLBAR_W: f64 = 78.0;
pub const STRIP: f64 = 4.0;
pub const RULE: f64 = 2.0;
pub const TAB_BAR_H: f64 = 44.0;
/// How thick a ruler is: room for its 18 px numbers.
pub const RULER: f64 = 30.0;
pub const STATUS_H: f64 = 48.0;
pub const PANEL_W: f64 = 400.0;
pub const PANEL_MIN: f64 = 260.0;
pub const PANEL_MAX: f64 = 520.0;
/// The panel's resize grip, over its left edge.
pub const GRIP: f64 = 6.0;
/// A toolbar tile, the icon in it, and the room between tiles.
pub const TOOL_TILE: f64 = 52.0;
pub const TOOL_ICON: f64 = 34.0;
pub const TOOL_GAP: f64 = 8.0;

/// A surface: exactly `base` at the top, `SHEEN_DROP` darker at the
/// bottom. Never lighter than the colour chosen.
pub fn sheen(base: Color) -> Gradient {
    let d = SHEEN_DROP;
    Gradient::new(base, Color::rgba((base.r - d).max(0.0), (base.g - d).max(0.0), (base.b - d).max(0.0), base.a))
}

/// Text of `size` logical px, as this frame draws it.
pub fn text(ui: &Ui, size: f64) -> TextStyle {
    TextStyle::new((size * ui.m.scale).round() as f32)
}

/// The gold a slider fills with, top to bottom (LS3's, verbatim).
pub const SLIDER_FILL: Gradient = Gradient::new(Color::rgb(1.0, 0.85, 0.15), Color::rgb(0.85, 0.60, 0.0));

/// LUI2's theme in Ink's colours, for what LUI2 draws itself. Its
/// sizes stay LUI2's (25 px text): only colours and lines change. A
/// dialog in a gold line, its default button gold, sliders in two
/// golds, nothing bevelled: LS3's, where LUI2 has it to give.
pub fn lui2() -> Theme {
    Theme {
        dialog_outline: ACCENT,
        fill: SLIDER_FILL,
        bevel: false,
        accent_buttons: true,
        bg: BG,
        title: sheen(PANEL),
        header: sheen(PANEL),
        panel: sheen(PANEL),
        widget: sheen(BUTTON_HOVER),
        field: INPUT_BG,
        text: TEXT,
        text_dim: TEXT_DIM,
        accent: ACCENT,
        accent_text: ON_ACCENT,
        selection: DROPDOWN_HOVER,
        selection_text: TEXT,
        focus: ACCENT,
        close: CLOSE,
        border_dark: BORDER,
        border_light: ACTIVE,
        border_width: 2.0,
        ..Theme::default()
    }
}
