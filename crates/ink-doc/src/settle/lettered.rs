//! A text through a transform (ARCHITECTURE §3.4, D13). A move goes
//! into its numbers: its `x` and `y`, and those of the `<tspan>`s in it
//! that say where they start (every line of a text of several). Anything
//! else stays in `transform`: turned or scaled, a text is still set
//! upright at its own size.
//!
//! A move stays in `transform` too when something the text is drawn
//! with would be left behind (a gradient laid out in the page's
//! coordinates, a clip path others use, a mask), or when a position is
//! said in a way a number can't be added to (`50%`, `2em`).

use ink_geom::{Affine, Vec2};

use super::drawn::{Cut, Filtered};
use super::{Edit, Settle};
use crate::gradient::{Gradient, Units};
use crate::kind::Kind;
use crate::node::Node;
use crate::style::{Paint, Style, prop};

/// `said` (an `x` or a `y`: a number, or a list of them) with `by`
/// added to each. `None` when any of it isn't a plain number.
fn shifted(said: &str, by: f64, write: impl Fn(f64) -> String) -> Option<String> {
    let parts: Vec<&str> = said.split(|c: char| c.is_whitespace() || c == ',').filter(|part| !part.is_empty()).collect();
    let moved: Option<Vec<String>> = parts.iter().map(|part| part.strip_suffix("px").unwrap_or(part).parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| write(v + by))).collect();
    moved.filter(|moved| !moved.is_empty()).map(|moved| moved.join(" "))
}

impl Settle<'_> {
    /// Whether `paint` stays where it is when what it paints moves: a
    /// gradient (or a pattern) laid out in the page's coordinates.
    fn stays_behind(&self, paint: &Paint) -> bool {
        let Paint::Server { id, .. } = paint else { return false };
        match self.ids.get(id).and_then(|id| self.doc.get(id)) {
            Some(server) if matches!(server.kind, Kind::LinearGradient | Kind::RadialGradient) => Gradient::of(self.doc, &self.ids, server).is_none_or(|g| g.units == Units::UserSpace),
            Some(server) => server.kind == Kind::Pattern,
            None => false,
        }
    }

    /// Give a text the transform `to`. Returns what of it is left for
    /// its `transform`.
    pub(super) fn text(&mut self, node: &Node, to: Affine, inherited: &Style) -> Option<Affine> {
        let hold = Some(to).filter(|t| !self.p.same(t, &Affine::IDENTITY));
        if hold.is_none() || !self.p.same(&to.without_move(), &Affine::IDENTITY) {
            return hold;
        }
        let cut = self.cut(node);
        if matches!(cut, Cut::Held) || matches!(self.filtered(node), Filtered::Fixed) {
            return hold;
        }
        // The text and what's lettered in it: each may paint for itself.
        let lettering: Vec<&Node> = self.doc.descendants(node.id).into_iter().filter_map(|id| self.doc.get(id)).collect();
        let own = inherited.cascade(node);
        let painted = |n: &Node, name: &str| prop(n, name).and_then(Paint::parse);
        let left_behind = self.stays_behind(&own.fill) || self.stays_behind(&own.stroke) || lettering.iter().any(|n| ["fill", "stroke"].iter().any(|name| painted(n, name).is_some_and(|paint| self.stays_behind(&paint))));
        if left_behind {
            return hold;
        }
        let by = to.apply(Vec2::ZERO);
        let mut edits = Vec::new();
        for element in &lettering {
            // Along the way it doesn't move, what it says stays as said.
            for (name, by) in [("x", by.x), ("y", by.y)].into_iter().filter(|(_, by)| self.p.number(*by) != "0") {
                let now = match element.attr(name) {
                    Some(said) => match shifted(said, by, |v| self.p.number(v)) {
                        Some(now) if now != said => now,
                        Some(_) => continue,
                        None => return hold,
                    },
                    // The text itself starts at 0 where it doesn't say.
                    None if element.id == node.id => self.p.number(by),
                    None => continue,
                };
                edits.push(Edit::Attr { node: element.id, name, value: Some(now) });
            }
        }
        self.edits.extend(edits);
        if let Cut::Own(clip) = cut {
            self.carry_clip(clip, &to);
        }
        None
    }
}
