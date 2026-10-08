//! The tools (ARCHITECTURE §8): which there are, how the toolbar groups
//! them, their names, keys and icons. The machinery is LS3's
//! (`studio-app/src/tools.rs`): a slot of the toolbar holds one tool or
//! a group of related ones, and a key steps through its cycle.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Pointer,
    Node,
    Pen,
    Rect,
    Ellipse,
    Line,
    Polygon,
    Text,
    Gradient,
    Eyedrop,
    Hand,
    Zoom,
}

impl Tool {
    pub const ALL: [Tool; 12] = [Tool::Pointer, Tool::Node, Tool::Pen, Tool::Rect, Tool::Ellipse, Tool::Line, Tool::Polygon, Tool::Text, Tool::Gradient, Tool::Eyedrop, Tool::Hand, Tool::Zoom];

    /// Whether Alt is this tool's own (a click of the Zoom tool zooms
    /// out with it; the shape tools draw out from the middle; the Node
    /// tool and the Pen break a mirror), so Alt+drag doesn't move the
    /// view.
    pub fn owns_alt(self) -> bool {
        matches!(self, Tool::Zoom | Tool::Rect | Tool::Ellipse | Tool::Line | Tool::Polygon | Tool::Node | Tool::Pen)
    }

    /// Its name: under its icon in a group's flyout, and in its tooltip.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Pointer => "Pointer",
            Tool::Node => "Node",
            Tool::Pen => "Pen",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Line => "Line",
            Tool::Polygon => "Polygon",
            Tool::Text => "Text",
            Tool::Gradient => "Gradient",
            Tool::Eyedrop => "Eyedropper",
            Tool::Hand => "Hand",
            Tool::Zoom => "Zoom",
        }
    }

    /// The key that picks it.
    pub fn key(self) -> Option<char> {
        KEYS.iter().find(|(_, cycle)| cycle.contains(&self)).map(|&(key, _)| key)
    }

    /// The tooltip: its name and its key, `Rectangle (R)`.
    pub fn tooltip(self) -> String {
        match self.key() {
            Some(key) => format!("{} ({})", self.label(), key.to_ascii_uppercase()),
            None => self.label().to_owned(),
        }
    }

    pub fn group(self) -> Option<Group> {
        Group::ALL.into_iter().find(|g| g.members().contains(&self))
    }

    pub fn icon(self) -> &'static [u8] {
        match self {
            Tool::Pointer => include_bytes!("../assets/icons/pointer.svg"),
            Tool::Node => include_bytes!("../assets/icons/node.svg"),
            Tool::Pen => include_bytes!("../assets/icons/pen.svg"),
            Tool::Rect => include_bytes!("../assets/icons/rectangle.svg"),
            Tool::Ellipse => include_bytes!("../assets/icons/ellipse.svg"),
            Tool::Line => include_bytes!("../assets/icons/line.svg"),
            Tool::Polygon => include_bytes!("../assets/icons/polygon.svg"),
            Tool::Text => include_bytes!("../assets/icons/text.svg"),
            Tool::Gradient => include_bytes!("../assets/icons/gradient.svg"),
            Tool::Eyedrop => include_bytes!("../assets/icons/eyedropper.svg"),
            Tool::Hand => include_bytes!("../assets/icons/hand.svg"),
            Tool::Zoom => include_bytes!("../assets/icons/zoom.svg"),
        }
    }
}

/// The plain-letter keys. Each is a cycle: from another tool it picks
/// the member its toolbar slot shows; pressed again with one of them in
/// hand, it steps to the next (Rectangle, Ellipse, Line, Polygon).
const KEYS: [(char, &[Tool]); 9] = [
    ('v', &[Tool::Pointer]),
    ('a', &[Tool::Node]),
    ('p', &[Tool::Pen]),
    ('r', &[Tool::Rect, Tool::Ellipse, Tool::Line, Tool::Polygon]),
    ('t', &[Tool::Text]),
    ('g', &[Tool::Gradient]),
    ('i', &[Tool::Eyedrop]),
    ('h', &[Tool::Hand]),
    ('z', &[Tool::Zoom]),
];

/// The tools `key` (lowercase) cycles through.
pub fn key_cycle(key: char) -> Option<&'static [Tool]> {
    KEYS.iter().find(|&&(k, _)| k == key).map(|&(_, cycle)| cycle)
}

/// Related tools sharing one toolbar slot: a click picks the member it
/// shows, a right-click opens a flyout of them all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    Shape,
}

impl Group {
    pub const ALL: [Group; 1] = [Group::Shape];

    /// Its tools; the first is the one its slot starts with.
    pub fn members(self) -> &'static [Tool] {
        match self {
            Group::Shape => &[Tool::Rect, Tool::Ellipse, Tool::Line, Tool::Polygon],
        }
    }
}

/// One place down the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Separator,
    Single(Tool),
    Group(Group),
}

/// The toolbar, top to bottom.
pub const LAYOUT: [Slot; 12] = [
    Slot::Single(Tool::Pointer),
    Slot::Single(Tool::Node),
    Slot::Separator,
    Slot::Single(Tool::Pen),
    Slot::Group(Group::Shape),
    Slot::Single(Tool::Text),
    Slot::Separator,
    Slot::Single(Tool::Gradient),
    Slot::Single(Tool::Eyedrop),
    Slot::Separator,
    Slot::Single(Tool::Hand),
    Slot::Single(Tool::Zoom),
];

/// The tool in hand, and the member each group's slot shows (the one
/// last used).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tools {
    active: Tool,
    shown: [Tool; 1],
}

impl Default for Tools {
    fn default() -> Tools {
        Tools { active: Tool::Pointer, shown: Group::ALL.map(|g| g.members()[0]) }
    }
}

impl Tools {
    pub fn active(&self) -> Tool {
        self.active
    }

    /// Take `tool` in hand; its group's slot shows it from now on.
    pub fn select(&mut self, tool: Tool) {
        self.active = tool;
        if let Some(g) = tool.group() {
            self.shown[g as usize] = tool;
        }
    }

    /// The member `group`'s slot shows.
    pub fn shown(&self, group: Group) -> Tool {
        self.shown[group as usize]
    }

    /// The tool in `slot`, if it holds one.
    pub fn in_slot(&self, slot: Slot) -> Option<Tool> {
        match slot {
            Slot::Separator => None,
            Slot::Single(tool) => Some(tool),
            Slot::Group(group) => Some(self.shown(group)),
        }
    }

    /// A key was pressed: step its cycle. Whether the tool changed.
    pub fn press(&mut self, key: char) -> bool {
        let Some(cycle) = key_cycle(key.to_ascii_lowercase()) else { return false };
        let target = match cycle.iter().position(|&t| t == self.active) {
            Some(i) => cycle[(i + 1) % cycle.len()],
            // The member the slot shows, so the key and the button agree.
            None => cycle[0].group().map(|g| self.shown(g)).filter(|t| cycle.contains(t)).unwrap_or(cycle[0]),
        };
        let changed = target != self.active;
        self.select(target);
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on_toolbar() -> Vec<Tool> {
        LAYOUT
            .iter()
            .flat_map(|slot| match *slot {
                Slot::Separator => Vec::new(),
                Slot::Single(t) => vec![t],
                Slot::Group(g) => g.members().to_vec(),
            })
            .collect()
    }

    #[test]
    fn every_tool_has_one_place_on_the_toolbar() {
        let placed = on_toolbar();
        for tool in Tool::ALL {
            assert_eq!(placed.iter().filter(|&&t| t == tool).count(), 1, "{tool:?}");
        }
        assert_eq!(placed.len(), Tool::ALL.len());
        // A single tool isn't also in a group.
        for slot in LAYOUT {
            if let Slot::Single(t) = slot {
                assert_eq!(t.group(), None, "{t:?}");
            }
        }
        assert_eq!(Tool::ALL.iter().map(|t| *t as usize).collect::<Vec<_>>(), (0..Tool::ALL.len()).collect::<Vec<_>>());
    }

    #[test]
    fn keys_and_tooltips_agree() {
        // Every tool under one key, no key twice.
        for tool in Tool::ALL {
            assert_eq!(KEYS.iter().filter(|(_, c)| c.contains(&tool)).count(), 1, "{tool:?}");
        }
        for (i, a) in KEYS.iter().enumerate() {
            assert!(KEYS[i + 1..].iter().all(|b| a.0 != b.0), "{:?}", a.0);
        }
        assert_eq!(Tool::Pointer.tooltip(), "Pointer (V)");
        assert_eq!(Tool::Polygon.tooltip(), "Polygon (R)");
        assert_eq!(key_cycle('r'), Some(&[Tool::Rect, Tool::Ellipse, Tool::Line, Tool::Polygon][..]));
        assert_eq!(key_cycle('x'), None);
    }

    #[test]
    fn a_key_steps_its_cycle_and_a_slot_remembers() {
        let mut t = Tools::default();
        assert_eq!(t.active(), Tool::Pointer, "the Pointer adjusts everything: it's where Ink starts");
        assert!(t.press('r'));
        assert_eq!((t.active(), t.shown(Group::Shape)), (Tool::Rect, Tool::Rect));
        assert!(t.press('R'), "caps lock or not");
        assert_eq!(t.active(), Tool::Ellipse);
        // From elsewhere, a key picks what its slot shows.
        assert!(t.press('v'));
        assert!(!t.press('v'), "already in hand");
        assert!(t.press('r'));
        assert_eq!(t.active(), Tool::Ellipse);
        assert!(t.press('r') && t.press('r') && t.press('r'));
        assert_eq!((t.active(), t.shown(Group::Shape)), (Tool::Rect, Tool::Rect), "round the cycle");
        t.select(Tool::Polygon);
        assert_eq!(t.in_slot(LAYOUT[4]), Some(Tool::Polygon));
        assert_eq!(t.in_slot(Slot::Separator), None);
        assert!(!t.press('x'), "not a tool's key");
    }
}
