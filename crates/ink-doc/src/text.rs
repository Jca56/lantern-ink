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
    /// Each of its characters' own cell, in the order [`said`] says
    /// them: where a caret stands before one (its left side) and after
    /// it (its right).
    pub chars: Vec<Rect>,
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

/// A text as it would be given to be written again
/// ([`crate::lettering::content`]): its lines, each a row of stretches
/// with what they set for themselves, and how far apart they're set.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    pub lines: Vec<Vec<crate::lettering::Span>>,
    /// In ems.
    pub leading: f64,
}

/// What `text` says, as lines to edit and write back: a new line
/// wherever an element starts one (its `x`), the empty lines between
/// counted from how far down it starts, and each stretch a `<tspan>`
/// letters or paints for itself a span with those properties. What a
/// span says that isn't a property of SVG's (an `id`, a `class`) isn't
/// kept by writing it again.
pub fn written(doc: &Document, text: &Node) -> Written {
    use crate::lettering::{LEADING, Span};
    let g = gather(doc, text, Vec2::ZERO);
    let leading = doc.leading(text.id).unwrap_or(LEADING);
    // The rows: the elements of the text's own that start a line, and
    // how many lines down each says it is.
    let rows: Vec<&Node> = text.elements().filter_map(|id| doc.get(id)).filter(|n| n.kind == Kind::TSpan && n.attr("x").is_some()).collect();
    let down = |row: &Node| row.attr("dy").and_then(|dy| dy.trim().strip_suffix("em")?.trim().parse::<f64>().ok()).map_or(1, |ems| (ems / leading).round().max(1.0) as usize);
    let row_of = |node: NodeId| std::iter::once(node).chain(doc.ancestors(node).map(|n| n.id)).find_map(|id| rows.iter().position(|row| row.id == id));
    // What a piece's characters set for themselves: what the spans
    // round them say, from the row (or the text) in.
    let set_of = |node: NodeId| -> Vec<(String, String)> {
        let mut between: Vec<&Node> = std::iter::once(node).chain(doc.ancestors(node).map(|n| n.id)).take_while(|id| *id != text.id && !rows.iter().any(|row| row.id == *id)).filter_map(|id| doc.get(id)).collect();
        between.reverse();
        let mut set: Vec<(String, String)> = Vec::new();
        for attr in between.iter().flat_map(|n| &n.attrs).filter(|a| crate::styling::check(&a.name, &a.value).is_ok()) {
            set.retain(|(name, _)| *name != attr.name);
            set.push((attr.name.clone(), attr.value.clone()));
        }
        set
    };
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    let mut row = None;
    let mut piece = usize::MAX;
    for &(c, of) in &g.chars {
        let node = g.pieces[of].node;
        let here = row_of(node);
        if here != row {
            // Another row: its line, the empty ones before it first
            // (as many lines down as it says: from the line before, or
            // for the first thing said, from the text's own place).
            lines.extend(std::iter::repeat_with(Vec::new).take(here.map_or(1, |i| down(rows[i]))));
            (row, piece) = (here, usize::MAX);
        }
        let line = lines.last_mut().expect("there is always a line");
        if of != piece {
            line.push(Span { text: String::new(), set: set_of(node) });
            piece = of;
        }
        // (Stretches set alike are one.)
        if line.len() >= 2 && line[line.len() - 1].text.is_empty() && line[line.len() - 2].set == line[line.len() - 1].set {
            line.pop();
        }
        line.last_mut().expect("just put there").text.push(c);
    }
    Written { lines, leading }
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
    /// Italic was asked for, and the family has none: it's upright.
    pub no_italic: bool,
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
        let italic = face.italic && crate::fonts::slants(&face);
        let set = Lettered { family: crate::fonts::called(&face.family), bold: face.bold, italic, no_italic: face.italic && !italic, size: piece.font.size, missing };
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
pub(crate) mod tests;
