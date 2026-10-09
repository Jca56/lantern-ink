//! The paint section at the top of the right panel (ARCHITECTURE §8):
//! a row each for Fill and Stroke, the palette grid, the palettes, and
//! the picker they all open. The picker, the palettes and their drawer
//! are LS3's, copied (D12); the section is Ink's own, since what Ink
//! paints with is a fill and a stroke, not a foreground and a
//! background.

pub mod drawer;
pub mod line;
pub mod palettes;
pub mod picker;
pub mod section;
