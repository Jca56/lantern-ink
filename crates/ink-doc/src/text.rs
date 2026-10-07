//! What a `<text>` draws (ARCHITECTURE §5.5). Its characters are
//! gathered from it and from the `<tspan>`s in it, their white space
//! dealt with as a browser deals with it, shaped a run at a time in the
//! machine's fonts ([`crate::fonts`]) and set where SVG says: from the
//! text's `x` and `y`, a `<tspan>`'s `x`, `y`, `dx` and `dy` moving what
//! follows, each chunk hung from its position by its `text-anchor`.
//!
//! What comes back is outlines: each element's glyphs as a [`Path`] in
//! the text's own coordinates, to fill and stroke like any shape's.
//!
//! **Not set:** text along a path, characters placed or turned one by
//! one, text set top to bottom, text stretched to a length. A text that
//! asks for one of those isn't drawn at all, rather than drawn wrong:
//! [`Unset`] says which.

mod font;
mod lay;

use core::fmt;

use ink_geom::{Affine, Path, Rect, Vec2};

pub use self::font::{Anchor, Baseline, Font};
use crate::document::Document;
use crate::id::NodeId;
use crate::kind::Kind;
use crate::length::Length;
use crate::node::{Child, Node, characters};
use crate::style::{Style, prop};

/// Why a text can't be set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unset {
    /// It runs along a path (`<textPath>`).
    OnPath,
    /// Its characters are placed or turned one by one (`rotate`, or a
    /// list of numbers in `x`, `y`, `dx` or `dy`).
    Placed,
    /// It's set top to bottom (`writing-mode`).
    Upright,
    /// It's stretched to a length (`textLength`).
    Stretched,
}

impl fmt::Display for Unset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Unset::OnPath => "it runs along a path (<textPath>)",
            Unset::Placed => "its characters are placed or turned one by one (rotate, or a list in x, y, dx or dy)",
            Unset::Upright => "it's set top to bottom (writing-mode)",
            Unset::Stretched => "it's stretched to a length (textLength)",
        })
    }
}

/// One element's characters as drawn: the `<text>`'s own, or those of a
/// `<tspan>` in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The element whose characters these are: what says how they're
    /// painted.
    pub node: NodeId,
    /// Their glyphs' outlines in the text's coordinates, filled
    /// non-zero.
    pub outline: Path,
}

/// A text, set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Laid {
    /// In the order they're painted.
    pub runs: Vec<Run>,
    /// The box around its glyphs' cells (each as wide as its advance,
    /// from the font's top to its bottom): what SVG calls a text's
    /// bounding box, and measures a gradient across. `None` for a text
    /// with no characters.
    pub cells: Option<Rect>,
    /// How many of its glyphs are pictures (emoji), which aren't drawn.
    pub pictures: usize,
}

impl Laid {
    /// Everything it draws, as one outline.
    pub fn outline(&self) -> Path {
        Path { subpaths: self.runs.iter().flat_map(|run| run.outline.subpaths.iter().cloned()).collect() }
    }

    /// The box around what it draws, through `t`.
    pub fn bounds_through(&self, t: &Affine) -> Option<Rect> {
        self.runs.iter().filter_map(|run| run.outline.bounds_through(t)).reduce(|a, b| a.union(&b))
    }
}

/// An element with characters of its own, and how they're lettered.
struct Piece {
    node: NodeId,
    font: Font,
}

/// Where an element says its first character goes.
#[derive(Clone, Copy, Debug, Default)]
struct Start {
    /// Which character, among all the text's.
    at: usize,
    x: Option<f64>,
    y: Option<f64>,
    dx: Option<f64>,
    dy: Option<f64>,
}

struct Gather<'a> {
    doc: &'a Document,
    /// The size percentages are of.
    view: Vec2,
    pieces: Vec<Piece>,
    /// Every character to set, with the piece it's of.
    chars: Vec<(char, usize)>,
    /// Outer elements before the ones in them.
    starts: Vec<Start>,
    /// The last character kept was a space that swallows the spaces
    /// after it (as the text's start does).
    spaced: bool,
    /// Why it can't be set, once something in it says so.
    unset: Option<Unset>,
}

impl Gather<'_> {
    /// The one length an `x`, `y`, `dx` or `dy` gives. More than one
    /// places characters one by one.
    fn one(&mut self, node: &Node, name: &str, em: f64, whole: f64) -> Option<f64> {
        let mut parts = node.attr(name)?.split(|c: char| c.is_whitespace() || c == ',').filter(|part| !part.is_empty());
        let first = parts.next();
        if parts.next().is_some() {
            self.unset.get_or_insert(Unset::Placed);
        }
        first.and_then(|part| Length::parse_in(part, em)).map(|length| length.of(whole))
    }

    /// Keep `c`, or not. Where white space isn't kept as written, a
    /// line break or a tab is a space, and spaces in a row are one.
    fn push(&mut self, c: char, piece: usize, preserve: bool) {
        let c = if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c };
        let collapses = c == ' ' && !preserve;
        if collapses && self.spaced {
            return;
        }
        self.spaced = collapses;
        self.chars.push((c, piece));
    }

    /// Gather `node`'s characters and those of the elements in it.
    fn element(&mut self, node: &Node, font: &Font) {
        let said = if node.local() == "textPath" {
            Some(Unset::OnPath)
        } else if node.attr("rotate").is_some_and(|turns| turns.split(|c: char| c.is_whitespace() || c == ',').any(|turn| turn.parse::<f64>().is_ok_and(|t| t != 0.0))) {
            Some(Unset::Placed)
        } else if node.attr("textLength").is_some() {
            Some(Unset::Stretched)
        } else if matches!(prop(node, "writing-mode"), Some("tb" | "tb-rl" | "vertical-rl" | "vertical-lr" | "sideways-rl" | "sideways-lr")) {
            Some(Unset::Upright)
        } else {
            None
        };
        self.unset = self.unset.or(said);
        let begin = self.chars.len();
        let slot = self.starts.len();
        let (width, height) = (self.view.x, self.view.y);
        let start = Start { at: begin, x: self.one(node, "x", font.size, width), y: self.one(node, "y", font.size, height), dx: self.one(node, "dx", font.size, width), dy: self.one(node, "dy", font.size, height) };
        self.starts.push(start);
        let mut piece = None;
        for child in &node.children {
            match child {
                Child::Text(raw) => {
                    let mut said = String::new();
                    characters(raw, &mut said);
                    let piece = *piece.get_or_insert_with(|| {
                        self.pieces.push(Piece { node: node.id, font: font.clone() });
                        self.pieces.len() - 1
                    });
                    for c in said.replace("\r\n", "\n").chars() {
                        self.push(c, piece, font.preserve);
                    }
                }
                Child::Node(id) => {
                    let Some(child) = self.doc.get(*id) else { continue };
                    // A link in a text is lettered as a span is, and so
                    // are the words along a path (which can't be set,
                    // but are still what the text says). What else can
                    // stand there (a title) says nothing.
                    if (matches!(child.kind, Kind::TSpan | Kind::A) || child.local() == "textPath") && prop(child, "display") != Some("none") {
                        self.element(child, &font.cascade(child));
                    }
                }
            }
        }
        if self.chars.len() == begin {
            // No characters: nowhere for its position to count.
            self.starts[slot].at = usize::MAX;
        }
    }
}

/// `text`'s characters and where its elements put them.
fn gather<'a>(doc: &'a Document, text: &Node, view: Vec2) -> Gather<'a> {
    let mut g = Gather { doc, view, pieces: Vec::new(), chars: Vec::new(), starts: Vec::new(), spaced: true, unset: None };
    g.element(text, &Font::of(doc, text));
    // A space left at the end is dropped, as the ones at the start were.
    if g.spaced {
        g.chars.pop();
    }
    g.starts.retain(|start| start.at < g.chars.len());
    g
}

/// Set `text` (a `<text>`): what it draws, in its own coordinates.
/// Percentages are of `view`.
pub fn lay(doc: &Document, text: &Node, view: Vec2) -> Result<Laid, Unset> {
    let g = gather(doc, text, view);
    match g.unset {
        Some(why) => Err(why),
        None => Ok(lay::set(&g.pieces, &g.chars, &g.starts)),
    }
}

/// What `text` says: its characters as they're set, its spans' among
/// them, white space dealt with.
pub fn said(doc: &Document, text: &Node) -> String {
    gather(doc, text, Vec2::ZERO).chars.iter().map(|c| c.0).collect()
}

/// What `text` says, a line at a time: a new line is wherever an
/// element in it says where it starts (`x` or `y`), which is how SVG
/// breaks one.
pub fn lines(doc: &Document, text: &Node) -> Vec<String> {
    let g = gather(doc, text, Vec2::ZERO);
    let mut breaks: Vec<usize> = g.starts.iter().filter(|start| start.at > 0 && (start.x.is_some() || start.y.is_some())).map(|start| start.at).collect();
    breaks.sort_unstable();
    breaks.dedup();
    breaks.push(g.chars.len());
    let mut from = 0;
    breaks.into_iter().map(|to| g.chars[std::mem::replace(&mut from, to)..to].iter().map(|c| c.0).collect()).collect()
}

/// Why `text` can't be set, if it can't.
pub fn unset(doc: &Document, text: &Node) -> Option<Unset> {
    gather(doc, text, Vec2::ZERO).unset
}

/// A font some of a text is set in.
#[derive(Clone, Debug, PartialEq)]
pub struct Lettered {
    /// The family, as this machine calls it.
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    pub size: f64,
    /// Families asked for ahead of it that aren't installed here.
    pub missing: Vec<String>,
}

/// The fonts `text` is set in, in the order they first come.
pub fn lettered(doc: &Document, text: &Node) -> Vec<Lettered> {
    let mut all: Vec<Lettered> = Vec::new();
    for piece in &gather(doc, text, Vec2::ZERO).pieces {
        let face = piece.font.face();
        let missing = piece.font.families.iter().take_while(|name| crate::fonts::family(name).is_none()).cloned().collect();
        let set = Lettered { family: crate::fonts::called(&face.family), bold: face.bold, italic: face.italic, size: piece.font.size, missing };
        if !all.contains(&set) {
            all.push(set);
        }
    }
    all
}

/// How `run`'s glyphs are painted: `style` (what `text` itself is drawn
/// with), with what every element from there down to `run` says.
pub fn style_of(doc: &Document, text: &Node, style: &Style, run: NodeId) -> Style {
    if run == text.id {
        return style.clone();
    }
    let between: Vec<&Node> = doc.ancestors(run).take_while(|n| n.id != text.id).collect();
    let above = between.iter().rev().fold(style.clone(), |st, n| st.cascade(n));
    doc.get(run).map_or(above.clone(), |n| above.cascade(n))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Once;

    use super::*;
    use crate::fonts;
    use crate::id::DocId;

    /// The fonts the tests set text in (`tests/font.rs` makes them).
    pub(crate) fn test_fonts() {
        static ADDED: Once = Once::new();
        ADDED.call_once(|| {
            fonts::add(include_bytes!("../../../tests/fonts/InkTest-Regular.ttf").to_vec()).unwrap();
            fonts::add(include_bytes!("../../../tests/fonts/InkTest-Bold.ttf").to_vec()).unwrap();
        });
    }

    /// The text that is `N2` of a 100 × 100 drawing, set.
    fn laid(element: &str) -> Result<Laid, Unset> {
        test_fonts();
        let d = Document::parse(DocId(1), &format!(r#"<svg viewBox="0 0 100 100" font-family="Ink Test" font-size="10">{element}</svg>"#)).unwrap();
        lay(&d, d.node(NodeId(2)).unwrap(), Vec2::new(100.0, 100.0))
    }

    /// Each run's node and outline.
    fn runs(element: &str) -> Vec<(u64, String)> {
        laid(element).unwrap().runs.iter().map(|run| (run.node.0, run.outline.to_data(3))).collect()
    }

    /// To a millionth: what a sum of ems is out by.
    fn near(v: f64) -> f64 {
        (v * 1e6).round() / 1e6
    }

    fn cells(element: &str) -> (f64, f64, f64, f64) {
        let b = laid(element).unwrap().cells.unwrap();
        (near(b.min.x), near(b.min.y), near(b.width()), near(b.height()))
    }

    #[test]
    fn glyphs_stand_on_the_baseline_from_the_text_s_position() {
        // A capital is a box 0.1..0.5 em across and 0.7 em tall; a
        // descender hangs 0.2 em below the line.
        assert_eq!(runs(r#"<text x="20" y="50">Hp</text>"#), [(2, "M21 50 V43 H25 V50 Z M27 52 V45 H31 V52 Z".into())]);
        assert_eq!(cells(r#"<text x="20" y="50">Hp</text>"#), (20.0, 42.0, 12.0, 10.0), "two advances wide, the font's top to its bottom");
        assert_eq!(runs("<text>H</text>"), [(2, "M1 0 V-7 H5 V0 Z".into())], "no position is the origin");
        assert_eq!(runs(r#"<text x="50%" y="2em" font-size="20">I</text>"#), [(2, "M52 40 V26 H54 V40 Z".into())], "a share of the page, and ems of its own size");
        let empty = laid("<text x=\"5\">  </text>").unwrap();
        assert!(empty.runs.is_empty() && empty.cells.is_none());
    }

    #[test]
    fn white_space_collapses_unless_it_is_kept() {
        // A space is 0.3 em.
        let width = |element: &str| cells(element).2;
        assert_eq!(width("<text>\n   H   H\n\tH  </text>"), 6.0 + 3.0 + 6.0 + 3.0 + 6.0);
        assert_eq!(width("<text xml:space=\"preserve\"> H  H </text>"), 3.0 + 6.0 + 3.0 + 3.0 + 6.0 + 3.0);
        assert_eq!(width("<text style=\"white-space: pre;\">H\nH</text>"), 15.0, "a kept line break is a space");
        assert_eq!(width("<text>H <tspan> H</tspan> </text>"), 15.0, "spaces collapse across elements");
        assert_eq!(width("<text>H<!-- not said -->&#72;<![CDATA[H]]></text>"), 18.0);
    }

    #[test]
    fn spans_move_the_pen_and_keep_their_own_paint() {
        let two = r#"<text x="10" y="20">H<tspan x="10" dy="1.2em">H</tspan>H</text>"#;
        assert_eq!(runs(two), [(2, "M11 20 V13 H15 V20 Z".into()), (3, "M11 32 V25 H15 V32 Z".into()), (2, "M17 32 V25 H21 V32 Z".into())], "what follows a span goes on from where it ended");
        assert_eq!(runs(r#"<text x="10" y="20">H<tspan dx="4" dy="-2" font-size="5">H</tspan></text>"#)[1], (3, "M20.5 18 V14.5 H22.5 V18 Z".into()));
        // The innermost element at a character says where it goes.
        assert_eq!(runs(r#"<text x="10" y="20"><tspan x="30"><tspan x="50">H</tspan></tspan></text>"#), [(4, "M51 20 V13 H55 V20 Z".into())]);
        assert_eq!(runs(r#"<text x="10" y="20"><tspan x="30" y="40"><tspan y="60">H</tspan></tspan></text>"#), [(4, "M31 60 V53 H35 V60 Z".into())]);
        assert_eq!(runs(r#"<text x="10" y="20">H<tspan display="none">HHH</tspan><title>not text</title>H</text>"#), [(2, "M11 20 V13 H15 V20 Z M17 20 V13 H21 V20 Z".into())]);
    }

    #[test]
    fn a_chunk_hangs_from_its_position_by_its_anchor() {
        let x = |element: &str| cells(element).0;
        assert_eq!(x(r#"<text x="50" text-anchor="middle">HH</text>"#), 44.0);
        assert_eq!(x(r#"<text x="50" text-anchor="end">HH</text>"#), 38.0);
        assert_eq!(x(r#"<text x="50" dx="10" text-anchor="middle">HH</text>"#), 54.0, "from where its first character goes");
        // Each line is a chunk of its own, with its own anchor.
        let lines = r#"<text x="50" y="20" text-anchor="middle">HH<tspan x="50" dy="12">HHHH</tspan><tspan x="50" dy="12" text-anchor="start">H</tspan></text>"#;
        assert_eq!(runs(lines).iter().map(|(_, d)| d.split(' ').next().unwrap().to_owned()).collect::<Vec<_>>(), ["M45", "M39", "M51"]);
    }

    #[test]
    fn spacing_goes_after_each_character() {
        assert_eq!(runs(r#"<text letter-spacing="2">HH</text>"#), [(2, "M1 0 V-7 H5 V0 Z M9 0 V-7 H13 V0 Z".into())]);
        assert_eq!(cells(r#"<text letter-spacing="2">HH</text>"#).2, 16.0, "after the last one too");
        assert_eq!(cells(r#"<text word-spacing="5">H H</text>"#).2, 6.0 + 3.0 + 5.0 + 6.0);
        assert_eq!(cells(r#"<text x="50" text-anchor="end" letter-spacing="1em">H</text>"#), (34.0, -8.0, 16.0, 10.0));
    }

    #[test]
    fn the_baseline_asked_for_sits_at_y() {
        let top = |baseline: &str| near(laid(&format!(r#"<text y="50" dominant-baseline="{baseline}">H</text>"#)).unwrap().runs[0].outline.bounds().unwrap().min.y);
        assert_eq!((top("auto"), top("alphabetic")), (43.0, 43.0));
        assert_eq!(top("middle"), 45.5, "half an x-height lower");
        assert_eq!(top("central"), 46.0, "halfway between the font's top and bottom");
        assert_eq!((top("text-before-edge"), top("text-after-edge")), (51.0, 41.0));
        assert_eq!((top("hanging"), top("mathematical")), (49.4, 47.0));
    }

    #[test]
    fn fonts_are_chosen_a_run_at_a_time() {
        // The bold face's boxes are 0.05 em wider on each side.
        assert_eq!(runs(r#"<text>H<tspan font-weight="bold">H</tspan></text>"#), [(2, "M1 0 V-7 H5 V0 Z".into()), (3, "M6.5 0 V-7 H11.5 V0 Z".into())]);
        assert_eq!(runs(r#"<text font-weight="700" font-size="20">I</text>"#), [(2, "M1 0 V-14 H5 V0 Z".into())]);
        // A span that only paints differently is still one run with its neighbours.
        assert_eq!(runs(r#"<text>H<tspan fill="red">H</tspan>H</text>"#).iter().map(|r| r.0).collect::<Vec<_>>(), [2, 3, 2]);
    }

    #[test]
    fn what_cannot_be_set_says_why() {
        assert_eq!(laid(r##"<text><textPath href="#p">H</textPath></text>"##), Err(Unset::OnPath));
        assert_eq!(laid(r#"<text x="1 2 3">HHH</text>"#), Err(Unset::Placed));
        assert_eq!(laid(r#"<text>H<tspan dy="1,2">HH</tspan></text>"#), Err(Unset::Placed));
        assert_eq!(laid(r#"<text rotate="30">H</text>"#), Err(Unset::Placed));
        assert!(laid(r#"<text rotate="0">H</text>"#).is_ok(), "no turn is no turn");
        assert_eq!(laid(r#"<text writing-mode="vertical-rl">H</text>"#), Err(Unset::Upright));
        assert_eq!(laid(r#"<text textLength="40">H</text>"#), Err(Unset::Stretched));
        assert!(Unset::OnPath.to_string().contains("<textPath>"));
    }

    #[test]
    fn a_text_says_its_words_whether_or_not_it_can_be_set() {
        let d = Document::parse(DocId(1), "<svg><text>\n  Hop <tspan>on</tspan>\n  <tspan display=\"none\">not</tspan><title>nor this</title> &amp; in\n</text><text><textPath>round</textPath> we go</text></svg>").unwrap();
        let (plain, on_path) = (d.node(NodeId(2)).unwrap(), d.node(NodeId(6)).unwrap());
        assert_eq!((said(&d, plain).as_str(), unset(&d, plain)), ("Hop on & in", None));
        assert_eq!((said(&d, on_path).as_str(), unset(&d, on_path)), ("round we go", Some(Unset::OnPath)));
        assert_eq!(lines(&d, plain), ["Hop on & in"]);
        let three = Document::parse(DocId(1), r#"<svg><text x="1" y="2">one<tspan x="1" dy="1.2em">two <tspan fill="red">red</tspan></tspan><tspan y="9">three</tspan><tspan dx="1">!</tspan></text><text/></svg>"#).unwrap();
        assert_eq!(lines(&three, three.node(NodeId(2)).unwrap()), ["one", "two red", "three!"], "a nudge isn't a new line");
        assert_eq!(lines(&three, three.node(NodeId(7)).unwrap()), [""]);
    }

    #[test]
    fn a_text_says_which_fonts_it_ended_up_in() {
        test_fonts();
        let d = Document::parse(DocId(1), r#"<svg><text font-family="No Such Font, 'Ink Test', serif" font-size="10">a<tspan font-weight="bold" font-size="20">b</tspan><tspan>c</tspan></text></svg>"#).unwrap();
        let fonts = lettered(&d, d.node(NodeId(2)).unwrap());
        assert_eq!(fonts.len(), 2, "a span lettered as its text is no new font: {fonts:?}");
        assert_eq!(fonts[0], Lettered { family: "Ink Test".into(), bold: false, italic: false, size: 10.0, missing: vec!["No Such Font".into()] });
        assert_eq!((fonts[1].bold, fonts[1].size), (true, 20.0));
    }

    #[test]
    fn a_run_is_painted_as_its_elements_say() {
        test_fonts();
        let d = Document::parse(DocId(1), r#"<svg><text fill="red" stroke-width="2">H<tspan stroke="blue"><tspan fill="lime">H</tspan></tspan></text></svg>"#).unwrap();
        let text = d.node(NodeId(2)).unwrap();
        let own = Style::default().cascade(text);
        assert_eq!(style_of(&d, text, &own, NodeId(2)), own);
        let inner = style_of(&d, text, &own, NodeId(4));
        assert_eq!((inner.fill, inner.stroke, inner.line.width), (crate::style::Paint::parse("lime").unwrap(), crate::style::Paint::parse("blue").unwrap(), 2.0));
    }
}
