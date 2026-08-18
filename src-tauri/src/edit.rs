//! Authoring: what the writer's own edits are, and what they would do.
//!
//! These are the codebase's first *inbound* DTOs. Everything else crossing the bridge is
//! `Serialize`-only, because until now the app only ever read. The shape is deliberately
//! flat and stringly-typed on dates, mirroring `wb_mcp::change::ChangeInput`: a form
//! sends what was typed, and the error it gets back names the field that carried it.
//!
//! # Why a whole record, and not a list of changes
//!
//! `wb_propose::Change` is granular so that a *reviewer* can read what is being asked
//! without diffing YAML in their head, and so an agent cannot smuggle an unrelated edit
//! into a blob. Neither reason applies when the author is the reviewer: the writer is
//! looking at the form they just filled in, and at the diff of the file it will produce.
//! So the direct path takes the complete desired record.
//!
//! That also means `marker`, `shape` and the prose body — none of which `Change` has any
//! vocabulary for — need no new operations, and the agent surface stays exactly as it is.
//! Granularity is not lost, it just moves down a layer: [`wb_store::write`] diffs the
//! record field by field and touches only the lines that moved.
//!
//! # The two-step
//!
//! [`plan_entity`] answers "what would this do" and writes nothing. [`commit`] writes.
//! The UI is expected to call the first, show what it says, and only then offer the
//! second — which is why impact is not something the writer can skip past.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use wb_core::DateExpr;
use wb_store::write::{Fidelity, Rendered};
use wb_store::{Entity, Event, Span, Value, World};

// ---------------------------------------------------------------- inbound

#[derive(Debug, Clone, Deserialize)]
pub struct FactDraft {
    pub attr: String,
    /// `wb_store::Value` is untagged, so a JSON number arrives as a number and a JSON
    /// string as text. That is the whole type system for fact values, and it is why the
    /// form has a visible kind control rather than guessing from the characters.
    pub value: Value,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntityDraft {
    pub id: String,
    pub name: String,
    /// What the prose calls this, beyond its name. Empty is the normal state.
    ///
    /// `None` — the key absent — leaves the aliases exactly as they are, the same
    /// contract [`Self::body`] has and for the same reason. A bare `Vec` with
    /// `#[serde(default)]` cannot tell *cleared* from *never sent*, and answered
    /// "cleared" for both: a form that omitted the field silently deleted what the
    /// manuscript scanner matches names against.
    #[serde(default)]
    pub aka: Option<Vec<String>>,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub existence_from: Option<String>,
    #[serde(default)]
    pub existence_to: Option<String>,
    /// Parentage edges. `None` keeps them, for the reason above — these are the whole
    /// of what the lineage view draws.
    #[serde(default)]
    pub parents: Option<Vec<String>>,
    #[serde(default)]
    pub facts: Vec<FactDraft>,
    #[serde(default)]
    pub marker: Option<[f64; 2]>,
    #[serde(default)]
    pub shape: Vec<[f64; 2]>,
    /// `None` leaves the prose exactly as it is. The app does not own prose, and a form
    /// that did not render the body must not be able to erase it.
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventDraft {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub date: String,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub location: Option<String>,
}

/// Parse a date the writer typed, naming the field if it will not parse.
///
/// Absent or empty means `?` — genuinely unplaced, which is a legitimate answer and the
/// common one early in a world's life.
pub(crate) fn date(expr: Option<&str>, field: &str) -> Result<DateExpr, String> {
    match expr.map(str::trim) {
        None | Some("") => Ok(DateExpr::Unknown),
        Some(raw) => {
            wb_core::parse_date(raw).map_err(|e| format!("bad `{field}` date {raw:?}: {e}"))
        }
    }
}

/// A list the form may or may not have rendered.
///
/// `Some` is what the writer sees, blanks dropped — so an emptied list really does clear.
/// `None` is "this form has no box for it", and keeps what the record already said.
fn kept(sent: Option<Vec<String>>, existing: Option<&Vec<String>>) -> Vec<String> {
    match sent {
        Some(list) => {
            list.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        }
        None => existing.cloned().unwrap_or_default(),
    }
}

impl EntityDraft {
    /// `existing` supplies the prose body and the file the record already lives in.
    pub fn into_entity(self, existing: Option<&Entity>) -> Result<Entity, String> {
        // A record that never stated an existence should not acquire an empty one just
        // because a form rendered two boxes for it.
        let existence = match (&self.existence_from, &self.existence_to) {
            (None, None) => None,
            _ => Some(Span {
                from: date(self.existence_from.as_deref(), "existence from")?,
                to: date(self.existence_to.as_deref(), "existence to")?,
            }),
        };

        let mut facts = Vec::with_capacity(self.facts.len());
        for (i, f) in self.facts.into_iter().enumerate() {
            if f.attr.trim().is_empty() {
                return Err(format!("fact {} has no attribute name", i + 1));
            }
            facts.push(wb_store::Fact {
                attr: f.attr.trim().to_string(),
                value: f.value,
                from: date(f.from.as_deref(), "from")?,
                to: date(f.to.as_deref(), "to")?,
            });
        }

        Ok(Entity {
            id: self.id,
            name: self.name,
            aliases: kept(self.aka, existing.map(|e| &e.aliases)),
            type_name: self.type_name,
            existence,
            parents: kept(self.parents, existing.map(|e| &e.parents)),
            facts,
            marker: self.marker,
            shape: self.shape,
            body: self.body.or_else(|| existing.map(|e| e.body.clone())).unwrap_or_default(),
            source: existing.map(|e| e.source.clone()).unwrap_or_default(),
        })
    }
}

impl EventDraft {
    pub fn into_event(self, existing: Option<&Event>) -> Result<Event, String> {
        Ok(Event {
            id: self.id,
            name: self.name,
            kind: self.kind.unwrap_or_default(),
            date: date(Some(&self.date), "date")?,
            participants: self.participants,
            location: self.location.filter(|l| !l.trim().is_empty()),
            body: existing.map(|e| e.body.clone()).unwrap_or_default(),
            source: existing.map(|e| e.source.clone()).unwrap_or_default(),
        })
    }
}

// ---------------------------------------------------------------- planning

/// One file a save would write, and what it would cost.
#[derive(Debug)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub before: Option<String>,
    pub rendered: Rendered,
}

/// Everything a save would do, before it does any of it.
#[derive(Debug)]
pub struct EditPlan {
    pub files: Vec<PlannedFile>,
    pub impact: wb_propose::Impact,
    /// What still points at the record, for a delete.
    pub references: Vec<wb_store::Reference>,
    /// The target file's content hash as it stands now. A save sends this back, and is
    /// refused if the file has moved on since.
    pub revision: Option<String>,
}

impl EditPlan {
    pub fn preserves_bytes(&self) -> bool {
        self.files.iter().all(|f| f.rendered.fidelity.preserves_bytes())
    }

    pub fn reformat_reason(&self) -> Option<String> {
        self.files.iter().find_map(|f| match &f.rendered.fidelity {
            Fidelity::Reformatted { reason, .. } => Some(reason.clone()),
            _ => None,
        })
    }

    pub fn comments_at_risk(&self) -> Vec<String> {
        self.files
            .iter()
            .flat_map(|f| match &f.rendered.fidelity {
                Fidelity::Reformatted { comments_lost, .. } => comments_lost.clone(),
                _ => Vec::new(),
            })
            .collect()
    }
}

pub fn plan_entity(world: &World, draft: EntityDraft) -> Result<EditPlan, String> {
    let existing = world.entities.get(&draft.id);
    guard_rename(world, &draft.id, existing.is_some())?;

    let entity = draft.into_entity(existing)?;
    // Reassembly *is* the validation: duplicate ids across the shared namespace, anchor
    // cycles, and dates that no longer resolve are all caught here and nowhere else.
    let after = world.with_entity(entity.clone()).map_err(|e| e.to_string())?;

    let path = wb_store::paths::entity_path(&after, &after.entities[&entity.id]);
    let before = read_if_present(&path);
    let rendered =
        wb_store::write::render_entity(&path, before.as_deref(), &after.entities[&entity.id])
            .map_err(|e| e.to_string())?;

    Ok(EditPlan {
        revision: before.as_deref().map(|t| wb_store::freshness::revision(t.as_bytes())),
        files: vec![PlannedFile { path, before, rendered }],
        impact: wb_propose::impact_between(world, &after),
        references: Vec::new(),
    })
}

pub fn plan_event(world: &World, draft: EventDraft) -> Result<EditPlan, String> {
    let existing = world.events.get(&draft.id);
    guard_rename(world, &draft.id, existing.is_some())?;

    let event = draft.into_event(existing)?;
    let after = world.with_event(event.clone()).map_err(|e| e.to_string())?;

    let path = wb_store::paths::event_path(&after, &after.events[&event.id]);
    let before = read_if_present(&path);
    let rendered =
        wb_store::write::render_event(&path, before.as_deref(), &after.events[&event.id])
            .map_err(|e| e.to_string())?;

    Ok(EditPlan {
        revision: before.as_deref().map(|t| wb_store::freshness::revision(t.as_bytes())),
        files: vec![PlannedFile { path, before, rendered }],
        impact: wb_propose::impact_between(world, &after),
        references: Vec::new(),
    })
}

/// What deleting a record would do. The references are the interesting half.
pub fn plan_delete(world: &World, id: &str) -> Result<EditPlan, String> {
    let path = if let Some(entity) = world.entities.get(id) {
        wb_store::paths::entity_path(world, entity)
    } else if let Some(event) = world.events.get(id) {
        wb_store::paths::event_path(world, event)
    } else if let Some(scene) = world.scenes.get(id) {
        wb_store::paths::scene_path(world, scene)
    } else {
        return Err(format!("no record `{id}`"));
    };

    let references = world.references_to(id);
    let after = world.without(id).map_err(|e| {
        format!(
            "{e}\nSomething still dates itself against this record, so it cannot be removed yet."
        )
    })?;
    let before = read_if_present(&path);

    Ok(EditPlan {
        revision: before.as_deref().map(|t| wb_store::freshness::revision(t.as_bytes())),
        // An empty render is the signal to delete rather than write.
        files: vec![PlannedFile {
            path,
            before,
            rendered: Rendered { text: String::new(), fidelity: Fidelity::Preserved },
        }],
        impact: wb_propose::impact_between(world, &after),
        references,
    })
}

/// An id is how everything else points at a record, so changing one is a refactor
/// across the whole world rather than an edit to a field.
fn guard_rename(world: &World, id: &str, exists: bool) -> Result<(), String> {
    if exists {
        return Ok(());
    }
    if world.knows(id) {
        return Err(format!("`{id}` is already taken by another record"));
    }
    Ok(())
}

// ---------------------------------------------------------------- committing

/// Write the plan's files, after checking nothing moved underneath it.
///
/// `expected` is the revision the caller was shown. A mismatch is refused rather than
/// merged: for a single-user tool with the files under git, "reload and reapply" is a
/// proportionate answer and a three-way merge is not.
pub fn commit(plan: &EditPlan, expected: Option<&str>, allow_reformat: bool) -> Result<(), String> {
    if !allow_reformat && !plan.preserves_bytes() {
        return Err(format!(
            "saving would reformat the file: {}",
            plan.reformat_reason().unwrap_or_default()
        ));
    }

    for file in &plan.files {
        if let Some(expected) = expected {
            let now = read_if_present(&file.path)
                .map(|t| wb_store::freshness::revision(t.as_bytes()))
                .unwrap_or_default();
            if now != expected {
                let name = file
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                return Err(format!(
                    "{name} changed on disk since you opened it. \
                     Reload the record and reapply your edit."
                ));
            }
        }

        if file.rendered.text.is_empty() {
            if file.path.is_file() {
                std::fs::remove_file(&file.path).map_err(|e| e.to_string())?;
            }
            continue;
        }
        wb_store::atomic::write(&file.path, &file.rendered.text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn read_if_present(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

// ---------------------------------------------------------------- outbound

/// The raw record behind an id: values with their types intact, dates exactly as the
/// writer typed them, and the prose body.
///
/// Deliberately not `EntityDto`. That one is a *rendered view* at a date — its facts have
/// lost their `from`/`to` to a certainty flag, its values have been stringified through
/// `Display`, and facts not valid at the scrubber's day are missing entirely. Editing it
/// and saving would quietly rewrite `to: "@evt_siege_of_marrow"` into nothing and turn
/// every number into text.
#[derive(Serialize)]
pub struct EntityRecordDto {
    pub id: String,
    pub name: String,
    pub aka: Vec<String>,
    #[serde(rename = "type")]
    pub type_name: String,
    pub primitive: Option<&'static str>,
    /// `null` for `?`, so the form shows an empty box rather than a literal question mark.
    pub existence_from: Option<String>,
    pub existence_to: Option<String>,
    pub parents: Vec<String>,
    pub facts: Vec<FactRecordDto>,
    pub marker: Option<[f64; 2]>,
    pub shape: Vec<[f64; 2]>,
    pub body: String,
    pub path: String,
    pub revision: Option<String>,
}

#[derive(Serialize)]
pub struct FactRecordDto {
    pub attr: String,
    pub value: serde_json::Value,
    /// `"text" | "int" | "float" | "bool"`, so the form can show what it is and the
    /// writer can change it deliberately.
    pub kind: &'static str,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Serialize)]
pub struct EventRecordDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub date: String,
    pub participants: Vec<String>,
    pub location: Option<String>,
    pub path: String,
    pub revision: Option<String>,
}

fn shown(d: &DateExpr) -> Option<String> {
    match d {
        DateExpr::Unknown => None,
        other => Some(other.to_string()),
    }
}

fn value_json(v: &Value) -> (serde_json::Value, &'static str) {
    match v {
        Value::Bool(b) => (serde_json::Value::Bool(*b), "bool"),
        Value::Int(i) => (serde_json::Value::from(*i), "int"),
        Value::Float(f) => (serde_json::Value::from(*f), "float"),
        Value::Text(t) => (serde_json::Value::String(t.clone()), "text"),
    }
}

impl EntityRecordDto {
    pub fn of(world: &World, entity: &Entity) -> Self {
        let path = wb_store::paths::entity_path(world, entity);
        let facts = entity
            .facts
            .iter()
            .map(|f| {
                let (value, kind) = value_json(&f.value);
                FactRecordDto {
                    attr: f.attr.clone(),
                    value,
                    kind,
                    from: shown(&f.from),
                    to: shown(&f.to),
                }
            })
            .collect();

        Self {
            id: entity.id.clone(),
            name: entity.name.clone(),
            aka: entity.aliases.clone(),
            type_name: entity.type_name.clone(),
            primitive: world.primitive_of(entity).map(crate::commands::primitive_name),
            existence_from: entity.existence.as_ref().and_then(|s| shown(&s.from)),
            existence_to: entity.existence.as_ref().and_then(|s| shown(&s.to)),
            parents: entity.parents.clone(),
            facts,
            marker: entity.marker,
            shape: entity.shape.clone(),
            body: entity.body.clone(),
            revision: read_if_present(&path).map(|t| wb_store::freshness::revision(t.as_bytes())),
            path: relative(world, &path),
        }
    }
}

impl EventRecordDto {
    pub fn of(world: &World, event: &Event) -> Self {
        let path = wb_store::paths::event_path(world, event);
        Self {
            id: event.id.clone(),
            name: event.name.clone(),
            kind: event.kind.clone(),
            date: event.date.to_string(),
            participants: event.participants.clone(),
            location: event.location.clone(),
            revision: read_if_present(&path).map(|t| wb_store::freshness::revision(t.as_bytes())),
            path: relative(world, &path),
        }
    }
}

fn relative(world: &World, path: &std::path::Path) -> String {
    path.strip_prefix(&world.root).unwrap_or(path).display().to_string()
}

// ---------------------------------------------------------------- commands

use tauri::State;

use crate::commands::{AppState, FindingDto, WorldSummary};

/// What a save would do. Nothing here touches disk.
#[derive(Serialize)]
pub struct EditPreviewDto {
    pub files: Vec<PreviewFileDto>,
    pub resolved: Vec<FindingDto>,
    pub introduced: Vec<FindingDto>,
    pub breaks: bool,
    /// False when saving would rewrite the file rather than patch it. The UI is expected
    /// to say so and ask, because that is the case where comments do not survive.
    pub preserves_bytes: bool,
    pub reformat_reason: Option<String>,
    pub comments_at_risk: Vec<String>,
    /// What still points at the record, for a delete.
    pub references: Vec<ReferenceDto>,
    pub revision: Option<String>,
}

#[derive(Serialize)]
pub struct PreviewFileDto {
    pub path: String,
    pub is_new: bool,
    pub diff: Vec<crate::commands::DiffLine>,
}

#[derive(Serialize)]
pub struct ReferenceDto {
    pub by: String,
    pub name: String,
    pub how: &'static str,
}

/// What a record is called, whatever kind of record it is.
///
/// Scenes were missing from the lookup this replaces, so a delete confirmation listing
/// what still points at a record named the events by name and the scenes by id —
/// `scn_gate_at_dusk` sitting under "The Siege of Marrow" in the same list. An id is what
/// you show when there is no name; a scene has one.
fn name_of(world: &World, id: &str) -> String {
    world
        .entities
        .get(id)
        .map(|e| e.name.clone())
        .or_else(|| world.events.get(id).map(|e| e.name.clone()))
        .or_else(|| world.scenes.get(id).map(|s| s.name.clone()))
        .unwrap_or_else(|| id.to_string())
}

/// Everything that names `id`, named back.
///
/// A thin wrapping of [`World::references_to`] and deliberately nothing more: the same
/// answer the delete confirmation has always shown, now reachable without proposing to
/// delete anything. An id the world has never heard of is not an error here — it is the
/// case this most needs to answer, because a reference left dangling by a delete is
/// exactly what the writer is trying to find.
pub fn references_of(world: &World, id: &str) -> Vec<ReferenceDto> {
    world
        .references_to(id)
        .into_iter()
        .map(|r| ReferenceDto { name: name_of(world, &r.by), by: r.by, how: r.how })
        .collect()
}

#[tauri::command]
pub fn references(id: String, state: State<'_, AppState>) -> Result<Vec<ReferenceDto>, String> {
    state.read(|world| references_of(world, &id))
}

#[derive(Serialize)]
pub struct SaveResultDto {
    pub summary: WorldSummary,
    pub written: Vec<String>,
    /// The new hash, so a panel left open can keep editing without refetching.
    pub revision: Option<String>,
}

pub(crate) fn preview_of(world: &World, plan: &EditPlan) -> EditPreviewDto {
    EditPreviewDto {
        files: plan
            .files
            .iter()
            .map(|f| PreviewFileDto {
                path: relative(world, &f.path),
                is_new: f.before.is_none(),
                diff: crate::commands::changed_lines(
                    f.before.as_deref().unwrap_or(""),
                    &f.rendered.text,
                ),
            })
            .collect(),
        resolved: plan.impact.resolved.iter().map(FindingDto::of).collect(),
        introduced: plan.impact.introduced.iter().map(FindingDto::of).collect(),
        breaks: plan.impact.breaks_something(),
        preserves_bytes: plan.preserves_bytes(),
        reformat_reason: plan.reformat_reason(),
        comments_at_risk: plan.comments_at_risk(),
        references: plan
            .references
            .iter()
            .map(|r| ReferenceDto { name: name_of(world, &r.by), by: r.by.clone(), how: r.how })
            .collect(),
        revision: plan.revision.clone(),
    }
}

#[tauri::command]
pub fn entity_record(id: String, state: State<'_, AppState>) -> Result<EntityRecordDto, String> {
    state.read(|world| {
        world
            .entities
            .get(&id)
            .map(|e| EntityRecordDto::of(world, e))
            .ok_or_else(|| format!("no entity `{id}`"))
    })?
}

#[tauri::command]
pub fn event_record(id: String, state: State<'_, AppState>) -> Result<EventRecordDto, String> {
    state.read(|world| {
        world
            .events
            .get(&id)
            .map(|e| EventRecordDto::of(world, e))
            .ok_or_else(|| format!("no event `{id}`"))
    })?
}

#[tauri::command]
pub fn preview_entity(
    draft: EntityDraft,
    state: State<'_, AppState>,
) -> Result<EditPreviewDto, String> {
    state.read(|world| plan_entity(world, draft).map(|plan| preview_of(world, &plan)))?
}

#[tauri::command]
pub fn preview_event(
    draft: EventDraft,
    state: State<'_, AppState>,
) -> Result<EditPreviewDto, String> {
    state.read(|world| plan_event(world, draft).map(|plan| preview_of(world, &plan)))?
}

#[tauri::command]
pub fn preview_delete(id: String, state: State<'_, AppState>) -> Result<EditPreviewDto, String> {
    state.read(|world| plan_delete(world, &id).map(|plan| preview_of(world, &plan)))?
}

#[tauri::command]
pub fn save_entity(
    draft: EntityDraft,
    revision: Option<String>,
    allow_reformat: bool,
    state: State<'_, AppState>,
) -> Result<SaveResultDto, String> {
    let (written, summary) = state.commit(|world| {
        let plan = plan_entity(world, draft)?;
        commit(&plan, revision.as_deref(), allow_reformat)?;
        Ok(written_paths(world, &plan))
    })?;
    Ok(saved(summary, written))
}

#[tauri::command]
pub fn save_event(
    draft: EventDraft,
    revision: Option<String>,
    allow_reformat: bool,
    state: State<'_, AppState>,
) -> Result<SaveResultDto, String> {
    let (written, summary) = state.commit(|world| {
        let plan = plan_event(world, draft)?;
        commit(&plan, revision.as_deref(), allow_reformat)?;
        Ok(written_paths(world, &plan))
    })?;
    Ok(saved(summary, written))
}

/// Geometry only, for a marker drop or a finished polygon.
///
/// No impact pass, and that is correctness rather than a shortcut: none of the six rules
/// in `wb-check` reads `marker` or `shape`, so there is nothing a moved point could
/// settle or break. Running the check anyway would be a lie of implication.
#[tauri::command]
pub fn save_geometry(
    id: String,
    marker: Option<[f64; 2]>,
    shape: Vec<[f64; 2]>,
    revision: Option<String>,
    state: State<'_, AppState>,
) -> Result<SaveResultDto, String> {
    let (written, summary) = state.commit(|world| {
        let entity = world.entities.get(&id).ok_or_else(|| format!("no entity `{id}`"))?;
        let mut moved = entity.clone();
        moved.marker = marker;
        moved.shape = shape;

        let path = wb_store::paths::entity_path(world, &moved);
        let before = read_if_present(&path);
        let rendered = wb_store::write::render_entity(&path, before.as_deref(), &moved)
            .map_err(|e| e.to_string())?;
        let plan = EditPlan {
            revision: before.as_deref().map(|t| wb_store::freshness::revision(t.as_bytes())),
            files: vec![PlannedFile { path: path.clone(), before, rendered }],
            impact: wb_propose::Impact::default(),
            references: Vec::new(),
        };
        commit(&plan, revision.as_deref(), true)?;
        Ok(vec![(relative(world, &path), path.clone())])
    })?;
    Ok(saved(summary, written))
}

#[tauri::command]
pub fn delete_record(
    id: String,
    revision: Option<String>,
    state: State<'_, AppState>,
) -> Result<SaveResultDto, String> {
    let (written, summary) = state.commit(|world| {
        let plan = plan_delete(world, &id)?;
        commit(&plan, revision.as_deref(), true)?;
        Ok(written_paths(world, &plan))
    })?;
    Ok(SaveResultDto {
        summary,
        written: written.into_iter().map(|(s, _)| s).collect(),
        revision: None,
    })
}

pub(crate) fn written_paths(world: &World, plan: &EditPlan) -> Vec<(String, PathBuf)> {
    plan.files.iter().map(|f| (relative(world, &f.path), f.path.clone())).collect()
}

/// Hand back the revision of what was actually written, so a panel left open can carry
/// on editing without refetching the whole record.
pub(crate) fn saved(summary: WorldSummary, written: Vec<(String, PathBuf)>) -> SaveResultDto {
    let revision = written
        .first()
        .and_then(|(_, path)| read_if_present(path))
        .map(|t| wb_store::freshness::revision(t.as_bytes()));
    SaveResultDto { summary, written: written.into_iter().map(|(s, _)| s).collect(), revision }
}
