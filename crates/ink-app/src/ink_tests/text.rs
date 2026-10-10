//! The Text tool: a click and typing make a text, typing lands as a
//! step when it pauses, a click on a text takes it up with the caret
//! where the click is, and the keyboard is the text's meanwhile. Text
//! is set in "Ink Test" (boxes six tenths of an em across, a space
//! three), so every place is the same on any machine.

use std::sync::Once;

use ink_core::NodeId;

use super::*;

const BLANK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 48 48\">\n  <rect id=\"card\" x=\"30\" y=\"30\" width=\"10\" height=\"10\" fill=\"#08f\"/>\n</svg>\n";
const NEW: NodeId = NodeId(3);

fn blank(name: &str) -> Running {
    static FONTS: Once = Once::new();
    FONTS.call_once(|| {
        ink_doc::fonts::add(include_bytes!("../../../../tests/fonts/InkTest-Regular.ttf").to_vec()).unwrap();
        ink_doc::fonts::add(include_bytes!("../../../../tests/fonts/InkTest-Bold.ttf").to_vec()).unwrap();
    });
    let path = scratch(name).join("blank.svg");
    std::fs::write(&path, BLANK).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r.frames(2);
    r.key(Key::Char('t'), Modifiers::NONE);
    assert_eq!(r.ink.tools.active(), Tool::Text);
    r.ink.texting.letters.family = "Ink Test".to_owned();
    r
}

impl Running {
    /// Type `text`, a frame for it to go in.
    fn type_in(&mut self, text: &str) {
        self.h.type_text(text);
        self.frames(2);
    }

    /// Stop typing for long enough that what was typed is a step.
    fn pause(&mut self) {
        self.h.advance(0.6);
        self.frames(2);
    }

    /// The text made here, as the drawing has it.
    fn text(&self) -> String {
        let doc = self.ink.core.doc(self.doc()).unwrap();
        doc.markup(NEW).unwrap_or_default()
    }
}

#[test]
fn a_click_and_typing_make_a_text() {
    let mut r = blank("text-types");
    let doc = r.doc();
    // A click puts the caret there, on the grid; nothing's in the
    // drawing till something is typed.
    r.click(r.spot(10.2, 20.2));
    assert_eq!((r.ink.texting.caret(), r.steps().len(), r.svg().as_str()), (Some((0, 0)), 0, BLANK));
    // Typed, it's shown as it goes: lettered as the Box says, painted
    // as the last shape was filled, a sixth of the icon's page tall.
    r.type_in("Hi");
    assert!(r.ink.core.gesturing(doc) && r.steps().is_empty() && r.svg() == BLANK);
    assert!(r.ink.core.shown(doc).unwrap().0.to_svg().contains("<text x=\"10\" y=\"20\" font-family=\"Ink Test\" font-size=\"4\" fill=\"#f3b700\">Hi</text>"));
    // A letter is the text's, not a tool's key.
    assert_eq!((r.ink.tools.active(), r.ink.texting.caret()), (Tool::Text, Some((0, 2))));
    // When the typing pauses it's one step, and the text is what's
    // selected.
    r.pause();
    assert_eq!((r.text().as_str(), r.steps(), r.selected(), r.ink.core.gesturing(doc)), ("<text x=\"10\" y=\"20\" font-family=\"Ink Test\" font-size=\"4\" fill=\"#f3b700\">Hi</text>", vec!["Type".to_owned()], vec![NEW], false));
    // Enter starts a line; what's typed after is on it.
    r.key(Key::Enter, Modifiers::NONE);
    r.type_in("yo");
    r.pause();
    assert!(r.text().ends_with(">Hi<tspan x=\"10\" dy=\"1.2em\">yo</tspan></text>"), "{}", r.text());
    assert_eq!((r.steps().len(), r.ink.texting.caret()), (2, Some((1, 2))));
    // Backspace takes characters out, and at a line's start joins it to
    // the one before; the arrows, Home and End move about.
    for _ in 0..3 {
        r.key(Key::Backspace, Modifiers::NONE);
    }
    r.key(Key::Home, Modifiers::NONE);
    r.type_in("O");
    r.key(Key::ArrowRight, Modifiers::NONE);
    r.key(Key::Delete, Modifiers::NONE);
    r.pause();
    assert!(r.text().ends_with(">OH</text>"), "{}", r.text());
    assert_eq!((r.steps().len(), r.ink.texting.caret(), r.selected()), (3, Some((0, 2)), vec![NEW]));
    // Escape lets go of it.
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!((r.ink.texting.caret(), r.ink.tools.active()), (None, Tool::Text));
    r.undo(3);
    assert_eq!(r.svg(), BLANK);
}

#[test]
fn a_click_on_a_text_takes_it_up_where_the_click_is() {
    let mut r = blank("text-takes");
    r.click(r.spot(10.0, 20.0));
    r.type_in("OHi");
    r.key(Key::Escape, Modifiers::NONE);
    assert_eq!(r.steps(), ["Type"]);
    // Each letter is 2.4 across from x = 10: a click between the first
    // two puts the caret there.
    r.click(r.spot(13.0, 19.0));
    assert_eq!((r.ink.texting.caret(), r.selected()), (Some((0, 1)), vec![NEW]));
    r.type_in("x");
    // Whatever else is done, what's typed lands first, as its own step:
    // an undo takes it back, and the text typed into says what the
    // drawing says.
    r.key(Key::Char('z'), Modifiers::CTRL);
    assert!(r.text().ends_with(">OHi</text>") && r.steps() == ["Type"], "{} {:?}", r.text(), r.steps());
    // (The caret stays where it was: after the "x" that's gone.)
    r.type_in("!");
    r.pause();
    assert!(r.text().ends_with(">OH!i</text>"), "{}", r.text());
    // A text whose last character is taken out goes; typed into again,
    // it's a new one in its place.
    r.key(Key::End, Modifiers::NONE);
    for _ in 0..4 {
        r.key(Key::Backspace, Modifiers::NONE);
    }
    r.pause();
    assert!(r.ink.core.doc(r.doc()).unwrap().get(NEW).is_none() && r.steps().len() == 3 && r.ink.texting.caret() == Some((0, 0)));
    // A click somewhere else, and nothing typed: nothing made.
    let before = r.svg();
    r.click(r.spot(4.0, 44.0));
    r.click(r.spot(20.0, 44.0));
    r.key(Key::Escape, Modifiers::NONE);
    // And with no text typed into, a letter is a tool's key again.
    r.key(Key::Char('v'), Modifiers::NONE);
    assert_eq!((r.svg(), r.steps().len(), r.ink.tools.active()), (before, 3, Tool::Pointer));
}

#[test]
fn the_box_letters_the_text_typed_into_and_the_next_one() {
    let mut r = blank("text-box");
    let doc = r.doc();
    r.click(r.ink.toolbox.rect().expect("the Text tool's Box").center());
    assert_eq!(r.box_rows(), ["Font", "Size", "Bold", "Italic", "Align", "Line Height"]);
    // Before anything is typed, its rows are the next text's: set
    // there, a text is typed that way.
    r.type_in_box("Size", "6");
    let bold = r.in_box("Bold");
    r.click(Vec2::new(bold.min.x + 27.0, bold.center().y));
    assert_eq!((r.ink.texting.letters.size, r.ink.texting.letters.bold, r.steps().len()), (6.0, true, 0));
    r.click(r.spot(10.0, 20.0));
    r.type_in("Hi");
    r.pause();
    assert_eq!(r.text(), "<text x=\"10\" y=\"20\" font-family=\"Ink Test\" font-size=\"6\" font-weight=\"bold\" fill=\"#f3b700\">Hi</text>");
    // With a text typed into, they're that text's: each a step of its
    // own, after what was typed.
    r.type_in("!");
    r.type_in_box("Size", "8");
    assert!(r.text().contains("font-size=\"8\"") && r.text().ends_with(">Hi!</text>"), "{}", r.text());
    assert_eq!(r.steps(), ["Type", "Type", "Font Size"]);
    let italic = r.in_box("Italic");
    r.click(Vec2::new(italic.min.x + 27.0, italic.center().y));
    let bold = r.in_box("Bold");
    r.click(Vec2::new(bold.min.x + 27.0, bold.center().y));
    assert!(r.text().contains("font-style=\"italic\"") && !r.text().contains("font-weight"), "{}", r.text());
    // A number dragged along is shown as it goes, and one step.
    let size = r.in_box("Size").center();
    r.drag_to(size, size + Vec2::new(20.0, 0.0));
    assert!(r.ink.core.gesturing(doc) && r.ink.core.shown(doc).unwrap().0.to_svg().contains("font-size=\"10\""));
    r.let_go();
    assert_eq!((r.steps().len(), r.text().contains("font-size=\"10\"")), (6, true));
    // Its lines' distance is written into its lines.
    r.click(r.spot(12.0, 19.0));
    r.key(Key::End, Modifiers::NONE);
    r.key(Key::Enter, Modifiers::NONE);
    r.type_in("yo");
    r.type_in_box("Line Height", "2");
    assert!(r.text().ends_with(">Hi!<tspan x=\"10\" dy=\"2em\">yo</tspan></text>"), "{}", r.text());
    assert_eq!(r.steps()[6..], ["Type", "Line Height"]);
    // A font that isn't installed says so, and what drew instead.
    r.key(Key::Escape, Modifiers::NONE);
    r.ink.edit(doc, &ink_core::Command::SetStyle { nodes: vec![NEW], set: vec![("font-family".to_owned(), Some("No Such Font, Ink Test".to_owned()))] }, "Font");
    r.frames(2);
    assert_eq!(r.box_rows(), ["Font", "Note", "Size", "Bold", "Italic", "Align", "Line Height"]);
    r.undo(9);
    assert_eq!(r.svg(), BLANK);
}
