//! Where the window's messages go (LS3's `log.rs`):
//! launched from the desktop there's no terminal, so stderr is pointed at
//! `~/.lantern/log/lantern-ink.log`. From a terminal, stderr stays
//! the terminal. A panic goes to the log either way, as `PANIC:` and a
//! backtrace.

use std::io::Write;
use std::path::PathBuf;

unsafe extern "C" {
    fn isatty(fd: i32) -> i32;
    fn dup2(from: i32, to: i32) -> i32;
}

/// A log past this size is started over at launch.
const MAX_BYTES: u64 = 8 << 20;

fn path() -> Option<PathBuf> {
    lntrn_sys::dirs::lantern().map(|l| l.join("log/lantern-ink.log"))
}

pub fn start() {
    let path = path();
    // SAFETY: plain libc calls on the standard descriptors.
    let terminal = unsafe { isatty(2) } != 0;
    if let Some(p) = &path
        && !terminal
        && let Some(dir) = p.parent()
        && std::fs::create_dir_all(dir).is_ok()
    {
        let fresh = std::fs::metadata(p).is_ok_and(|m| m.len() > MAX_BYTES);
        if let Ok(f) = std::fs::OpenOptions::new().create(true).append(!fresh).write(true).truncate(fresh).open(p) {
            use std::os::fd::AsRawFd;
            unsafe {
                dup2(f.as_raw_fd(), 2);
            }
            // Descriptor 2 holds the file now; this handle may go.
        }
    }
    eprintln!("---- lantern-ink started, pid {} ----", std::process::id());
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = panic_text(&info.to_string(), &std::backtrace::Backtrace::force_capture().to_string());
        let _ = std::io::stderr().write_all(text.as_bytes());
        // Stderr is the terminal: the log gets it too.
        if terminal
            && let Some(p) = &path
            && let Some(dir) = p.parent()
            && std::fs::create_dir_all(dir).is_ok()
            && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p)
        {
            let _ = f.write_all(text.as_bytes());
        }
        default(info);
    }));
}

/// What a panic leaves in the log: a line to search for, then where.
fn panic_text(info: &str, backtrace: &str) -> String {
    format!("PANIC: {info}\n{backtrace}\n")
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_panic_is_a_line_to_search_for() {
        let text = super::panic_text("panicked at src/x.rs:1:1:\nboom", "0: main");
        assert!(text.starts_with("PANIC: panicked at src/x.rs:1:1:") && text.ends_with("0: main\n"));
    }
}
