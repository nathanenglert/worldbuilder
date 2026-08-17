//! Save points, what-ifs, and reading one revision against another.
//!
//! `wb-git` does the git; `wb_propose::diff_worlds` does the comparison. What is left
//! here is the join, and the join has one job worth naming: **the other side of a
//! comparison has to be a world, not a directory of files.**
//!
//! Getting that wrong is invisible. Materialize a revision into a scratch directory, load
//! it, and `manuscript.root` — `../manuscript`, relative to the world folder — now points
//! at nothing. `wb_story::check` finds no prose on that side, the two open questions about
//! Aldric appear only on the working side, and the panel reports the branch as *settling
//! two contradictions* it has not touched. A comparison that is confidently wrong is worse
//! than none, because a writer keeps or discards a week of work on the strength of it.
//!
//! So [`materialized`] overrides the manuscript root in memory, to the *live* one. That is
//! not a patch over a bug: the book deliberately lives outside the repository (§8) and is
//! not versioned with the world, so comparing two revisions against today's manuscript is
//! the only reading the comparison can honestly have.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use wb_git::{Scratch, Standing};
use wb_store::{ManuscriptSpec, World};

use crate::commands::{AppState, DiffLine, FindingDto, WorldSummary, changed_lines};

/// How many per-file diffs travel with a comparison. The record-level summary above them
/// is complete; this is the reading material, and forty file diffs is not reading
/// material. The count that was dropped is reported rather than swallowed.
const MAX_FILES: usize = 12;

// ------------------------------------------------------------------ payloads

#[derive(Serialize)]
pub struct StandingDto {
    /// `none` · `nested` · `root`.
    pub kind: &'static str,
    pub repo: Option<String>,
    pub world: String,
    /// Present only for `nested`: the one sentence explaining what is off and why.
    pub note: Option<String>,
}

#[derive(Serialize)]
pub struct CommitDto {
    pub id: String,
    pub full: String,
    pub summary: String,
    pub author: String,
    pub when: i64,
}

impl From<wb_git::Commit> for CommitDto {
    fn from(c: wb_git::Commit) -> Self {
        Self { id: c.id, full: c.full, summary: c.summary, author: c.author, when: c.when }
    }
}

#[derive(Serialize)]
pub struct ChangeDto {
    pub path: String,
    pub state: &'static str,
}

#[derive(Serialize)]
pub struct VersionDto {
    pub standing: StandingDto,
    pub branch: Option<String>,
    pub canon: Option<String>,
    pub head: Option<CommitDto>,
    pub dirty: Vec<ChangeDto>,
    pub unborn: bool,
}

#[derive(Serialize)]
pub struct BranchDto {
    pub name: String,
    pub is_head: bool,
    /// What deleting this branch would make unreachable. The panel states it before the
    /// second click.
    pub ahead: usize,
    /// Non-zero means merging is not a fast-forward, and will be refused.
    pub behind: usize,
    pub tip: Option<CommitDto>,
}

#[derive(Serialize)]
pub struct HistoryDto {
    pub commits: Vec<CommitDto>,
    pub scanned: usize,
    pub truncated: bool,
}

#[derive(Serialize)]
pub struct MovedDto {
    pub what: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub days: i64,
}

#[derive(Serialize)]
pub struct RecordDiffDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub fields: Vec<String>,
    pub moved: Vec<MovedDto>,
}

#[derive(Serialize)]
pub struct FileDiffDto {
    pub path: String,
    pub diff: Vec<DiffLine>,
}

#[derive(Serialize)]
pub struct CompareDto {
    /// What was compared against, as the writer named it.
    pub rev: String,
    pub label: String,
    pub added: Vec<RecordDiffDto>,
    pub removed: Vec<RecordDiffDto>,
    pub changed: Vec<RecordDiffDto>,
    /// Contradictions the working tree settles relative to that revision, and creates.
    pub resolved: Vec<FindingDto>,
    pub introduced: Vec<FindingDto>,
    pub breaks: bool,
    pub files: Vec<FileDiffDto>,
    /// Files with a diff that did not fit. Said, never silently dropped.
    pub more_files: usize,
}

// ------------------------------------------------------------------- helpers

fn standing_of(world: &World) -> Standing {
    Standing::of(&world.root)
}

fn describe(standing: &Standing, world: &World) -> StandingDto {
    StandingDto {
        kind: standing.slug(),
        repo: standing.repo().map(|p| p.display().to_string()),
        world: world.root.display().to_string(),
        note: match standing {
            Standing::Nested { repo, .. } => Some(format!(
                "This world is a folder inside {}. Branching would move that whole \
                 repository, so making save points and what-ifs is off here. Reading its \
                 history and comparing revisions still work.",
                repo.file_name().map_or_else(
                    || repo.display().to_string(),
                    |n| n.to_string_lossy().to_string()
                )
            )),
            _ => None,
        },
    }
}

/// A directory name that is safe on every filesystem, from anything the writer typed.
fn slug(rev: &str) -> String {
    let cleaned: String =
        rev.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(48).collect();
    if cleaned.trim_matches('-').is_empty() { "rev".to_string() } else { cleaned }
}

/// Load `rev` as a world, and keep the scratch directory alive while it is used.
///
/// The caller must hold the returned [`Scratch`] for as long as it touches the world:
/// every record's `source` points inside it, and the per-file diff reads those paths.
fn materialized(standing: &Standing, rev: &str, world: &World) -> Result<(Scratch, World), String> {
    let into = world.root.join(wb_git::DERIVED_DIR).join("compare").join(slug(rev));
    let scratch = Scratch::at(&into).map_err(|e| e.to_string())?;
    wb_git::materialize(standing, rev, scratch.path()).map_err(|e| e.to_string())?;

    let mut other = wb_store::load(scratch.path()).map_err(|e| {
        format!("that revision does not load as a world: {e}. Nothing was changed.")
    })?;

    // The finding this module exists to close. See the module comment.
    if let Some(spec) = &world.manuscript {
        let live = world.root.join(&spec.root);
        other.manuscript = Some(ManuscriptSpec {
            root: live.canonicalize().unwrap_or(live),
            order: spec.order.clone(),
        });
    }

    Ok((scratch, other))
}

fn record(change: wb_propose::RecordChange) -> RecordDiffDto {
    RecordDiffDto {
        id: change.id,
        name: change.name,
        kind: change.kind.to_string(),
        fields: change.fields,
        moved: change
            .moved
            .into_iter()
            .map(|m| MovedDto { what: m.what.to_string(), from: m.from, to: m.to, days: m.days })
            .collect(),
    }
}

fn plain(reference: wb_propose::RecordRef) -> RecordDiffDto {
    RecordDiffDto {
        id: reference.id,
        name: reference.name,
        kind: reference.kind.to_string(),
        fields: Vec::new(),
        moved: Vec::new(),
    }
}

fn source_of(world: &World, id: &str) -> Option<PathBuf> {
    world
        .entities
        .get(id)
        .map(|e| e.source.clone())
        .or_else(|| world.events.get(id).map(|e| e.source.clone()))
        .or_else(|| world.scenes.get(id).map(|s| s.source.clone()))
}

fn text_at(world: &World, id: &str) -> String {
    source_of(world, id).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default()
}

fn shown_path(world: &World, id: &str) -> String {
    source_of(world, id)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| id.to_string())
}

// ------------------------------------------------------------------ commands

#[tauri::command]
pub fn version_status(state: State<'_, AppState>) -> Result<VersionDto, String> {
    state.read(|world| {
        let standing = standing_of(world);
        let described = describe(&standing, world);

        let Ok(status) = wb_git::status(&standing) else {
            return VersionDto {
                standing: described,
                branch: None,
                canon: None,
                head: None,
                dirty: Vec::new(),
                unborn: false,
            };
        };

        VersionDto {
            standing: described,
            branch: status.branch,
            canon: status.canon,
            head: status.head.map(CommitDto::from),
            dirty: status
                .dirty
                .into_iter()
                .map(|c| ChangeDto { path: c.path, state: c.state })
                .collect(),
            unborn: status.unborn,
        }
    })
}

#[tauri::command]
pub fn version_history(limit: usize, state: State<'_, AppState>) -> Result<HistoryDto, String> {
    state.read(|world| {
        let history =
            wb_git::history(&standing_of(world), limit.clamp(1, 200)).map_err(|e| e.to_string())?;
        Ok(HistoryDto {
            commits: history.commits.into_iter().map(CommitDto::from).collect(),
            scanned: history.scanned,
            truncated: history.truncated,
        })
    })?
}

#[tauri::command]
pub fn version_branches(state: State<'_, AppState>) -> Result<Vec<BranchDto>, String> {
    state.read(|world| {
        wb_git::branches(&standing_of(world))
            .map(|found| {
                found
                    .into_iter()
                    .map(|b| BranchDto {
                        name: b.name,
                        is_head: b.is_head,
                        ahead: b.ahead,
                        behind: b.behind,
                        tip: b.tip.map(CommitDto::from),
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    })?
}

/// What the world folder as it stands now would change, measured against `rev`.
#[tauri::command]
pub fn version_compare(rev: String, state: State<'_, AppState>) -> Result<CompareDto, String> {
    state.read(|world| {
        let standing = standing_of(world);
        let (scratch, other) = materialized(&standing, &rev, world)?;

        let diff = wb_propose::diff_worlds(&other, world);

        // Read the two sides' files while the scratch directory is still there.
        let mut files = Vec::new();
        let touched: Vec<(&str, bool, bool)> = diff
            .changed
            .iter()
            .map(|c| (c.id.as_str(), true, true))
            .chain(diff.added.iter().map(|r| (r.id.as_str(), false, true)))
            .chain(diff.removed.iter().map(|r| (r.id.as_str(), true, false)))
            .collect();
        let total = touched.len();

        for (id, in_before, in_after) in touched.iter().take(MAX_FILES) {
            let before = if *in_before { text_at(&other, id) } else { String::new() };
            let after = if *in_after { text_at(world, id) } else { String::new() };
            if before == after {
                continue;
            }
            files.push(FileDiffDto {
                path: shown_path(if *in_after { world } else { &other }, id),
                diff: changed_lines(&before, &after),
            });
        }
        drop(scratch);

        Ok(CompareDto {
            label: format!("{rev} → the world as it stands now"),
            rev,
            added: diff.added.into_iter().map(plain).collect(),
            removed: diff.removed.into_iter().map(plain).collect(),
            changed: diff.changed.into_iter().map(record).collect(),
            resolved: diff.impact.resolved.iter().map(FindingDto::of).collect(),
            introduced: diff.impact.introduced.iter().map(FindingDto::of).collect(),
            breaks: diff.impact.breaks_something(),
            files,
            more_files: total.saturating_sub(MAX_FILES),
        })
    })?
}

#[tauri::command]
pub fn version_commit(message: String, state: State<'_, AppState>) -> Result<CommitDto, String> {
    // Committing does not touch the working tree, so there is nothing to reload.
    state.read(|world| {
        wb_git::commit(&standing_of(world), &message)
            .map(CommitDto::from)
            .map_err(|e| e.to_string())
    })?
}

/// Start a what-if. Switching to it is a separate act unless asked for, because creating
/// a branch is free and moving the writer's files is not.
#[tauri::command]
pub fn version_branch(
    name: String,
    switch: bool,
    state: State<'_, AppState>,
) -> Result<WorldSummary, String> {
    state
        .commit(|world| {
            wb_git::create_branch(&standing_of(world), &name, switch).map_err(|e| e.to_string())
        })
        .map(|(_, summary)| summary)
}

/// Every one of these can rewrite the files under the app, so all three go through
/// `commit`, which reloads the world before anything else can read a stale copy of it.
#[tauri::command]
pub fn version_switch(name: String, state: State<'_, AppState>) -> Result<WorldSummary, String> {
    state
        .commit(|world| wb_git::switch(&standing_of(world), &name).map_err(|e| e.to_string()))
        .map(|(_, summary)| summary)
}

#[tauri::command]
pub fn version_merge(target: String, state: State<'_, AppState>) -> Result<String, String> {
    state.read(|world| {
        wb_git::merge_into(&standing_of(world), &target)
            .map(|m| match m.commits {
                0 => format!("`{}` was already up to date with `{}`.", m.into, m.from),
                1 => format!("`{}` moved up to `{}` — one save point.", m.into, m.from),
                n => format!("`{}` moved up to `{}` — {n} save points.", m.into, m.from),
            })
            .map_err(|e| e.to_string())
    })?
}

#[tauri::command]
pub fn version_delete(name: String, state: State<'_, AppState>) -> Result<(), String> {
    state.read(|world| {
        wb_git::delete_branch(&standing_of(world), &name).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
pub fn version_discard(state: State<'_, AppState>) -> Result<(usize, WorldSummary), String> {
    state.commit(|world| {
        wb_git::discard(&standing_of(world)).map(|p| p.len()).map_err(|e| e.to_string())
    })
}
