//! Files in and out. A save is atomic: the new file is written whole
//! beside the old one and renamed over it, so a crash never leaves half
//! a drawing.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// The biggest file read as a drawing.
pub const MAX_FILE_BYTES: u64 = 256 << 20;

fn failed(path: &Path, why: impl ToString) -> CoreError {
    CoreError::File { path: path.to_owned(), why: why.to_string() }
}

/// The one name the file at `path` has, however the path is spelled:
/// links followed, `.` and `..` taken out. A file that isn't there yet
/// is named through its folder.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    match (fs::canonicalize(dir), path.file_name()) {
        (Ok(dir), Some(name)) => dir.join(name),
        _ => path.to_owned(),
    }
}

/// The text of the file at `path`.
pub fn read(path: &Path) -> Result<String, CoreError> {
    let size = fs::metadata(path).map_err(|e| failed(path, e))?.len();
    if size > MAX_FILE_BYTES {
        return Err(failed(path, format!("it's {} MB, and a drawing is at most {} MB", size >> 20, MAX_FILE_BYTES >> 20)));
    }
    let bytes = fs::read(path).map_err(|e| failed(path, e))?;
    String::from_utf8(bytes).map_err(|_| failed(path, "it isn't UTF-8 text (Ink reads no other encoding)"))
}

/// Write `bytes` as the file at `path`, all or nothing.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let name = path.file_name().ok_or_else(|| failed(path, "that isn't a file's name"))?;
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".ink-{}", std::process::id()));
    let temp: PathBuf = dir.join(temp_name);
    let written = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        // The rename itself, onto the disk.
        File::open(dir).and_then(|d| d.sync_all())
    })();
    written.map_err(|e| {
        let _ = fs::remove_file(&temp);
        failed(path, e)
    })
}
