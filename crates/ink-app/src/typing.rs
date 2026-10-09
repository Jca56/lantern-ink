//! Typing into a text (LS3's `text_keys.rs`, cut to what Ink types
//! so far): the words as lines of stretches, the caret among them, and
//! what each key does to both. No window here: words in, words out.
//!
//! A stretch (`Span`) is a run of characters with what they set for
//! themselves (a colour, a weight). Typing goes into the stretch the
//! caret is in, or at the end of the one before it, so what's typed
//! beside a red word is red; nothing here makes a new kind of stretch.
//! Places are counted in characters.

use ink_doc::lettering::Span;

/// A text's words: its lines, each a row of stretches. Always a line
/// at least.
#[derive(Clone, Debug, PartialEq)]
pub struct Words {
    pub lines: Vec<Vec<Span>>,
}

impl Default for Words {
    fn default() -> Words {
        Words { lines: vec![Vec::new()] }
    }
}

/// Where typing goes: before character `col` of line `line`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Caret {
    pub line: usize,
    pub col: usize,
    /// The place across it keeps to, going up and down through shorter
    /// lines.
    want: Option<usize>,
}

impl Caret {
    pub fn at(line: usize, col: usize) -> Caret {
        Caret { line, col, want: None }
    }
}

/// A way for the caret to go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    Left,
    Right,
    Up,
    Down,
    /// To its line's start, or its end.
    Home,
    End,
}

/// Which stretch of `line` the character place `col` is in, and where
/// in that stretch, in bytes: at a join, the end of the one before.
fn place(line: &[Span], col: usize) -> Option<(usize, usize)> {
    let mut before = 0;
    for (i, span) in line.iter().enumerate() {
        let count = span.text.chars().count();
        if col <= before + count {
            let at = span.text.char_indices().nth(col - before).map_or(span.text.len(), |(at, _)| at);
            return Some((i, at));
        }
        before += count;
    }
    None
}

impl Words {
    /// From lines as the drawing has them: one line at least.
    pub fn of(lines: Vec<Vec<Span>>) -> Words {
        if lines.is_empty() { Words::default() } else { Words { lines } }
    }

    /// How many characters line `line` has.
    pub fn len(&self, line: usize) -> usize {
        self.lines.get(line).map_or(0, |line| line.iter().map(|span| span.text.chars().count()).sum())
    }

    /// It says nothing at all.
    pub fn is_empty(&self) -> bool {
        self.lines.iter().flatten().all(|span| span.text.is_empty())
    }

    /// How many characters come before `caret`, among all the text's
    /// (a line break isn't one).
    pub fn before(&self, caret: Caret) -> usize {
        (0..caret.line).map(|line| self.len(line)).sum::<usize>() + caret.col
    }

    /// `caret`, kept to where these words have a place for it.
    pub fn within(&self, caret: Caret) -> Caret {
        let line = caret.line.min(self.lines.len().saturating_sub(1));
        Caret { line, col: caret.col.min(self.len(line)), want: caret.want }
    }

    /// Type `text` at `caret` (what can't stand in a line is left out),
    /// which goes on past it. Whether anything was typed.
    pub fn insert(&mut self, caret: &mut Caret, text: &str) -> bool {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if text.is_empty() {
            return false;
        }
        *caret = self.within(*caret);
        let line = &mut self.lines[caret.line];
        match place(line, caret.col) {
            Some((span, at)) => line[span].text.insert_str(at, &text),
            None => line.push(Span::plain(text.clone())),
        }
        *caret = Caret::at(caret.line, caret.col + text.chars().count());
        true
    }

    /// Enter: the line parts at `caret`, which goes to the new line's
    /// start.
    pub fn newline(&mut self, caret: &mut Caret) {
        *caret = self.within(*caret);
        let line = &mut self.lines[caret.line];
        let rest = match place(line, caret.col) {
            Some((span, at)) => {
                let tail = line[span].text.split_off(at);
                let mut rest: Vec<Span> = line.drain(span + 1..).collect();
                if !tail.is_empty() {
                    rest.insert(0, Span { text: tail, set: line[span].set.clone() });
                }
                rest
            }
            None => Vec::new(),
        };
        line.retain(|span| !span.text.is_empty());
        self.lines.insert(caret.line + 1, rest);
        *caret = Caret::at(caret.line + 1, 0);
    }

    /// Take out the character after `caret`; at a line's end, the next
    /// line joins it. Whether anything went.
    pub fn delete(&mut self, caret: &mut Caret) -> bool {
        *caret = Caret { want: None, ..self.within(*caret) };
        if caret.col == self.len(caret.line) {
            if caret.line + 1 >= self.lines.len() {
                return false;
            }
            let next = self.lines.remove(caret.line + 1);
            self.lines[caret.line].extend(next);
            return true;
        }
        let line = &mut self.lines[caret.line];
        // The character after the caret is the first of the stretch
        // after a join: the place one further along is in it.
        let Some((span, at)) = place(line, caret.col + 1) else { return false };
        let from = line[span].text[..at].char_indices().next_back().map_or(0, |(i, _)| i);
        line[span].text.replace_range(from..at, "");
        line.retain(|span| !span.text.is_empty());
        true
    }

    /// Backspace: the character before `caret` goes; at a line's start,
    /// the line joins the one before. Whether anything went.
    pub fn backspace(&mut self, caret: &mut Caret) -> bool {
        *caret = self.within(*caret);
        if caret.col == 0 && caret.line == 0 {
            return false;
        }
        self.go(caret, Go::Left);
        self.delete(caret)
    }

    /// Move `caret`.
    pub fn go(&self, caret: &mut Caret, way: Go) {
        let at = self.within(*caret);
        let lines = self.lines.len();
        *caret = match way {
            Go::Left if at.col > 0 => Caret::at(at.line, at.col - 1),
            Go::Left if at.line > 0 => Caret::at(at.line - 1, self.len(at.line - 1)),
            Go::Right if at.col < self.len(at.line) => Caret::at(at.line, at.col + 1),
            Go::Right if at.line + 1 < lines => Caret::at(at.line + 1, 0),
            Go::Home => Caret::at(at.line, 0),
            Go::End => Caret::at(at.line, self.len(at.line)),
            Go::Up | Go::Down => {
                let want = at.want.unwrap_or(at.col);
                let line = if way == Go::Up { at.line.saturating_sub(1) } else { (at.line + 1).min(lines.saturating_sub(1)) };
                Caret { line, col: want.min(self.len(line)), want: Some(want) }
            }
            Go::Left | Go::Right => Caret::at(at.line, at.col),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, fill: Option<&str>) -> Span {
        Span { text: text.to_owned(), set: fill.map(|f| ("fill".to_owned(), f.to_owned())).into_iter().collect() }
    }

    /// "Hi <red>pop</red> on" over "low".
    fn words() -> Words {
        Words::of(vec![vec![span("Hi ", None), span("pop", Some("red")), span(" on", None)], vec![span("low", None)]])
    }

    fn said(w: &Words) -> Vec<String> {
        w.lines.iter().map(|line| line.iter().map(|s| if s.set.is_empty() { s.text.clone() } else { format!("[{}]", s.text) }).collect()).collect()
    }

    #[test]
    fn typing_goes_into_the_stretch_the_caret_is_in() {
        let mut w = words();
        // In a stretch; at a join, on the end of the one before; at the
        // very start; on an empty line.
        let mut caret = Caret::at(0, 5);
        assert!(w.insert(&mut caret, "é!"));
        assert_eq!((said(&w)[0].as_str(), caret), ("Hi [poé!p] on", Caret::at(0, 7)));
        let mut caret = Caret::at(0, 8);
        w.insert(&mut caret, "s");
        assert_eq!(said(&w)[0], "Hi [poé!ps] on");
        let mut caret = Caret::at(0, 0);
        w.insert(&mut caret, "O");
        assert_eq!((said(&w)[0].as_str(), w.len(0), w.before(Caret::at(1, 2))), ("OHi [poé!ps] on", 13, 15));
        let mut blank = Words::default();
        let mut caret = Caret::at(3, 9);
        assert!(blank.insert(&mut caret, "a") && !blank.insert(&mut caret, "\n\t") && said(&blank) == ["a"] && caret == Caret::at(0, 1));
        assert!(Words::default().is_empty() && !w.is_empty() && Words::of(Vec::new()).lines.len() == 1);
    }

    #[test]
    fn enter_parts_a_line_and_the_deleting_keys_join_them() {
        let mut w = words();
        // Parted inside the red word: both halves are red.
        let mut caret = Caret::at(0, 4);
        w.newline(&mut caret);
        assert_eq!((said(&w), caret), (vec!["Hi [p]".to_owned(), "[op] on".to_owned(), "low".to_owned()], Caret::at(1, 0)));
        // Backspace at a line's start joins it to the one before.
        assert!(w.backspace(&mut caret));
        assert_eq!((said(&w)[0].as_str(), caret, w.lines.len()), ("Hi [p][op] on", Caret::at(0, 4), 2));
        // Delete takes the character after; at a line's end, the next
        // line comes up.
        assert!(w.delete(&mut caret) && w.delete(&mut caret));
        assert_eq!(said(&w)[0], "Hi [p] on");
        let mut caret = Caret::at(0, 99);
        assert!(w.delete(&mut caret));
        assert_eq!((said(&w), caret.col), (vec!["Hi [p] onlow".to_owned()], 7));
        // Backspace takes the one before, and a stretch left empty goes.
        let mut caret = Caret::at(0, 4);
        assert!(w.backspace(&mut caret));
        assert_eq!((said(&w)[0].as_str(), w.lines[0].len(), caret), ("Hi  onlow", 3, Caret::at(0, 3)));
        // Nothing before the start, nothing after the end.
        assert!(!w.backspace(&mut Caret::at(0, 0)) && !w.delete(&mut Caret::at(0, 99)));
        // Enter at a line's end, and on an empty one.
        let mut caret = Caret::at(0, 99);
        w.newline(&mut caret);
        w.newline(&mut caret);
        assert_eq!((w.lines.len(), w.lines[1].len(), caret), (3, 0, Caret::at(2, 0)));
    }

    #[test]
    fn the_caret_goes_about() {
        let w = Words::of(vec![vec![span("abcdef", None)], vec![span("xy", None)], Vec::new(), vec![span("12345", None)]]);
        let go = |from: Caret, ways: &[Go]| ways.iter().fold(from, |mut caret, way| {
            w.go(&mut caret, *way);
            caret
        });
        let at = |line: usize, col: usize| (line, col);
        let place = |c: Caret| (c.line, c.col);
        // Along a line, and round its ends to the next.
        assert_eq!((place(go(Caret::at(0, 6), &[Go::Right])), place(go(Caret::at(1, 0), &[Go::Left])), place(go(Caret::at(0, 3), &[Go::Left, Go::Left]))), (at(1, 0), at(0, 6), at(0, 1)));
        assert_eq!((place(go(Caret::at(0, 0), &[Go::Left])), place(go(Caret::at(3, 5), &[Go::Right]))), (at(0, 0), at(3, 5)));
        // Down through shorter lines and back out: it keeps its place
        // across.
        assert_eq!(place(go(Caret::at(0, 4), &[Go::Down])), at(1, 2));
        assert_eq!(place(go(Caret::at(0, 4), &[Go::Down, Go::Down, Go::Down])), at(3, 4));
        assert_eq!(place(go(Caret::at(0, 4), &[Go::Down, Go::Left, Go::Down, Go::Down])), at(3, 1));
        assert_eq!((place(go(Caret::at(0, 4), &[Go::Up])), place(go(Caret::at(3, 2), &[Go::Down]))), (at(0, 4), at(3, 2)));
        assert_eq!((place(go(Caret::at(0, 4), &[Go::Home])), place(go(Caret::at(0, 4), &[Go::End])), place(w.within(Caret::at(9, 9)))), (at(0, 0), at(0, 6), at(3, 5)));
    }
}
