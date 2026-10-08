//! The canvas while the drawing changes under it: an edit costs the
//! tiles it touches, and a gesture shows as it goes without the
//! drawing being touched until it lands (ARCHITECTURE §4.3, §8).

use ink_core::{Actor, Command, NodeId};
use ink_geom::Affine;

use super::*;

/// A page of 240 units with a dot near its corner: at a 1080p window
/// it's a dozen tiles or more, and the dot is in one of them.
const DOTTED: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 240 240\">\n  <rect width=\"240\" height=\"240\" fill=\"#234\"/>\n  <circle cx=\"30\" cy=\"30\" r=\"6\" fill=\"#fc0\"/>\n</svg>\n";
const DOT: NodeId = NodeId(3);

fn dotted(name: &str) -> Running {
    let path = scratch(name).join("dotted.svg");
    std::fs::write(&path, DOTTED).unwrap();
    let mut r = Running::start(1920.0, 1080.0, 1.0);
    r.open(&path);
    r
}

fn moved(dx: f64, dy: f64) -> Command {
    Command::Transform { nodes: vec![DOT], by: Affine::translate(dx, dy) }
}

impl Running {
    /// Wait until the canvas shows the drawing as it's to be shown now.
    fn caught_up(&mut self) {
        self.until("the picture of the drawing as it is", |r| r.ink.tiles.look(r.doc()) == r.ink.core.shown(r.doc()).ok().map(|(_, look)| look));
        self.frames(2);
    }

    /// Every picture kept is one that shows, once each.
    fn nothing_left_behind(&self) {
        let showing = self.ink.tiles.showing(self.doc()).unwrap().1;
        assert_eq!(self.kept.live.len(), showing, "pictures kept, against tiles showing");
    }
}

#[test]
fn an_edit_draws_only_the_tiles_it_touches() {
    let mut r = dotted("an-edit");
    let doc = r.doc();
    let tiles = r.drawn();
    let asked = r.ink.tiles.asked;
    assert!(tiles >= 9 && asked >= tiles, "{tiles} tiles, {asked} asked for");
    r.ink.core.apply(doc, &moved(3.0, 2.0), Actor::Alva, "Move").unwrap();
    r.caught_up();
    let again = r.ink.tiles.asked - asked;
    assert!((1..=4).contains(&again), "{again} of {tiles} tiles drawn again for a dot moved");
    assert_eq!(r.ink.tiles.showing(doc).unwrap().1, tiles);
    r.nothing_left_behind();
    // Undone: the same few.
    let asked = r.ink.tiles.asked;
    r.ink.core.undo(doc).unwrap();
    r.caught_up();
    assert!((1..=4).contains(&(r.ink.tiles.asked - asked)));
    r.nothing_left_behind();
    // What changes nothing that shows draws nothing again.
    let asked = r.ink.tiles.asked;
    r.ink.core.apply(doc, &Command::SetLabel { node: DOT, label: Some("dot".into()) }, Actor::Alva, "Rename").unwrap();
    r.caught_up();
    assert_eq!(r.ink.tiles.asked, asked);
    r.nothing_left_behind();
}

#[test]
fn a_gesture_shows_as_it_goes_and_lands_once() {
    let mut r = dotted("a-gesture");
    let doc = r.doc();
    let tiles = r.drawn();
    r.ink.core.begin(doc, Actor::Alva).unwrap();
    // A drag that never rests: somewhere new at every frame. The
    // canvas still gets to show it on the way.
    let mut shown_going = false;
    for step in 1..=1500 {
        r.ink.core.update(doc, &moved(f64::from(step) * 0.01, 0.0)).unwrap();
        r.frames(1);
        if r.ink.tiles.look(doc).is_some_and(|look| look.is_preview()) {
            shown_going = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(shown_going, "the drag was never shown while it went");
    // Come to rest, the canvas shows where.
    r.ink.core.update(doc, &moved(8.0, 4.0)).unwrap();
    r.caught_up();
    assert!(r.ink.tiles.look(doc).unwrap().is_preview());
    r.nothing_left_behind();
    // All the while the drawing is what it was: no step taken, and
    // still what's on disk.
    assert_eq!((r.ink.core.doc(doc).unwrap().to_svg().as_str(), r.ink.is_modified(doc), r.ink.core.history(doc).unwrap().undoable().count()), (DOTTED, false, 0));
    // It lands as one step, whose picture is the one the drag ended
    // on: nothing is drawn again.
    let asked = r.ink.tiles.asked;
    r.ink.core.commit(doc, "Move").unwrap();
    r.caught_up();
    let landed = r.ink.tiles.look(doc).unwrap();
    assert!(!landed.is_preview() && r.ink.is_modified(doc));
    assert_eq!((r.ink.tiles.asked, r.ink.core.history(doc).unwrap().undoable().count(), r.ink.tiles.showing(doc).unwrap().1), (asked, 1, tiles));
    assert_eq!(r.ink.core.doc(doc).unwrap().node(DOT).unwrap().attr("cx"), Some("38"));
    r.nothing_left_behind();
    // Another, given up: the canvas goes back to the drawing as it is.
    r.ink.core.begin(doc, Actor::Alva).unwrap();
    r.ink.core.update(doc, &moved(-20.0, 30.0)).unwrap();
    r.caught_up();
    assert!(r.ink.tiles.look(doc).unwrap().is_preview());
    r.ink.core.cancel(doc);
    r.caught_up();
    assert_eq!((r.ink.tiles.look(doc), r.ink.core.history(doc).unwrap().undoable().count()), (Some(landed), 1));
    r.nothing_left_behind();
}
