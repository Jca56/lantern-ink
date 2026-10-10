//! Guides (ARCHITECTURE §3.3, D19): lines a person lays over a drawing
//! to line things up by, kept in the file as one of Ink's own marks on
//! the root: `ink:guides="x12 y4.5"`. `x12` is a line down the page at
//! x = 12, `y4.5` one across it at y = 4.5, in the document's
//! coordinates (the root's user units). They're no part of what the
//! drawing draws: other programs ignore them, and a clean copy to ship
//! leaves them out with the rest of Ink's marks.

use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::value::Precision;

/// The mark's name, after Ink's prefix.
pub const GUIDES: &str = "guides";

/// One guide: a line down the page at an x, or across it at a y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Guide {
    X(f64),
    Y(f64),
}

impl Guide {
    /// Where it is, along the way it doesn't run.
    pub fn at(self) -> f64 {
        match self {
            Guide::X(at) | Guide::Y(at) => at,
        }
    }

    /// The same kind of guide, at `at`.
    pub fn moved(self, at: f64) -> Guide {
        match self {
            Guide::X(_) => Guide::X(at),
            Guide::Y(_) => Guide::Y(at),
        }
    }
}

/// The guides `doc` keeps, in the order it says them. What isn't one
/// (a word from a later Ink, a slip of a hand) is passed over.
pub fn of(doc: &Document) -> Vec<Guide> {
    let Some(said) = doc.mark(doc.root(), GUIDES) else { return Vec::new() };
    said.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter_map(|word| {
            let at = word.get(1..)?.parse::<f64>().ok().filter(|at| at.is_finite())?;
            match word.chars().next()? {
                'x' | 'X' => Some(Guide::X(at)),
                'y' | 'Y' => Some(Guide::Y(at)),
                _ => None,
            }
        })
        .collect()
}

impl Document {
    /// See [`crate::Command::SetGuides`]. Whether anything changed.
    pub(crate) fn set_guides(&mut self, guides: &[Guide]) -> Result<bool, DocError> {
        if guides.iter().any(|g| !g.at().is_finite()) {
            return invalid("a guide is at a number: one of these isn't");
        }
        let root = self.root();
        let p = Precision::of(self);
        let words: Vec<String> = guides
            .iter()
            .map(|g| match g {
                Guide::X(at) => format!("x{}", p.number(*at)),
                Guide::Y(at) => format!("y{}", p.number(*at)),
            })
            .collect();
        let say = words.join(" ");
        match (self.mark(root, GUIDES), say.is_empty()) {
            (None, true) => Ok(false),
            (Some(said), false) if said == say => Ok(false),
            (_, none) => {
                let name = self.mark_name(root, GUIDES)?;
                self.set_attr(root, &name, (!none).then_some(say.as_str()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;

    fn doc(root: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg xmlns=\"http://www.w3.org/2000/svg\"{root}><rect width=\"4\" height=\"4\"/></svg>")).unwrap()
    }

    #[test]
    fn guides_are_a_mark_on_the_root() {
        let mut d = doc(" viewBox=\"0 0 24 24\"");
        assert!(of(&d).is_empty());
        // The first one brings Ink's namespace with it; numbers are
        // written as the drawing writes its own.
        let set = |d: &mut Document, guides: Vec<Guide>| d.apply(&Command::SetGuides { guides });
        let applied = set(&mut d, vec![Guide::X(12.0), Guide::Y(4.50004), Guide::X(-3.25)]).unwrap();
        assert_eq!(applied.changed, [d.root()]);
        assert!(d.to_svg().starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" xmlns:ink=\"urn:lantern:ink\" ink:guides=\"x12 y4.5 x-3.25\">"), "{}", d.to_svg());
        assert_eq!(of(&d), [Guide::X(12.0), Guide::Y(4.5), Guide::X(-3.25)]);
        // The same again is nothing; none at all takes the mark off.
        assert!(set(&mut d, vec![Guide::X(12.0), Guide::Y(4.5), Guide::X(-3.25)]).unwrap().is_nothing());
        set(&mut d, Vec::new()).unwrap();
        assert!(!d.to_svg().contains("guides") && of(&d).is_empty());
        assert!(set(&mut d, Vec::new()).unwrap().is_nothing());
        assert_eq!(set(&mut d, vec![Guide::Y(f64::NAN)]).unwrap_err().to_string(), "a guide is at a number: one of these isn't");
        assert_eq!((Guide::X(2.0).moved(5.0), Guide::Y(2.0).moved(5.0).at()), (Guide::X(5.0), 5.0));
    }

    #[test]
    fn what_is_no_guide_is_passed_over() {
        let d = doc(" xmlns:ink=\"urn:lantern:ink\" ink:guides=\" x1.5,Y20; z3 x y-2 xone 7 \"");
        assert_eq!(of(&d), [Guide::X(1.5), Guide::Y(20.0), Guide::Y(-2.0)]);
        // Another namespace's `guides` isn't Ink's.
        let other = doc(" xmlns:b=\"urn:other\" b:guides=\"x1\"");
        assert!(of(&other).is_empty());
    }
}
