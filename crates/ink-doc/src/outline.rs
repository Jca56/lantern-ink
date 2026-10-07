//! A path as its anchors (ARCHITECTURE §3.1, D18): the points it goes
//! through, each with an id that is its own for as long as the document
//! is open, and how each is joined to the next. This is what path
//! editing works on; the path's `d` is still the truth, and an
//! [`Outline`] is read from it and written back to it.
//!
//! The ids are kept beside the path on its node, never in the file. A
//! change to `d` that leaves the path the same shape of thing (as many
//! runs, as many anchors in each: a move, a scale) keeps them; any
//! other gives it new ones. An edit made through an outline says
//! exactly which anchors are which.

use core::fmt;
use core::str::FromStr;
use std::sync::Arc;

use ink_geom::{ArcTo, Path, Piece, Seg, Subpath, Vec2};

use crate::document::Document;
use crate::error::DocError;
use crate::id::{NodeId, ParseIdError};
use crate::kind::Kind;
use crate::value::Precision;

/// An anchor of a path, per document: `A7`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnchorId(pub u64);

impl fmt::Display for AnchorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "A{}", self.0)
    }
}

impl FromStr for AnchorId {
    type Err = ParseIdError;

    /// `"A7"` → `AnchorId(7)`. Ids start at 1.
    fn from_str(s: &str) -> Result<Self, ParseIdError> {
        let number = s.strip_prefix('A').filter(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())).and_then(|rest| rest.parse::<u64>().ok()).filter(|n| *n > 0);
        number.map(AnchorId).ok_or_else(|| ParseIdError { text: s.to_owned(), expected: "A" })
    }
}

/// How one anchor is joined to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Link {
    Line,
    /// One control point, shared by both ends.
    Quad { c: Vec2 },
    /// A control point for the anchor it leaves, and one for the anchor
    /// it comes to.
    Cubic { c1: Vec2, c2: Vec2 },
    Arc { arc: ArcTo },
}

impl Link {
    /// As a segment ending at `to`.
    pub fn seg(&self, to: Vec2) -> Seg {
        match *self {
            Link::Line => Seg::Line { to },
            Link::Quad { c } => Seg::Quad { c, to },
            Link::Cubic { c1, c2 } => Seg::Cubic { c1, c2, to },
            Link::Arc { arc } => Seg::Arc { arc, to },
        }
    }

    /// What joins a segment's two ends.
    pub fn of(seg: &Seg) -> Link {
        match *seg {
            Seg::Line { .. } => Link::Line,
            Seg::Quad { c, .. } => Link::Quad { c },
            Seg::Cubic { c1, c2, .. } => Link::Cubic { c1, c2 },
            Seg::Arc { arc, .. } => Link::Arc { arc },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub id: AnchorId,
    pub at: Vec2,
}

/// One run of anchors: a subpath. `links[i]` joins `anchors[i]` to the
/// next one, which for the last link of a closed run is the first
/// anchor again. An open run has one link fewer than anchors; a closed
/// one as many.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub anchors: Vec<Anchor>,
    pub links: Vec<Link>,
    pub closed: bool,
}

impl Run {
    /// The anchor after the one at `i` (round to the first, when
    /// closed); `None` past an open run's end.
    pub fn next(&self, i: usize) -> Option<usize> {
        if i + 1 < self.anchors.len() { Some(i + 1) } else { (self.closed && i < self.links.len()).then_some(0) }
    }

    /// The anchor before the one at `i`.
    pub fn prev(&self, i: usize) -> Option<usize> {
        if i > 0 { Some(i - 1) } else { (self.closed && self.links.len() == self.anchors.len()).then(|| self.anchors.len() - 1) }
    }

    /// The link that leaves the anchor at `i`, as a piece of line.
    pub fn piece(&self, i: usize) -> Option<Piece> {
        let to = self.anchors.get(self.next(i)?)?.at;
        Some(Piece::new(self.anchors[i].at, self.links.get(i)?.seg(to)))
    }

    fn subpath(&self) -> Subpath {
        let Some(first) = self.anchors.first() else { return Subpath::default() };
        // A closed run's last link, when it's a straight line, is what
        // closing draws anyway.
        let drawn = if self.closed && self.links.last() == Some(&Link::Line) { self.links.len() - 1 } else { self.links.len() };
        let segs = (0..drawn).filter_map(|i| Some(self.links[i].seg(self.anchors[self.next(i)?].at))).collect();
        Subpath { start: first.at, segs, closed: self.closed }
    }

    /// Make the run what its path reads back as, once written at `p`.
    /// A closed run's last anchor that is written where its first is,
    /// with only the line closing draws between them, isn't one the
    /// file can tell from the first: the segment that came to it is
    /// the one that closes the run, and the first anchor stands for
    /// both.
    fn settle(&mut self, p: &Precision) {
        while self.closed && self.links.len() == self.anchors.len() && self.links.last() == Some(&Link::Line) {
            let at: Vec<Vec2> = self.anchors.iter().map(|a| Vec2::new(p.round(a.at.x), p.round(a.at.y))).collect();
            if !comes_home(&at) {
                break;
            }
            self.anchors.pop();
            self.links.pop();
        }
    }
}

/// A path as runs of anchors.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub runs: Vec<Run>,
}

/// Whether the last of a closed subpath's points lies on its first:
/// the subpath has come home before it's closed.
fn comes_home(at: &[Vec2]) -> bool {
    let scale = at.iter().fold(1.0f64, |m, p| m.max(p.abs().max_element()));
    at.len() > 1 && at[at.len() - 1].distance(at[0]) <= 1e-9 * scale
}

/// A subpath's anchors (where each is) and links. A closed one whose
/// last segment comes back to where it started ends at its first anchor,
/// not at another on top of it.
fn laid_out(sub: &Subpath) -> (Vec<Vec2>, Vec<Link>) {
    let mut at = vec![sub.start];
    let mut links: Vec<Link> = sub.segs.iter().map(Link::of).collect();
    at.extend(sub.segs.iter().map(Seg::to));
    if sub.closed {
        if comes_home(&at) {
            at.pop();
        } else {
            links.push(Link::Line);
        }
    }
    (at, links)
}

/// How many anchors each of a path's runs has.
fn layout(path: &Path) -> Vec<usize> {
    path.subpaths.iter().map(|sub| laid_out(sub).0.len()).collect()
}

/// The ids of a path's anchors, kept on its node: in order, and how
/// many each run has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Anchored {
    pub ids: Vec<AnchorId>,
    pub runs: Vec<usize>,
}

impl Outline {
    /// `path` as runs of anchors, called by `ids` in order (as many as
    /// it has anchors).
    pub(crate) fn build(path: &Path, ids: &[AnchorId]) -> Outline {
        let mut ids = ids.iter().copied();
        let runs = path
            .subpaths
            .iter()
            .map(|sub| {
                let (at, links) = laid_out(sub);
                Run { anchors: at.into_iter().zip(&mut ids).map(|(at, id)| Anchor { id, at }).collect(), links, closed: sub.closed }
            })
            .collect();
        Outline { runs }
    }

    /// The path it draws.
    pub fn path(&self) -> Path {
        Path { subpaths: self.runs.iter().filter(|run| !run.anchors.is_empty()).map(Run::subpath).collect() }
    }

    /// Where the anchor `id` is: its run, and its place in it.
    pub fn find(&self, id: AnchorId) -> Option<(usize, usize)> {
        self.runs.iter().enumerate().find_map(|(r, run)| Some((r, run.anchors.iter().position(|a| a.id == id)?)))
    }

    /// Every anchor, in order.
    pub fn anchors(&self) -> impl Iterator<Item = &Anchor> {
        self.runs.iter().flat_map(|run| &run.anchors)
    }

    fn anchored(&self) -> Anchored {
        Anchored { ids: self.anchors().map(|a| a.id).collect(), runs: self.runs.iter().map(|run| run.anchors.len()).collect() }
    }

    /// Make the outline what its path reads back as, once written at
    /// `p`: each run settled (see [`Run::settle`]), and no run without
    /// an anchor. What it draws stays, to what `p` writes.
    pub(crate) fn settle(&mut self, p: &Precision) {
        self.runs.retain(|run| !run.anchors.is_empty());
        self.runs.iter_mut().for_each(|run| run.settle(p));
    }
}

impl Document {
    /// The path `id` as runs of anchors. `None` for what isn't a path,
    /// and for one whose `d` can't all be read (it's drawn as far as it
    /// reads, but there'd be no writing it back whole).
    pub fn outline(&self, id: NodeId) -> Option<Outline> {
        let node = self.get(id).filter(|n| n.kind == Kind::Path)?;
        let parsed = Path::parse(node.attr("d").unwrap_or(""));
        let anchored = node.anchors.as_deref()?;
        (parsed.stopped_at.is_none() && anchored.runs == layout(&parsed.path)).then(|| Outline::build(&parsed.path, &anchored.ids))
    }

    /// See that the path `id` has an id for each of its anchors: the
    /// ones it had, when it's still as many runs of as many anchors;
    /// new ones otherwise. (What isn't a path has none.) Not a change to
    /// the document: nothing of it is drawn or written.
    pub(crate) fn reanchor(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(&id) else { return };
        let wanted = (node.kind == Kind::Path).then(|| layout(&Path::parse(node.attr("d").unwrap_or("")).path));
        let have = node.anchors.as_deref().map(|a| &a.runs);
        if wanted.as_ref() == have {
            return;
        }
        let anchored = wanted.map(|runs| Arc::new(Anchored { ids: (0..runs.iter().sum()).map(|_| self.next.anchor()).collect(), runs }));
        if let Some(node) = self.nodes.get_mut(&id) {
            Arc::make_mut(node).anchors = anchored;
        }
    }

    /// Make the path `id` what `outline` says: its `d` written afresh
    /// (to the document's precision), and its anchors called what the
    /// outline calls them, the ones its written path has (the outline
    /// [settled](Outline::settle)). Whether that changed anything.
    pub(crate) fn set_outline(&mut self, id: NodeId, outline: &Outline) -> Result<bool, DocError> {
        let p = Precision::of(self);
        let mut outline = outline.clone();
        outline.settle(&p);
        let text = p.path(&outline.path());
        let node = self.node(id)?;
        let same = node.attr("d").is_some_and(|d| p.path(&Path::parse(d).path) == text);
        // Its anchors first, so that its new `d` finds them the right
        // shape and keeps them.
        let named = outline.anchored();
        debug_assert_eq!(layout(&Path::parse(&text).path), named.runs, "a settled outline reads back as it was written: {text}");
        let mut changed = node.anchors.as_deref() != Some(&named);
        if changed {
            self.edit(id)?.anchors = Some(Arc::new(named));
        }
        if !same {
            changed |= self.set_attr(id, "d", Some(&text))?;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use crate::id::DocId;

    fn doc(d: &str) -> Document {
        Document::parse(DocId(1), &format!("<svg><path d=\"{d}\"/></svg>")).unwrap()
    }

    const N: NodeId = NodeId(2);

    fn ids(outline: &Outline) -> Vec<Vec<u64>> {
        outline.runs.iter().map(|run| run.anchors.iter().map(|a| a.id.0).collect()).collect()
    }

    #[test]
    fn a_path_is_runs_of_anchors_with_ids_of_their_own() {
        let d = doc("M0 0 H10 V10 Z M20 0 C20 5 25 10 30 10 Q35 10 35 5");
        let outline = d.outline(N).unwrap();
        assert_eq!(ids(&outline), [vec![1, 2, 3], vec![4, 5, 6]]);
        let (square, curve) = (&outline.runs[0], &outline.runs[1]);
        // Closed: as many links as anchors, the last one back to the first.
        assert_eq!((square.closed, square.links.as_slice()), (true, &[Link::Line; 3][..]));
        assert_eq!((square.next(2), square.prev(0), square.piece(2).map(|p| p.to())), (Some(0), Some(2), Some(Vec2::ZERO)));
        // Open: one link fewer, and nothing past its ends.
        assert_eq!((curve.closed, curve.links.len(), curve.next(2), curve.prev(0)), (false, 2, None, None));
        assert!(matches!(curve.links[0], Link::Cubic { c2, .. } if c2 == Vec2::new(25.0, 10.0)));
        assert_eq!(outline.find(AnchorId(5)), Some((1, 1)));
        assert_eq!(outline.find(AnchorId(9)), None);
        assert_eq!(outline.path().to_data(3), "M0 0 H10 V10 Z M20 0 C20 5 25 10 30 10 Q35 10 35 5");
        assert_eq!(("A7".parse(), AnchorId(7).to_string()), (Ok(AnchorId(7)), "A7".to_owned()));
        for bad in ["", "A", "A0", "7", "a7", "N7", "A-1", "A7x"] {
            assert!(bad.parse::<AnchorId>().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_closed_run_that_comes_back_to_its_start_ends_at_its_first_anchor() {
        // A curve drawn all the way home, and a line: neither leaves an
        // anchor lying on the first.
        for (d, anchors, back) in [("M0 0 C0 10 10 10 10 0 C10 -10 0 -10 0 0 Z", 2, "M0 0 C0 10 10 10 10 0 C10 -10 0 -10 0 0 Z"), ("M0 0 H10 V10 H0 V0 Z", 4, "M0 0 H10 V10 H0 Z"), ("M5 5 Z", 1, "M5 5 Z"), ("M5 5", 1, "M5 5")] {
            let outline = doc(d).outline(N).unwrap();
            assert_eq!(outline.runs[0].anchors.len(), anchors, "{d}");
            assert_eq!(outline.path().to_data(3), back, "{d}");
        }
        assert!(doc("").outline(N).unwrap().runs.is_empty());
        // What can't all be read can't be picked apart.
        assert!(doc("M0 0 L5 5 nonsense").outline(N).is_none());
        assert!(Document::parse(DocId(1), "<svg><rect/></svg>").unwrap().outline(N).is_none());
    }

    #[test]
    fn anchors_keep_their_ids_while_the_path_keeps_its_shape() {
        let mut d = doc("M0 0 H10 V10 Z");
        let set = |d: &mut Document, value: &str| d.apply(&Command::SetAttr { node: N, name: "d".into(), value: Some(value.into()) }).map(|_| ());
        // Moved and scaled: the same three anchors.
        set(&mut d, "M2 3 L22 3 L22 23 Z").unwrap();
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![1, 2, 3]]);
        d.apply(&Command::Transform { nodes: vec![N], by: ink_geom::Affine::rotate(1.0) }).unwrap();
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![1, 2, 3]]);
        // A different shape of thing: new ones, never the old again.
        let before = d.snapshot();
        set(&mut d, "M0 0 H10 V10 H0 Z").unwrap();
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![4, 5, 6, 7]]);
        // Undone, it has the ones it had.
        d.restore(&before);
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![1, 2, 3]]);
        // A copy's anchors are its own.
        d.apply(&Command::Duplicate { nodes: vec![N] }).unwrap();
        assert_eq!(ids(&d.outline(NodeId(3)).unwrap()), [vec![8, 9, 10]]);
    }

    /// A small random number generator: the next of `n` choices.
    fn pick(seed: &mut u64, n: u64) -> usize {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 33) % n) as usize
    }

    /// Outlines of every awkward kind: anchors on top of each other, or
    /// a rounding apart; runs of one anchor; every way of joining two.
    /// Its anchors are called `first` and up.
    fn awkward(seed: &mut u64, first: u64) -> Outline {
        fn place(seed: &mut u64) -> Vec2 {
            Vec2::new([0.0, 5.0, 10.0][pick(seed, 3)] + [0.0, 0.0004, -0.0004, 0.0006][pick(seed, 4)], [0.0, 5.0][pick(seed, 2)])
        }
        let mut id = first;
        let mut runs = Vec::new();
        for _ in 0..1 + pick(seed, 3) {
            let (count, closed) = (1 + pick(seed, 5), pick(seed, 2) == 0);
            let mut anchors = Vec::new();
            for _ in 0..count {
                anchors.push(Anchor { id: AnchorId(id), at: place(seed) });
                id += 1;
            }
            let mut links = Vec::new();
            for _ in 0..if closed { count } else { count - 1 } {
                links.push(match pick(seed, 5) {
                    0 | 1 => Link::Line,
                    2 => Link::Quad { c: place(seed) },
                    3 => Link::Cubic { c1: place(seed), c2: place(seed) },
                    _ => Link::Arc { arc: ArcTo { rx: 4.0, ry: 3.0, rotation: 0.0, large: false, sweep: true } },
                });
            }
            runs.push(Run { anchors, links, closed });
        }
        Outline { runs }
    }

    #[test]
    fn whatever_outline_is_written_its_anchors_are_the_ones_it_reads_back() {
        let mut d = doc("M0 0");
        let (mut seed, mut first) = (7u64, 1000u64);
        for round in 0..4000 {
            let outline = awkward(&mut seed, first);
            let given: Vec<u64> = outline.anchors().map(|a| a.id.0).collect();
            first += given.len() as u64;
            d.set_outline(N, &outline).unwrap();
            // None of them was made up afresh (the document's own
            // counter is far below these): the path as written is as
            // many runs of as many anchors as were kept beside it.
            let read = d.outline(N).unwrap_or_else(|| panic!("round {round}: {:?}", d.node(N).unwrap().attr("d")));
            let kept: Vec<u64> = read.anchors().map(|a| a.id.0).collect();
            assert!(!kept.is_empty() && kept.iter().all(|id| given.contains(id)), "round {round}: {given:?} became {kept:?} in {:?}", d.node(N).unwrap().attr("d"));
            // And written again, it is the same path with the same names.
            assert_eq!(d.set_outline(N, &read), Ok(false), "round {round}");
        }
    }

    #[test]
    fn a_closed_run_given_its_first_anchor_twice_has_it_once() {
        let mut d = doc("M0 0");
        let at = |id: u64, x: f64, y: f64| Anchor { id: AnchorId(id), at: Vec2::new(x, y) };
        // Home by a curve, then closed: the curve is what closes it.
        let leaf = Outline { runs: vec![Run { anchors: vec![at(50, 0.0, 0.0), at(51, 10.0, 0.0), at(52, 0.0004, 0.0)], links: vec![Link::Line, Link::Quad { c: Vec2::new(5.0, 8.0) }, Link::Line], closed: true }] };
        d.set_outline(N, &leaf).unwrap();
        assert_eq!(d.node(N).unwrap().attr("d"), Some("M0 0 H10 Q5 8 0 0 Z"));
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![50, 51]]);
        // Home by lines, twice over: both go.
        let twice = Outline { runs: vec![Run { anchors: vec![at(60, 0.0, 0.0), at(61, 10.0, 0.0), at(62, 0.0, 0.0), at(63, 0.0, 0.0)], links: vec![Link::Line; 4], closed: true }] };
        d.set_outline(N, &twice).unwrap();
        assert_eq!(d.node(N).unwrap().attr("d"), Some("M0 0 H10 Z"));
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![60, 61]]);
        // Open, it is two anchors in one place, as the file says.
        let open = Outline { runs: vec![Run { anchors: vec![at(70, 0.0, 0.0), at(71, 10.0, 0.0), at(72, 0.0, 0.0)], links: vec![Link::Line; 2], closed: false }] };
        d.set_outline(N, &open).unwrap();
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![70, 71, 72]]);
    }

    #[test]
    fn an_outline_written_back_says_which_anchor_is_which() {
        let mut d = doc("M0 0 H10 V10 Z");
        let mut outline = d.outline(N).unwrap();
        // A fourth anchor between the second and third, by a name of
        // its own; the others keep theirs.
        outline.runs[0].anchors.insert(2, Anchor { id: AnchorId(40), at: Vec2::new(10.0, 5.0) });
        outline.runs[0].links.insert(1, Link::Line);
        assert_eq!(d.set_outline(N, &outline), Ok(true));
        assert_eq!(d.node(N).unwrap().attr("d"), Some("M0 0 H10 V5 V10 Z"));
        assert_eq!(ids(&d.outline(N).unwrap()), [vec![1, 2, 40, 3]]);
        assert_eq!(d.set_outline(N, &outline), Ok(false), "the same again changes nothing");
        // The file's own way of writing a path stays while nothing of it
        // changes.
        let mut relative = doc("m0 0h10v10z");
        let as_read = relative.outline(N).unwrap();
        assert_eq!(relative.set_outline(N, &as_read), Ok(false));
        assert_eq!(relative.node(N).unwrap().attr("d"), Some("m0 0h10v10z"));
    }
}
