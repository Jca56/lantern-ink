//! The pointer's pictures over the selection's handles (LS3's
//! `cursors.rs`, cut to what Ink has so far): the desktop's own resize
//! arrows and its turn arrow, read from its icons at launch and drawn
//! by Ink's renderer at the window's physical scale, so none is
//! stretched soft. One the desktop doesn't have leaves the system's
//! shape in its place.

use std::path::PathBuf;

use lntrn_app::CursorImage;
use lntrn_ui::CursorIcon;

use crate::icons;

/// A cursor of Ink's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Ew,
    Ns,
    Nesw,
    Nwse,
    Rotate,
}

/// The desktop's cursors, in [`Cursor::ALL`]'s order: each one's SVG,
/// where there is one.
pub type Themed = [Option<String>; 5];

/// A cursor's side and the point that points (its middle), logical px.
const SIDE: f64 = 32.0;

impl Cursor {
    pub const ALL: [Cursor; 5] = [Cursor::Ew, Cursor::Ns, Cursor::Nesw, Cursor::Nwse, Cursor::Rotate];

    /// What the UI asks for it with.
    pub fn icon(self) -> CursorIcon {
        CursorIcon::Custom(self as u32 + 1)
    }

    fn file(self) -> &'static str {
        match self {
            Cursor::Ew => "lntrn-cursor-ew.svg",
            Cursor::Ns => "lntrn-cursor-ns.svg",
            Cursor::Nesw => "lntrn-cursor-nesw.svg",
            Cursor::Nwse => "lntrn-cursor-nwse.svg",
            Cursor::Rotate => "rotate.svg",
        }
    }

    /// It drawn for a window at `scale` physical px per logical one, if
    /// the desktop has it.
    fn image(self, scale: f64, themed: &Themed) -> Option<CursorImage> {
        let px = (SIDE * scale).round().max(1.0) as u32;
        let picture = icons::drawn(themed[self as usize].as_deref()?.as_bytes(), px)?;
        let CursorIcon::Custom(id) = self.icon() else { return None };
        let middle = (px / 2) as u16;
        Some(CursorImage { id, width: px as u16, height: px as u16, rgba: picture.rgba, hotspot: (middle, middle) })
    }
}

/// The desktop's cursors as its icons have them now
/// (`~/.lantern/icons/cursors`): read once, at launch.
pub fn themed() -> Themed {
    let dir: Option<PathBuf> = lntrn_sys::dirs::lantern().map(|l| l.join("icons/cursors"));
    Cursor::ALL.map(|cursor| {
        let path = dir.as_ref()?.join(cursor.file());
        let svg = std::fs::read_to_string(&path).ok();
        if svg.is_none() {
            lntrn_core::log_info!("cursor: no {}; the system's shape stands in", path.display());
        }
        svg
    })
}

/// Every cursor the desktop has, drawn for a window at `scale`.
pub fn images(scale: f64, themed: &Themed) -> Vec<CursorImage> {
    Cursor::ALL.iter().filter_map(|c| c.image(scale, themed)).collect()
}

/// What the window shows for the pointer the UI asked for: the
/// desktop's arrows for the system's resize shapes where it has them,
/// the arrow for a turn where it has no turn arrow, anything else as
/// asked.
pub fn shown(wanted: CursorIcon, themed: &Themed) -> CursorIcon {
    let own = |c: Cursor, or: CursorIcon| if themed[c as usize].is_some() { c.icon() } else { or };
    match wanted {
        CursorIcon::EwResize => own(Cursor::Ew, wanted),
        CursorIcon::NsResize => own(Cursor::Ns, wanted),
        CursorIcon::NeswResize => own(Cursor::Nesw, wanted),
        CursorIcon::NwseResize => own(Cursor::Nwse, wanted),
        CursorIcon::Custom(_) if wanted == Cursor::Rotate.icon() => own(Cursor::Rotate, CursorIcon::Default),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktops_cursors_are_drawn_at_the_screens_own_pixels() {
        let square = || Some(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect x="8" y="8" width="16" height="16" fill="#fff"/></svg>"##.to_owned());
        let all: Themed = std::array::from_fn(|_| square());
        for scale in [1.0, 1.4, 2.0] {
            let images = images(scale, &all);
            assert_eq!(images.len(), Cursor::ALL.len());
            let px = (32.0 * scale).round() as u16;
            for (cursor, image) in Cursor::ALL.iter().zip(&images) {
                assert_eq!((image.width, image.height, image.hotspot, CursorIcon::Custom(image.id)), (px, px, (px / 2, px / 2), cursor.icon()), "{cursor:?} at {scale}");
                assert!(image.rgba.chunks_exact(4).any(|p| p[3] > 0));
            }
        }
        assert_eq!(Cursor::ALL.iter().map(|c| *c as usize).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
        // With them, the system's resize shapes are the desktop's; a
        // turn is its turn arrow. Without, the system's own, and the
        // arrow for a turn.
        let none = Themed::default();
        assert_eq!((shown(CursorIcon::EwResize, &all), shown(Cursor::Rotate.icon(), &all), shown(CursorIcon::Text, &all)), (Cursor::Ew.icon(), Cursor::Rotate.icon(), CursorIcon::Text));
        assert_eq!((shown(CursorIcon::EwResize, &none), shown(Cursor::Rotate.icon(), &none), images(1.0, &none).len()), (CursorIcon::EwResize, CursorIcon::Default, 0));
    }
}
