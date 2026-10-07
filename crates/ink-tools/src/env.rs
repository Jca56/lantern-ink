//! Where files go and come from.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use ink_core::DocId;

/// How many of a run's previews stay on disk: its newest.
const KEPT_PREVIEWS: usize = 32;

/// The previews a run has written.
#[derive(Debug, Default)]
struct Shots {
    count: u64,
    /// The ones still on disk, the oldest first.
    kept: VecDeque<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Env {
    /// Previews are written here too, for the model to open at their
    /// own size.
    pub previews: PathBuf,
    /// Relative paths resolve here: Claude Code's project directory.
    pub base: Option<PathBuf>,
    /// Tells this process's files from another's (its pid).
    pub tag: String,
    shots: Arc<Mutex<Shots>>,
}

impl Env {
    pub fn new(previews: PathBuf, base: Option<PathBuf>, tag: impl Into<String>) -> Env {
        Env { previews, base, tag: tag.into(), shots: Arc::default() }
    }

    /// `~/.lantern/cache/lantern-ink/previews/`, `CLAUDE_PROJECT_DIR` (or
    /// the working directory), and the process id.
    pub fn from_process() -> Env {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let base = std::env::var_os("CLAUDE_PROJECT_DIR").map(PathBuf::from).or_else(|| std::env::current_dir().ok());
        Env::new(home.join(".lantern/cache/lantern-ink/previews"), base, std::process::id().to_string())
    }

    /// A path as the model wrote it: absolute, `~/…`, or relative.
    pub fn resolve(&self, path: &str) -> PathBuf {
        if let Some(rest) = path.strip_prefix("~/")
            && let Some(home) = std::env::var_os("HOME")
        {
            return Path::new(&home).join(rest);
        }
        match &self.base {
            Some(base) if Path::new(path).is_relative() => base.join(path),
            _ => PathBuf::from(path),
        }
    }

    /// Where `doc`'s next preview goes. Each has a file of its own,
    /// numbered through the run, so a second look doesn't replace the
    /// first; past the newest [`KEPT_PREVIEWS`], the oldest is removed.
    pub(crate) fn next_preview(&self, doc: DocId) -> PathBuf {
        let mut shots = self.shots.lock().unwrap_or_else(PoisonError::into_inner);
        shots.count += 1;
        let path = self.previews.join(format!("{}-{doc}-{}.png", self.tag, shots.count));
        shots.kept.push_back(path.clone());
        while shots.kept.len() > KEPT_PREVIEWS {
            if let Some(oldest) = shots.kept.pop_front() {
                let _ = std::fs::remove_file(oldest);
            }
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_resolve_as_the_model_writes_them() {
        let env = Env::new("/cache/previews".into(), Some("/work/project".into()), "77");
        assert_eq!(env.resolve("icons/a.svg"), PathBuf::from("/work/project/icons/a.svg"));
        assert_eq!(env.resolve("/abs/a.svg"), PathBuf::from("/abs/a.svg"));
        if let Some(home) = std::env::var_os("HOME") {
            assert_eq!(env.resolve("~/a.svg"), Path::new(&home).join("a.svg"));
        }
        assert_eq!(Env::new("/c".into(), None, "1").resolve("a.svg"), PathBuf::from("a.svg"));
    }

    #[test]
    fn each_preview_has_a_file_of_its_own() {
        let env = Env::new("/cache/previews".into(), None, "77");
        assert_eq!(env.next_preview(DocId(3)), PathBuf::from("/cache/previews/77-d3-1.png"));
        assert_eq!(env.next_preview(DocId(1)), PathBuf::from("/cache/previews/77-d1-2.png"));
        // A copy counts on from the same run.
        assert_eq!(env.clone().next_preview(DocId(3)), PathBuf::from("/cache/previews/77-d3-3.png"));
    }
}
