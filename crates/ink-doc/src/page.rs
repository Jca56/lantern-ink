//! A drawing's page (ARCHITECTURE §3): the size its root asks to be
//! shown at (`width`, `height`), the coordinates that fill it (its
//! `viewBox`), and how finely Ink writes its numbers (`ink:decimals`,
//! D15). Setting them is one Command made here, so that every front
//! end sets a page the same way: the MCP's `doc_set`, the window's
//! File > Page.
//!
//! Nothing in the drawing moves when the viewBox changes, unless it's
//! asked to be fitted: then everything, the guides too, goes through
//! the transform that takes the old viewBox to the new, and the picture
//! sits in its new coordinates as it sat in the old (how a 500-unit
//! drawing becomes a 24-unit one in one step).

use ink_geom::number::parse_list;
use ink_geom::{Affine, Vec2};

use crate::command::Command;
use crate::document::Document;
use crate::error::{DocError, invalid};
use crate::guides::{self, Guide};
use crate::kind::{INK_NS, INK_PREFIX};
use crate::value::{DECIMALS_ATTR, MAX_DECIMALS, Precision};
use crate::viewport::Viewport;

/// What to make of a page: each part as given, or left as it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Page {
    /// The size it asks to be shown at, px.
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// `[x, y, width, height]`: the coordinates that fill the page.
    pub view_box: Option<[f64; 4]>,
    /// With a `view_box`: everything in the drawing is fitted to it.
    pub fit: bool,
    /// How many decimals the drawing's numbers are written with.
    pub decimals: Option<usize>,
}

/// The page `doc` has: the size it's shown at, and the coordinates
/// that fill it (its viewBox, or without one the page itself).
pub fn of(doc: &Document) -> (Vec2, [f64; 4]) {
    let Some(root) = doc.get(doc.root()) else { return (Vec2::ZERO, [0.0; 4]) };
    let size = Viewport::of(root).size;
    let said = root.attr("viewBox").and_then(parse_list).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0);
    (size, said.map_or([0.0, 0.0, size.x, size.y], |v| [v[0], v[1], v[2], v[3]]))
}

/// The Command that makes `doc`'s page what `page` says: one step.
pub fn set(doc: &Document, page: &Page) -> Result<Command, DocError> {
    let root = doc.root();
    let attr = |name: &str, value: String| Command::SetAttr { node: root, name: name.to_owned(), value: Some(value) };
    let mut commands = Vec::new();
    // First, so that what follows is written as finely as it says.
    let mut precision = Precision::of(doc);
    if let Some(decimals) = page.decimals {
        if decimals > MAX_DECIMALS {
            return invalid(format!("a drawing's numbers have {MAX_DECIMALS} decimals at most"));
        }
        match doc.namespace(root, Some(INK_PREFIX)) {
            Some(INK_NS) => {}
            None => commands.push(attr(&format!("xmlns:{INK_PREFIX}"), INK_NS.to_owned())),
            Some(other) => return invalid(format!("this drawing uses the prefix \"{INK_PREFIX}:\" for something else ({other}), so Ink's own attributes have nowhere to go")),
        }
        commands.push(attr(&format!("{INK_PREFIX}:{DECIMALS_ATTR}"), decimals.to_string()));
        precision.decimals = decimals;
    }
    let sized = |v: f64| v.is_finite() && v > 0.0;
    if let Some([x, y, w, h]) = page.view_box
        && !(x.is_finite() && y.is_finite() && sized(w) && sized(h))
    {
        return invalid("a viewBox is x, y, width and height, its width and height more than nothing");
    }
    match (page.fit, page.view_box) {
        (false, _) => {}
        (true, None) => return invalid("there's no viewBox to fit the drawing to"),
        (true, Some([x, y, w, h])) => {
            let (_, [ox, oy, ow, oh]) = of(doc);
            let by = Affine::translate(-ox, -oy).then(&Affine::scale(w / ow, h / oh)).then(&Affine::translate(x, y));
            commands.push(Command::Transform { nodes: vec![root], by });
            // The guides are no nodes: they go along by themselves.
            let moved: Vec<Guide> = guides::of(doc)
                .into_iter()
                .map(|g| match g {
                    Guide::X(at) => Guide::X(by.apply(Vec2::new(at, 0.0)).x),
                    Guide::Y(at) => Guide::Y(by.apply(Vec2::new(0.0, at)).y),
                })
                .collect();
            if !moved.is_empty() {
                commands.push(Command::SetGuides { guides: moved });
            }
        }
    }
    for (name, v) in [("width", page.width), ("height", page.height)] {
        if let Some(v) = v {
            if !sized(v) {
                return invalid(format!("a page's {name} is more than nothing"));
            }
            commands.push(attr(name, precision.number(v)));
        }
    }
    if let Some(view_box) = page.view_box {
        commands.push(attr("viewBox", view_box.map(|v| precision.number(v)).join(" ")));
    }
    Ok(Command::Batch(commands))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{DocId, NodeId};

    const ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" width=\"500\" height=\"500\" viewBox=\"0 0 500 500\" ink:guides=\"x250 y100\">\n  <rect id=\"a\" x=\"100\" y=\"50\" width=\"300\" height=\"400\"/>\n</svg>\n";

    fn doc() -> Document {
        Document::parse(DocId(1), ICON).unwrap()
    }

    #[test]
    fn a_page_is_its_size_and_the_coordinates_that_fill_it() {
        assert_eq!(of(&doc()), (Vec2::new(500.0, 500.0), [0.0, 0.0, 500.0, 500.0]));
        // Without a viewBox, the page itself.
        let plain = Document::parse(DocId(1), "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"24\"/>").unwrap();
        assert_eq!(of(&plain), (Vec2::new(32.0, 24.0), [0.0, 0.0, 32.0, 24.0]));
    }

    #[test]
    fn a_new_view_box_leaves_the_drawing_where_its_numbers_are_unless_its_fitted() {
        let mut kept = doc();
        kept.apply(&set(&kept, &Page { view_box: Some([0.0, 0.0, 24.0, 24.0]), ..Page::default() }).unwrap()).unwrap();
        assert!(kept.to_svg().contains("viewBox=\"0 0 24 24\" ink:guides=\"x250 y100\">\n  <rect id=\"a\" x=\"100\" y=\"50\" width=\"300\" height=\"400\"/>"));
        // Fitted: everything through the old viewBox to the new, the
        // guides with it; and the size it's shown at as asked.
        let mut fitted = doc();
        let page = Page { width: Some(24.0), height: Some(24.0), view_box: Some([0.0, 0.0, 24.0, 24.0]), fit: true, decimals: None };
        let applied = fitted.apply(&set(&fitted, &page).unwrap()).unwrap();
        assert_eq!(fitted.to_svg(), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:ink=\"urn:lantern:ink\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" ink:guides=\"x12 y4.8\">\n  <rect id=\"a\" x=\"4.8\" y=\"2.4\" width=\"14.4\" height=\"19.2\"/>\n</svg>\n");
        assert!(applied.changed.contains(&NodeId(2)));
        // The same again changes nothing.
        assert!(fitted.apply(&set(&fitted, &Page { fit: false, ..page }).unwrap()).unwrap().is_nothing());
    }

    #[test]
    fn decimals_come_first_and_what_cannot_be_is_refused() {
        let mut d = doc();
        d.apply(&set(&d, &Page { width: Some(10.0 / 3.0), decimals: Some(1), ..Page::default() }).unwrap()).unwrap();
        assert!(d.to_svg().contains("width=\"3.3\"") && d.to_svg().contains("ink:decimals=\"1\""));
        let refused = |page: Page| set(&doc(), &page).unwrap_err().to_string();
        assert_eq!(refused(Page { width: Some(0.0), ..Page::default() }), "a page's width is more than nothing");
        assert_eq!(refused(Page { view_box: Some([0.0, 0.0, 24.0, -1.0]), ..Page::default() }), "a viewBox is x, y, width and height, its width and height more than nothing");
        assert_eq!(refused(Page { fit: true, ..Page::default() }), "there's no viewBox to fit the drawing to");
        assert!(refused(Page { decimals: Some(99), ..Page::default() }).contains("decimals at most"));
    }
}
