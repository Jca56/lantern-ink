//! Lantern Ink's vector maths (ARCHITECTURE §2, §5): what a shape is made
//! of and what can be worked out from it, with no idea of SVG documents,
//! styles or pixels. Ours, pure std, in f64.
//!
//! - [`Path`]: subpaths of lines, quadratics, cubics and elliptical arcs,
//!   each segment kept as the kind it is; read from and written as SVG
//!   path data ([`Path::parse`], [`Path::to_data`]).
//! - [`Affine`]: a 2D transform, in SVG's `matrix(a b c d e f)` order.
//! - [`Path::flatten`]: a path as polylines, never further than a
//!   tolerance from its curves.
//! - [`stroke()`]: a stroke's outline as polygons, with joins, caps and
//!   dashes.
//! - [`Path::bounds`]: the exact box around a path.
//! - [`Path::transformed`]: a path through a transform, its segments
//!   still the kinds they were.
//! - [`Piece`]: one segment with where it starts, to cut in two, walk
//!   backwards and find the nearest point of.
//! - [`Path::contains`] and [`Path::distance`]: whether a point is in a
//!   path's fill, and how far it is from its line.

mod affine;
mod arc;
mod bounds;
mod dash;
mod data;
mod flatten;
mod hit;
mod map;
pub mod number;
mod path;
mod piece;
mod stroke;

pub use affine::{Affine, Axes};
pub use data::Parsed;
pub use flatten::Polyline;
pub use lntrn_math::{Rect, Vec2};
pub use path::{ArcTo, Path, Seg, Subpath};
pub use piece::{Piece, circle_through};
pub use stroke::{Cap, Join, Stroke, stroke};

/// Which parts of a path that crosses itself are inside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    /// Wherever the outline winds around at all.
    #[default]
    NonZero,
    /// Where it winds an odd number of times: overlaps make holes.
    EvenOdd,
}
