//! The Text tool's Box (ARCHITECTURE §8; LS3's `text_type.rs`): the
//! font (every family installed, each row lettered in its own), the
//! size, bold, italic, how lines hang from their place, and how far
//! apart they are. It's the lettering of the text typed into, or with
//! the tool in hand and none typed into, of the texts selected; and
//! always of the next one typed.
//!
//! **A font that isn't installed says so**, and which one drew instead:
//! the text keeps asking for the one it names (it's right on a machine
//! that has it), and the Box says what this machine made of it.

use ink_core::{Command, DocId, Document, NodeId};
use ink_doc::lettering::LEADING;
use ink_doc::text::{self, Anchor};
use ink_doc::{Kind, Precision, fonts};
use lntrn_math::{Rect, Vec2};
use lntrn_ui::{FILL, Ui};

use crate::controls;
use crate::ink::Ink;
use crate::paint;
use crate::shapebox::Laid;
use crate::texting::{Align, Lettering};
use crate::theme::TEXT_DIM;
use crate::toolbox;
use crate::tools::Tool;

/// The names every machine has a font for: Lantern's own stand for
/// them (ARCHITECTURE §5.5).
const GENERIC: [&str; 3] = ["sans-serif", "serif", "monospace"];
const ALIGNS: [&str; 3] = ["Left", "Centre", "Right"];

/// The fonts the Box offers: the generic names, then every family
/// installed, as the fonts write them.
pub fn offered() -> Vec<String> {
    GENERIC.iter().map(|name| (*name).to_owned()).chain(fonts::families()).collect()
}

/// How the text `node` is lettered; and, where this machine set it
/// otherwise than it asks, what to say of that.
pub fn read(drawing: &Document, node: NodeId) -> Option<(Lettering, Option<String>)> {
    let n = drawing.get(node).filter(|n| n.kind == Kind::Text)?;
    let font = text::Font::of(drawing, n);
    let align = match font.anchor {
        Anchor::Start => Align::Start,
        Anchor::Middle => Align::Middle,
        Anchor::End => Align::End,
    };
    let letters = Lettering { family: font.families.first().cloned().unwrap_or_else(|| GENERIC[0].to_owned()), size: font.size, bold: font.weight >= 600.0, italic: font.italic, align, leading: drawing.leading(node).unwrap_or(LEADING) };
    let note = text::lettered(drawing, n).into_iter().find_map(|set| {
        if !set.missing.is_empty() {
            Some(format!("{} {} installed: drawn in {}", set.missing.join(" and "), if set.missing.len() == 1 { "isn\u{2019}t" } else { "aren\u{2019}t" }, set.family))
        } else {
            set.no_italic.then(|| format!("{} has no italic: drawn upright", set.family))
        }
    });
    Some((letters, note))
}

/// The Command that letters the texts `nodes` as `now`, where that's
/// not how `was` has them; and what the step is called. None with
/// nothing to change.
pub fn set(drawing: &Document, nodes: &[NodeId], was: &Lettering, now: &Lettering) -> Option<(Command, &'static str)> {
    let p = Precision::of(drawing);
    let on = |on: bool, word: &str| on.then(|| word.to_owned());
    let mut said: Vec<(String, Option<String>)> = Vec::new();
    let mut label = "";
    let mut say = |differs: bool, name: &str, value: Option<String>, called: &'static str| {
        if differs {
            said.push((name.to_owned(), value));
            label = called;
        }
    };
    say(now.family != was.family, "font-family", Some(now.family.clone()), "Font");
    say(now.size != was.size && now.size > 0.0, "font-size", Some(p.number(now.size)), "Font Size");
    say(now.bold != was.bold, "font-weight", on(now.bold, "bold"), "Bold");
    say(now.italic != was.italic, "font-style", on(now.italic, "italic"), "Italic");
    say(now.align != was.align, "text-anchor", on(now.align != Align::Start, now.align.word()), "Align");
    let mut steps: Vec<Command> = Vec::new();
    if !said.is_empty() {
        steps.push(Command::SetStyle { nodes: nodes.to_vec(), set: said });
    }
    // How far apart its lines are is written into its lines.
    if now.leading != was.leading && now.leading > 0.0 {
        label = "Line Height";
        steps.extend(nodes.iter().filter_map(|&node| Some(Command::SetText { node, lines: text::written(drawing, drawing.get(node)?).lines, leading: Some(now.leading) })));
    }
    match steps.len() {
        0 => None,
        1 => steps.pop().map(|step| (step, label)),
        _ => Some((Command::Batch(steps), label)),
    }
}

/// The Box's rows, showing `l` and changing it: `families` to pick
/// from, `note` what this machine made of a font it hasn't.
fn rows(ui: &mut Ui, l: &mut Lettering, families: &[String], step: f64, note: Option<&str>, laid: &mut Laid) {
    let row = |ui: &Ui| Rect::from_min_size(ui.cursor(), Vec2::new(ui.avail_width(), ui.m.widget_h));
    let names: Vec<&str> = families.iter().map(String::as_str).collect();
    // A font the text asks for that isn't here shows as none picked.
    let mut font = names.iter().position(|name| name.eq_ignore_ascii_case(&l.family)).unwrap_or(usize::MAX);
    laid.push(("Font", row(ui)));
    if controls::dropdown(ui, "Font", &mut font, &names, true)
        && let Some(name) = names.get(font)
    {
        l.family = (*name).to_owned();
    }
    if let Some(note) = note {
        let line = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
        laid.push(("Note", line));
        let style = ui.text_style();
        ui.draw.push_clip(line);
        ui.text_in_rect(note, &style, line, TEXT_DIM);
        ui.draw.pop_clip();
    }
    let size = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    laid.push(("Size", size));
    controls::number_in(ui, ui.id("Size"), size, "Size", &mut l.size, step, Some((step.min(0.001), f64::INFINITY)), 3);
    laid.push(("Bold", row(ui)));
    controls::toggle(ui, "Bold", &mut l.bold);
    laid.push(("Italic", row(ui)));
    controls::toggle(ui, "Italic", &mut l.italic);
    let mut align = Align::ALL.iter().position(|a| *a == l.align).unwrap_or(0);
    laid.push(("Align", row(ui)));
    if controls::dropdown(ui, "Align", &mut align, &ALIGNS, false) {
        l.align = Align::ALL[align.min(2)];
    }
    let leading = ui.alloc(Vec2::new(FILL, ui.m.widget_h));
    laid.push(("Line Height", leading));
    controls::number_in(ui, ui.id("Line Height"), leading, "Line Height", &mut l.leading, 0.01, Some((0.5, 4.0)), 2);
}

impl Ink {
    /// The texts the Box letters: the one typed into, else the ones
    /// selected (or in what's selected) that may be changed.
    fn lettered(&self, doc: DocId) -> Vec<NodeId> {
        if let Some(e) = self.texting.editing.as_ref().filter(|e| e.doc == doc) {
            return e.node.into_iter().collect();
        }
        let Some((drawing, tab)) = self.core.doc(doc).ok().zip(self.tabs.iter().find(|t| t.doc == doc)) else { return Vec::new() };
        paint::painted(drawing, &tab.selection.tops(drawing)).into_iter().filter(|&id| drawing.get(id).is_some_and(|n| n.kind == Kind::Text) && drawing.lock_over(id).is_none()).collect()
    }

    /// The Box under the Text tool.
    pub(crate) fn text_box(&mut self, ui: &mut Ui, canvas: Rect) {
        let Some(doc) = self.tabs.active_doc() else { return self.toolbox.gone() };
        let step = self.core.doc(doc).map_or(1.0, |drawing| {
            let page = ink_doc::arrange::page_box(drawing);
            crate::boxes::step_for(page.width().max(page.height()))
        });
        let texts = self.lettered(doc);
        // The next text's, at the size it would be; or the one in
        // hand's, as the drawing shows it.
        let next = Lettering { size: self.texting.letters.size_on(step), ..self.texting.letters.clone() };
        let of = |ink: &Ink, live: bool| {
            let drawing = if live { ink.core.shown(doc).ok()?.0 } else { ink.core.doc(doc).ok()? };
            read(drawing, *texts.last()?)
        };
        let (was, note) = of(self, true).unwrap_or((next, None));
        let mut now = was.clone();
        if self.texting.families.is_none() {
            self.texting.families = Some(offered());
        }
        let families = self.texting.families.as_deref().unwrap_or_default();
        let mut laid = Laid::new();
        toolbox::draw_with(ui, canvas, &mut self.toolbox, Tool::Text.label(), |ui| rows(ui, &mut now, families, step, note.as_deref(), &mut laid));
        #[cfg(test)]
        {
            self.toolbox.laid = laid;
        }
        let held = ui.state.down;
        if now != was {
            // The next one typed is lettered so too.
            self.texting.letters = now.clone();
            // What's been typed lands first: this is a step of its own.
            self.type_settled();
            let made = of(self, false).and_then(|(base, _)| set(self.core.doc(doc).ok()?, &texts, &base, &now));
            if let Some((command, label)) = made {
                self.box_set(doc, &command, label, held);
            }
        }
        if !held {
            self.tune_settled();
        }
    }
}

#[cfg(test)]
mod tests {
    use ink_doc::DocId as Id;

    use super::*;

    fn doc() -> Document {
        Document::parse(Id(1), r##"<svg viewBox="0 0 48 48"><text id="a" x="4" y="10" font-family="No Such Font, sans-serif" font-size="6" font-weight="700" text-anchor="middle">one<tspan x="4" dy="1.5em">two</tspan></text><text id="b" x="4" y="30">plain</text><rect width="4" height="4"/></svg>"##).unwrap()
    }

    const A: NodeId = NodeId(2);
    const B: NodeId = NodeId(4);

    #[test]
    fn a_text_says_how_its_lettered_and_what_this_machine_made_of_it() {
        let d = doc();
        let (letters, note) = read(&d, A).unwrap();
        assert_eq!(letters, Lettering { family: "No Such Font".to_owned(), size: 6.0, bold: true, italic: false, align: Align::Middle, leading: 1.5 });
        // The font it asks for isn't here: said, with what drew.
        assert!(note.as_deref().is_some_and(|n| n.starts_with("No Such Font isn\u{2019}t installed: drawn in ")), "{note:?}");
        // What says nothing of its own: SVG's defaults (its size, 16).
        let (plain, note) = read(&d, B).unwrap();
        assert_eq!((plain.family.as_str(), plain.size, plain.bold, plain.align, plain.leading, note), ("sans-serif", 16.0, false, Align::Start, LEADING, None));
        assert!(read(&d, NodeId(5)).is_none() && offered()[..3] == GENERIC.map(String::from));
    }

    #[test]
    fn only_what_changed_is_set() {
        let mut d = doc();
        let was = read(&d, A).unwrap().0;
        assert_eq!(set(&d, &[A], &was, &was), None);
        // One thing: one property, on each text, and the step named
        // for it.
        let (command, label) = set(&d, &[A, B], &was, &Lettering { italic: true, ..was.clone() }).unwrap();
        assert_eq!((command, label), (Command::SetStyle { nodes: vec![A, B], set: vec![("font-style".to_owned(), Some("italic".to_owned()))] }, "Italic"));
        // Off again, and back to how lines hang unless told: the
        // property is taken off, not set to its default.
        let (command, _) = set(&d, &[A], &was, &Lettering { bold: false, align: Align::Start, family: "serif".to_owned(), size: 7.5, ..was.clone() }).unwrap();
        d.apply(&command).unwrap();
        assert!(d.markup(A).unwrap().starts_with(r#"<text id="a" x="4" y="10" font-family="serif" font-size="7.5">one"#), "{}", d.markup(A).unwrap());
        // How far apart its lines are is written into them.
        let now = read(&d, A).unwrap().0;
        let (command, label) = set(&d, &[A], &now, &Lettering { leading: 2.0, ..now.clone() }).unwrap();
        d.apply(&command).unwrap();
        assert!(label == "Line Height" && d.markup(A).unwrap().contains(r#"<tspan x="4" dy="2em">two</tspan>"#), "{}", d.markup(A).unwrap());
    }
}
