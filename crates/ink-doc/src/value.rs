//! Typed values as attribute text (ARCHITECTURE §3.4). The one place a
//! number Ink writes is formatted, so every Command rounds the same way
//! (D15): three decimals unless the document says otherwise, nothing
//! that says nothing (no trailing zeros, no `-0`).

use ink_geom::{Affine, Path, Vec2, number};

use crate::document::Document;
use crate::kind::{INK_NS, INK_PREFIX};
use crate::transform;
use crate::viewport::Viewport;

/// The decimals a document's numbers are written with, unless its root
/// says otherwise.
pub const DECIMALS: usize = 3;
/// The most a document may ask for.
pub const MAX_DECIMALS: usize = 8;
/// The root attribute (in Ink's namespace) that says otherwise.
pub const DECIMALS_ATTR: &str = "decimals";

/// How a document's numbers are written, and how close two things are
/// before they are the same thing as far as the file can say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Precision {
    pub decimals: usize,
    /// How far from the origin the drawing reaches: a turn or a scale
    /// off by a little moves a point that far out by that much more.
    pub reach: f64,
}

impl Precision {
    /// The precision `doc` is written with: its root's `ink:decimals`,
    /// else [`DECIMALS`]; reaching as far as its page does.
    pub fn of(doc: &Document) -> Precision {
        let root = doc.get(doc.root());
        let said = root.filter(|_| doc.namespace(doc.root(), Some(INK_PREFIX)) == Some(INK_NS)).and_then(|r| r.attr(&format!("{INK_PREFIX}:{DECIMALS_ATTR}")));
        let decimals = said.and_then(|d| d.trim().parse::<usize>().ok()).filter(|d| *d <= MAX_DECIMALS).unwrap_or(DECIMALS);
        // The page's corners, in the drawing's coordinates.
        let reach = root.map(Viewport::of).map_or(1.0, |v| {
            let far = v.to_page.inverse().map_or(v.size, |back| back.apply(Vec2::ZERO).abs().max(back.apply(v.size).abs()));
            far.x.max(far.y)
        });
        Precision { decimals, reach: if reach.is_finite() { reach.max(1.0) } else { 1.0 } }
    }

    /// The same precision for something reaching `reach` from the
    /// origin, if that's further than the page does.
    pub fn reaching(self, reach: f64) -> Precision {
        Precision { reach: if reach.is_finite() { self.reach.max(reach) } else { self.reach }, ..self }
    }

    /// Half a unit in the last place written: two points closer than
    /// this are written the same.
    pub fn within(&self) -> f64 {
        0.5 * 10f64.powi(-(self.decimals as i32))
    }

    /// Whether two transforms put everything within reach in the same
    /// place, as far as the file can say.
    pub fn same(&self, a: &Affine, b: &Affine) -> bool {
        a.gap(b, self.reach) <= self.within()
    }

    /// `v` as it's written, and the number that then reads back.
    pub fn number(&self, v: f64) -> String {
        number::format(v, self.decimals)
    }

    /// `v` as the file will hold it: rounded to what's written.
    pub fn round(&self, v: f64) -> f64 {
        number::parse(&self.number(v)).unwrap_or(v)
    }

    /// A `points` list: `x,y x,y`.
    pub fn points(&self, points: &[Vec2]) -> String {
        points.iter().map(|p| format!("{},{}", self.number(p.x), self.number(p.y))).collect::<Vec<_>>().join(" ")
    }

    /// Path data.
    pub fn path(&self, path: &Path) -> String {
        path.to_data(self.decimals)
    }

    /// How many more decimals a factor or an angle takes than a
    /// coordinate, to place a point at the drawing's far edge as finely.
    fn extra(&self) -> usize {
        self.reach.log10().ceil().max(0.0) as usize
    }

    /// A `transform` value: `None` for a transform that changes nothing
    /// (the attribute isn't needed). Written the way a person would,
    /// when that says the same: a `translate`, a `scale` or a `rotate`
    /// about a point; a `matrix` otherwise.
    pub fn transform(&self, t: &Affine) -> Option<String> {
        if self.same(t, &Affine::IDENTITY) {
            return None;
        }
        let n = |v: f64| self.number(v);
        let factor = |v: f64| number::format(v, self.decimals + 1 + self.extra());
        let mut tries = vec![format!("translate({} {})", n(t.e), n(t.f)), if factor(t.a) == factor(t.d) { format!("scale({})", factor(t.a)) } else { format!("scale({} {})", factor(t.a), factor(t.d)) }];
        // A turn, about the one point it leaves where it was.
        let det = (1.0 - t.a) * (1.0 - t.d) - t.b * t.c;
        if det.abs() > 1e-12 {
            let degrees = number::format(t.b.atan2(t.a).to_degrees(), self.decimals + self.extra());
            // A point halfway between two written numbers has one more
            // decimal than they do.
            let half = |v: f64| number::format(v, self.decimals + 1);
            let (cx, cy) = (half(((1.0 - t.d) * t.e + t.c * t.f) / det), half((t.b * t.e + (1.0 - t.a) * t.f) / det));
            tries.push(if cx == "0" && cy == "0" { format!("rotate({degrees})") } else { format!("rotate({degrees} {cx} {cy})") });
        }
        // Each is kept only if it reads back as the transform it is for.
        let said = tries.into_iter().find(|text| self.same(&transform::parse(text), t));
        Some(said.unwrap_or_else(|| format!("matrix({} {} {} {} {} {})", factor(t.a), factor(t.b), factor(t.c), factor(t.d), n(t.e), n(t.f))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::DocId;

    fn precision(root: &str) -> Precision {
        Precision::of(&Document::parse(DocId(1), root).unwrap())
    }

    #[test]
    fn a_document_says_how_finely_it_is_written() {
        let icon = precision(r#"<svg viewBox="0 0 24 24"/>"#);
        assert_eq!((icon.decimals, icon.reach, icon.within()), (3, 24.0, 0.0005));
        assert_eq!((icon.number(1.23456), icon.round(1.23456), icon.number(-0.0001)), ("1.235".to_owned(), 1.235, "0".to_owned()));
        // Its page's far corner, wherever its viewBox starts.
        assert!((precision(r#"<svg viewBox="-250 100 500 300" width="50"/>"#).reach - 400.0).abs() < 1e-9);
        assert_eq!(precision("<svg/>").reach, 16.0);
        // What its root asks for, in Ink's own namespace.
        let fine = precision(r#"<svg xmlns:ink="urn:lantern:ink" ink:decimals="5"/>"#);
        assert_eq!((fine.decimals, fine.number(1.23456789)), (5, "1.23457".to_owned()));
        for other in [r#"<svg ink:decimals="5"/>"#, r#"<svg xmlns:ink="urn:other" ink:decimals="5"/>"#, r#"<svg xmlns:ink="urn:lantern:ink" ink:decimals="99"/>"#, r#"<svg xmlns:ink="urn:lantern:ink" ink:decimals="two"/>"#] {
            assert_eq!(precision(other).decimals, 3, "{other}");
        }
        assert_eq!(icon.points(&[Vec2::new(1.0, 2.5), Vec2::new(-0.00001, 4.0)]), "1,2.5 0,4");
        assert_eq!(icon.reaching(500.0).reach, 500.0);
        assert_eq!(icon.reaching(2.0).reach, 24.0);
    }

    #[test]
    fn a_transform_is_written_the_way_a_person_would() {
        let p = precision(r#"<svg viewBox="0 0 24 24"/>"#);
        let t = |t: Affine| p.transform(&t).unwrap();
        assert_eq!(p.transform(&Affine::IDENTITY), None);
        assert_eq!(p.transform(&Affine::translate(0.0001, 0.0)), None, "nothing the file could say");
        assert_eq!(t(Affine::translate(2.0, 3.5)), "translate(2 3.5)");
        assert_eq!(t(Affine::translate(-4.0, 0.0)), "translate(-4 0)");
        assert_eq!(t(Affine::scale(2.0, 2.0)), "scale(2)");
        assert_eq!(t(Affine::scale(-1.0, 1.5)), "scale(-1 1.5)");
        assert_eq!(t(Affine::rotate(45f64.to_radians())), "rotate(45)");
        assert_eq!(t(Affine::rotate(45f64.to_radians()).about(Vec2::new(8.0, 8.5))), "rotate(45 8 8.5)");
        assert_eq!(t(Affine::rotate(-90f64.to_radians()).about(Vec2::new(1.0005, 2.0))), "rotate(-90 1.0005 2)", "a centre between two written numbers");
        assert_eq!(t(Affine::rotate(std::f64::consts::PI).about(Vec2::new(12.0, 12.0))), "rotate(180 12 12)");
        // What none of those says is a matrix, its factors finer than
        // its moves.
        assert_eq!(t(Affine::scale(2.0, 2.0).then(&Affine::translate(1.0, 0.0))), "matrix(2 0 0 2 1 0)");
        assert_eq!(t(Affine::skew_x(0.5)), "matrix(1 0 0.546302 1 0 0)");
        // Every one of them reads back as what it was.
        for a in [Affine::rotate(0.123).about(Vec2::new(7.0, -3.0)), Affine::scale(1.5, 0.25).then(&Affine::rotate(2.0)).then(&Affine::translate(3.0, 4.0)), Affine::rotate(1e-4).about(Vec2::new(5000.0, 0.0))] {
            let text = p.transform(&a).unwrap();
            assert!(p.same(&transform::parse(&text), &a), "{text}");
        }
        // A bigger drawing takes more decimals for the same care.
        let big = precision(r#"<svg viewBox="0 0 500 500"/>"#);
        assert_eq!(big.transform(&Affine::skew_x(0.5)).unwrap(), "matrix(1 0 0.5463025 1 0 0)");
    }
}
