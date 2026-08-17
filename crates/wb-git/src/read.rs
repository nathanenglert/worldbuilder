//! Everything that touches nothing.
//!
//! All four functions here work under [`Standing::Nested`] as well as
//! [`Standing::Root`], which is the whole reason a world inside a bigger repository is
//! still worth opening the version panel for: it can see its own history and can be
//! compared against any point in it.

use std::path::{Path, PathBuf};

use git2::{DiffOptions, Repository, Sort, StatusOptions, TreeWalkMode, TreeWalkResult};

use crate::error::{Error, Result};
use crate::{Standing, is_derived};

/// How many commits `history` will look at before giving up. A world folder in a busy
/// monorepo can be a handful of commits inside a hundred thousand, and walking all of
/// them to find them is not worth a panel refresh.
const MAX_SCAN: usize = 500;

/// Materializing more than this is refused. A world is text and one map image; a folder
/// of raw scans is somebody else's problem and should fail with a sentence rather than
/// by filling the disk.
const MAX_MATERIALIZE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Abbreviated, for display. Seven characters, as git prints it.
    pub id: String,
    pub full: String,
    pub summary: String,
    pub author: String,
    /// Unix seconds. Formatting belongs to whoever is showing it — this crate has no
    /// opinion about the writer's locale, and the *world's* calendar is emphatically not
    /// the one to use for a commit timestamp.
    pub when: i64,
}

impl Commit {
    pub(crate) fn of(commit: &git2::Commit<'_>) -> Self {
        let full = commit.id().to_string();
        Self {
            id: full.chars().take(7).collect(),
            full,
            summary: commit.summary().ok().flatten().unwrap_or("(no message)").to_string(),
            author: commit.author().name().unwrap_or("unknown").to_string(),
            when: commit.time().seconds(),
        }
    }
}

/// One uncommitted difference. `path` is relative to the repository root, so a nested
/// world's changes read as `examples/vashen/entities/…` — which is where they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    /// `new` · `modified` · `deleted`. Renames are reported as their two halves rather
    /// than guessed at, because a renamed record in this app is a record whose *name*
    /// changed, and the writer already knows.
    pub state: &'static str,
}

#[derive(Debug, Clone)]
pub struct Status {
    /// `None` when HEAD is detached — an ordinary state to be in after inspecting an old
    /// revision, and one the panel should name rather than hide.
    pub branch: Option<String>,
    /// The branch a what-if is measured against. Displayed, never assumed silently.
    pub canon: Option<String>,
    pub head: Option<Commit>,
    /// Empty means a clean world folder. Anything under `.worldbuilder/` is excluded —
    /// it is a build product, and this app writes it.
    pub dirty: Vec<Change>,
    /// A repository with no commits yet. Not an error: it is what `git init` leaves.
    pub unborn: bool,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub is_head: bool,
    /// Commits this branch has that canon does not — what would be lost by deleting it.
    pub ahead: usize,
    /// Commits canon has that this branch does not. Non-zero means merging is not a
    /// fast-forward, which this crate refuses.
    pub behind: usize,
    pub tip: Option<Commit>,
}

#[derive(Debug, Clone)]
pub struct History {
    pub commits: Vec<Commit>,
    /// How many commits were examined to find them.
    pub scanned: usize,
    /// True when the walk hit [`MAX_SCAN`] and there may be older commits it never saw.
    /// Reported rather than swallowed: a list that silently stops is a list that lies.
    pub truncated: bool,
}

/// The branch a what-if is compared against: what the remote calls its default, else
/// `main`, else `master`, else wherever HEAD is now.
fn canon_of(repo: &Repository) -> Option<String> {
    if let Ok(reference) = repo.find_reference("refs/remotes/origin/HEAD")
        && let Ok(Some(target)) = reference.symbolic_target()
        && let Some(name) = target.strip_prefix("refs/remotes/origin/")
    {
        return Some(name.to_string());
    }
    for name in ["main", "master"] {
        if repo.find_branch(name, git2::BranchType::Local).is_ok() {
            return Some(name.to_string());
        }
    }
    repo.head().ok().and_then(|h| h.shorthand().ok().map(str::to_string))
}

fn pathspec(standing: &Standing) -> Option<String> {
    let prefix = standing.prefix();
    (!prefix.as_os_str().is_empty()).then(|| prefix.to_string_lossy().to_string())
}

/// Where the world stands: which branch, against which canon, and what is unsaved.
pub fn status(standing: &Standing) -> Result<Status> {
    let repo = standing.open()?;

    let head = repo.head().ok();
    let unborn = repo.head().is_err() && repo.is_empty().unwrap_or(false);
    let branch = if repo.head_detached().unwrap_or(false) {
        None
    } else {
        head.as_ref().and_then(|h| h.shorthand().ok()).map(str::to_string)
    };
    let head_commit = head.as_ref().and_then(|h| h.peel_to_commit().ok()).map(|c| Commit::of(&c));

    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true).include_ignored(false);
    if let Some(spec) = pathspec(standing) {
        options.pathspec(spec);
    }

    let mut dirty = Vec::new();
    for entry in repo.statuses(Some(&mut options))?.iter() {
        let Ok(path) = entry.path() else { continue };
        if is_derived(Path::new(path)) {
            continue;
        }
        let flags = entry.status();
        let state = if flags.is_wt_new() || flags.is_index_new() {
            "new"
        } else if flags.is_wt_deleted() || flags.is_index_deleted() {
            "deleted"
        } else {
            "modified"
        };
        dirty.push(Change { path: path.to_string(), state });
    }
    dirty.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(Status { branch, canon: canon_of(&repo), head: head_commit, dirty, unborn })
}

/// Commits that touched this world, newest first.
///
/// For a nested world that is genuinely "commits touching this subtree" — each candidate
/// is diffed against its parent through a pathspec — so a world folder in a busy
/// repository shows its own history and not everybody else's.
pub fn history(standing: &Standing, limit: usize) -> Result<History> {
    let repo = standing.open()?;
    if repo.head().is_err() {
        return Ok(History { commits: Vec::new(), scanned: 0, truncated: false });
    }

    let spec = pathspec(standing);
    let mut walk = repo.revwalk()?;
    walk.set_sorting(Sort::TIME)?;
    walk.push_head()?;

    let mut commits = Vec::new();
    let mut scanned = 0;
    let mut truncated = false;

    for oid in walk {
        if commits.len() >= limit {
            break;
        }
        if scanned >= MAX_SCAN {
            truncated = true;
            break;
        }
        scanned += 1;

        let commit = repo.find_commit(oid?)?;
        if let Some(spec) = &spec {
            let tree = commit.tree()?;
            let parent = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
            let mut options = DiffOptions::new();
            options.pathspec(spec);
            let diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&tree), Some(&mut options))?;
            if diff.deltas().len() == 0 {
                continue;
            }
        }
        commits.push(Commit::of(&commit));
    }

    Ok(History { commits, scanned, truncated })
}

/// Local branches, each measured against canon.
///
/// `ahead` is the number the panel shows before offering to delete one — it is exactly
/// how many commits stop being reachable.
pub fn branches(standing: &Standing) -> Result<Vec<Branch>> {
    let repo = standing.open()?;
    let canon = canon_of(&repo);
    let canon_oid = canon.as_ref().and_then(|name| {
        repo.find_branch(name, git2::BranchType::Local).ok().and_then(|b| b.get().target())
    });

    let mut out = Vec::new();
    for found in repo.branches(Some(git2::BranchType::Local))? {
        let (branch, _) = found?;
        let Some(name) = branch.name()?.map(str::to_string) else { continue };
        let target = branch.get().target();

        let (ahead, behind) = match (target, canon_oid) {
            (Some(a), Some(b)) if a != b => repo.graph_ahead_behind(a, b).unwrap_or((0, 0)),
            _ => (0, 0),
        };

        out.push(Branch {
            is_head: branch.is_head(),
            tip: target.and_then(|oid| repo.find_commit(oid).ok()).map(|c| Commit::of(&c)),
            name,
            ahead,
            behind,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Write the world folder as it stood at `rev` into `into`, and return that path.
///
/// Only the world's own subtree comes out, so a nested world materializes as a world
/// folder and not as somebody's monorepo. Symlinks and submodule entries are skipped:
/// a symlink restored into a scratch directory points somewhere that has nothing to do
/// with where it pointed, which is a worse answer than its absence.
pub fn materialize(standing: &Standing, rev: &str, into: &Path) -> Result<PathBuf> {
    let repo = standing.open()?;
    let commit = repo.revparse_single(rev)?.peel_to_commit()?;
    let mut tree = commit.tree()?;

    let prefix = standing.prefix();
    if !prefix.as_os_str().is_empty() {
        tree = tree.get_path(prefix)?.to_object(&repo)?.peel_to_tree()?;
    }

    // Collected first, written second: the walk callback cannot return a `Result`, and
    // an error swallowed inside it would leave a half-written tree that loads.
    let mut blobs: Vec<(PathBuf, git2::Oid)> = Vec::new();
    tree.walk(TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob)
            && matches!(entry.filemode(), 0o100644 | 0o100755)
            && let Ok(name) = entry.name()
        {
            blobs.push((PathBuf::from(dir).join(name), entry.id()));
        }
        TreeWalkResult::Ok
    })?;

    let total: u64 = blobs
        .iter()
        .filter_map(|(_, oid)| repo.find_blob(*oid).ok())
        .map(|blob| blob.size() as u64)
        .sum();
    if total > MAX_MATERIALIZE {
        return Err(Error::TooLarge { bytes: total, cap: MAX_MATERIALIZE });
    }

    for (relative, oid) in blobs {
        let path = into.join(&relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| Error::Io { path: parent.to_path_buf(), source })?;
        }
        let blob = repo.find_blob(oid)?;
        std::fs::write(&path, blob.content())
            .map_err(|source| Error::Io { path: path.clone(), source })?;
    }

    Ok(into.to_path_buf())
}
