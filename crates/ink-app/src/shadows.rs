//! Object > Drop Shadow and Blur, and their settings in the Box
//! (ARCHITECTURE §8): a filter of one step, the kind Claude's
//! `filter_set` makes, with room round the shape for it to show in
//! (`ink_doc::filter::region`, which both go by).
//!
//! **A filter is changed where it is while it's the selection's
//! alone**; one that other things use too is never touched (as a
//! shared gradient isn't): the selection is given one of its own. A
//! filter that's more than the one step (a chain, written by hand) has
//! no settings here, and is left as it is.

use ink_core::{Command, DocId, Document, NodeId};
use ink_doc::filter::{self, Effect, Filter, Input};
use ink_doc::refs::{self, Ids};
use ink_doc::style::{prop, url_id};
use ink_doc::{Element, Kind, Precision};
use lntrn_math::{Color, Rect, Vec2};
use lntrn_ui::{FILL, Ui};

use crate::colour::palettes::to_hex;
use crate::colour::picker::swatch_face;
use crate::controls;
use crate::icons::Icons;
use crate::ink::Ink;
use crate::shapebox::Laid;
use crate::tools::Tool;

/// A shadow under a thing, or the thing itself blurred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Soft {
    /// The thing's shape, `blur` soft, `dx` right and `dy` down of it,
    /// in `color` (whose alpha is how dark it is).
    Shadow { dx: f64, dy: f64, blur: f64, color: Color },
    Blur(f64),
}

impl Soft {
    /// What the step that sets it is called, and its row of the menu.
    pub fn label(&self) -> &'static str {
        match self {
            Soft::Shadow { .. } => "Drop Shadow",
            Soft::Blur(_) => "Blur",
        }
    }

    fn is_shadow(&self) -> bool {
        matches!(self, Soft::Shadow { .. })
    }

    /// One to begin with, on a page whose numbers are dragged by
    /// `step` a pixel: a soft black shadow just under, or a light blur.
    pub fn first(shadow: bool, step: f64) -> Soft {
        let unit = step * 10.0;
        if shadow { Soft::Shadow { dx: 0.0, dy: unit, blur: unit, color: Color::BLACK.with_alpha(0.5) } } else { Soft::Blur(unit) }
    }

    /// How far past a thing's shape it reaches, in the thing's units.
    fn reach(&self) -> f64 {
        match *self {
            Soft::Shadow { dx, dy, blur, .. } => 3.0 * blur + dx.abs().max(dy.abs()),
            Soft::Blur(blur) => 3.0 * blur,
        }
    }

    /// The filter's one step: its element's name, and what it says.
    fn step(&self, p: &Precision) -> (&'static str, Vec<(&'static str, String)>) {
        match *self {
            Soft::Shadow { dx, dy, blur, color } => ("feDropShadow", vec![("dx", p.number(dx)), ("dy", p.number(dy)), ("stdDeviation", p.number(blur.max(0.0))), ("flood-color", to_hex(color)), ("flood-opacity", p.number(color.a.clamp(0.0, 1.0)))]),
            Soft::Blur(blur) => ("feGaussianBlur", vec![("stdDeviation", p.number(blur.max(0.0)))]),
        }
    }
}

/// The `<filter>` `node` is drawn through, if it names one.
fn filter_of(doc: &Document, node: NodeId) -> Option<(String, NodeId)> {
    let (id, _) = url_id(prop(doc.get(node)?, "filter")?)?;
    Some((id.to_owned(), Ids::of(doc).get(id).filter(|f| doc.get(*f).is_some_and(|n| n.kind == Kind::Filter))?))
}

/// What `node`'s filter is, where it's one of these and nothing else.
pub fn read(doc: &Document, node: NodeId) -> Option<Soft> {
    let (_, at) = filter_of(doc, node)?;
    let [step] = Filter::of(doc, doc.get(at)?)?.steps.try_into().ok()?;
    match step.effect {
        Effect::DropShadow { of: Input::Graphic, dx, dy, std, color } => Some(Soft::Shadow { dx, dy, blur: std.0, color }),
        Effect::Blur { of: Input::Graphic, std } => Some(Soft::Blur(std.0)),
        _ => None,
    }
}

/// The filter the things `nodes` have between them, and its one step,
/// where it's theirs alone and already what `soft` is a kind of: what's
/// changed where it is.
fn own(doc: &Document, nodes: &[NodeId], soft: &Soft) -> Option<(NodeId, NodeId)> {
    let (id, at) = filter_of(doc, *nodes.first()?)?;
    let same = nodes.iter().all(|&n| filter_of(doc, n).is_some_and(|(other, _)| other == id));
    let alone = refs::users(doc).get(&id).is_some_and(|users| users.iter().all(|user| nodes.contains(user)));
    let kind = read(doc, nodes[0]).is_some_and(|has| has.is_shadow() == soft.is_shadow());
    let step = doc.get(at)?.elements().find(|&fe| doc.get(fe).is_some_and(|n| n.kind == Kind::FilterPrimitive))?;
    (same && alone && kind).then_some((at, step))
}

/// The Command that gives each of `nodes` `soft`. Refused, with the
/// one of them it can't be given to: a thing with no box to measure
/// the filter's room by (a level line; group it with what it's part
/// of).
pub fn set(doc: &Document, nodes: &[NodeId], soft: Soft) -> Result<Command, NodeId> {
    let p = Precision::of(doc);
    let room = filter::region(doc, nodes, soft.reach())?;
    let (name, says) = soft.step(&p);
    if let Some((at, step)) = own(doc, nodes, &soft) {
        let attr = |node: NodeId, (name, value): (&str, String)| Command::SetAttr { node, name: name.to_owned(), value: Some(value) };
        return Ok(Command::Batch(room.into_iter().map(|a| attr(at, a)).chain(says.into_iter().map(|a| attr(step, a))).collect()));
    }
    let ids = Ids::of(doc);
    let stem = if soft.is_shadow() { "shadow" } else { "blur" };
    let id = (1..).map(|n| format!("{stem}-{n}")).find(|id| ids.get(id).is_none()).expect("there is always another number");
    let step = says.into_iter().fold(Element::new(name), |fe, (name, value)| fe.with(name, value));
    let made = room.into_iter().fold(Element::new("filter").with("id", id.as_str()), |f, (name, value)| f.with(name, value)).child(step);
    Ok(Command::Batch(vec![Command::Define { elements: vec![made] }, Command::SetStyle { nodes: nodes.to_vec(), set: vec![("filter".to_owned(), Some(format!("url(#{id})")))] }]))
}

/// What takes the filters off those of `nodes` that have one; and with
/// them a filter that was theirs alone, which nothing would use.
pub fn removed(doc: &Document, nodes: &[NodeId]) -> Option<Command> {
    let with: Vec<NodeId> = nodes.iter().copied().filter(|&n| doc.get(n).and_then(|n| prop(n, "filter")).is_some_and(|v| v.trim() != "none")).collect();
    if with.is_empty() {
        return None;
    }
    let users = refs::users(doc);
    let mut unused: Vec<NodeId> = Vec::new();
    for (id, at) in with.iter().filter_map(|&n| filter_of(doc, n)) {
        if !unused.contains(&at) && users.get(&id).is_some_and(|u| u.iter().all(|user| with.contains(user))) {
            unused.push(at);
        }
    }
    let off = Command::SetStyle { nodes: with, set: vec![("filter".to_owned(), None)] };
    Some(if unused.is_empty() { off } else { Command::Batch(vec![off, Command::Delete { nodes: unused }]) })
}

/// What its rows in the Box were asked for, beside the numbers set.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Asked {
    pub remove: bool,
    /// Its colour's swatch: where it is, and whether it was pressed.
    pub swatch: Option<(Rect, bool)>,
}

/// Two numbers side by side on a row of the Box.
fn pair_row(ui: &mut Ui, pair: [(&'static str, &mut f64); 2], step: f64, laid: &mut Laid) {
    let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    let (gap, half) = (ui.m.gap * 2.0, ((row.width() - ui.m.gap * 2.0) / 2.0).floor());
    for (k, (name, value)) in pair.into_iter().enumerate() {
        let r = Rect::from_xywh(row.min.x + (half + gap) * k as f64, row.min.y, half, row.height());
        laid.push((name, r));
        controls::number_in(ui, ui.id(name), r, name, value, step, None, 3);
    }
}

/// The rows of `soft` in the Box, changing it: a shadow's place, how
/// soft and how dark it is and its colour; a blur's softness; and a
/// button to take it off. `lit`: its colour's picker is open.
pub fn rows(ui: &mut Ui, soft: &mut Soft, step: f64, icons: &Icons, lit: bool, laid: &mut Laid) -> Asked {
    let mut asked = Asked::default();
    let soft_range = Some((0.0, f64::INFINITY));
    match soft {
        Soft::Shadow { dx, dy, blur, color } => {
            pair_row(ui, [("Shadow X", dx), ("Shadow Y", dy)], step, laid);
            // How soft, and its colour beside it.
            let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
            let (gap, half) = (ui.m.gap * 2.0, ((row.width() - ui.m.gap * 2.0) / 2.0).floor());
            let field = Rect::from_xywh(row.min.x, row.min.y, half, row.height());
            laid.push(("Soft", field));
            controls::number_in(ui, ui.id("Soft"), field, "Soft", blur, step, soft_range, 3);
            let well = Rect::from_xywh(row.min.x + half + gap, row.min.y, half, row.height());
            laid.push(("Shadow Colour", well));
            let resp = ui.interact(ui.id("Shadow Colour"), well, lntrn_ui::Sense::CLICK);
            swatch_face(ui, well, color.with_alpha(1.0), lit || resp.hovered, icons);
            asked.swatch = Some((well, resp.clicked));
            // How dark.
            let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
            let style = ui.text_style();
            let name_w = ui.measure("Dark", &style).ceil() + ui.m.gap * 2.0;
            ui.text_in_rect("Dark", &style, row, crate::theme::TEXT);
            let rail = Rect::new(Vec2::new(row.min.x + name_w, row.min.y), row.max);
            laid.push(("Dark", rail));
            let mut percent = (color.a * 100.0).round();
            if controls::Slider::new(0.0, 100.0, 1.0).unit("%").rest(50.0).in_row(ui, ui.id("Dark"), rail, &mut percent) {
                color.a = percent / 100.0;
            }
        }
        Soft::Blur(blur) => {
            let row = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
            laid.push(("Blur", row));
            controls::number_in(ui, ui.id("Blur"), row, "Blur", blur, step, soft_range, 3);
        }
    }
    let (clicked, rect) = controls::button_if(ui, if soft.is_shadow() { "Remove Shadow" } else { "Remove Blur" }, true);
    laid.push(("Remove", rect));
    asked.remove = clicked;
    asked
}

impl Ink {
    /// The things of the selection `tops` a shadow or a blur goes on:
    /// the ones drawn, that may be changed.
    fn softened(&self, doc: DocId, tops: &[NodeId]) -> Vec<NodeId> {
        self.core.doc(doc).map_or(Vec::new(), |drawing| crate::effects::drawn(drawing, tops))
    }

    /// What the Box shows of the selection's shadow or blur: the one in
    /// hand's, as the drawing shows it now.
    pub(crate) fn soft_shown(&self, doc: DocId, tops: &[NodeId]) -> Option<Soft> {
        let (drawing, _) = self.core.shown(doc).ok()?;
        let active = self.tabs.active().and_then(|tab| tab.selection.active);
        let in_hand = active.filter(|a| tops.contains(a)).or(tops.last().copied())?;
        read(drawing, in_hand).filter(|_| drawing.lock_over(in_hand).is_none())
    }

    /// Give the selection `tops` of `doc` `soft`. `held`: the button is
    /// down on the row that chose it, so it's shown and not yet done.
    pub(crate) fn soften(&mut self, doc: DocId, tops: &[NodeId], soft: Soft, held: bool) {
        let nodes = self.softened(doc, tops);
        let Ok(drawing) = self.core.doc(doc) else { return };
        if nodes.is_empty() {
            return;
        }
        match set(drawing, &nodes, soft) {
            Ok(command) => self.box_set(doc, &command, soft.label(), held),
            Err(node) => {
                let name = crate::select::name_of(drawing, node).0;
                self.toast(format!("{name} has no box to measure a {} by (it has no width, or no height): group it with what it's part of, and give that one", soft.label().to_lowercase()));
            }
        }
    }

    /// Take the selection's shadows and blurs off.
    pub(crate) fn unsoften(&mut self, doc: DocId, tops: &[NodeId]) {
        let nodes = self.softened(doc, tops);
        if let Some(command) = self.core.doc(doc).ok().and_then(|drawing| removed(drawing, &nodes)) {
            self.edit(doc, &command, "Remove Effect");
        }
    }

    /// Object > Drop Shadow, or Blur: the selection is given one, where
    /// the one in hand hasn't one already; and the Box, where its
    /// settings are, is opened with the Pointer in hand.
    pub(crate) fn soft_menu(&mut self, shadow: bool) {
        let Some((doc, tops)) = self.tabs.active().and_then(|tab| Some((tab.doc, tab.selection.tops(self.core.doc(tab.doc).ok()?)))) else { return };
        if self.softened(doc, &tops).is_empty() {
            return;
        }
        if self.soft_shown(doc, &tops).is_none_or(|has| has.is_shadow() != shadow) {
            let step = self.core.doc(doc).map_or(1.0, |drawing| {
                let page = ink_doc::arrange::page_box(drawing);
                crate::boxes::step_for(page.width().max(page.height()))
            });
            self.soften(doc, &tops, Soft::first(shadow, step), false);
        }
        self.tools.select(Tool::Pointer);
        self.toolbox.open = true;
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId as Id;

    use super::*;

    /// Two squares, a circle with a chain of a filter written by hand,
    /// and a level line.
    fn doc() -> Document {
        Document::parse(Id(1), r##"<svg viewBox="0 0 48 48"><defs><filter id="chain"><feGaussianBlur stdDeviation="1"/><feOffset dx="2"/></filter></defs><rect id="a" width="10" height="10"/><rect id="b" x="20" width="10" height="20"/><circle r="4" filter="url(#chain)"/><line x2="8" stroke="#000"/></svg>"##).unwrap()
    }

    const A: NodeId = NodeId(6);
    const B: NodeId = NodeId(7);
    const CHAINED: NodeId = NodeId(8);
    const LINE: NodeId = NodeId(9);

    fn shadow(dy: f64, blur: f64) -> Soft {
        Soft::Shadow { dx: 0.0, dy, blur, color: Color::BLACK.with_alpha(0.5) }
    }

    #[test]
    fn a_shadow_is_made_read_changed_and_taken_off() {
        let mut d = doc();
        assert_eq!((read(&d, A), read(&d, CHAINED), removed(&d, &[A, B])), (None, None, None));
        // One filter for the two of them, with room for the shadow
        // round the smaller: 3 × 1 + 1 of 10, and a tenth, each way.
        d.apply(&set(&d, &[A, B], shadow(1.0, 1.0)).unwrap()).unwrap();
        assert!(d.to_svg().contains(r##"<filter id="shadow-1" x="-50%" y="-50%" width="200%" height="200%"><feDropShadow dx="0" dy="1" stdDeviation="1" flood-color="#000000" flood-opacity="0.5"/></filter>"##), "{}", d.to_svg());
        assert_eq!((d.node(A).unwrap().attr("filter"), d.node(B).unwrap().attr("filter"), read(&d, B)), (Some("url(#shadow-1)"), Some("url(#shadow-1)"), Some(shadow(1.0, 1.0))));
        // Theirs alone: changed where it is, its room with it.
        d.apply(&set(&d, &[A, B], Soft::Shadow { dx: -2.0, dy: 0.5, blur: 2.0, color: Color::hex(0x0088FF).with_alpha(0.25) }).unwrap()).unwrap();
        assert!(d.to_svg().contains(r##"<filter id="shadow-1" x="-90%" y="-90%" width="280%" height="280%"><feDropShadow dx="-2" dy="0.5" stdDeviation="2" flood-color="#0088ff" flood-opacity="0.25"/></filter>"##), "{}", d.to_svg());
        assert!(!d.to_svg().contains("shadow-2"));
        // One of them alone: the other still uses it, so it's left as
        // it is and this one is given its own.
        d.apply(&set(&d, &[A], shadow(1.0, 0.0)).unwrap()).unwrap();
        assert_eq!((d.node(A).unwrap().attr("filter"), d.node(B).unwrap().attr("filter"), read(&d, A), read(&d, B).is_some_and(|s| s != shadow(1.0, 0.0))), (Some("url(#shadow-2)"), Some("url(#shadow-1)"), Some(shadow(1.0, 0.0)), true));
        // A blur in a shadow's place is another filter; taken off, the
        // filter that was its alone goes too, and no other.
        d.apply(&set(&d, &[A], Soft::Blur(1.5)).unwrap()).unwrap();
        assert_eq!((d.node(A).unwrap().attr("filter"), read(&d, A)), (Some("url(#blur-1)"), Some(Soft::Blur(1.5))));
        d.apply(&removed(&d, &[A, LINE]).unwrap()).unwrap();
        assert_eq!((d.node(A).unwrap().attr("filter"), d.to_svg().contains("blur-1"), d.to_svg().contains("shadow-1"), d.to_svg().contains("id=\"chain\"")), (None, false, true, true));
    }

    #[test]
    fn what_isnt_one_of_these_is_left_alone() {
        let d = doc();
        // A chain written by hand has no settings here: given a shadow,
        // the thing gets a filter of its own, and the chain stays.
        let Ok(Command::Batch(steps)) = set(&d, &[CHAINED], shadow(1.0, 1.0)) else { panic!("a new filter") };
        assert!(matches!(steps.as_slice(), [Command::Define { .. }, Command::SetStyle { .. }]));
        // A thing with no box to measure by is said, not given one.
        assert_eq!(set(&d, &[A, LINE], Soft::Blur(1.0)), Err(LINE));
        assert_eq!((Soft::first(true, 0.1), Soft::first(false, 1.0), Soft::Blur(1.0).label(), shadow(0.0, 0.0).label()), (shadow(1.0, 1.0), Soft::Blur(10.0), "Blur", "Drop Shadow"));
    }
}
