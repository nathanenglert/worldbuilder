//! A directory that removes itself.
//!
//! Reading an old revision means writing it somewhere, and the somewhere is inside the
//! world folder under `.worldbuilder/` — not the OS temp directory. Three reasons, and
//! the third is the one that matters:
//!
//! - it is where derived build products already live, beside the terrain cache;
//! - it is gitignored, so it does not appear as a change to the world;
//! - `freshness::walk` skips any entry whose name starts with `.`, so writing it cannot
//!   change the world's fingerprint. Materialize anywhere else inside the folder and
//!   every comparison would reload the world underneath itself.
//!
//! It still has to go away afterwards, because a writer's own `.gitignore` need not
//! mention `.worldbuilder/` — and if it does not, one comparison would leave untracked
//! files behind and the next branch switch would be refused on account of them.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// A directory that exists for as long as this value does.
///
/// Removal happens in `Drop`, so it also happens on the error paths — a revision that
/// fails to load halfway through leaves nothing behind.
#[derive(Debug)]
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Create `path`, replacing anything already there. A leftover from a previous run
    /// that was killed mid-comparison is a stale half-tree, and reusing it would be
    /// worse than the cost of rewriting.
    pub fn at(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .map_err(|source| Error::Io { path: path.clone(), source })?;
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
        // And the holding folder, if this was the last thing in it. `remove_dir` refuses
        // a directory with anything in it, which is exactly the check wanted here: a
        // second comparison running concurrently keeps its own.
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directory_goes_away_when_the_guard_does() {
        let base = std::env::temp_dir().join("wb-git-scratch-test");
        let path = base.join("one");
        {
            let scratch = Scratch::at(&path).expect("created");
            std::fs::write(scratch.path().join("file"), "x").expect("wrote");
            assert!(path.is_dir());
        }
        assert!(!path.exists(), "and it takes its contents with it");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_stale_directory_from_a_killed_run_is_replaced_not_reused() {
        let base = std::env::temp_dir().join("wb-git-scratch-stale");
        let path = base.join("two");
        std::fs::create_dir_all(&path).expect("stale dir");
        std::fs::write(path.join("half-written.yaml"), "id: broken").expect("stale file");

        let scratch = Scratch::at(&path).expect("created");
        assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
        drop(scratch);
        let _ = std::fs::remove_dir_all(&base);
    }
}
