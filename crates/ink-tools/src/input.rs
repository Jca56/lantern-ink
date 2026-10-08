//! A call's arguments as Ink's tools read them: document and node ids,
//! where a node goes, attribute values. Every mistake says how to fix
//! the call.

use std::collections::HashMap;

use ink_core::ink_doc::{DocError, Document};
use ink_core::{CoreError, DocId, NodeId, Place};
use ink_geom::number;
use lntrn_data::Doc;
use lntrn_mcp::{Args, ToolError, fail, schema};

use crate::env::Env;

/// Nodes named by earlier steps of a batch (`"as": "sun"` → `"@sun"`).
pub(crate) type Names = HashMap<String, NodeId>;

/// What a document's refusal leaves for a tool's caller to be told: the
/// argument that says what it asks to be told, the tool that gets
/// round it. (The document says why in words the window can use too.)
fn hint(why: &str) -> Option<&'static str> {
    if why.contains(" is locked: ") {
        // A lock is Alva's way of saying leave this alone.
        Some("node_mark unlocks, but Alva locks what she doesn't want changed: ask her first")
    } else if why.ends_with(" or say to drop it") || why.ends_with(" or say to drop them") {
        Some("drop: true says so")
    } else if why.ends_with(" Say so to make them anyway") {
        Some("as_drawn: true says so")
    } else if why.contains(" is a <text>: ") {
        Some("text_to_path makes a text paths, which are shapes")
    } else {
        None
    }
}

/// A core's refusal, in words that say what to do next.
pub(crate) fn refused(e: CoreError) -> ToolError {
    match e {
        CoreError::NoSuchDoc(id) => ToolError(format!("no open document {id} (doc_list shows the open ones)")),
        CoreError::Doc(DocError::NoSuchNode(id)) => ToolError(format!("no node {id} in this document (doc_info lists its nodes)")),
        CoreError::Doc(DocError::Invalid(why)) => match hint(&why) {
            Some(hint) => ToolError(format!("{why} ({hint})")),
            None => ToolError(why),
        },
        e => ToolError(e.to_string()),
    }
}

/// A document's refusal of a Command, the same way.
pub(crate) fn refused_edit(e: DocError) -> ToolError {
    refused(CoreError::Doc(e))
}

/// A call's arguments, the names a batch has given so far, and where
/// files are.
pub(crate) struct In<'a> {
    pub args: Args<'a>,
    pub names: Option<&'a Names>,
    pub env: &'a Env,
}

impl In<'_> {
    pub fn doc(&self) -> Result<DocId, ToolError> {
        let text = self.args.str("doc_id")?;
        text.parse().or_else(|_| fail(format!("doc_id \"{text}\" isn't a document id like \"d1\"")))
    }

    pub fn node(&self, key: &str) -> Result<NodeId, ToolError> {
        self.node_in(self.args.need(key)?, key)
    }

    pub fn opt_node(&self, key: &str) -> Result<Option<NodeId>, ToolError> {
        self.args.get(key).map(|d| self.node_in(d, key)).transpose()
    }

    /// A list of node ids, at least one.
    pub fn nodes(&self, key: &str) -> Result<Vec<NodeId>, ToolError> {
        let ids: Vec<NodeId> = self.args.list(key, "node ids")?.iter().map(|d| self.node_in(d, key)).collect::<Result<_, _>>()?;
        if ids.is_empty() {
            return fail(format!("\"{key}\" is empty: name at least one node"));
        }
        Ok(ids)
    }

    fn node_in(&self, d: &Doc, key: &str) -> Result<NodeId, ToolError> {
        let text = d.as_str().ok_or_else(|| ToolError(format!("\"{key}\" should be a node id like \"N3\"")))?;
        if let Some(name) = text.strip_prefix('@') {
            let names = self.names.ok_or_else(|| ToolError(format!("\"{text}\": @names work only between the steps of a batch")))?;
            return names.get(name).copied().ok_or_else(|| ToolError(format!("\"{text}\": no earlier step of this batch is named \"{name}\" (name one with \"as\")")));
        }
        text.parse().or_else(|_| fail(format!("\"{key}\": \"{text}\" isn't a node id like \"N3\"")))
    }

    /// `above`, `below`, `into` or `at`: at most one; on top of the
    /// whole drawing when none. Later in the file is further up.
    pub fn place(&self, doc: &Document) -> Result<Place, ToolError> {
        let given: Vec<&str> = ["above", "below", "into", "at"].into_iter().filter(|k| self.args.has(k)).collect();
        if given.len() > 1 {
            return fail(format!("give one of above, below, into or at, not {}", given.join(" and ")));
        }
        Ok(match given.first() {
            None => Place::LastIn(doc.root()),
            Some(&"above") => Place::After(self.node("above")?),
            Some(&"below") => Place::Before(self.node("below")?),
            Some(&"into") => Place::LastIn(self.node("into")?),
            _ => match self.args.str("at")? {
                "top" => Place::LastIn(doc.root()),
                "bottom" => Place::FirstIn(doc.root()),
                other => return fail(format!("\"at\" is \"top\" or \"bottom\", not \"{other}\"")),
            },
        })
    }
}

/// An attribute's value as a tool is given it: a string as it is, a
/// number written as short as it can be, to the `decimals` its document
/// keeps (D15). `None` for null: take it off.
pub(crate) fn attr_value(name: &str, value: &Doc, decimals: usize) -> Result<Option<String>, ToolError> {
    match value {
        Doc::Null => Ok(None),
        Doc::Str(s) => Ok(Some(s.clone())),
        Doc::Int(n) => Ok(Some(n.to_string())),
        Doc::Float(v) if v.is_finite() => Ok(Some(number::format(*v, decimals))),
        _ => fail(format!("the attribute \"{name}\" should be a string or a number (or null, to take it off)")),
    }
}

/// The schema pieces every tool shares.
pub(crate) mod common {
    use super::*;
    use lntrn_data::Map;
    use lntrn_mcp::schema::Props;

    pub fn doc_id() -> Doc {
        schema::pattern("^[dw][0-9]+$", "The document, like \"d1\"")
    }

    pub fn node_id(desc: &str) -> Doc {
        schema::pattern("^(N[0-9]+|@[A-Za-z0-9_-]+)$", &format!("{desc}: a node id like \"N3\" (or \"@name\" inside a batch)"))
    }

    pub fn path(desc: &str) -> Doc {
        schema::string(&format!("{desc}. Absolute, \"~/…\", or relative to the project directory"))
    }

    /// An attribute's value: a string or a number, and with `or_null`
    /// null too (take the attribute off).
    pub fn value(desc: &str, or_null: bool) -> Doc {
        let types: &[&str] = if or_null { &["string", "number", "null"] } else { &["string", "number"] };
        let mut m = Map::new();
        m.insert("type", Doc::List(types.iter().map(|&t| t.into()).collect()));
        m.insert("description", desc.into());
        Doc::Map(m)
    }

    /// An object of whatever another schema says: a batch step's
    /// arguments, which are its tool's.
    pub fn any_object(desc: &str) -> Doc {
        let mut m = Map::new();
        m.insert("type", "object".into());
        m.insert("description", desc.into());
        Doc::Map(m)
    }

    /// Where a new or moved node goes: at most one of these; on top of
    /// the whole drawing when none.
    pub fn placement() -> Props {
        vec![
            ("above", node_id("Put it just above this node (right after it in the file)")),
            ("below", node_id("Put it just below this node (right before it in the file)")),
            ("into", node_id("Put it in this node (a group, usually), on top of what's there")),
            ("at", schema::one_of(&["top", "bottom"], "The top or the bottom of the whole drawing")),
        ]
    }

    /// An edit's schema: `doc_id` first, `preview` last.
    pub fn edit(required: &[&str], props: Props) -> Doc {
        let mut all: Props = vec![("doc_id", doc_id())];
        all.extend(props);
        all.push(("preview", schema::boolean("Attach a picture of the result", false)));
        let mut req = vec!["doc_id"];
        req.extend_from_slice(required);
        schema::object(&req, all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lntrn_data::json;

    fn env() -> Env {
        Env::new("/tmp/p".into(), None, "t")
    }

    #[test]
    fn ids_and_places() {
        let doc = Document::parse(DocId(1), "<svg><g/></svg>").unwrap();
        let d = json::parse(r#"{"doc_id":"d2","node_id":"N7","node_ids":["N2","@sun"],"above":"N3"}"#).unwrap();
        let names = Names::from([("sun".to_owned(), NodeId(9))]);
        let env = env();
        let a = In { args: Args::new(&d).unwrap(), names: Some(&names), env: &env };
        assert_eq!(a.doc(), Ok(DocId(2)));
        assert_eq!(a.node("node_id"), Ok(NodeId(7)));
        assert_eq!(a.nodes("node_ids"), Ok(vec![NodeId(2), NodeId(9)]));
        assert_eq!(a.place(&doc), Ok(Place::After(NodeId(3))));
        assert_eq!(a.opt_node("below"), Ok(None));
        let places = |text: &str| {
            let d = json::parse(text).unwrap();
            In { args: Args::new(&d).unwrap(), names: None, env: &env }.place(&doc)
        };
        assert_eq!(places("{}"), Ok(Place::LastIn(NodeId(1))), "on top of everything, by default");
        assert_eq!(places(r#"{"below":"N2"}"#), Ok(Place::Before(NodeId(2))));
        assert_eq!(places(r#"{"into":"N2"}"#), Ok(Place::LastIn(NodeId(2))));
        assert_eq!(places(r#"{"at":"bottom"}"#), Ok(Place::FirstIn(NodeId(1))));
        assert_eq!(places(r#"{"at":"middle"}"#).unwrap_err().0, "\"at\" is \"top\" or \"bottom\", not \"middle\"");
        assert_eq!(places(r#"{"above":"N2","at":"top"}"#).unwrap_err().0, "give one of above, below, into or at, not above and at");
    }

    #[test]
    fn mistakes_say_how_to_fix_them() {
        let d = json::parse(r#"{"doc_id":"1","node_id":"@sun","other":"L3","none":[],"n":7}"#).unwrap();
        let env = env();
        let a = In { args: Args::new(&d).unwrap(), names: None, env: &env };
        assert_eq!(a.doc().unwrap_err().0, "doc_id \"1\" isn't a document id like \"d1\"");
        assert!(a.node("node_id").unwrap_err().0.contains("only between the steps of a batch"));
        assert_eq!(a.node("other").unwrap_err().0, "\"other\": \"L3\" isn't a node id like \"N3\"");
        assert_eq!(a.node("n").unwrap_err().0, "\"n\" should be a node id like \"N3\"");
        assert_eq!(a.nodes("none").unwrap_err().0, "\"none\" is empty: name at least one node");
        let names = Names::new();
        let b = In { args: Args::new(&d).unwrap(), names: Some(&names), env: &env };
        assert!(b.node("node_id").unwrap_err().0.contains("no earlier step of this batch is named \"sun\""));
        assert_eq!(refused(CoreError::NoSuchDoc(DocId(4))).0, "no open document d4 (doc_list shows the open ones)");
        assert_eq!(refused_edit(DocError::NoSuchNode(NodeId(4))).0, "no node N4 in this document (doc_info lists its nodes)");
        // What a document's refusal asks to be told, a tool is told how.
        let told = |why: &str| refused_edit(DocError::Invalid(why.to_owned())).0;
        assert_eq!(told("N2 is locked: nothing about it changes until it's unlocked"), "N2 is locked: nothing about it changes until it's unlocked (node_mark unlocks, but Alva locks what she doesn't want changed: ask her first)");
        assert!(told("N5 has a clip path: ungrouping would lose it. Take it off first, or say to drop it").ends_with("or say to drop it (drop: true says so)"));
        assert!(told("paths made of it would be that font's for good. Say so to make them anyway").ends_with("(as_drawn: true says so)"));
        assert!(told("N5 is a <text>: a clip path is cut from shapes").ends_with("(text_to_path makes a text paths, which are shapes)"));
        assert_eq!(told("there's nothing to copy"), "there's nothing to copy");
    }

    #[test]
    fn attribute_values_are_written_short() {
        let v = |text: &str| attr_value("x", &json::parse(text).unwrap(), 3);
        assert_eq!(v("\"#ffc800\""), Ok(Some("#ffc800".into())));
        assert_eq!(v("12"), Ok(Some("12".into())));
        assert_eq!(v("12.50004"), Ok(Some("12.5".into())));
        assert_eq!(v("-0.0001"), Ok(Some("0".into())));
        assert_eq!(v("null"), Ok(None));
        assert!(v("true").unwrap_err().0.contains("string or a number"));
        assert!(v("[1]").is_err());
        assert_eq!(attr_value("x", &json::parse("12.50004").unwrap(), 5), Ok(Some("12.50004".into())));
    }
}
