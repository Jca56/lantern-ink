//! File pickers: `lntrn-file-manager` in its picking mode, out of
//! process (LS3's `picker.rs`, its D15: the same browser as the
//! desktop). The picker runs to completion on a thread of its own and
//! prints the chosen path, so the window keeps drawing however long it
//! stays open.

use std::path::PathBuf;
use std::process::{Command, Stdio};

/// What Open and Save As offer.
pub const FILTERS: &str = "SVG:*.svg";

pub enum Ask {
    Open,
    /// Save, suggesting `name`.
    Save { name: String },
}

/// The picker, found in Lantern's `bin` or on the path.
fn program() -> PathBuf {
    lntrn_sys::dirs::lantern().map(|l| l.join("bin/lntrn-file-manager")).filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from("lntrn-file-manager"))
}

/// Run a picker and wait for the path it prints; `None` when it was
/// cancelled or couldn't run. Blocks: only on a picker's own thread.
pub fn pick(ask: &Ask) -> Option<PathBuf> {
    let mut cmd = Command::new(program());
    match ask {
        Ask::Open => cmd.args(["--pick", "--filters", FILTERS]),
        Ask::Save { name } => cmd.args(["--pick-save", "--filters", FILTERS, "--save-name", name]),
    };
    let output = match cmd.stdout(Stdio::piped()).stderr(Stdio::inherit()).output() {
        Ok(o) => o,
        Err(e) => {
            lntrn_core::log_error!("no file picker: {e}");
            return None;
        }
    };
    let text = String::from_utf8(output.stdout).ok()?;
    let path = text.trim();
    (output.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
}

/// A path Save As chose, with `.svg` on the end.
pub fn with_extension(mut path: PathBuf) -> PathBuf {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        let mut name = path.file_name().unwrap_or_default().to_owned();
        name.push(".svg");
        path.set_file_name(name);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_end_in_svg() {
        assert_eq!(with_extension(PathBuf::from("/a/b/icon")), PathBuf::from("/a/b/icon.svg"));
        assert_eq!(with_extension(PathBuf::from("/a/b/icon.SVG")), PathBuf::from("/a/b/icon.SVG"));
        assert_eq!(with_extension(PathBuf::from("/a/b/icon.png")), PathBuf::from("/a/b/icon.png.svg"));
    }
}
