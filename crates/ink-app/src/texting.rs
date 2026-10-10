//! The Text tool on the canvas (ARCHITECTURE §8; LS3's `texting.rs`). A
//! click on a text takes it up, the caret where the click is; a click
//! anywhere else begins a new one there, on top of the level the
//! Pointer is in. Typing goes in at the caret, Enter starts a new line,
//! the arrows, Home and End move about, Backspace and Delete take
//! characters out (`typing.rs` has what each does).
//!
//! **Typing is tried on the canvas as it goes, and is one undo step
//! once it pauses for half a second** (LS3's rule), or when anything
//! else is done. Till then it's a gesture in the core.
//!
//! **A text begins with its first character.** A click that's typed
//! nothing into leaves nothing in the drawing; and a text whose last
//! character is taken out goes.
//!
//! The keyboard is the text's while one is typed into: its keys are
//! taken at the frame's start (`text_keys_in`), before the window's own
//! (a letter is a tool's key, Delete is the selection's). A field being
//! typed into, a menu and a dialog keep the keyboard.

use ink_core::{Actor, Command, DocId, Document, NodeId, Place};
use ink_doc::lettering::{self, LEADING};
use ink_doc::length::number;
use ink_doc::{Kind, Precision, Viewport, fonts, geometry, text};
use ink_geom::Affine;
use lntrn_math::Vec2;
use lntrn_ui::{CursorIcon, Key, KeyPress, Ui};

use crate::canvas::CanvasInput;
use crate::edits::in_row_names;
use crate::ink::Ink;
use crate::overlay::Scene;
use crate::paint::{self, Paint, Which};
use crate::picking::top_at;
use crate::pointer::View;
use crate::tools::Tool;
use crate::typing::{Caret, Go, Words};

/// Typing that pauses this long, seconds, is an undo step; the caret is
/// on, then off, this long each (LS3's). How far from the pointer a
/// click still finds a text, logical px.
const PAUSE: f64 = 0.5;
const BLINK: f64 = 0.53;
const REACH: f64 = 3.0;

/// How a line starts from its place: at it, about it, or up to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Middle,
    End,
}

impl Align {
    pub const ALL: [Align; 3] = [Align::Start, Align::Middle, Align::End];

    /// What `text-anchor` says for it.
    pub fn word(self) -> &'static str {
        match self {
            Align::Start => "start",
            Align::Middle => "middle",
            Align::End => "end",
        }
    }
}

/// How a text is lettered: what the Box sets, for the next one typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Lettering {
    pub family: String,
    /// In the drawing's units; nothing: a size that suits the page.
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    /// How far apart lines are, in ems.
    pub leading: f64,
}

impl Default for Lettering {
    fn default() -> Lettering {
        Lettering { family: "sans-serif".to_owned(), size: 0.0, bold: false, italic: false, align: Align::Start, leading: LEADING }
    }
}

impl Lettering {
    /// The size a new text is typed at on a page whose numbers are
    /// dragged by `step` a pixel: the one set, or a sixth of an icon's
    /// page.
    pub fn size_on(&self, step: f64) -> f64 {
        if self.size > 0.0 { self.size } else { step * 40.0 }
    }
}

/// A text being typed into.
pub(crate) struct Editing {
    pub(crate) doc: DocId,
    /// The text. A new one isn't in the drawing till it says something:
    /// then `fresh` is what the drawing shows of it, till it lands.
    pub(crate) node: Option<NodeId>,
    fresh: Option<NodeId>,
    /// Where it starts (its first line's baseline), in the coordinates
    /// of `group`, which a new one goes on top of.
    at: Vec2,
    group: NodeId,
    words: Words,
    caret: Caret,
    leading: f64,
    /// When the last of what's typed and not yet a step was typed (a
    /// gesture is open from the first of it).
    typed: Option<f64>,
    /// When the caret last moved: it shows solid from then.
    moved: f64,
    /// The drawing's state its words were last read in: after an undo
    /// or anything else that changes it, they're read again.
    stamp: u64,
}

/// What came from the keyboard, in the order it came.
enum Came {
    Text(String),
    Key(KeyPress),
}

/// What the Text tool keeps between frames.
#[derive(Default)]
pub struct Texting {
    pub(crate) editing: Option<Editing>,
    inbox: Vec<(u32, Came)>,
    /// A field of the Box's or a panel's is being typed into (it said
    /// so last frame): the keyboard is its.
    field: bool,
    /// How the next text is lettered.
    pub letters: Lettering,
    /// The fonts the Box offers, once they've been asked for.
    pub(crate) families: Option<Vec<String>>,
}

impl Texting {
    #[cfg(test)]
    pub(crate) fn caret(&self) -> Option<(usize, usize)> {
        self.editing.as_ref().map(|e| (e.caret.line, e.caret.col))
    }
}

/// A key the text takes for itself while it's typed into.
fn takes(press: &KeyPress) -> bool {
    let m = press.mods;
    !(m.ctrl() || m.alt() || m.super_key()) && matches!(press.key, Key::Char(_) | Key::Space | Key::Enter | Key::Backspace | Key::Delete | Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End)
}

/// The first number of a list an attribute gives.
fn first(node: &ink_doc::Node, name: &str) -> f64 {
    node.attr(name).and_then(|v| v.split(|c: char| c.is_whitespace() || c == ',').find(|part| !part.is_empty())).and_then(number).unwrap_or(0.0)
}

/// Where the caret stands in the text `node` as `looks` has it: its top
/// and bottom, in the text's own coordinates.
fn caret_in(looks: &Document, node: &ink_doc::Node, words: &Words, caret: Caret, leading: f64) -> Option<(Vec2, Vec2)> {
    let view = looks.get(looks.root()).map_or(Vec2::ZERO, |root| Viewport::of(root).view);
    let cells = text::lay(looks, node, view).ok()?.chars;
    let font = text::Font::of(looks, node);
    let step = leading * font.size;
    let caret = words.within(caret);
    let (len, before) = (words.len(caret.line), words.before(caret));
    let upright = |x: f64, top: f64, bottom: f64| Some((Vec2::new(x, top), Vec2::new(x, bottom)));
    if len > 0 {
        // Before a character: its left side; after the last: its right.
        let cell = if caret.col < len { cells.get(before) } else { cells.get(before.checked_sub(1)?) }?;
        return upright(if caret.col < len { cell.min.x } else { cell.max.x }, cell.min.y, cell.max.y);
    }
    // An empty line: as far down from the nearest line that says
    // something as it's lines away, back at the text's own x.
    let x = first(node, "x");
    let said = |line: usize| words.len(line) > 0;
    let above = (0..caret.line).rev().find(|&line| said(line));
    let below = (caret.line + 1..words.lines.len()).find(|&line| said(line));
    let by = |line: usize, cell: usize| cells.get(cell).and_then(|c| upright(x, c.min.y + step * (caret.line as f64 - line as f64), c.max.y + step * (caret.line as f64 - line as f64)));
    match (above, below) {
        (Some(line), _) => by(line, words.before(Caret::at(line, 0))),
        (None, Some(line)) => by(line, words.before(Caret::at(line, 0))),
        (None, None) => {
            let metrics = fonts::shape("M", &font.face());
            let y = first(node, "y") + step * caret.line as f64;
            upright(x, y - metrics.ascent * font.size, y + metrics.descent * font.size)
        }
    }
}

/// The place in `words` nearest `point` (the text's own coordinates),
/// by where its characters are in `looks`.
fn caret_at(looks: &Document, node: &ink_doc::Node, words: &Words, point: Vec2) -> Caret {
    let view = looks.get(looks.root()).map_or(Vec2::ZERO, |root| Viewport::of(root).view);
    let Ok(laid) = text::lay(looks, node, view) else { return Caret::default() };
    // The line whose letters are nearest up or down; then how many of
    // its characters' middles are left of the point.
    let lines = (0..words.lines.len()).filter(|&line| words.len(line) > 0).filter_map(|line| {
        let from = words.before(Caret::at(line, 0));
        Some((line, laid.chars.get(from..from + words.len(line))?))
    });
    let nearest = lines.min_by(|a, b| {
        let off = |cells: &[ink_geom::Rect]| cells.first().map_or(f64::INFINITY, |c| (c.center().y - point.y).abs());
        off(a.1).total_cmp(&off(b.1))
    });
    nearest.map_or(Caret::default(), |(line, cells)| Caret::at(line, cells.iter().filter(|c| c.center().x < point.x).count()))
}

impl Ink {
    /// At a frame's start: this frame's keys and text that are the
    /// text's, taken while one is typed into. `popup`: a menu or a
    /// dialog is up, and the keyboard is its.
    pub(crate) fn text_keys_in(&mut self, ui: &mut Ui, popup: bool) {
        self.texting.inbox.clear();
        let typing = self.tools.active() == Tool::Text && self.texting.editing.as_ref().is_some_and(|e| Some(e.doc) == self.tabs.active_doc());
        // A field of the Box's or a panel's that's being typed into
        // keeps the keyboard.
        if !typing || popup || self.texting.field {
            return;
        }
        let text = std::mem::take(&mut ui.state.text_input).into_iter().map(|(seq, text)| (seq, Came::Text(text)));
        self.texting.inbox.extend(text);
        while let Some(press) = ui.state.take_key(takes) {
            // Letters come as text: their keys are only kept from the
            // window's own (the tools').
            if !matches!(press.key, Key::Char(_) | Key::Space) {
                self.texting.inbox.push((press.seq, Came::Key(press)));
            }
        }
        self.texting.inbox.sort_by_key(|(seq, _)| *seq);
    }

    /// What's been typed and isn't a step yet lands as one. A new text
    /// is in the drawing from then, and selected; one emptied is gone.
    pub(crate) fn type_settled(&mut self) {
        let Some(e) = self.texting.editing.as_mut().filter(|e| e.typed.is_some()) else { return };
        let doc = e.doc;
        e.typed = None;
        match self.core.commit(doc, "Type") {
            Ok(applied) => {
                let e = self.texting.editing.as_mut().expect("still typing");
                e.fresh = None;
                e.stamp = self.core.history(doc).map_or(0, |h| h.stamp());
                let e = self.texting.editing.as_mut().expect("still typing");
                if e.node.is_some_and(|node| applied.removed.contains(&node)) {
                    e.node = None;
                } else if e.node.is_none()
                    && let Some(&made) = applied.created.first()
                {
                    e.node = Some(made);
                    self.select(vec![made]);
                }
            }
            Err(err) => {
                let why = err.to_string();
                let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                self.toast(said);
            }
        }
    }

    /// Stop typing: what was typed lands, and the text is let go of.
    pub(crate) fn type_done(&mut self) {
        self.type_settled();
        self.texting.editing = None;
    }

    /// Whether a text is being typed into.
    pub(crate) fn typing(&self) -> bool {
        self.texting.editing.is_some()
    }

    /// How a new text is written: where it starts, how it's lettered,
    /// and painted as the last shape was filled.
    fn new_text(&self, drawing: &Document, e: &Editing) -> Vec<(String, String)> {
        let p = Precision::of(drawing);
        let letters = &self.texting.letters;
        let page = ink_doc::arrange::page_box(drawing);
        let size = letters.size_on(crate::boxes::step_for(page.width().max(page.height())));
        let mut attrs = vec![("x".to_owned(), p.number(e.at.x)), ("y".to_owned(), p.number(e.at.y)), ("font-family".to_owned(), letters.family.clone()), ("font-size".to_owned(), p.number(size))];
        attrs.extend(letters.bold.then(|| ("font-weight".to_owned(), "bold".to_owned())));
        attrs.extend(letters.italic.then(|| ("font-style".to_owned(), "italic".to_owned())));
        attrs.extend((letters.align != Align::Start).then(|| ("text-anchor".to_owned(), letters.align.word().to_owned())));
        // (A gradient is the shape's it was made for: a text gets
        // Lantern's gold in its place, as a new shape does.)
        let fill = match &self.paints.fill {
            Paint::Server(_) => Paint::Color(lntrn_math::Color::hex(0xF3B700)),
            other => other.clone(),
        };
        attrs.extend(paint::set(Which::Fill, &fill).into_iter().filter_map(|(name, value)| Some((name, value?))));
        attrs
    }

    /// The Command that makes the drawing say what's typed.
    fn typed(&self, drawing: &Document, e: &Editing) -> Command {
        match e.node {
            Some(node) if e.words.is_empty() => Command::Delete { nodes: vec![node] },
            Some(node) => Command::SetText { node, lines: e.words.lines.clone(), leading: Some(e.leading) },
            None if e.words.is_empty() => Command::Batch(Vec::new()),
            None => match lettering::element(&self.new_text(drawing, e), &e.words.lines, e.leading) {
                Ok(made) => Command::Insert { place: Place::LastIn(e.group), elements: vec![made] },
                Err(_) => Command::Batch(Vec::new()),
            },
        }
    }

    /// One frame of the Text tool on `doc` (`active`: it's in hand):
    /// its caret goes into `scene`.
    pub(crate) fn text_tool(&mut self, ui: &mut Ui, view: &View, doc: DocId, input: &CanvasInput, active: bool, scene: &mut Scene) {
        // Let go of with the tool, its tab, or the text itself.
        let gone = self.texting.editing.as_ref().is_some_and(|e| !active || e.doc != doc || e.node.is_some_and(|node| self.core.doc(doc).is_ok_and(|d| d.get(node).is_none_or(|n| n.kind != Kind::Text))));
        if gone {
            self.type_done();
        }
        if !active {
            return;
        }
        let (s, now, pointer) = (view.scale, ui.now(), ui.state.pointer);
        let at = view.to_doc.apply(pointer);
        let stamp = self.core.history(doc).map_or(0, |h| h.stamp());
        // The Box and the panels have been drawn: a field of theirs
        // typed into has said so.
        self.texting.field = ui.state.ime_rect.is_some();
        if input.over {
            ui.state.cursor_icon = CursorIcon::Text;
        }
        // A press: the text under it is taken up, the caret where the
        // press is; anywhere else, a new one begins there.
        if input.pressed {
            self.type_done();
            let per_unit = view.to_window.linear(Vec2::X).length().max(1e-12);
            let lines = self.snap_to(ui, view, doc, &[]);
            let Ok(drawing) = self.core.doc(doc) else { return };
            let under = top_at(drawing, at, REACH * s / per_unit).filter(|&id| drawing.get(id).is_some_and(|n| n.kind == Kind::Text));
            let editing = match under.and_then(|id| drawing.get(id)) {
                Some(node) => {
                    let written = text::written(drawing, node);
                    let words = Words::of(written.lines);
                    let own = geometry::to_doc(drawing, node.id).and_then(|t| t.inverse()).unwrap_or(Affine::IDENTITY);
                    let caret = caret_at(drawing, node, &words, own.apply(at));
                    Editing { doc, node: Some(node.id), fresh: None, at: Vec2::new(first(node, "x"), first(node, "y")), group: node.parent.unwrap_or(drawing.root()), words, caret, leading: written.leading, typed: None, moved: now, stamp }
                }
                None => {
                    // On the grid or a line of what's there, unless Ctrl.
                    let place = lines.map_or(at, |lines| lines.point(at).0);
                    let group = self.tabs.iter().find(|t| t.doc == doc).map_or(drawing.root(), |tab| tab.selection.context(drawing));
                    let into = geometry::to_doc(drawing, group).and_then(|t| t.inverse()).unwrap_or(Affine::IDENTITY);
                    Editing { doc, node: None, fresh: None, at: into.apply(place), group, words: Words::default(), caret: Caret::default(), leading: self.texting.letters.leading, typed: None, moved: now, stamp }
                }
            };
            if let Some(node) = editing.node {
                self.select(vec![node]);
            }
            self.texting.editing = Some(editing);
        }
        let Some(mut e) = self.texting.editing.take() else { return };
        // The drawing changed under it (an undo): its words are what the
        // drawing has now.
        if e.typed.is_none() && e.stamp != stamp {
            e.stamp = stamp;
            if let Some(node) = e.node.and_then(|id| self.core.doc(doc).ok()?.get(id)) {
                let written = self.core.doc(doc).map(|drawing| text::written(drawing, node));
                if let Ok(written) = written {
                    (e.words, e.leading) = (Words::of(written.lines), written.leading);
                    e.caret = e.words.within(e.caret);
                }
            }
        }
        // What came from the keyboard, in order.
        let mut changed = false;
        for (_, came) in std::mem::take(&mut self.texting.inbox) {
            e.moved = now;
            match came {
                Came::Text(said) => changed |= e.words.insert(&mut e.caret, &said),
                Came::Key(press) => match press.key {
                    Key::Enter => {
                        e.words.newline(&mut e.caret);
                        changed = true;
                    }
                    Key::Backspace => changed |= e.words.backspace(&mut e.caret),
                    Key::Delete => changed |= e.words.delete(&mut e.caret),
                    Key::ArrowLeft => e.words.go(&mut e.caret, Go::Left),
                    Key::ArrowRight => e.words.go(&mut e.caret, Go::Right),
                    Key::ArrowUp => e.words.go(&mut e.caret, Go::Up),
                    Key::ArrowDown => e.words.go(&mut e.caret, Go::Down),
                    Key::Home => e.words.go(&mut e.caret, Go::Home),
                    Key::End => e.words.go(&mut e.caret, Go::End),
                    _ => {}
                },
            }
        }
        if changed {
            let command = self.core.doc(doc).ok().map(|drawing| self.typed(drawing, &e));
            let begun = e.typed.is_some() || self.core.begin(doc, Actor::Alva).is_ok();
            if let (Some(command), true) = (command, begun) {
                match self.core.update(doc, &command) {
                    Ok(applied) => e.fresh = applied.created.first().copied().filter(|_| e.node.is_none()),
                    Err(err) => {
                        let why = err.to_string();
                        let said = self.core.doc(doc).map_or(why.clone(), |d| in_row_names(d, &why));
                        self.toast(said);
                    }
                }
                e.typed = Some(now);
            }
        }
        // The caret: where the drawing shows the text, or where a new
        // one will start.
        let on = (now - e.moved).rem_euclid(BLINK * 2.0) < BLINK;
        ui.state.request_redraw_after(BLINK - (now - e.moved).rem_euclid(BLINK));
        if on && let Ok((looks, _)) = self.core.shown(doc) {
            let line = match e.node.or(e.fresh).and_then(|id| looks.get(id)).filter(|n| n.kind == Kind::Text) {
                Some(node) => caret_in(looks, node, &e.words, e.caret, e.leading).zip(geometry::to_doc(looks, node.id)),
                None => {
                    let page = ink_doc::arrange::page_box(looks);
                    let size = self.texting.letters.size_on(crate::boxes::step_for(page.width().max(page.height())));
                    let y = e.at.y + e.leading * size * e.caret.line as f64;
                    Some((Vec2::new(e.at.x, y - size * 0.8), Vec2::new(e.at.x, y + size * 0.2))).zip(geometry::to_doc(looks, e.group))
                }
            };
            scene.caret = line.map(|((top, bottom), to_doc)| (view.to_window.apply(to_doc.apply(top)), view.to_window.apply(to_doc.apply(bottom))));
        }
        let pause = e.typed.map(|since| now - since);
        self.texting.editing = Some(e);
        // A pause, and what was typed is a step.
        match pause {
            Some(idle) if idle >= PAUSE => self.type_settled(),
            Some(idle) => ui.state.request_redraw_after(PAUSE - idle),
            None => {}
        }
    }
}
