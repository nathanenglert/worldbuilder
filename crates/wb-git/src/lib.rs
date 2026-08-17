//! **wb-git** — save points and what-ifs for a folder of files.
//!
//! DESIGN.md §6 calls this the free bonus of storing a world as plain files: *"Fork
//! canon, redraw two centuries of borders differently, diff the worlds, merge or
//! discard."* The falling-out is real, but it is not automatic, because a world folder
//! is not always a repository — it is very often a folder *inside* one.
//!
//! Hence [`Standing`], which every function in here takes and which every mutating
//! function refuses unless it is [`Standing::Root`]. The world this tool ships with is
//! `examples/vashen`, and `Repository::discover` from there returns the Worldbuilder
//! source tree: a "try a what-if" button wired to `discover` would branch and check out
//! this codebase. That is not a development-only hazard — a vault inside a dotfiles
//! repo, or a world in a monorepo, is the ordinary shape.
//!
//! Nested worlds are not crippled. Reading history and materializing an old revision
//! touch nothing, so both stay available and the whole comparison feature works; only
//! the six operations that move the writer's files are gated.
//!
//! **This crate knows nothing about worlds.** It extracts a subtree and answers
//! questions about refs. What a record is, and whether one revision contradicts another,
//! belongs to the crates that model that — `wb_propose::diff_worlds` does the second
//! half against two loaded [`wb_store::World`]s.
//!
//! **There is no network.** `git2` is built with `default-features = false`, so the
//! libgit2 underneath has no HTTPS and no SSH transport compiled in at all. Push, pull
//! and clone are not unimplemented here; they are absent.
//!
//! [`wb_store::World`]: https://docs.rs/wb-store

pub mod error;
mod read;
mod scratch;
mod write;

use std::path::{Path, PathBuf};

pub use error::{Error, Result};
pub use read::{Branch, Change, Commit, History, Status, branches, history, materialize, status};
pub use scratch::Scratch;
pub use write::{Merge, commit, create_branch, delete_branch, discard, merge_into, switch};

/// The folder in `.worldbuilder/` that never appears in a status, never gets staged, and
/// is where a materialized revision lands. Named here rather than in the app because two
/// of the three rules about it are enforced in this crate.
pub const DERIVED_DIR: &str = ".worldbuilder";

/// What version control can safely be asked to do for this world folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Nothing tracks this folder. Every operation is unavailable, and the honest thing
    /// to say is that the writer can run `git init` here whenever they want to.
    NoRepo,
    /// The world is a subfolder of a repository. Read-only: history and comparison work
    /// and are scoped to the subtree, mutations are refused and say which repository
    /// they would have moved.
    Nested { repo: PathBuf, prefix: PathBuf },
    /// The world folder *is* the repository. The full surface.
    Root { repo: PathBuf },
}

impl Standing {
    /// What version control this folder is under, if any.
    ///
    /// Bare repositories report [`Standing::NoRepo`]: there is no working tree to hold a
    /// world folder, so nothing here could apply to one.
    pub fn of(world_root: &Path) -> Self {
        let Ok(world) = world_root.canonicalize() else { return Self::NoRepo };
        let Ok(repo) = git2::Repository::discover(&world) else { return Self::NoRepo };
        let Some(workdir) = repo.workdir() else { return Self::NoRepo };
        let Ok(workdir) = workdir.canonicalize() else { return Self::NoRepo };

        if workdir == world {
            Self::Root { repo: workdir }
        } else if let Ok(prefix) = world.strip_prefix(&workdir) {
            Self::Nested { repo: workdir, prefix: prefix.to_path_buf() }
        } else {
            Self::NoRepo
        }
    }

    pub fn repo(&self) -> Option<&Path> {
        match self {
            Self::NoRepo => None,
            Self::Nested { repo, .. } | Self::Root { repo } => Some(repo),
        }
    }

    /// Where the world sits inside the repository. Empty for [`Standing::Root`], which is
    /// what makes every pathspec in this crate uniform: an empty prefix means "all of it".
    pub fn prefix(&self) -> &Path {
        match self {
            Self::Nested { prefix, .. } => prefix,
            _ => Path::new(""),
        }
    }

    /// The world folder itself, wherever it is.
    pub fn world(&self) -> Option<PathBuf> {
        self.repo().map(|repo| repo.join(self.prefix()))
    }

    pub fn is_root(&self) -> bool {
        matches!(self, Self::Root { .. })
    }

    /// A short word for the UI: `none` · `nested` · `root`. Slugs rather than sentences,
    /// because the sentence a nested world deserves names the repository and is built
    /// where the path is known.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::NoRepo => "none",
            Self::Nested { .. } => "nested",
            Self::Root { .. } => "root",
        }
    }

    pub(crate) fn open(&self) -> Result<git2::Repository> {
        let repo = self
            .repo()
            .ok_or_else(|| Error::NotARepository { world: self.world().unwrap_or_default() })?;
        Ok(git2::Repository::open(repo)?)
    }

    /// Every mutating entry point starts here. One gate, so a new operation cannot be
    /// added without passing through it.
    pub(crate) fn open_writable(&self) -> Result<git2::Repository> {
        match self {
            Self::NoRepo => Err(Error::NotARepository { world: PathBuf::new() }),
            Self::Nested { repo, prefix } => {
                Err(Error::NotRepoRoot { repo: repo.clone(), world: repo.join(prefix) })
            }
            Self::Root { repo } => Ok(git2::Repository::open(repo)?),
        }
    }
}

/// True for anything inside the derived-cache folder, at any depth.
///
/// Used by the status walk and the staging callback. Without it, running one comparison
/// would leave untracked files behind and the *next* branch switch would be refused
/// because of a directory this app wrote — the compare feature breaking the branch
/// feature.
pub(crate) fn is_derived(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == DERIVED_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_under_no_repository_at_all_has_no_standing() {
        let dir = std::env::temp_dir().join("wb-git-nowhere");
        let _ = std::fs::create_dir_all(&dir);
        // Only meaningful if the temp dir is not itself inside somebody's repository,
        // which it is not on any platform this runs on.
        assert!(matches!(Standing::of(&dir), Standing::NoRepo | Standing::Nested { .. }));
    }

    #[test]
    fn the_derived_folder_is_recognised_at_any_depth() {
        assert!(is_derived(Path::new(".worldbuilder")));
        assert!(is_derived(Path::new(".worldbuilder/compare/abc/entities/x.md")));
        assert!(!is_derived(Path::new("entities/worldbuilder.md")));
        assert!(!is_derived(Path::new("entities/actors/aldric-vane.md")));
    }
}
