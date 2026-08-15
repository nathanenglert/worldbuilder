//! Replacing a file in one step, so a crash cannot leave half of one.
//!
//! Worth the ceremony for two reasons beyond the obvious. The MCP server re-reads this
//! tree on every call while the app is running, so a partially written record is a file
//! another process will genuinely try to parse. And the writer's own editor may have the
//! same file open; giving it a torn read is a good way to have it saved back torn.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Write `text` to `path` via a sibling temporary file and a rename.
///
/// The temporary lives in the same directory so the rename stays within one filesystem —
/// across a mount boundary it would degrade to a copy, which is exactly the torn write
/// this exists to avoid. It is named with a leading dot so the loader's dotfile skip
/// ignores any that a crash strands.
pub fn write(path: &Path, text: &str) -> Result<()> {
    let io = |source| Error::Io { path: path.to_path_buf(), source };

    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(io)?;

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("record");
    let temp: PathBuf = parent.join(format!(".{name}.wb-tmp"));

    let result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        // Without this the rename can land before the contents do, which turns a crash
        // into an empty file rather than the old one.
        file.sync_all()?;
        drop(file);

        // A fresh file is 0644 masked by the umask, so a record the writer deliberately
        // made private would quietly become readable by everyone on the next save.
        if let Ok(meta) = std::fs::metadata(path) {
            std::fs::set_permissions(&temp, meta.permissions())?;
        }

        std::fs::rename(&temp, path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wb-atomic-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_write_replaces_the_file_and_leaves_no_litter() {
        let dir = scratch("replace");
        let path = dir.join("marrow.md");
        write(&path, "first\n").expect("writes");
        write(&path, "second\n").expect("writes");

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
        let strays: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains("wb-tmp"))
            .collect();
        assert!(strays.is_empty(), "left a temporary behind: {strays:?}");
    }

    #[test]
    fn a_write_creates_the_folder_it_needs() {
        let dir = scratch("mkdir");
        let path = dir.join("entities").join("places").join("new.md");
        write(&path, "body\n").expect("writes");
        assert!(path.exists());
    }

    /// A record the writer chose to keep to themselves stays that way.
    #[cfg(unix)]
    #[test]
    fn the_original_file_mode_is_kept() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("mode");
        let path = dir.join("private.md");
        write(&path, "one\n").expect("writes");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");

        write(&path, "two\n").expect("writes");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the file became more readable than it was");
    }
}
