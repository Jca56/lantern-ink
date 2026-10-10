//! Edit > Preferences (`docs/M4.md`, slice f): the essentials and the
//! look. What snapping lands things on (the grid, other shapes, guides:
//! View > Snapping is the one switch over them); how many decimals a
//! new drawing's numbers are written with (D15: three, unless said);
//! the pixel grid's and the guides' colours; and what's behind the
//! page where a drawing is see-through. Each is kept the moment it's
//! changed, as LS3's one preference is.

use ink_doc::value::MAX_DECIMALS;
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{CursorIcon, Dialog, FILL, HostCx, Sense, ShellRequest, Ui};

use crate::controls;
use crate::ink::Ink;
use crate::menus::DIALOG_PREFERENCES;
use crate::settings::Settings;
use crate::theme::{ACCENT, BORDER, PANEL};

/// The pixel grid's colours to choose from: the first is what it has
/// unless another is chosen (a grey that shows on light and on dark).
pub const GRID_TINTS: [Color; 6] = [Color::rgba(0.5, 0.5, 0.5, 0.45), Color::rgba(0.0, 0.0, 0.0, 0.35), Color::rgba(1.0, 1.0, 1.0, 0.5), Color::rgba(0.0, 0.75, 1.0, 0.5), Color::rgba(1.0, 0.2, 0.85, 0.5), Color::rgba(1.0, 0.78, 0.0, 0.5)];
/// The guides': the first is one that no part of the window has. None
/// is the cyan of a line landed on, nor the selection's gold.
pub const GUIDE_TINTS: [Color; 6] = [Color::rgba(1.0, 0.2, 0.85, 1.0), Color::rgba(1.0, 0.23, 0.19, 1.0), Color::rgba(0.17, 0.85, 0.39, 1.0), Color::rgba(0.24, 0.55, 1.0, 1.0), Color::rgba(1.0, 0.58, 0.0, 1.0), Color::rgba(1.0, 1.0, 1.0, 1.0)];
/// What can be behind the page, in the list's order.
pub const GROUNDS: [&str; 3] = ["Checks", "White", "Dark, as Lantern\u{2019}s panels"];

/// What's behind the page where the drawing is see-through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ground {
    /// Transparency checks: the drawing's see-through there.
    Checks,
    White,
    /// The dark an icon is shown on in Lantern's apps.
    Dark,
}

impl Settings {
    pub fn grid_color(&self) -> Color {
        GRID_TINTS[(self.grid_tint.max(0) as usize).min(GRID_TINTS.len() - 1)]
    }

    pub fn guide_color(&self) -> Color {
        GUIDE_TINTS[(self.guide_tint.max(0) as usize).min(GUIDE_TINTS.len() - 1)]
    }

    pub fn ground(&self) -> Ground {
        match self.page_ground {
            1 => Ground::White,
            2 => Ground::Dark,
            _ => Ground::Checks,
        }
    }

    /// How many decimals a new drawing is written with.
    pub fn decimals(&self) -> usize {
        (self.new_decimals.max(0) as usize).min(MAX_DECIMALS)
    }
}

/// A row of colours to choose one of: which is chosen. Whether another
/// was.
fn tints(ui: &mut Ui, name: &str, tints: &[Color], chosen: &mut i64) -> bool {
    let s = ui.m.scale;
    let (cell, gap) = ((40.0 * s).round(), (8.0 * s).round());
    let r = ui.alloc(Vec2::new(FILL, cell));
    let mut changed = false;
    for (i, tint) in tints.iter().enumerate() {
        let at = Rect::from_xywh(r.min.x + i as f64 * (cell + gap), r.min.y, cell, cell);
        let resp = ui.interact(ui.id(&format!("{name} {i}")), at, Sense::CLICK);
        if resp.hovered {
            ui.state.cursor_icon = CursorIcon::Pointer;
        }
        if resp.clicked && *chosen != i as i64 {
            *chosen = i as i64;
            changed = true;
        }
        // On the panel's dark, as it's seen; the one chosen ringed.
        let on = *chosen == i as i64;
        ui.draw.rounded_rect(at, 6.0 * s, PANEL);
        ui.draw.rounded_rect(at.expand(-4.0 * s), 4.0 * s, tint.with_alpha(1.0));
        ui.draw.stroke_rect(at, (if on { 3.0 } else { 2.0 } * s).round().max(1.0), 6.0 * s, if on { ACCENT } else { BORDER });
    }
    changed
}

impl Settings {
    /// The dialog's rows. Whether anything changed.
    pub fn rows(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
        ui.label("Things dragged land on");
        changed |= controls::toggle(ui, "The grid: whole and half units", &mut self.snap_grid);
        changed |= controls::toggle(ui, "Other shapes, and the page", &mut self.snap_shapes);
        changed |= controls::toggle(ui, "Guides", &mut self.snap_guides);
        ui.space(ui.m.gap);
        ui.label("A new drawing");
        let places: Vec<String> = (0..=MAX_DECIMALS).map(|n| n.to_string()).collect();
        let places: Vec<&str> = places.iter().map(String::as_str).collect();
        let mut picked = self.decimals();
        ui.labelled("Decimals", |ui| {
            if controls::dropdown(ui, "Decimals", &mut picked, &places, false) {
                self.new_decimals = picked as i64;
                changed = true;
            }
        });
        ui.space(ui.m.gap);
        ui.label("The look");
        ui.labelled("Pixel grid", |ui| changed |= tints(ui, "grid", &GRID_TINTS, &mut self.grid_tint));
        ui.labelled("Guides", |ui| changed |= tints(ui, "guide", &GUIDE_TINTS, &mut self.guide_tint));
        let mut ground = self.page_ground.clamp(0, 2) as usize;
        ui.labelled("Behind the page", |ui| {
            if controls::dropdown(ui, "Behind the page", &mut ground, &GROUNDS, false) {
                self.page_ground = ground as i64;
                changed = true;
            }
        });
        changed
    }
}

impl Ink {
    /// Edit > Preferences.
    pub(crate) fn ask_preferences(&mut self, cx: &mut HostCx) {
        cx.request(ShellRequest::Dialog(Dialog::new("Preferences", "").content(DIALOG_PREFERENCES).button("Close", None)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preference_out_of_range_is_the_nearest_there_is() {
        let mut s = Settings::default();
        assert_eq!((s.grid_color(), s.guide_color(), s.ground(), s.decimals()), (GRID_TINTS[0], GUIDE_TINTS[0], Ground::Checks, 3));
        (s.grid_tint, s.guide_tint, s.page_ground, s.new_decimals) = (99, -4, 2, 40);
        assert_eq!((s.grid_color(), s.guide_color(), s.ground(), s.decimals()), (GRID_TINTS[5], GUIDE_TINTS[0], Ground::Dark, MAX_DECIMALS));
        s.page_ground = 1;
        assert_eq!(s.ground(), Ground::White);
    }
}
