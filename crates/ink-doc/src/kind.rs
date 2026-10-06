//! What an element is, by its name and the namespace it's in. Only an
//! element in the SVG namespace is anything but [`Kind::Other`].

/// The SVG namespace.
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";
/// Lantern Ink's own namespace, for the attributes only Ink reads
/// (ARCHITECTURE §3.3).
pub const INK_NS: &str = "urn:lantern:ink";
/// The prefix Ink declares its namespace under.
pub const INK_PREFIX: &str = "ink";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Svg,
    G,
    /// A link (`<a>`): drawn as a group.
    A,
    /// A `<switch>`: drawn as a group.
    Switch,
    Defs,
    Path,
    Rect,
    Circle,
    Ellipse,
    Line,
    Polyline,
    Polygon,
    Text,
    TSpan,
    Image,
    Use,
    Symbol,
    LinearGradient,
    RadialGradient,
    Stop,
    ClipPath,
    Mask,
    Pattern,
    Marker,
    Filter,
    /// One step of a filter: `feDropShadow`, `feGaussianBlur`, …
    FilterPrimitive,
    Style,
    Title,
    Desc,
    Metadata,
    /// Not SVG's, or nothing Ink knows: kept, shown, never drawn.
    Other,
}

impl Kind {
    /// The kind of the SVG element with this local name.
    pub fn of_svg(local: &str) -> Kind {
        match local {
            "svg" => Kind::Svg,
            "g" => Kind::G,
            "a" => Kind::A,
            "switch" => Kind::Switch,
            "defs" => Kind::Defs,
            "path" => Kind::Path,
            "rect" => Kind::Rect,
            "circle" => Kind::Circle,
            "ellipse" => Kind::Ellipse,
            "line" => Kind::Line,
            "polyline" => Kind::Polyline,
            "polygon" => Kind::Polygon,
            "text" => Kind::Text,
            "tspan" => Kind::TSpan,
            "image" => Kind::Image,
            "use" => Kind::Use,
            "symbol" => Kind::Symbol,
            "linearGradient" => Kind::LinearGradient,
            "radialGradient" => Kind::RadialGradient,
            "stop" => Kind::Stop,
            "clipPath" => Kind::ClipPath,
            "mask" => Kind::Mask,
            "pattern" => Kind::Pattern,
            "marker" => Kind::Marker,
            "filter" => Kind::Filter,
            "style" => Kind::Style,
            "title" => Kind::Title,
            "desc" => Kind::Desc,
            "metadata" => Kind::Metadata,
            fe if fe.starts_with("fe") && fe[2..].starts_with(|c: char| c.is_ascii_uppercase()) => Kind::FilterPrimitive,
            _ => Kind::Other,
        }
    }

    /// Something drawn by drawing what's in it.
    pub fn is_group(self) -> bool {
        matches!(self, Kind::Svg | Kind::G | Kind::A | Kind::Switch)
    }

    /// A shape: something with an outline of its own to fill and stroke.
    pub fn is_shape(self) -> bool {
        matches!(self, Kind::Path | Kind::Rect | Kind::Circle | Kind::Ellipse | Kind::Line | Kind::Polyline | Kind::Polygon)
    }

    /// Something that is never drawn where it stands: definitions for
    /// others to refer to, and words about the picture.
    pub fn is_never_drawn(self) -> bool {
        matches!(
            self,
            Kind::Defs | Kind::Symbol | Kind::LinearGradient | Kind::RadialGradient | Kind::Stop | Kind::ClipPath | Kind::Mask | Kind::Pattern | Kind::Marker | Kind::Filter | Kind::FilterPrimitive | Kind::Style | Kind::Title | Kind::Desc | Kind::Metadata | Kind::Other
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_go_by_exact_names() {
        assert_eq!(Kind::of_svg("linearGradient"), Kind::LinearGradient);
        assert_eq!(Kind::of_svg("lineargradient"), Kind::Other, "names are case-sensitive");
        assert_eq!(Kind::of_svg("feDropShadow"), Kind::FilterPrimitive);
        assert_eq!(Kind::of_svg("feed"), Kind::Other);
        assert_eq!(Kind::of_svg("fe"), Kind::Other);
        assert!(Kind::Rect.is_shape() && !Kind::G.is_shape());
        assert!(Kind::G.is_group() && Kind::of_svg("a").is_group() && !Kind::Defs.is_group());
        assert!(Kind::Defs.is_never_drawn() && !Kind::G.is_never_drawn() && !Kind::Path.is_never_drawn());
    }
}
