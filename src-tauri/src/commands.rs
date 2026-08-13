//! The query surface the frontend scrubs against.
//!
//! Everything crossing the bridge is an owned DTO with day numbers as plain integers.
//! Rendering decisions the *world* owns — which polity holds a region, what colour that
//! is, whether the claim is settled — are resolved here rather than reassembled in the
//! UI, so there is one place that can be wrong about them.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use wb_core::{Containment, Day, Resolved};
use wb_store::{Primitive, World, load};

#[derive(Default)]
pub struct AppState {
    world: Mutex<Option<World>>,
}

impl AppState {
    fn read<T>(&self, f: impl FnOnce(&World) -> T) -> Result<T, String> {
        let guard = self.world.lock().map_err(|_| "world state is poisoned".to_string())?;
        let world = guard.as_ref().ok_or_else(|| "no world is open".to_string())?;
        Ok(f(world))
    }
}

fn certainty(c: Containment) -> &'static str {
    match c {
        Containment::Yes => "yes",
        Containment::Maybe => "maybe",
        Containment::No => "no",
    }
}

fn primitive_name(p: Primitive) -> &'static str {
    match p {
        Primitive::Actor => "actor",
        Primitive::Polity => "polity",
        Primitive::Place => "place",
        Primitive::Event => "event",
        Primitive::Thing => "thing",
    }
}

#[derive(Serialize)]
pub struct WorldSummary {
    pub name: String,
    pub calendar: String,
    pub months: Vec<String>,
    pub entity_count: usize,
    pub event_count: usize,
    /// Inclusive day range worth scrubbing, padded so the outermost events do not sit
    /// pinned against the ends of the track.
    pub span: [i64; 2],
    /// Every instant anything could change. The UI snaps to these and skips requerying
    /// between them.
    pub change_points: Vec<i64>,
    pub undeclared_types: Vec<String>,
}

impl WorldSummary {
    pub fn of(world: &World) -> Self {
        let change_points: Vec<i64> = world.change_points().iter().map(|d| d.0).collect();
        let year = world.calendar.days_in_year(0).max(1);
        let (lo, hi) = match (change_points.first(), change_points.last()) {
            (Some(&a), Some(&b)) => (a, b),
            _ => (0, year),
        };
        let pad = ((hi - lo) / 12).max(year * 2);

        Self {
            name: world.name.clone(),
            calendar: world.calendar.name.clone(),
            months: world.calendar.months.iter().map(|m| m.name.clone()).collect(),
            entity_count: world.entities.len(),
            event_count: world.events.len(),
            span: [lo - pad, hi + pad],
            change_points,
            undeclared_types: world.undeclared_types().keys().cloned().collect(),
        }
    }
}

#[derive(Serialize)]
pub struct FactDto {
    pub attr: String,
    pub value: String,
    pub certainty: &'static str,
}

/// One live `owner` claim. Two at once is not a bug — it is a vague handover date, and
/// the map is expected to show the doubt rather than resolve it.
#[derive(Serialize)]
pub struct ClaimDto {
    pub owner: String,
    pub name: String,
    pub color: Option<String>,
    pub certainty: &'static str,
}

#[derive(Serialize)]
pub struct EntityDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub primitive: Option<&'static str>,
    pub existence: &'static str,
    pub facts: Vec<FactDto>,
    pub marker: Option<[f64; 2]>,
    pub shape: Vec<[f64; 2]>,
    pub claims: Vec<ClaimDto>,
}

#[derive(Serialize)]
pub struct SnapshotDto {
    pub day: i64,
    pub label: String,
    pub entities: Vec<EntityDto>,
}

impl SnapshotDto {
    pub fn of(world: &World, day: Day) -> Self {
        let entities = world
            .at(day)
            .entities
            .iter()
            .map(|view| {
                let entity = view.entity;
                let claims = view
                    .facts
                    .iter()
                    .filter(|f| f.attr == "owner")
                    .map(|f| {
                        let owner = f.value.to_string();
                        ClaimDto {
                            name: world
                                .entities
                                .get(&owner)
                                .map(|o| o.name.clone())
                                .unwrap_or_else(|| owner.clone()),
                            color: world
                                .value_at(&owner, "color", day)
                                .map(|c| c.value.to_string()),
                            certainty: certainty(f.certainty),
                            owner,
                        }
                    })
                    .collect();

                EntityDto {
                    id: entity.id.clone(),
                    name: entity.name.clone(),
                    type_name: entity.type_name.clone(),
                    primitive: world.primitive_of(entity).map(primitive_name),
                    existence: certainty(view.existence),
                    facts: view
                        .facts
                        .iter()
                        .map(|f| FactDto {
                            attr: f.attr.to_string(),
                            value: f.value.to_string(),
                            certainty: certainty(f.certainty),
                        })
                        .collect(),
                    marker: entity.marker,
                    shape: entity.shape.clone(),
                    claims,
                }
            })
            .collect();

        Self { day: day.0, label: world.calendar.format_long(day), entities }
    }
}

#[derive(Serialize)]
pub struct EventDto {
    id: String,
    name: String,
    kind: String,
    /// Where the marker sits. `earliest`/`latest` are how wide to draw its doubt.
    nominal: Option<i64>,
    earliest: Option<i64>,
    latest: Option<i64>,
    label: String,
    participants: Vec<String>,
    location: Option<String>,
}

/// A consistency finding. `certainty` is the field that matters: `definite` is wrong
/// under every reading of every fuzzy date, `possible` is where mysteries live.
#[derive(Serialize)]
pub struct FindingDto {
    pub rule: &'static str,
    pub title: &'static str,
    pub certainty: &'static str,
    pub subject: String,
    pub related: Vec<String>,
    pub message: String,
    pub at: Option<i64>,
    pub sources: Vec<String>,
}

impl FindingDto {
    pub fn of(finding: &wb_check::Finding) -> Self {
        Self {
            rule: finding.rule.slug(),
            title: finding.rule.title(),
            certainty: finding.certainty.slug(),
            subject: finding.subject.clone(),
            related: finding.related.clone(),
            message: finding.message.clone(),
            at: finding.at.map(|d| d.0),
            sources: finding.sources.iter().map(|p| p.display().to_string()).collect(),
        }
    }
}

#[tauri::command]
pub fn check_world(state: State<'_, AppState>) -> Result<Vec<FindingDto>, String> {
    state.read(|world| wb_check::check(world).findings.iter().map(FindingDto::of).collect())
}

/// A pending change, with what accepting it would do to the world's consistency.
#[derive(Serialize)]
pub struct ProposalDto {
    pub id: String,
    pub title: String,
    pub author: String,
    pub note: String,
    pub status: &'static str,
    pub changes: Vec<String>,
    pub resolves: usize,
    pub introduces: usize,
    /// True when accepting would add a contradiction wrong under every reading.
    pub breaks: bool,
}

#[derive(Serialize)]
pub struct DiffLine {
    pub tag: &'static str,
    pub text: String,
}

#[derive(Serialize)]
pub struct FileEditDto {
    pub path: String,
    pub is_new: bool,
    pub diff: Vec<DiffLine>,
}

#[derive(Serialize)]
pub struct ProposalDetailDto {
    #[serde(flatten)]
    pub summary: ProposalDto,
    pub resolved: Vec<FindingDto>,
    pub introduced: Vec<FindingDto>,
    pub files: Vec<FileEditDto>,
    /// Set when the proposal cannot even be simulated — a stale or malformed change.
    pub error: Option<String>,
}

fn summarize(world: &World, proposal: &wb_propose::Proposal) -> ProposalDto {
    let effect = wb_propose::impact(world, proposal).ok();
    ProposalDto {
        id: proposal.id.clone(),
        title: proposal.title.clone(),
        author: proposal.author.clone(),
        note: proposal.note.clone(),
        status: proposal.status.slug(),
        changes: proposal.changes.iter().map(|c| c.summary()).collect(),
        resolves: effect.as_ref().map_or(0, |e| e.resolved.len()),
        introduces: effect.as_ref().map_or(0, |e| e.introduced.len()),
        breaks: effect.as_ref().is_some_and(|e| e.breaks_something()),
    }
}

/// Only the lines that changed. Full context would drown a side panel, and the file is
/// on disk for anyone who wants to read the rest.
fn changed_lines(before: &str, after: &str) -> Vec<DiffLine> {
    use similar::ChangeTag;
    similar::TextDiff::from_lines(before, after)
        .iter_all_changes()
        .filter_map(|change| {
            let tag = match change.tag() {
                ChangeTag::Insert => "+",
                ChangeTag::Delete => "-",
                ChangeTag::Equal => return None,
            };
            Some(DiffLine { tag, text: change.value().trim_end().to_string() })
        })
        .take(80)
        .collect()
}

#[tauri::command]
pub fn list_proposals(state: State<'_, AppState>) -> Result<Vec<ProposalDto>, String> {
    let guard = state.world.lock().map_err(|_| "world state is poisoned".to_string())?;
    let world = guard.as_ref().ok_or_else(|| "no world is open".to_string())?;
    let proposals = wb_propose::store::load_all(&world.root).map_err(|e| e.to_string())?;
    Ok(proposals.iter().map(|p| summarize(world, p)).collect())
}

#[tauri::command]
pub fn proposal_detail(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProposalDetailDto, String> {
    let guard = state.world.lock().map_err(|_| "world state is poisoned".to_string())?;
    let world = guard.as_ref().ok_or_else(|| "no world is open".to_string())?;

    let proposal = wb_propose::store::load_all(&world.root)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("no proposal `{id}`"))?;

    let summary = summarize(world, &proposal);

    let (resolved, introduced) = match wb_propose::impact(world, &proposal) {
        Ok(effect) => (
            effect.resolved.iter().map(FindingDto::of).collect(),
            effect.introduced.iter().map(FindingDto::of).collect(),
        ),
        Err(_) => (Vec::new(), Vec::new()),
    };

    let (files, error) = match wb_propose::preview(world, &proposal) {
        Ok(edits) => (
            edits
                .iter()
                .map(|edit| FileEditDto {
                    path: edit
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    is_new: edit.is_new(),
                    diff: changed_lines(edit.before.as_deref().unwrap_or(""), &edit.after),
                })
                .collect(),
            None,
        ),
        Err(e) => (Vec::new(), Some(e.to_string())),
    };

    Ok(ProposalDetailDto { summary, resolved, introduced, files, error })
}

/// Accept or reject. Accepting writes files and reloads the world from disk, so the
/// map and timeline reflect the new canon immediately.
#[tauri::command]
pub fn decide_proposal(
    id: String,
    accept: bool,
    state: State<'_, AppState>,
) -> Result<WorldSummary, String> {
    let mut guard = state.world.lock().map_err(|_| "world state is poisoned".to_string())?;
    let root = guard.as_ref().ok_or_else(|| "no world is open".to_string())?.root.clone();

    let mut proposal = wb_propose::store::load_all(&root)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("no proposal `{id}`"))?;

    if !accept {
        wb_propose::reject(&mut proposal).map_err(|e| e.to_string())?;
        return Ok(WorldSummary::of(guard.as_ref().expect("world present")));
    }

    wb_propose::accept(guard.as_ref().expect("world present"), &mut proposal)
        .map_err(|e| e.to_string())?;

    let reloaded = load(&root).map_err(|e| e.to_string())?;
    let summary = WorldSummary::of(&reloaded);
    *guard = Some(reloaded);
    Ok(summary)
}

#[tauri::command]
pub fn open_world(path: String, state: State<'_, AppState>) -> Result<WorldSummary, String> {
    let world = load(PathBuf::from(&path)).map_err(|e| e.to_string())?;
    let summary = WorldSummary::of(&world);
    *state.world.lock().map_err(|_| "world state is poisoned".to_string())? = Some(world);
    Ok(summary)
}

/// Where the bundled example world lives, so a first run has something to open.
#[tauri::command]
pub fn example_world_path() -> Option<String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/vashen")
        .canonicalize()
        .ok()
        .map(|p| p.display().to_string())
}

#[tauri::command]
pub fn snapshot(day: i64, state: State<'_, AppState>) -> Result<SnapshotDto, String> {
    state.read(|world| SnapshotDto::of(world, Day(day)))
}

#[tauri::command]
pub fn timeline(state: State<'_, AppState>) -> Result<Vec<EventDto>, String> {
    state.read(|world| {
        let mut events: Vec<EventDto> = world
            .events
            .values()
            .map(|e| {
                let r = world.resolved_node(&e.id).unwrap_or_else(Resolved::unknown);
                EventDto {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    kind: e.kind.clone(),
                    nominal: r.nominal.map(|d| d.0),
                    earliest: r.earliest.map(|d| d.0),
                    latest: r.latest.map(|d| d.0),
                    label: r
                        .nominal
                        .map(|d| world.calendar.format_long(d))
                        .unwrap_or_else(|| "undated".to_string()),
                    participants: e.participants.clone(),
                    location: e.location.clone(),
                }
            })
            .collect();
        events.sort_by_key(|e| e.nominal.unwrap_or(i64::MAX));
        events
    })
}

/// Resolve a date the user typed — `0812-04`, `812~`, `@evt_siege_of_marrow+2y`.
#[tauri::command]
pub fn resolve_expr(expr: String, state: State<'_, AppState>) -> Result<Option<i64>, String> {
    state.read(|world| world.day_of(&expr).map(|day| day.map(|d| d.0)))?.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn format_day(day: i64, state: State<'_, AppState>) -> Result<String, String> {
    state.read(|world| world.calendar.format_long(Day(day)))
}
