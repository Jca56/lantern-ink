//! Setting gathered characters: where each glyph goes.
//!
//! The characters are shaped a run at a time: as far as one font goes
//! with no jump in it (elements that only paint differently share a
//! run, so they kern as one word). The pen walks from the text's
//! position; an element's `x` or `y` puts it somewhere else and starts
//! a new chunk, its `dx` and `dy` nudge it. When a chunk ends it's hung
//! from where it started by its `text-anchor`.

use ink_geom::{Affine, Path, Rect, Vec2};

use super::font::Anchor;
use super::{Laid, Piece, Run, Start};
use crate::fonts;

/// A glyph that draws, where it goes.
struct Placed {
    piece: usize,
    at: Vec2,
    size: f64,
    /// For a font size of 1, about its place.
    outline: Path,
}

/// What's hung together from one position.
struct Chunk {
    /// Its first glyph and its first cell, among all of them.
    glyphs: usize,
    cells: usize,
    /// Where its first character went.
    start: f64,
    anchor: Anchor,
}

impl Chunk {
    /// Hang it, now that the pen has reached `end`: move everything in
    /// it so that its start, its middle or its end is where it started.
    /// Returns how far that was.
    fn hang(&self, placed: &mut [Placed], cells: &mut [Rect], end: f64) -> f64 {
        let shift = match self.anchor {
            Anchor::Start => return 0.0,
            Anchor::Middle => (self.start - end) / 2.0,
            Anchor::End => self.start - end,
        };
        for glyph in &mut placed[self.glyphs..] {
            glyph.at.x += shift;
        }
        for cell in &mut cells[self.cells..] {
            *cell = cell.translate(Vec2::new(shift, 0.0));
        }
        shift
    }
}

/// Set `chars` (each with the piece it's of), `starts` saying where
/// elements put their first characters (outer elements first).
pub(super) fn set(pieces: &[Piece], chars: &[(char, usize)], starts: &[Start]) -> Laid {
    // Where the pen jumps: the innermost element at a character has the
    // say, in each of the four.
    let mut jumps: Vec<Option<Start>> = vec![None; chars.len()];
    for start in starts {
        let jump = jumps[start.at].get_or_insert_with(Start::default);
        *jump = Start { at: start.at, x: start.x.or(jump.x), y: start.y.or(jump.y), dx: start.dx.or(jump.dx), dy: start.dy.or(jump.dy) };
    }
    let faces: Vec<fonts::Face> = pieces.iter().map(|piece| piece.font.face()).collect();
    let shapes_with = |a: usize, b: usize| a == b || (faces[a] == faces[b] && pieces[a].font.size == pieces[b].font.size && pieces[a].font.baseline == pieces[b].font.baseline);

    let (mut placed, mut cells): (Vec<Placed>, Vec<Rect>) = (Vec::new(), Vec::new());
    let mut pictures = 0;
    let mut pen = Vec2::ZERO;
    let mut chunk = Chunk { glyphs: 0, cells: 0, start: 0.0, anchor: Anchor::Start };
    let mut from = 0;
    while from < chars.len() {
        let mut to = from + 1;
        while to < chars.len() && jumps[to].is_none() && shapes_with(chars[to - 1].1, chars[to].1) {
            to += 1;
        }
        let piece = chars[from].1;
        let (font, face) = (&pieces[piece].font, &faces[piece]);
        let jump = jumps[from].unwrap_or_default();
        let fresh = from == 0 || jump.x.is_some() || jump.y.is_some();
        if fresh {
            // What follows a hung chunk goes on from where it ends up.
            pen.x += chunk.hang(&mut placed, &mut cells, pen.x);
            pen = Vec2::new(jump.x.unwrap_or(pen.x), jump.y.unwrap_or(pen.y));
        }
        pen += Vec2::new(jump.dx.unwrap_or(0.0), jump.dy.unwrap_or(0.0));
        if fresh {
            chunk = Chunk { glyphs: placed.len(), cells: cells.len(), start: pen.x, anchor: font.anchor };
        }

        let said: String = chars[from..to].iter().map(|c| c.0).collect();
        let line = fonts::shape(&said, face);
        let size = font.size;
        let (ascent, descent) = (line.ascent * size, line.descent * size);
        let drop = font.drop(face, ascent, descent);
        // Which character a glyph stands for, from where in the run's
        // bytes it starts.
        let offsets: Vec<usize> = said.char_indices().map(|(at, _)| at).collect();
        let character = |at: usize| from + offsets.partition_point(|&o| o <= at).saturating_sub(1);
        // What's added after a character: its element's letter spacing,
        // and its word spacing after a space.
        let gap = |c: usize| {
            let font = &pieces[chars[c].1].font;
            font.letter_spacing + if matches!(chars[c].0, ' ' | '\u{a0}') { font.word_spacing } else { 0.0 }
        };
        // The pen along the line as shaped, and the spacing put in so
        // far. A character's spacing is owed once the next one comes (a
        // mark on it, or the rest of a ligature, isn't the next one).
        let (mut along, mut extra) = (0.0, 0.0);
        let mut owed: Option<(usize, usize)> = None;
        for glyph in line.glyphs {
            let c = character(glyph.at);
            match owed {
                Some((at, last)) if at != glyph.at && glyph.advance > 0.0 => {
                    extra += gap(last);
                    if let Some(cell) = cells.last_mut() {
                        cell.max.x += gap(last);
                    }
                    owed = Some((glyph.at, c));
                }
                None => owed = Some((glyph.at, c)),
                Some(_) => {}
            }
            let origin = pen + Vec2::new(extra, drop);
            cells.push(Rect::new(Vec2::new(origin.x + along * size, origin.y - ascent), Vec2::new(origin.x + (along + glyph.advance) * size, origin.y + descent)));
            along += glyph.advance;
            pictures += usize::from(glyph.picture);
            if !glyph.outline.is_empty() && size > 0.0 {
                placed.push(Placed { piece: chars[c].1, at: origin + glyph.place * size, size, outline: glyph.outline });
            }
        }
        if let Some((_, last)) = owed {
            extra += gap(last);
            if let Some(cell) = cells.last_mut() {
                cell.max.x += gap(last);
            }
        }
        pen.x += line.width * size + extra;
        from = to;
    }
    chunk.hang(&mut placed, &mut cells, pen.x);

    // Glyphs of one element in a row are one run.
    let mut runs: Vec<Run> = Vec::new();
    for glyph in placed {
        let node = pieces[glyph.piece].node;
        let outline = glyph.outline.transformed(&Affine::new(glyph.size, 0.0, 0.0, glyph.size, glyph.at.x, glyph.at.y));
        match runs.last_mut() {
            Some(run) if run.node == node => run.outline.subpaths.extend(outline.subpaths),
            _ => runs.push(Run { node, outline }),
        }
    }
    Laid { runs, cells: cells.into_iter().reduce(|a, b| a.union(&b)), pictures }
}
