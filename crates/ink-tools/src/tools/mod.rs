//! The tools, in one fixed order (it helps the client cache the list).
//! A tool either answers directly (reads, files, history) or is an edit:
//! it builds one Command from its arguments, which the core applies as
//! one undo step, and which a batch can hold.

mod docs;
mod history;
mod nodes;

use ink_core::ink_doc::Document;
use ink_core::{Actor, Applied, Command, Core, DocId};
use lntrn_mcp::{Reply, Tool, ToolError};

use crate::env::Env;
use crate::input::{In, refused};
use crate::preview::{self, Options};

/// A tool that answers for itself.
pub(crate) type Direct = fn(&mut Ctx, &In) -> Result<Reply, ToolError>;
/// An edit's Command, built from its arguments against the document it
/// will apply to (a batch builds against its dry run).
pub(crate) type Build = fn(&Document, &In) -> Result<Command, ToolError>;
/// What an edit tells the model, from the document after it.
pub(crate) type Report = fn(&Document, &Applied) -> Reply;

pub(crate) enum Handler {
    Direct(Direct),
    Edit { build: Build, report: Report },
}

/// A tool as the client is told of it, and what runs it.
pub(crate) struct Entry {
    pub spec: Tool,
    pub handler: Handler,
}

pub(crate) struct Ctx<'a> {
    pub core: &'a mut Core,
    pub env: &'a Env,
    pub entries: &'a [Entry],
}

pub(crate) fn all() -> Vec<Entry> {
    let mut tools = docs::tools();
    tools.extend(nodes::tools());
    tools.extend(history::tools());
    tools
}

/// Run `entry` on `input`.
pub(crate) fn run(ctx: &mut Ctx, entry: &Entry, input: &In) -> Result<Reply, ToolError> {
    match &entry.handler {
        Handler::Direct(f) => f(ctx, input),
        Handler::Edit { build, report } => {
            let doc = input.doc()?;
            let wanted = input.args.opt_bool("preview")? == Some(true);
            let command = build(ctx.core.doc(doc).map_err(refused)?, input)?;
            let applied = ctx.core.apply(doc, &command, Actor::Claude, entry.spec.name).map_err(refused)?;
            let reply = report(ctx.core.doc(doc).map_err(refused)?, &applied);
            previewed(ctx, doc, reply, wanted)
        }
    }
}

/// `reply`, with a picture of `doc` as it now is attached if it's
/// `wanted`.
pub(crate) fn previewed(ctx: &Ctx, doc: DocId, mut reply: Reply, wanted: bool) -> Result<Reply, ToolError> {
    if !wanted {
        return Ok(reply);
    }
    let (image, note) = preview::preview(ctx.core, ctx.env, doc, &Options::default())?;
    reply.text = format!("{}\n{note}", reply.text);
    Ok(reply.image(image))
}

#[cfg(test)]
mod tests {
    use super::history::EDITS;
    use super::*;

    #[test]
    fn a_batch_takes_every_edit_and_nothing_else() {
        let mut edits: Vec<&str> = all().iter().filter(|t| matches!(t.handler, Handler::Edit { .. })).map(|t| t.spec.name).collect();
        let mut listed = EDITS.to_vec();
        edits.sort();
        listed.sort();
        assert_eq!(edits, listed);
    }
}
