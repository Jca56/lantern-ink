//! What a `<text>` draws, held to it: set in "Ink Test" (`tests/fonts/`),
//! whose every glyph is a box of known numbers.

use std::sync::Once;

use super::*;
use crate::fonts;
use crate::id::DocId;

/// The fonts the tests set text in (`tests/font.rs` makes them).
pub(crate) fn test_fonts() {
    static ADDED: Once = Once::new();
    ADDED.call_once(|| {
        fonts::add(include_bytes!("../../../../tests/fonts/InkTest-Regular.ttf").to_vec()).unwrap();
        fonts::add(include_bytes!("../../../../tests/fonts/InkTest-Bold.ttf").to_vec()).unwrap();
    });
}

/// The text that is `N2` of a 100 × 100 drawing, set.
fn laid(element: &str) -> Result<Laid, Unset> {
    test_fonts();
    let d = Document::parse(DocId(1), &format!(r#"<svg viewBox="0 0 100 100" font-family="Ink Test" font-size="10">{element}</svg>"#)).unwrap();
    lay(&d, d.node(NodeId(2)).unwrap(), Vec2::new(100.0, 100.0))
}

/// Each run's node and outline.
fn runs(element: &str) -> Vec<(u64, String)> {
    laid(element).unwrap().runs.iter().map(|run| (run.node.0, run.outline.to_data(3))).collect()
}

/// To a millionth: what a sum of ems is out by.
fn near(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

fn cells(element: &str) -> (f64, f64, f64, f64) {
    let b = laid(element).unwrap().cells.unwrap();
    (near(b.min.x), near(b.min.y), near(b.width()), near(b.height()))
}

#[test]
fn each_character_has_a_cell_for_a_caret_to_stand_by() {
    let cells_of = |element: &str| laid(element).unwrap().chars.iter().map(|c| (near(c.min.x), near(c.max.x), near(c.min.y), near(c.max.y))).collect::<Vec<_>>();
    // One a character, in the order they're said: an advance wide,
    // the font's top to its bottom, on its own line.
    assert_eq!(cells_of(r#"<text x="20" y="50">Hp<tspan x="20" dy="1.2em">a b</tspan></text>"#), [(20.0, 26.0, 42.0, 52.0), (26.0, 32.0, 42.0, 52.0), (20.0, 26.0, 54.0, 64.0), (26.0, 29.0, 54.0, 64.0), (29.0, 35.0, 54.0, 64.0)]);
    // Hung from its middle, they're where the glyphs are.
    assert_eq!(cells_of(r#"<text x="50" y="50" text-anchor="middle">Hp</text>"#), [(44.0, 50.0, 42.0, 52.0), (50.0, 56.0, 42.0, 52.0)]);
    assert!(cells_of("<text/>").is_empty());
}

#[test]
fn a_text_reads_back_as_the_lines_it_was_written_from() {
    use crate::command::Command;
    use crate::lettering::Span;
    let span = |text: &str, set: &[(&str, &str)]| Span { text: text.to_owned(), set: set.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect() };
    let read = |element: &str| {
        let d = Document::parse(DocId(1), &format!("<svg>{element}</svg>")).unwrap();
        written(&d, d.node(NodeId(2)).unwrap())
    };
    // What SetText writes, it reads back: lines, the empty ones
    // among them, and what each stretch sets for itself.
    let lines = vec![vec![span("one", &[])], vec![span("two ", &[]), span("red", &[("fill", "red")])], vec![], vec![], vec![span("five", &[])]];
    let mut d = Document::parse(DocId(1), r#"<svg><text x="4" y="9"/></svg>"#).unwrap();
    d.apply(&Command::SetText { node: NodeId(2), lines: lines.clone(), leading: Some(1.5) }).unwrap();
    let back = written(&d, d.node(NodeId(2)).unwrap());
    assert_eq!((back.lines.clone(), back.leading), (lines, 1.5));
    // And written again from that, nothing changes.
    assert!(d.apply(&Command::SetText { node: NodeId(2), lines: back.lines, leading: Some(back.leading) }).unwrap().is_nothing());
    // A text that starts with empty lines; one of one line; none.
    assert_eq!(read(r#"<text><tspan x="0" dy="2.4em">low</tspan></text>"#).lines, vec![vec![], vec![span("low", &[])]]);
    assert_eq!((read("<text>Hop on</text>"), read("<text/>").lines), (Written { lines: vec![vec![span("Hop on", &[])]], leading: 1.2 }, vec![vec![]]));
    // By another hand: a span in a span sets both's; what isn't a
    // property of SVG's (an id) isn't kept; spans set alike are one
    // stretch; a row is a row however it says how far down.
    let other = read(r#"<text x="1" y="2">a<tspan fill="red" id="r">b<tspan font-weight="bold" fill="blue">c</tspan></tspan><tspan x="1" y="14"><tspan fill="red">d</tspan><tspan fill="red">e</tspan></tspan></text>"#);
    assert_eq!(other.lines, vec![vec![span("a", &[]), span("b", &[("fill", "red")]), span("c", &[("font-weight", "bold"), ("fill", "blue")])], vec![span("de", &[("fill", "red")])]]);
}

#[test]
fn glyphs_stand_on_the_baseline_from_the_text_s_position() {
    // A capital is a box 0.1..0.5 em across and 0.7 em tall; a
    // descender hangs 0.2 em below the line.
    assert_eq!(runs(r#"<text x="20" y="50">Hp</text>"#), [(2, "M21 50 V43 H25 V50 Z M27 52 V45 H31 V52 Z".into())]);
    assert_eq!(cells(r#"<text x="20" y="50">Hp</text>"#), (20.0, 42.0, 12.0, 10.0), "two advances wide, the font's top to its bottom");
    assert_eq!(runs("<text>H</text>"), [(2, "M1 0 V-7 H5 V0 Z".into())], "no position is the origin");
    assert_eq!(runs(r#"<text x="50%" y="2em" font-size="20">I</text>"#), [(2, "M52 40 V26 H54 V40 Z".into())], "a share of the page, and ems of its own size");
    let empty = laid("<text x=\"5\">  </text>").unwrap();
    assert!(empty.runs.is_empty() && empty.cells.is_none());
}

#[test]
fn white_space_collapses_unless_it_is_kept() {
    // A space is 0.3 em.
    let width = |element: &str| cells(element).2;
    assert_eq!(width("<text>\n   H   H\n\tH  </text>"), 6.0 + 3.0 + 6.0 + 3.0 + 6.0);
    assert_eq!(width("<text xml:space=\"preserve\"> H  H </text>"), 3.0 + 6.0 + 3.0 + 3.0 + 6.0 + 3.0);
    assert_eq!(width("<text style=\"white-space: pre;\">H\nH</text>"), 15.0, "a kept line break is a space");
    assert_eq!(width("<text>H <tspan> H</tspan> </text>"), 15.0, "spaces collapse across elements");
    assert_eq!(width("<text>H<!-- not said -->&#72;<![CDATA[H]]></text>"), 18.0);
}

#[test]
fn spans_move_the_pen_and_keep_their_own_paint() {
    let two = r#"<text x="10" y="20">H<tspan x="10" dy="1.2em">H</tspan>H</text>"#;
    assert_eq!(runs(two), [(2, "M11 20 V13 H15 V20 Z".into()), (3, "M11 32 V25 H15 V32 Z".into()), (2, "M17 32 V25 H21 V32 Z".into())], "what follows a span goes on from where it ended");
    assert_eq!(runs(r#"<text x="10" y="20">H<tspan dx="4" dy="-2" font-size="5">H</tspan></text>"#)[1], (3, "M20.5 18 V14.5 H22.5 V18 Z".into()));
    // The innermost element at a character says where it goes.
    assert_eq!(runs(r#"<text x="10" y="20"><tspan x="30"><tspan x="50">H</tspan></tspan></text>"#), [(4, "M51 20 V13 H55 V20 Z".into())]);
    assert_eq!(runs(r#"<text x="10" y="20"><tspan x="30" y="40"><tspan y="60">H</tspan></tspan></text>"#), [(4, "M31 60 V53 H35 V60 Z".into())]);
    assert_eq!(runs(r#"<text x="10" y="20">H<tspan display="none">HHH</tspan><title>not text</title>H</text>"#), [(2, "M11 20 V13 H15 V20 Z M17 20 V13 H21 V20 Z".into())]);
}

#[test]
fn a_chunk_hangs_from_its_position_by_its_anchor() {
    let x = |element: &str| cells(element).0;
    assert_eq!(x(r#"<text x="50" text-anchor="middle">HH</text>"#), 44.0);
    assert_eq!(x(r#"<text x="50" text-anchor="end">HH</text>"#), 38.0);
    assert_eq!(x(r#"<text x="50" dx="10" text-anchor="middle">HH</text>"#), 54.0, "from where its first character goes");
    // Each line is a chunk of its own, with its own anchor.
    let lines = r#"<text x="50" y="20" text-anchor="middle">HH<tspan x="50" dy="12">HHHH</tspan><tspan x="50" dy="12" text-anchor="start">H</tspan></text>"#;
    assert_eq!(runs(lines).iter().map(|(_, d)| d.split(' ').next().unwrap().to_owned()).collect::<Vec<_>>(), ["M45", "M39", "M51"]);
}

#[test]
fn spacing_goes_after_each_character() {
    assert_eq!(runs(r#"<text letter-spacing="2">HH</text>"#), [(2, "M1 0 V-7 H5 V0 Z M9 0 V-7 H13 V0 Z".into())]);
    assert_eq!(cells(r#"<text letter-spacing="2">HH</text>"#).2, 16.0, "after the last one too");
    assert_eq!(cells(r#"<text word-spacing="5">H H</text>"#).2, 6.0 + 3.0 + 5.0 + 6.0);
    assert_eq!(cells(r#"<text x="50" text-anchor="end" letter-spacing="1em">H</text>"#), (34.0, -8.0, 16.0, 10.0));
}

#[test]
fn the_baseline_asked_for_sits_at_y() {
    let top = |baseline: &str| near(laid(&format!(r#"<text y="50" dominant-baseline="{baseline}">H</text>"#)).unwrap().runs[0].outline.bounds().unwrap().min.y);
    assert_eq!((top("auto"), top("alphabetic")), (43.0, 43.0));
    assert_eq!(top("middle"), 45.5, "half an x-height lower");
    assert_eq!(top("central"), 46.0, "halfway between the font's top and bottom");
    assert_eq!((top("text-before-edge"), top("text-after-edge")), (51.0, 41.0));
    assert_eq!((top("hanging"), top("mathematical")), (49.4, 47.0));
}

#[test]
fn fonts_are_chosen_a_run_at_a_time() {
    // The bold face's boxes are 0.05 em wider on each side.
    assert_eq!(runs(r#"<text>H<tspan font-weight="bold">H</tspan></text>"#), [(2, "M1 0 V-7 H5 V0 Z".into()), (3, "M6.5 0 V-7 H11.5 V0 Z".into())]);
    assert_eq!(runs(r#"<text font-weight="700" font-size="20">I</text>"#), [(2, "M1 0 V-14 H5 V0 Z".into())]);
    // A span that only paints differently is still one run with its neighbours.
    assert_eq!(runs(r#"<text>H<tspan fill="red">H</tspan>H</text>"#).iter().map(|r| r.0).collect::<Vec<_>>(), [2, 3, 2]);
}

#[test]
fn what_cannot_be_set_says_why() {
    assert_eq!(laid(r##"<text><textPath href="#p">H</textPath></text>"##), Err(Unset::OnPath));
    assert_eq!(laid(r#"<text x="1 2 3">HHH</text>"#), Err(Unset::Placed));
    assert_eq!(laid(r#"<text>H<tspan dy="1,2">HH</tspan></text>"#), Err(Unset::Placed));
    assert_eq!(laid(r#"<text rotate="30">H</text>"#), Err(Unset::Placed));
    assert!(laid(r#"<text rotate="0">H</text>"#).is_ok(), "no turn is no turn");
    assert_eq!(laid(r#"<text writing-mode="vertical-rl">H</text>"#), Err(Unset::Upright));
    assert_eq!(laid(r#"<text textLength="40">H</text>"#), Err(Unset::Stretched));
    assert!(Unset::OnPath.to_string().contains("<textPath>"));
}

#[test]
fn a_text_says_its_words_whether_or_not_it_can_be_set() {
    let d = Document::parse(DocId(1), "<svg><text>\n  Hop <tspan>on</tspan>\n  <tspan display=\"none\">not</tspan><title>nor this</title> &amp; in\n</text><text><textPath>round</textPath> we go</text></svg>").unwrap();
    let (plain, on_path) = (d.node(NodeId(2)).unwrap(), d.node(NodeId(6)).unwrap());
    assert_eq!((said(&d, plain).as_str(), unset(&d, plain)), ("Hop on & in", None));
    assert_eq!((said(&d, on_path).as_str(), unset(&d, on_path)), ("round we go", Some(Unset::OnPath)));
    assert_eq!(lines(&d, plain), ["Hop on & in"]);
    let three = Document::parse(DocId(1), r#"<svg><text x="1" y="2">one<tspan x="1" dy="1.2em">two <tspan fill="red">red</tspan></tspan><tspan y="9">three</tspan><tspan dx="1">!</tspan></text><text/></svg>"#).unwrap();
    assert_eq!(lines(&three, three.node(NodeId(2)).unwrap()), ["one", "two red", "three!"], "a nudge isn't a new line");
    assert_eq!(lines(&three, three.node(NodeId(7)).unwrap()), [""]);
}

#[test]
fn a_text_says_which_fonts_it_ended_up_in() {
    test_fonts();
    let d = Document::parse(DocId(1), r#"<svg><text font-family="No Such Font, 'Ink Test', serif" font-size="10">a<tspan font-weight="bold" font-size="20">b</tspan><tspan>c</tspan></text></svg>"#).unwrap();
    let fonts = lettered(&d, d.node(NodeId(2)).unwrap());
    assert_eq!(fonts.len(), 2, "a span lettered as its text is no new font: {fonts:?}");
    assert_eq!(fonts[0], Lettered { family: "Ink Test".into(), bold: false, italic: false, no_italic: false, size: 10.0, missing: vec!["No Such Font".into()] });
    assert_eq!((fonts[1].bold, fonts[1].size), (true, 20.0));
    // Italic in a family with no italic of its own is upright, and says so.
    let leaning = Document::parse(DocId(1), r#"<svg><text font-family="Ink Test" font-style="italic">a</text></svg>"#).unwrap();
    let fonts = lettered(&leaning, leaning.node(NodeId(2)).unwrap());
    assert_eq!((fonts[0].italic, fonts[0].no_italic), (false, true));
}

#[test]
fn a_run_is_painted_as_its_elements_say() {
    test_fonts();
    let d = Document::parse(DocId(1), r#"<svg><text fill="red" stroke-width="2">H<tspan stroke="blue"><tspan fill="lime">H</tspan></tspan></text></svg>"#).unwrap();
    let text = d.node(NodeId(2)).unwrap();
    let own = Style::default().cascade(text);
    assert_eq!(style_of(&d, text, &own, NodeId(2)), own);
    let inner = style_of(&d, text, &own, NodeId(4));
    assert_eq!((inner.fill, inner.stroke, inner.line.width), (crate::style::Paint::parse("lime").unwrap(), crate::style::Paint::parse("blue").unwrap(), 2.0));
}
