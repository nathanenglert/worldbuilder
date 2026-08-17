//! The book, for the app.
//!
//! Three things the UI needs and the world alone cannot answer: where the scenes sit on
//! the timeline, where they sit on the map, and what of the world reaches the page.
//!
//! Scene *editing* deliberately reuses `edit.rs` wholesale — the same plan-preview-commit
//! shape, the same revision check, the same format-preserving writer. A second write path
//! with its own idea of what a safe save looks like is how the two drift.

use serde::{Deserialize, Serialize};
use tauri::State;
use wb_core::DateExpr;
use wb_store::{Scene, World};

use crate::commands::AppState;
use crate::edit::{EditPlan, EditPreviewDto, PlannedFile, SaveResultDto, date, read_if_present};

/// A scene as the timeline and the map need it.
#[derive(Serialize)]
pub struct SceneDto {
    pub id: String,
    pub name: String,
    /// Resolved position, and the doubt around it — the same three numbers events carry,
    /// so the timeline can draw both bands with one piece of code.
    pub nominal: Option<i64>,
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
    pub label: String,
    pub pov: Option<String>,
    pub on_page: Vec<String>,
    pub location: Option<String>,
    pub prose: Option<String>,
    /// Position in the book, from zero. Not the same ordering as `nominal`, and it is not
    /// meant to be: a flashback reads second and happens first.
    pub order: usize,
    /// Where to draw it, from the location's **record** marker.
    ///
    /// Deliberately not from the snapshot, which is filtered by date: a scene set before
    /// its location was founded would lose its dot exactly when the story needs it most.
    pub point: Option<[f64; 2]>,
    /// Why the prose could not be read, if it could not.
    pub unreadable: Option<String>,
    pub words: Option<usize>,
    pub names: Vec<String>,
}

/// What the story panel shows.
#[derive(Serialize)]
pub struct StoryDto {
    pub standing: &'static str,
    pub scenes_read: usize,
    pub surfaced: usize,
    pub total: usize,
    pub percent: Option<u32>,
    pub records: Vec<SurfacingDto>,
    pub unreadable: Vec<UnreadableDto>,
    /// The manuscript root as declared, for the panel to name when it is missing.
    pub root: Option<String>,
}

#[derive(Serialize)]
pub struct SurfacingDto {
    pub id: String,
    pub name: String,
    pub standing: &'static str,
    pub mentions: usize,
    pub scenes: Vec<String>,
    pub referenced_by: usize,
    pub appears_in: usize,
    pub cast_in: usize,
    pub facts: usize,
    pub prose_bytes: usize,
    pub first_seen: Option<String>,
}

#[derive(Serialize)]
pub struct UnreadableDto {
    pub scene: String,
    pub reason: String,
}

/// One scene's prose, for reading beside the record being edited.
#[derive(Serialize)]
pub struct PassageDto {
    pub scene: String,
    pub file: String,
    pub heading: Option<String>,
    pub text: String,
    pub words: usize,
    pub truncated: bool,
}

/// The raw record, for the editor. Dates as authored, never resolved.
#[derive(Serialize)]
pub struct SceneRecordDto {
    pub id: String,
    pub name: String,
    pub date: String,
    pub pov: Option<String>,
    pub on_page: Vec<String>,
    pub location: Option<String>,
    pub prose: Option<String>,
    pub path: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SceneDraft {
    pub id: String,
    pub name: String,
    pub date: String,
    #[serde(default)]
    pub pov: Option<String>,
    #[serde(default)]
    pub on_page: Vec<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub prose: Option<String>,
}

impl SceneDraft {
    pub fn into_scene(self, existing: Option<&Scene>) -> Result<Scene, String> {
        // A blank box means "no link yet", not "a link to the empty string". Same rule
        // as the date fields: an empty control is an absence, never a value.
        let trimmed = |s: Option<String>| s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

        Ok(Scene {
            id: self.id.trim().to_string(),
            name: self.name,
            date: date(Some(&self.date), "date")?,
            pov: trimmed(self.pov),
            on_page: self.on_page.into_iter().filter(|p| !p.trim().is_empty()).collect(),
            location: trimmed(self.location),
            prose: trimmed(self.prose),
            source: existing.map(|s| s.source.clone()).unwrap_or_default(),
        })
    }
}

fn shown(date: &DateExpr) -> String {
    match date {
        DateExpr::Unknown => String::new(),
        other => other.to_string(),
    }
}

impl SceneRecordDto {
    pub fn of(world: &World, scene: &Scene) -> Self {
        let path = wb_store::paths::scene_path(world, scene);
        Self {
            id: scene.id.clone(),
            name: scene.name.clone(),
            date: shown(&scene.date),
            pov: scene.pov.clone(),
            on_page: scene.on_page.clone(),
            location: scene.location.clone(),
            prose: scene.prose.clone(),
            revision: read_if_present(&path).map(|t| wb_store::freshness::revision(t.as_bytes())),
            path: path.display().to_string(),
        }
    }
}

pub fn plan_scene(world: &World, draft: SceneDraft) -> Result<EditPlan, String> {
    let existing = world.scenes.get(draft.id.trim());
    let scene = draft.into_scene(existing)?;
    if scene.id.is_empty() {
        return Err("a scene needs an id".into());
    }

    let after = world.with_scene(scene.clone()).map_err(|e| e.to_string())?;
    let path = wb_store::paths::scene_path(&after, &scene);
    let before = read_if_present(&path);
    let rendered = wb_store::write::render_scene(&path, before.as_deref(), &scene)
        .map_err(|e| e.to_string())?;

    // The impact pass reads the *merged* report, so moving a scene's date shows the
    // prose findings it settles or opens — which is the whole reason a scene has a date.
    let impact = wb_propose::impact_between(world, &after);

    Ok(EditPlan {
        revision: before.as_ref().map(|t| wb_store::freshness::revision(t.as_bytes())),
        files: vec![PlannedFile { path, before, rendered }],
        impact,
        references: Vec::new(),
    })
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub fn scenes(state: State<'_, AppState>) -> Result<Vec<SceneDto>, String> {
    state.read(|world| {
        let story = wb_story::Story::read(world);
        let mut out: Vec<SceneDto> = story
            .reads
            .iter()
            .filter_map(|read| {
                let scene = world.scenes.get(&read.scene)?;
                let resolved = world.resolved_node(&scene.id);
                let nominal = resolved.and_then(|r| r.nominal);

                let point = scene
                    .location
                    .as_ref()
                    .and_then(|id| world.entities.get(id))
                    .and_then(|e| e.marker);

                Some(SceneDto {
                    id: scene.id.clone(),
                    name: scene.name.clone(),
                    nominal: nominal.map(|d| d.0),
                    earliest: resolved.and_then(|r| r.earliest).map(|d| d.0),
                    latest: resolved.and_then(|r| r.latest).map(|d| d.0),
                    label: nominal
                        .map(|d| world.calendar.format_long(d))
                        .unwrap_or_else(|| "undated".into()),
                    pov: scene.pov.clone(),
                    on_page: scene.on_page.clone(),
                    location: scene.location.clone(),
                    prose: scene.prose.clone(),
                    order: story.order.get(&scene.id).copied().unwrap_or(0),
                    point,
                    unreadable: read.passage.as_ref().err().cloned(),
                    words: read.passage.as_ref().ok().map(|p| p.words),
                    names: wb_story::mentions::distinct(&read.mentions),
                })
            })
            .collect();

        out.sort_by_key(|s| s.order);
        out
    })
}

#[tauri::command]
pub fn story(state: State<'_, AppState>) -> Result<StoryDto, String> {
    state.read(|world| {
        let story = wb_story::Story::read(world);
        let report = wb_story::iceberg::report(world, &story);

        StoryDto {
            standing: match report.standing {
                wb_story::Standing::Unlinked => "unlinked",
                wb_story::Standing::RootMissing => "root_missing",
                wb_story::Standing::Linked => "linked",
            },
            scenes_read: report.scenes_read,
            surfaced: report.surfaced,
            total: report.total,
            percent: report.ratio(),
            records: report
                .entries
                .iter()
                .map(|e| SurfacingDto {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    standing: e.quadrant.slug(),
                    mentions: e.mentions,
                    scenes: e.scenes.clone(),
                    referenced_by: e.referenced_by,
                    appears_in: e.appears_in,
                    cast_in: e.cast_in,
                    facts: e.facts,
                    prose_bytes: e.prose_bytes,
                    first_seen: e.first_seen.clone(),
                })
                .collect(),
            unreadable: report
                .unreadable
                .iter()
                .map(|(scene, reason)| UnreadableDto {
                    scene: scene.clone(),
                    reason: reason.clone(),
                })
                .collect(),
            root: world.manuscript.as_ref().map(|m| m.root.display().to_string()),
        }
    })
}

/// One scene's prose, read fresh rather than from the cached story.
///
/// Fresh on purpose: this is what a writer opens *while editing the link*, and the whole
/// question they are asking is whether the string they just typed points at the right
/// section. An answer from a snapshot taken before they typed it would be worse than
/// useless.
#[tauri::command]
pub fn passage(scene: String, state: State<'_, AppState>) -> Result<PassageDto, String> {
    state.read(|world| {
        let base = wb_story::manuscript::root(world)
            .ok_or_else(|| "this world has no `manuscript.root` in world.yaml".to_string())?;
        let link = world
            .scenes
            .get(&scene)
            .and_then(|s| s.prose.clone())
            .ok_or_else(|| format!("`{scene}` is not linked to any prose"))?;

        let p = wb_story::manuscript::read(&base, &link)?;
        Ok(PassageDto {
            scene,
            file: p.file,
            heading: p.heading,
            text: p.text,
            words: p.words,
            truncated: p.truncated,
        })
    })?
}

/// Resolve a link the writer is typing, without it being on a record yet.
///
/// The `DateField` move, applied to prose: show what the string means before it is
/// saved, so the grammar is learned by using it rather than by reading about it.
#[tauri::command]
pub fn resolve_prose(link: String, state: State<'_, AppState>) -> Result<PassageDto, String> {
    state.read(|world| {
        let base = wb_story::manuscript::root(world)
            .ok_or_else(|| "this world has no `manuscript.root` in world.yaml".to_string())?;
        let p = wb_story::manuscript::read(&base, &link)?;
        Ok(PassageDto {
            scene: String::new(),
            file: p.file,
            heading: p.heading,
            text: p.text,
            words: p.words,
            truncated: p.truncated,
        })
    })?
}

/// Chapter files under the manuscript root, in reading order — for the link field's
/// suggestions, so a writer never has to remember a filename.
#[tauri::command]
pub fn chapters(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.read(|world| {
        let Some(base) = wb_story::manuscript::root(world) else { return Vec::new() };
        let declared = world.manuscript.as_ref().map(|m| m.order.clone()).unwrap_or_default();
        wb_story::manuscript::chapters(&base, &declared)
    })
}

#[tauri::command]
pub fn scene_record(id: String, state: State<'_, AppState>) -> Result<SceneRecordDto, String> {
    state.read(|world| {
        world
            .scenes
            .get(&id)
            .map(|s| SceneRecordDto::of(world, s))
            .ok_or_else(|| format!("no scene `{id}`"))
    })?
}

#[tauri::command]
pub fn preview_scene(
    draft: SceneDraft,
    state: State<'_, AppState>,
) -> Result<EditPreviewDto, String> {
    state
        .read(|world| plan_scene(world, draft).map(|plan| crate::edit::preview_of(world, &plan)))?
}

#[tauri::command]
pub fn save_scene(
    draft: SceneDraft,
    revision: Option<String>,
    allow_reformat: bool,
    state: State<'_, AppState>,
) -> Result<SaveResultDto, String> {
    let (written, summary) = state.commit(|world| {
        let plan = plan_scene(world, draft)?;
        crate::edit::commit(&plan, revision.as_deref(), allow_reformat)?;
        Ok(crate::edit::written_paths(world, &plan))
    })?;
    Ok(crate::edit::saved(summary, written))
}
