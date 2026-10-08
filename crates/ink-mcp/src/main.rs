//! `lantern-ink-mcp`: Lantern Ink's MCP server over stdio (ARCHITECTURE
//! §6). `lntrn-mcp` speaks the protocol and runs the loop; `ink-tools`
//! is the tools; this is where they meet a process: its log, its
//! folders, its autosave, the desktop's font. stdout carries only
//! protocol: log lines go to stderr and
//! `~/.lantern/log/lantern-ink-mcp.log`. Unsaved drawings are autosaved
//! while it runs and once more when stdin closes, then it exits. No GPU,
//! no window.

#![deny(clippy::print_stdout)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ink_core::{Autosave, Core, autosave};
use ink_tools::{Env, Ink};
use lntrn_mcp::stdio::{self, Event};
use lntrn_mcp::{Log, Server};

/// Previews older than this, from any run, are cleared at start.
const PREVIEW_AGE: Duration = Duration::from_secs(24 * 3600);

fn main() {
    lntrn_core::log::init();
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let log = Log::open(&home.join(".lantern/log/lantern-ink-mcp.log"));
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    log.line(&format!("start: pid {}, unix time {epoch}", std::process::id()));
    // Text that asks for `sans-serif` is set in the desktop's own font.
    if let Some(font) = ink_core::desktop::use_font(&home) {
        log.line(&format!("sans-serif is {font} (lantern.toml)"));
    }
    let env = Env::from_process();
    prune(&env.previews, &log);
    let mut server = Server::new(Ink::new(Core::headless(), env));
    let sink = log.clone();
    server.set_log(move |line| sink.line(line));

    let mut saver = Autosave::new(home.join(".lantern/config/lantern-ink/autosave"), "mcp");
    let say = |line: &str| log.line(line);
    stdio::serve(&mut server, autosave::IDLE, |server, event| match event {
        Event::Handled if !saver.overdue() => {}
        Event::Handled | Event::Idle => saver.run(server.host().core(), &say),
        Event::Closed => {
            log.line("the client has gone");
            saver.run(server.host().core(), &say);
        }
    });
    log.line("exit");
}

/// Clear previews older than a day: every run leaves its newest.
fn prune(dir: &Path, log: &Log) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let old = |e: &std::fs::DirEntry| e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > PREVIEW_AGE);
    let stale: Vec<PathBuf> = entries.filter_map(Result::ok).filter(old).map(|e| e.path()).collect();
    for path in &stale {
        let _ = std::fs::remove_file(path);
    }
    if !stale.is_empty() {
        log.line(&format!("cleared {} old previews", stale.len()));
    }
}
