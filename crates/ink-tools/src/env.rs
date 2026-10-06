//! Where files go and come from.

use std::path::{Path, PathBuf};

use ink_core::DocId;

#[derive(Clone, Debug)]
pub struct Env {
    /// Previews are written here too, for the model to open at their
    /// own size.
    pub previews: PathBuf,
    /// Relative paths resolve here: Claude Code's project directory.
    pub base: Option<PathBuf>,
    /// Tells this process's files from another's (its pid).
    pub tag: String,
}

impl Env {
    pub fn new(previews: PathBuf, base: Option<PathBuf>, tag: impl Into<String>) -> Env {
        Env { previews, base, tag: tag.into() }
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

    /// Where `doc`'s latest preview goes.
    pub(crate) fn preview_path(&self, doc: DocId) -> PathBuf {
        self.previews.join(format!("{}-{doc}.png", self.tag))
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
        assert_eq!(env.preview_path(DocId(3)), PathBuf::from("/cache/previews/77-d3.png"));
    }
}
