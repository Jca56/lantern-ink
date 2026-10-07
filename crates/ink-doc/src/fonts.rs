//! The fonts text is set in (ARCHITECTURE §5.5): the machine's, found
//! once and shared by every drawing in the process. Shaping is
//! `lntrn-text`'s (fallback from font to font, ligatures, kerning,
//! right-to-left runs); what comes back here is plain geometry, for a
//! font size of 1: each glyph's outline as a [`Path`], and where it sits
//! on its line.

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use ink_geom::{Path, Seg, Vec2};
use lntrn_text::{LineGlyph, PathCmd, PlacedGlyph, TextEngine, TextStyle};

/// The px to the em that text is shaped at. At a big size nothing of an
/// outline is lost to the engine's f32, and at this one the usual fonts
/// (1000 or 2048 units to the em) come through exactly: a glyph's
/// numbers here are the font's own.
const EM: f32 = 1000.0;

/// What `sans-serif` and `monospace` stand for until [`defaults`] says:
/// Lantern's own, as its apps start with.
const SANS: &str = "Inter";
const MONO: &str = "JetBrains Mono";
/// What `serif` stands for: the first of these that's installed
/// (Lantern's own first).
const SERIFS: [&str; 6] = ["Lora", "Merriweather", "Noto Serif", "DejaVu Serif", "Liberation Serif", "FreeSerif"];
/// Where a character that came back as a picture is asked for again:
/// families that draw symbols and emoji as outlines.
const OUTLINED: [&str; 6] = ["DejaVu Sans", "Noto Sans Symbols", "Noto Sans Symbols 2", "Noto Emoji", "FreeSans", "Symbola"];

/// A family text can be set in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Family {
    /// Lantern's proportional default.
    Sans,
    /// Lantern's monospace default.
    Mono,
    /// One that's installed, by its name.
    Named(String),
}

/// A font, short of its size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    pub family: Family,
    pub bold: bool,
    pub italic: bool,
}

/// One glyph of a shaped line, for a font size of 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    /// Where in the text the characters it stands for start, in bytes.
    pub at: usize,
    /// Where it's drawn: along the line from its start, and below the
    /// baseline.
    pub place: Vec2,
    /// How far it moves the pen along the line.
    pub advance: f64,
    /// Its outline about its place (y down), filled non-zero. Empty for
    /// what draws nothing: a space.
    pub outline: Path,
    /// A colour glyph (an emoji) is a picture, not an outline: Ink
    /// doesn't draw those yet.
    pub picture: bool,
}

/// A line of text, shaped, for a font size of 1.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    /// In the order they're drawn, left to right.
    pub glyphs: Vec<Glyph>,
    /// The pen's advance over the whole line.
    pub width: f64,
    /// How far the font reaches above the baseline, and below it.
    pub ascent: f64,
    pub descent: f64,
}

static NAMES: OnceLock<(String, String)> = OnceLock::new();
static ENGINE: OnceLock<Mutex<TextEngine>> = OnceLock::new();

/// The machine's fonts, found the first time they're asked for.
fn engine() -> MutexGuard<'static, TextEngine> {
    let made = ENGINE.get_or_init(|| {
        let (sans, mono) = NAMES.get().map_or((SANS, MONO), |(sans, mono)| (sans.as_str(), mono.as_str()));
        Mutex::new(TextEngine::new(sans, mono))
    });
    // A panic while shaping leaves nothing half-written that the next
    // line would trip on.
    made.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Say which families `sans-serif` and `monospace` stand for (where
/// they're installed; Lantern's own otherwise, and for one not said).
/// It counts only before any text has been set: `false` when it came
/// too late.
pub fn defaults(sans: Option<&str>, mono: Option<&str>) -> bool {
    ENGINE.get().is_none() && NAMES.set((sans.unwrap_or(SANS).to_owned(), mono.unwrap_or(MONO).to_owned())).is_ok()
}

/// Every family installed, as the fonts write them, in alphabetical
/// order.
pub fn families() -> Vec<String> {
    engine().families()
}

/// Whether text asked for in `family` is set in it.
pub fn has(family: &str) -> bool {
    engine().has_family(family)
}

/// Add a font from its file's bytes (`.ttf`, `.otf`, `.ttc`), for this
/// process.
pub fn add(data: Vec<u8>) -> Result<(), String> {
    engine().load_font_data(data).map_err(|e| e.to_string())
}

/// What `family` is called on this machine: a named one's own name,
/// and for a generic one the family it stands for.
pub fn called(family: &Family) -> String {
    let (sans, mono) = NAMES.get().map_or((SANS, MONO), |(sans, mono)| (sans.as_str(), mono.as_str()));
    let standing = |wanted: &str, kind: &str| if has(wanted) { wanted.to_owned() } else { format!("this machine's {kind} font ({wanted} isn't installed)") };
    match family {
        Family::Sans => standing(sans, "sans"),
        Family::Mono => standing(mono, "monospace"),
        Family::Named(name) => name.clone(),
    }
}

/// The family a name in a `font-family` list stands for on this
/// machine: one of the generic names, or a family that's installed.
/// `None` for one that isn't: the next in the list gets its turn.
pub fn family(name: &str) -> Option<Family> {
    match name.to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" | "ui-sans-serif" | "ui-rounded" | "cursive" | "fantasy" => Some(Family::Sans),
        "monospace" | "ui-monospace" => Some(Family::Mono),
        "serif" | "ui-serif" | "math" => {
            let fonts = engine();
            Some(SERIFS.iter().find(|serif| fonts.has_family(serif)).map_or(Family::Sans, |serif| Family::Named((*serif).to_owned())))
        }
        _ => has(name).then(|| Family::Named(name.to_owned())),
    }
}

/// Close `path`'s last contour. A line the font draws back to where the
/// contour began is the closing itself.
fn close(path: &mut Path) {
    if let Some(contour) = path.subpaths.last_mut() {
        if matches!(contour.segs.last(), Some(Seg::Line { to }) if *to == contour.start) {
            contour.segs.pop();
        }
        contour.closed = true;
    }
}

/// An outline from the engine (px at [`EM`]), for a font size of 1.
/// Each of its contours is closed.
fn outline(cmds: &[PathCmd]) -> Path {
    let at = |p: [f32; 2]| Vec2::new(f64::from(p[0]), f64::from(p[1])) / f64::from(EM);
    let mut path = Path::new();
    for cmd in cmds {
        match *cmd {
            PathCmd::Move(p) => {
                close(&mut path);
                path.move_to(at(p));
            }
            PathCmd::Line(p) => {
                path.line_to(at(p));
            }
            PathCmd::Quad(c, p) => {
                path.quad_to(at(c), at(p));
            }
            PathCmd::Cubic(c1, c2, p) => {
                path.cubic_to(at(c1), at(c2), at(p));
            }
        }
    }
    close(&mut path);
    path
}

/// Shape one line of `text` in `face`. Text with a `\n` in it is shaped
/// up to there.
///
/// A character the engine falls back to a colour font for (a heart, an
/// emoji) comes back as a picture, which can't be filled or stroked:
/// each of those is asked for again in a family that draws it as an
/// outline, where one is installed, and the line closes up or opens
/// out around it.
pub fn shape(text: &str, face: &Face) -> Line {
    let mut line = shape_as_said(text, face);
    if !line.glyphs.iter().any(|g| g.picture) {
        return line;
    }
    let shaped = std::mem::take(&mut line.glyphs);
    // The pen as the engine had it, and as it is now.
    let (mut was, mut pen) = (0.0, 0.0);
    for (i, glyph) in shaped.iter().enumerate() {
        // The characters a glyph stands for run up to the next glyph's
        // (whichever side of it that one is drawn).
        let end = shaped.iter().map(|g| g.at).filter(|at| *at > glyph.at).min().unwrap_or(text.len()).min(text.len());
        let again = if glyph.picture && shaped.iter().position(|g| g.at == glyph.at) == Some(i) { text.get(glyph.at..end).and_then(|said| outlined(said, face)) } else { None };
        match again {
            Some(drawn) => {
                line.glyphs.extend(drawn.glyphs.into_iter().map(|g| Glyph { at: g.at + glyph.at, place: g.place + Vec2::new(pen, 0.0), ..g }));
                pen += drawn.width;
            }
            None => {
                line.glyphs.push(Glyph { place: glyph.place + Vec2::new(pen - was, 0.0), ..glyph.clone() });
                pen += glyph.advance;
            }
        }
        was += glyph.advance;
    }
    line.width = pen;
    line
}

/// `said` in the first family that draws all of it as outlines.
fn outlined(said: &str, face: &Face) -> Option<Line> {
    OUTLINED.iter().filter(|family| has(family)).find_map(|family| {
        let line = shape_as_said(said, &Face { family: Family::Named((*family).to_owned()), ..face.clone() });
        (line.glyphs.iter().all(|g| !g.picture) && line.glyphs.iter().any(|g| !g.outline.is_empty())).then_some(line)
    })
}

/// A line as the engine shapes it, pictures and all.
fn shape_as_said(text: &str, face: &Face) -> Line {
    let mut style = TextStyle::new(EM);
    style = match &face.family {
        Family::Sans => style,
        Family::Mono => style.mono(),
        Family::Named(name) => style.family(name),
    };
    if face.bold {
        style = style.bold();
    }
    if face.italic {
        style = style.italic();
    }
    let mut shaped: Vec<LineGlyph> = Vec::new();
    let metrics = engine().line_glyphs(text, &style, &mut shaped);
    let em = f64::from(EM);
    let glyphs = shaped
        .iter()
        .map(|g| Glyph {
            at: g.cluster as usize,
            place: Vec2::new(f64::from(g.x), f64::from(g.y)) / em,
            advance: f64::from(g.advance) / em,
            outline: match &g.glyph {
                Some(PlacedGlyph::Outline(cmds)) => outline(cmds),
                _ => Path::new(),
            },
            picture: matches!(g.glyph, Some(PlacedGlyph::Color { .. })),
        })
        .collect();
    Line { glyphs, width: f64::from(metrics.width) / em, ascent: f64::from(metrics.ascent) / em, descent: f64::from(metrics.descent) / em }
}

/// How tall `face`'s lowercase letters are, for a font size of 1: its
/// `x`, measured. Half an em where it has none.
pub fn x_height(face: &Face) -> f64 {
    shape("x", face).glyphs.first().and_then(|g| g.outline.bounds()).map_or(0.5, |b| -b.min.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_names_stand_for_lantern_s_own() {
        assert_eq!(family("sans-serif"), Some(Family::Sans));
        assert_eq!(family("Monospace"), Some(Family::Mono));
        assert!(matches!(family("serif"), Some(Family::Sans | Family::Named(_))));
        assert_eq!(family("No Such Font Anywhere"), None, "the next in the list gets its turn");
        assert!(!has("No Such Font Anywhere") && !has(""));
        assert!(!defaults(Some("Inter"), None), "too late once text has been set");
    }

    /// Needs a colour emoji font and one of the outlined families: says
    /// so and passes on a machine without them.
    #[test]
    fn a_picture_is_asked_for_again_as_an_outline() {
        let face = |family: Family| Face { family, bold: false, italic: false };
        // A heart falls to the colour font from some families and not
        // from others; a face does from them all.
        let tries = [("\u{2665}", face(Family::Mono)), ("\u{2665}", face(Family::Sans)), ("\u{1F600}", face(Family::Sans))];
        let Some((picture, face)) = tries.iter().find(|(said, face)| shape_as_said(said, face).glyphs.iter().any(|g| g.picture) && outlined(said, face).is_some()) else {
            eprintln!("no colour font falls in here, or nothing draws its glyphs as outlines: skipping");
            return;
        };
        let said = format!("a{picture}b");
        let (as_said, line) = (shape_as_said(&said, face), shape(&said, face));
        assert!(as_said.glyphs[1].picture && as_said.glyphs[1].outline.is_empty());
        assert!(line.glyphs.iter().all(|g| !g.picture && !g.outline.is_empty()), "every glyph an outline now");
        assert_eq!(line.glyphs.iter().map(|g| g.at).collect::<Vec<_>>(), [0, 1, 1 + picture.len()], "each still at its character");
        assert_eq!((line.glyphs[0].place, line.glyphs[0].advance), (as_said.glyphs[0].place, as_said.glyphs[0].advance));
        // What follows it starts where it now ends.
        let pen = line.glyphs[0].advance + line.glyphs[1].advance;
        assert!((line.glyphs[2].place.x - pen).abs() < 0.05 && (line.width - pen - line.glyphs[2].advance).abs() < 1e-9, "{line:?}");
        assert_eq!((line.ascent, line.descent), (as_said.ascent, as_said.descent), "the line is still its own font's");
    }
}
