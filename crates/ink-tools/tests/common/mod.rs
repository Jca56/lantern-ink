//! What the transcript tests share: a server on a folder of its own,
//! and calls to it the way Claude Code makes them.
#![allow(dead_code)]

use std::path::PathBuf;

use ink_core::Core;
use ink_tools::{Env, Ink};
use lntrn_data::{Doc, json};
use lntrn_mcp::Server;

pub fn server(test: &str) -> (Server<Ink>, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("ink-transcripts").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Server::new(Ink::new(Core::headless(), Env::new(dir.join("previews"), Some(dir.clone()), "test")));
    s.set_log(|_| {});
    (s, dir)
}

/// A tools/call on the 2026-07-28 protocol; its result.
pub fn call(s: &mut Server<Ink>, tool: &str, args: &str) -> Doc {
    let line = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{tool}","arguments":{args},"_meta":{{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}"#);
    let out = s.handle_line(&line);
    assert_eq!(out.len(), 1, "{tool}: {out:?}");
    assert!(!out[0].contains('\n'), "one line per message");
    let reply = json::parse(&out[0]).unwrap();
    reply.get("result").cloned().unwrap_or_else(|| panic!("{tool}: {}", out[0]))
}

pub fn text(result: &Doc) -> &str {
    result.get("content").and_then(Doc::as_list).unwrap().iter().rev().find_map(|b| b.get("text").and_then(Doc::as_str)).unwrap()
}

pub fn is_error(result: &Doc) -> bool {
    result.get("isError").and_then(Doc::as_bool) == Some(true)
}

/// A successful call (panics on a refusal, showing it).
pub fn ok(s: &mut Server<Ink>, tool: &str, args: &str) -> Doc {
    let r = call(s, tool, args);
    assert!(!is_error(&r), "{tool} refused: {}", text(&r));
    // Claude Code shows the model only the structured data when there is
    // any, so the text must be in it too.
    if let Some(d) = r.get("structuredContent") {
        assert_eq!(d.get("message").and_then(Doc::as_str), Some(text(&r)), "{tool}: its text is in its data");
    }
    r
}

/// A refused call's reason, without the tool's name in front.
pub fn refused(s: &mut Server<Ink>, tool: &str, args: &str) -> String {
    let r = call(s, tool, args);
    assert!(is_error(&r), "{tool} wasn't refused: {}", text(&r));
    text(&r).strip_prefix(&format!("{tool} refused: ")).unwrap_or_else(|| panic!("{}", text(&r))).to_owned()
}

pub fn data<'a>(result: &'a Doc, key: &str) -> &'a str {
    result.get("structuredContent").and_then(|d| d.get(key)).and_then(Doc::as_str).unwrap_or_else(|| panic!("no \"{key}\" in {}", json::write(result)))
}

/// A result's picture: its MIME type and pixels.
pub fn picture(result: &Doc) -> (String, lntrn_image::Image) {
    let image = result.path("content[0]").expect("an image block");
    let bytes = lntrn_core::encoding::base64_decode(image.get("data").and_then(Doc::as_str).unwrap()).unwrap();
    (image.get("mimeType").and_then(Doc::as_str).unwrap().to_owned(), lntrn_image::decode(&bytes).unwrap())
}
