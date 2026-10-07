//! What text inherits: the font it's set in, its spacing, where it sits
//! against its position, and whether its white space is kept.

use crate::document::Document;
use crate::fonts::{self, Face};
use crate::length::{EM, Length};
use crate::node::Node;
use crate::style::prop;

/// Which end of a chunk of text its position is (`text-anchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    Start,
    Middle,
    End,
}

/// Which line of the font sits at the text's y (`dominant-baseline`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Baseline {
    /// The line the letters stand on.
    #[default]
    Alphabetic,
    /// Half the height of a lowercase `x` above that.
    Middle,
    /// Halfway between the font's top and bottom.
    Central,
    /// Where scripts that hang from a line hang from.
    Hanging,
    /// Halfway up the capitals, nearly.
    Mathematical,
    /// The font's top.
    Top,
    /// The font's bottom.
    Bottom,
}

/// What a text element inherits about its lettering.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    /// `font-family` as said: its names in order, quotes off.
    pub families: Vec<String>,
    /// In user units.
    pub size: f64,
    /// 1 to 1000; 400 is normal, 700 bold.
    pub weight: f64,
    pub italic: bool,
    /// Added after each character, and after each space besides.
    pub letter_spacing: f64,
    pub word_spacing: f64,
    pub anchor: Anchor,
    pub baseline: Baseline,
    /// White space is kept as written (`white-space: pre`,
    /// `xml:space="preserve"`), not collapsed.
    pub preserve: bool,
}

impl Default for Font {
    /// What a document starts from: `medium`, upright, at normal weight,
    /// in no family (so in Lantern's own).
    fn default() -> Font {
        Font { families: Vec::new(), size: EM, weight: 400.0, italic: false, letter_spacing: 0.0, word_spacing: 0.0, anchor: Anchor::Start, baseline: Baseline::Alphabetic, preserve: false }
    }
}

/// The names in a `font-family`: split at commas, quotes off.
fn families(list: &str) -> Vec<String> {
    list.split(',').map(|name| name.trim().trim_matches(['"', '\'']).trim().to_owned()).filter(|name| !name.is_empty()).collect()
}

/// A `font-size`, where the parent's is `parent`: a length, a share of
/// the parent's, or one of CSS's names for a size.
fn size(said: &str, parent: f64) -> Option<f64> {
    let named = match said.to_ascii_lowercase().as_str() {
        "xx-small" => 9.0,
        "x-small" => 10.0,
        "small" => 13.0,
        "medium" => 16.0,
        "large" => 18.0,
        "x-large" => 24.0,
        "xx-large" => 32.0,
        "xxx-large" => 48.0,
        "larger" => parent * 1.2,
        "smaller" => parent / 1.2,
        _ => Length::parse_in(said, parent)?.of(parent),
    };
    (named.is_finite() && named >= 0.0).then_some(named)
}

/// A `font-weight`, where the parent's is `parent`.
fn weight(said: &str, parent: f64) -> Option<f64> {
    match said.to_ascii_lowercase().as_str() {
        "normal" => Some(400.0),
        "bold" => Some(700.0),
        "bolder" => Some(if parent < 350.0 {
            400.0
        } else if parent < 550.0 {
            700.0
        } else {
            900.0
        }),
        "lighter" => Some(if parent < 550.0 {
            100.0
        } else if parent < 750.0 {
            400.0
        } else {
            700.0
        }),
        number => number.parse::<f64>().ok().filter(|w| (1.0..=1000.0).contains(w)),
    }
}

/// A `letter-spacing` or `word-spacing`, in a font of size `em`.
fn spacing(said: &str, em: f64) -> Option<f64> {
    if said.eq_ignore_ascii_case("normal") {
        return Some(0.0);
    }
    match Length::parse_in(said, em)? {
        Length::Px(v) => Some(v),
        Length::Percent(_) => None,
    }
}

impl Font {
    /// This, with what `node` says for itself.
    pub fn cascade(&self, node: &Node) -> Font {
        let mut font = self.clone();
        if let Some(list) = prop(node, "font-family").map(families).filter(|names| !names.is_empty()) {
            font.families = list;
        }
        if let Some(size) = prop(node, "font-size").and_then(|said| size(said, self.size)) {
            font.size = size;
        }
        if let Some(weight) = prop(node, "font-weight").and_then(|said| weight(said, self.weight)) {
            font.weight = weight;
        }
        match prop(node, "font-style").map(|said| said.split_whitespace().next().unwrap_or("")) {
            Some("italic" | "oblique") => font.italic = true,
            Some("normal") => font.italic = false,
            _ => {}
        }
        // Spacing given in ems is of this element's own size.
        if let Some(gap) = prop(node, "letter-spacing").and_then(|said| spacing(said, font.size)) {
            font.letter_spacing = gap;
        }
        if let Some(gap) = prop(node, "word-spacing").and_then(|said| spacing(said, font.size)) {
            font.word_spacing = gap;
        }
        match prop(node, "text-anchor") {
            Some("start") => font.anchor = Anchor::Start,
            Some("middle") => font.anchor = Anchor::Middle,
            Some("end") => font.anchor = Anchor::End,
            _ => {}
        }
        match prop(node, "dominant-baseline") {
            Some("auto" | "alphabetic") => font.baseline = Baseline::Alphabetic,
            Some("middle") => font.baseline = Baseline::Middle,
            Some("central") => font.baseline = Baseline::Central,
            Some("hanging") => font.baseline = Baseline::Hanging,
            Some("mathematical") => font.baseline = Baseline::Mathematical,
            Some("text-before-edge" | "text-top") => font.baseline = Baseline::Top,
            Some("text-after-edge" | "text-bottom" | "ideographic") => font.baseline = Baseline::Bottom,
            _ => {}
        }
        // The property outvotes the attribute it replaced.
        match node.attr("xml:space") {
            Some("preserve") => font.preserve = true,
            Some("default") => font.preserve = false,
            _ => {}
        }
        match prop(node, "white-space") {
            Some("pre" | "pre-wrap" | "break-spaces") => font.preserve = true,
            Some("normal" | "nowrap" | "pre-line") => font.preserve = false,
            _ => {}
        }
        font
    }

    /// What `node` is lettered in: what its ancestors say from the root
    /// down, then what it says itself.
    pub fn of(doc: &Document, node: &Node) -> Font {
        let above: Vec<&Node> = doc.ancestors(node.id).collect();
        above.iter().rev().fold(Font::default(), |font, n| font.cascade(n)).cascade(node)
    }

    /// The font on this machine it's set in: the first of its families
    /// that's installed (or is a generic name), in Lantern's own where
    /// none is. `lntrn-text` has two weights: from 600 up is bold.
    pub fn face(&self) -> Face {
        let family = self.families.iter().find_map(|name| fonts::family(name)).unwrap_or(fonts::Family::Sans);
        Face { family, bold: self.weight >= 600.0, italic: self.italic }
    }

    /// How far below its y text in `face` is drawn, for its baseline to
    /// be the one asked for. `ascent` and `descent` are the font's, at
    /// this size.
    pub fn drop(&self, face: &Face, ascent: f64, descent: f64) -> f64 {
        match self.baseline {
            Baseline::Alphabetic => 0.0,
            Baseline::Middle => fonts::x_height(face) * self.size / 2.0,
            Baseline::Central => (ascent - descent) / 2.0,
            Baseline::Hanging => ascent * 0.8,
            Baseline::Mathematical => ascent * 0.5,
            Baseline::Top => ascent,
            Baseline::Bottom => -descent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{DocId, NodeId};

    fn font(inner: &str, id: u64) -> Font {
        let d = Document::parse(DocId(1), &format!("<svg>{inner}</svg>")).unwrap();
        Font::of(&d, d.node(NodeId(id)).unwrap())
    }

    #[test]
    fn lettering_is_inherited_and_overridden() {
        let f = font(r#"<g font-family="'Fira Code', monospace" font-size="10" font-weight="bold" letter-spacing="0.1em" text-anchor="middle"><text style="font-size: 150%; font-style: italic">a<tspan font-weight="normal" font-size="2em" text-anchor="end">b</tspan></text></g>"#, 3);
        assert_eq!(f.families, ["Fira Code", "monospace"]);
        assert_eq!((f.size, f.weight, f.italic, f.anchor), (15.0, 700.0, true, Anchor::Middle));
        assert_eq!(f.letter_spacing, 1.0, "an em of the element that said it, inherited as that length");
        let span = font(r#"<g font-size="10" font-weight="bold" letter-spacing="0.1em"><text style="font-size: 150%">a<tspan font-weight="normal" font-size="2em" text-anchor="end">b</tspan></text></g>"#, 4);
        assert_eq!((span.size, span.weight, span.anchor, span.letter_spacing), (30.0, 400.0, Anchor::End, 1.0));
        assert_eq!(font("<text/>", 2), Font::default());
    }

    #[test]
    fn sizes_weights_and_spacing_read_as_css_says() {
        assert_eq!((size("12", 16.0), size("12pt", 16.0), size("50%", 16.0), size("large", 16.0), size("smaller", 12.0)), (Some(12.0), Some(16.0), Some(8.0), Some(18.0), Some(10.0)));
        assert_eq!((size("-3", 16.0), size("big", 16.0)), (None, None));
        assert_eq!((weight("bolder", 400.0), weight("bolder", 700.0), weight("lighter", 700.0), weight("lighter", 400.0), weight("650", 400.0)), (Some(700.0), Some(900.0), Some(400.0), Some(100.0), Some(650.0)));
        assert_eq!((weight("heavy", 400.0), weight("0", 400.0)), (None, None));
        assert_eq!((spacing("normal", 10.0), spacing("2", 10.0), spacing("-0.05em", 10.0), spacing("10%", 10.0)), (Some(0.0), Some(2.0), Some(-0.5), None));
        assert_eq!(families(r#" "Arial Black" , sans-serif,, 'DM Sans'"#), ["Arial Black", "sans-serif", "DM Sans"]);
    }

    #[test]
    fn white_space_is_kept_where_either_way_of_saying_so_does() {
        assert!(font(r#"<text style="white-space: pre;">a</text>"#, 2).preserve);
        assert!(font(r#"<g xml:space="preserve"><text>a</text></g>"#, 3).preserve);
        assert!(!font(r#"<g xml:space="preserve"><text xml:space="default">a</text></g>"#, 3).preserve);
        assert!(!font(r#"<text xml:space="preserve" style="white-space: normal">a</text>"#, 2).preserve, "the property outvotes the attribute");
        assert_eq!(font(r#"<text dominant-baseline="central">a</text>"#, 2).baseline, Baseline::Central);
    }

    #[test]
    fn a_font_that_is_not_there_gives_way_to_the_next() {
        let named = |list: &str| Font { families: families(list), ..Font::default() }.face().family;
        assert_eq!(named("No Such Font Anywhere, monospace"), fonts::Family::Mono);
        assert_eq!(named("No Such Font Anywhere"), fonts::Family::Sans);
        assert_eq!(named(""), fonts::Family::Sans);
        let heavy = Font { weight: 900.0, italic: true, ..Font::default() }.face();
        assert!(heavy.bold && heavy.italic && !Font::default().face().bold);
    }
}
