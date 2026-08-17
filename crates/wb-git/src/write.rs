//! The six operations that move the writer's files.
//!
//! Every one of them starts with [`Standing::open_writable`], which is the single gate
//! that keeps a world nested inside a larger repository read-only. Adding a seventh
//! operation without passing through it would take deliberate effort, which is the
//! point of having exactly one door.
//!
//! Four things are refused rather than attempted, and each refusal names the next move:
//! switching with unsaved changes, merging anything that is not a fast-forward,
//! committing with no configured author, and deleting the branch you are standing on.
//! A half-merged world folder is the worst state this app could hand back, and "stash"
//! is a word a novelist should never have to learn.

use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{IndexAddOption, Repository};

use crate::error::{Error, Result};
use crate::read::{Commit, status};
use crate::{Standing, is_derived};

/// What a fast-forward moved.
#[derive(Debug, Clone)]
pub struct Merge {
    pub from: String,
    pub into: String,
    /// Commits the target gained. Zero means it was already up to date, which is a
    /// success and not an error — the writer asked for a state, and it holds.
    pub commits: usize,
}

/// The head commit, or `None` in a repository nobody has committed to yet.
fn head_commit(repo: &Repository) -> Option<git2::Commit<'_>> {
    repo.head().ok().and_then(|head| head.peel_to_commit().ok())
}

/// Refuse when the world folder has changes that are not saved.
///
/// Called before switching and before merging. Both would otherwise silently decide what
/// happens to work the writer has not committed, and neither has any business doing that.
fn require_clean(standing: &Standing) -> Result<()> {
    let dirty = status(standing)?.dirty;
    if dirty.is_empty() {
        return Ok(());
    }
    Err(Error::Dirty { paths: dirty.into_iter().map(|c| c.path).take(8).collect() })
}

fn branch_name(repo: &Repository) -> Result<String> {
    if repo.head_detached().unwrap_or(false) {
        return Err(Error::Detached);
    }
    repo.head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_string))
        .ok_or(Error::Detached)
}

/// Make a save point out of everything in the world folder.
///
/// Ignored files stay ignored — `add_all` honours `.gitignore` — and `.worldbuilder/` is
/// skipped explicitly on top of that, because a writer's own world folder need not have
/// a `.gitignore` at all and the terrain cache has no business in anyone's history.
pub fn commit(standing: &Standing, message: &str) -> Result<Commit> {
    let repo = standing.open_writable()?;

    let message = message.trim();
    let message = if message.is_empty() { "Save point" } else { message };

    // Before touching the index, so "nothing changed" is not reported as a commit of
    // nothing. `status` already excludes the derived folder.
    if status(standing)?.dirty.is_empty() {
        return Err(Error::NothingToSave);
    }

    let signature = repo.signature().map_err(|_| Error::NoAuthor)?;

    let mut index = repo.index()?;
    let mut skip_derived = |path: &Path, _: &[u8]| i32::from(is_derived(path));
    index.add_all(["*"].iter(), IndexAddOption::DEFAULT, Some(&mut skip_derived))?;
    // `add_all` stages new and modified files; deletions need the second pass. Together
    // they are what `git add -A` means.
    index.update_all(["*"].iter(), Some(&mut skip_derived))?;
    index.write()?;

    let tree = repo.find_tree(index.write_tree()?)?;
    let parent = head_commit(&repo);
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();

    let oid = repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parents)?;
    Ok(Commit::of(&repo.find_commit(oid)?))
}

/// Start a what-if from where the world stands now.
pub fn create_branch(standing: &Standing, name: &str, switch_to: bool) -> Result<()> {
    let repo = standing.open_writable()?;

    let name = name.trim();
    if name.is_empty() || !git2::Reference::is_valid_name(&format!("refs/heads/{name}")) {
        return Err(Error::BadBranchName { name: name.to_string() });
    }
    if repo.find_branch(name, git2::BranchType::Local).is_ok() {
        return Err(Error::BranchExists { name: name.to_string() });
    }
    // Checked before the branch is created, so a refusal leaves nothing half-done.
    if switch_to {
        require_clean(standing)?;
    }

    let from = head_commit(&repo).ok_or(Error::Unborn)?;
    repo.branch(name, &from, false)?;

    if switch_to {
        switch(standing, name)?;
    }
    Ok(())
}

/// Move the world folder to another branch.
pub fn switch(standing: &Standing, name: &str) -> Result<()> {
    let repo = standing.open_writable()?;
    if repo.find_branch(name, git2::BranchType::Local).is_err() {
        return Err(Error::NoSuchBranch { name: name.to_string() });
    }
    require_clean(standing)?;

    let reference = format!("refs/heads/{name}");
    let object = repo.revparse_single(&reference)?;
    // `safe()` refuses to overwrite anything modified. `require_clean` has already made
    // that impossible, and the belt is cheap.
    repo.checkout_tree(&object, Some(CheckoutBuilder::new().safe()))?;
    repo.set_head(&reference)?;
    Ok(())
}

/// Fast-forward `target` to the branch the writer is on, or refuse.
///
/// The working tree is untouched: this moves another branch's ref up to where this one
/// already is. The writer stays where they are and can switch afterwards, which keeps
/// the operation trivially reversible — the old ref position is in the reflog and
/// nothing on disk moved.
pub fn merge_into(standing: &Standing, target: &str) -> Result<Merge> {
    let repo = standing.open_writable()?;
    let from = branch_name(&repo)?;
    if from == target {
        return Err(Error::CurrentBranch { name: target.to_string() });
    }
    require_clean(standing)?;

    let source_oid = head_commit(&repo).ok_or(Error::Unborn)?.id();
    let target_branch = repo
        .find_branch(target, git2::BranchType::Local)
        .map_err(|_| Error::NoSuchBranch { name: target.to_string() })?;
    let target_oid = target_branch.get().target().ok_or(Error::Unborn)?;

    if source_oid == target_oid {
        return Ok(Merge { from, into: target.to_string(), commits: 0 });
    }

    let (ahead, behind) = repo.graph_ahead_behind(source_oid, target_oid)?;
    if behind > 0 {
        return Err(Error::NotFastForward { from, into: target.to_string(), behind });
    }

    repo.reference(
        &format!("refs/heads/{target}"),
        source_oid,
        true,
        &format!("worldbuilder: fast-forward {target} to {from}"),
    )?;
    Ok(Merge { from, into: target.to_string(), commits: ahead })
}

/// Throw a what-if away.
///
/// Deliberately a force delete: abandoning an unmerged experiment is the *normal* end of
/// a what-if, not an accident to be guarded against here. The guard belongs one level up,
/// where `branches()` already reports how many commits stop being reachable, and the
/// panel states that number before the second click.
pub fn delete_branch(standing: &Standing, name: &str) -> Result<()> {
    let repo = standing.open_writable()?;
    let mut branch = repo
        .find_branch(name, git2::BranchType::Local)
        .map_err(|_| Error::NoSuchBranch { name: name.to_string() })?;
    if branch.is_head() {
        return Err(Error::CurrentBranch { name: name.to_string() });
    }
    branch.delete()?;
    Ok(())
}

/// Put the world folder back the way the last save point left it.
///
/// Returns what it threw away. The caller shows that list *before* calling — this return
/// value is the receipt, not the warning.
pub fn discard(standing: &Standing) -> Result<Vec<PathBuf>> {
    let repo = standing.open_writable()?;
    if repo.head().is_err() {
        return Err(Error::Unborn);
    }

    let discarded: Vec<PathBuf> =
        status(standing)?.dirty.into_iter().map(|c| PathBuf::from(c.path)).collect();
    if discarded.is_empty() {
        return Ok(discarded);
    }

    let mut checkout = CheckoutBuilder::new();
    checkout.force().remove_untracked(true);
    repo.checkout_head(Some(&mut checkout))?;
    Ok(discarded)
}
