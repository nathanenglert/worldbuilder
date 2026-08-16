//! Confining a writer-supplied path to a folder.
//!
//! Two places take a path from outside this process and open it: an agent asking for a
//! note by name, and a scene pointing at a chapter of the manuscript. Both are handed a
//! *folder* — the writer's notes, the writer's manuscript — and neither is handed the
//! disk. This is the one piece of code that makes that distinction hold.
//!
//! **Checked after canonicalization, never by inspecting the string.** Rejecting `..`
//! textually is the obvious implementation and it is wrong twice over: it misses the
//! symlink case entirely, and it refuses paths a writer may legitimately have arranged
//! that way. Resolving both sides to real paths first and asking whether one contains
//! the other answers the question that was actually being asked.
//!
//! The refusal is returned as a [`Denied`] rather than a string, because the two callers
//! phrase it differently and should — "use `list_notes` to see what is there" is useless
//! advice about a manuscript.

use std::path::{Path, PathBuf};

/// Why a path was refused. Callers turn these into their own wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// An absolute path. Everything here is addressed relative to its folder.
    Absolute,
    /// The folder itself is not there — an unopened drawer, not a bad request.
    NoBase,
    /// Nothing at that path.
    Missing,
    /// It resolves, but not inside the folder. The interesting one.
    Outside,
    /// A directory where a file was wanted.
    NotAFile,
}

/// Resolve `requested` inside `base`, or say why not.
///
/// `base` is canonicalized too, so a base that is itself reached through a symlink still
/// compares equal to the paths found under it — otherwise every lookup in a symlinked
/// world folder would be refused as `Outside`, which is both wrong and baffling.
pub fn resolve(base: &Path, requested: &str) -> Result<PathBuf, Denied> {
    let relative = Path::new(requested.trim());
    if relative.is_absolute() {
        return Err(Denied::Absolute);
    }

    let root = base.canonicalize().map_err(|_| Denied::NoBase)?;
    let real = root.join(relative).canonicalize().map_err(|_| Denied::Missing)?;

    if !real.starts_with(&root) {
        return Err(Denied::Outside);
    }
    if !real.is_file() {
        return Err(Denied::NotAFile);
    }
    Ok(real)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world folder built under the OS temp directory, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("wb-sandbox-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("inside")).unwrap();
            std::fs::write(dir.join("inside/chapter.md"), "# One\n").unwrap();
            std::fs::write(dir.join("secret.txt"), "not yours\n").unwrap();
            Self(dir)
        }

        fn base(&self) -> PathBuf {
            self.0.join("inside")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_file_in_the_folder_resolves() {
        let scratch = Scratch::new("plain");
        let found = resolve(&scratch.base(), "chapter.md").unwrap();
        assert!(found.ends_with("chapter.md"));
    }

    #[test]
    fn climbing_out_of_the_folder_is_refused() {
        let scratch = Scratch::new("climb");
        assert_eq!(resolve(&scratch.base(), "../secret.txt"), Err(Denied::Outside));
    }

    #[test]
    fn an_absolute_path_is_refused_before_anything_is_opened() {
        let scratch = Scratch::new("abs");
        assert_eq!(resolve(&scratch.base(), "/etc/passwd"), Err(Denied::Absolute));
    }

    /// The case that makes textual `..` rejection insufficient, and the reason this
    /// function canonicalizes: the string is innocent and the path is not.
    #[test]
    #[cfg(unix)]
    fn a_symlink_pointing_out_of_the_folder_is_refused() {
        let scratch = Scratch::new("symlink");
        let link = scratch.base().join("innocent.md");
        std::os::unix::fs::symlink(scratch.0.join("secret.txt"), &link).unwrap();

        assert_eq!(resolve(&scratch.base(), "innocent.md"), Err(Denied::Outside));
    }

    #[test]
    fn a_missing_folder_is_told_apart_from_a_missing_file() {
        let scratch = Scratch::new("absent");
        assert_eq!(resolve(&scratch.0.join("nowhere"), "chapter.md"), Err(Denied::NoBase));
        assert_eq!(resolve(&scratch.base(), "nothing.md"), Err(Denied::Missing));
    }

    #[test]
    fn a_folder_is_not_a_file() {
        let scratch = Scratch::new("dir");
        std::fs::create_dir_all(scratch.base().join("part-two")).unwrap();
        assert_eq!(resolve(&scratch.base(), "part-two"), Err(Denied::NotAFile));
    }
}
