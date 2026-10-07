//! Lantern Ink's MCP tools (ARCHITECTURE §6): their schemas, and the
//! host that turns a call into a Command (or a query) on an `ink-core`.
//! `ink-mcp` runs it over stdio with `lntrn-mcp`; the live bridge will
//! run the same host in Ink's window.

mod describe;
mod env;
mod input;
mod preview;
mod tools;

use ink_core::Core;
use lntrn_mcp::{Call, Host, Info, Reply, Tool, ToolError};

pub use env::Env;

use crate::input::In;
use crate::tools::{Ctx, Entry};

/// What the model reads up front (Claude Code keeps 2048 chars).
pub const INSTRUCTIONS: &str = "Lantern Ink: make and edit SVG drawings (icons, logos), headless in this server (ids like d1). \
A drawing is its SVG file's own tree: elements (nodes \"N3\") with their attributes exactly as the file writes them. \
Whatever you don't change is saved byte for byte as it was, so an edit is a small diff. \
Conventions: an attribute's numbers are the SVG's own, in the element's own coordinates (inside whatever transforms its groups have), y down; \
paint is any SVG paint (\"#rrggbb\", \"none\", \"url(#id)\"); later in the file is further up the picture. \
Address everything by the ids results return: doc \"d1\", node \"N3\". Ids never change or get reused while a drawing is open; \
an element's own id=\"…\" is just an attribute. \
Make things with node_add (one element and its attributes) or node_add_svg (markup as you'd write it); \
change them with node_set (any attribute; null takes one off), node_move (the stacking order, or into a group) and node_delete. \
Move, scale, turn and flip things on the page with node_transform, and line them up with node_align: both work in the drawing's coordinates \
whatever groups a node is in, and write the change into its own numbers where those can say it. \
node_duplicate copies, node_group and node_ungroup make and dissolve groups; none of them changes the picture. \
doc_info lists nodes front to back with the box where each shows; doc_source reads the markup itself. \
Workflow: doc_new or doc_open, then edit, then doc_preview to look (ask at milestones, not after every call; \
renderer \"lantern\" shows how Lantern's apps will draw it at icon sizes), then doc_save (.svg) or doc_export (png, jpeg, webp). \
Use batch for many edits: one undo step, all or nothing; name what a step makes with \"as\" and refer to it as \"@name\". \
Every edit is undoable (history_undo). Refused calls say why and how to fix them. \
Not drawn yet (kept in the file all the same): <text>, <use>, <image>, masks, patterns, <style> rules, filters other than feDropShadow. \
Unsaved drawings live only in this server process: save what matters.";

/// Ink as an MCP server's host: a core, and the tools that work on it.
pub struct Ink {
    core: Core,
    env: Env,
    entries: Vec<Entry>,
    specs: Vec<Tool>,
}

impl Ink {
    pub fn new(core: Core, env: Env) -> Ink {
        let entries = tools::all();
        let specs = entries.iter().map(|e| e.spec).collect();
        Ink { core, env, entries, specs }
    }

    pub fn core(&self) -> &Core {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    pub fn env(&self) -> &Env {
        &self.env
    }
}

impl Host for Ink {
    fn info(&self) -> Info {
        Info { name: "lantern-ink", title: "Lantern Ink", version: env!("CARGO_PKG_VERSION"), instructions: INSTRUCTIONS }
    }

    fn tools(&self) -> &[Tool] {
        &self.specs
    }

    fn call(&mut self, call: &Call) -> Result<Reply, ToolError> {
        let entry = self.entries.iter().find(|e| e.spec.name == call.tool.name).ok_or_else(|| ToolError(format!("there's no tool \"{}\"", call.tool.name)))?;
        let mut ctx = Ctx { core: &mut self.core, env: &self.env, entries: &self.entries };
        tools::run(&mut ctx, entry, &In { args: call.args, names: None, env: &self.env })
    }
}
