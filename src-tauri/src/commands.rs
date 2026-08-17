//! The query surface the frontend scrubs against.
//!
//! Everything crossing the bridge is an owned DTO with day numbers as plain integers.
//! Rendering decisions the *world* owns — which polity holds a region, what colour that
//! is, whether the claim is settled — are resolved here rather than reassembled in the
//! UI, so there is one place that can be wrong about them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;
use wb_core::{Containment, Day, Resolved};
use wb_store::{Primitive, World, load};

#[derive(Default)]
pub struct AppState {
    world: Mutex<Option<Open>>,
}

/// The open world, plus the stamp of the tree it was loaded from.
struct Open {
    world: World,
    fingerprint: u64,
}

impl AppState {
    /// Run a query against the current world, reloading first if the files moved.
    ///
    /// The app used to load once and reload only when a proposal was accepted, which was
    /// merely stale while everything was read-only. Now that the app writes, an impact
    /// analysis run against a stale copy is a confident wrong answer shown immediately
    /// before a commit — so this checks, the way the MCP server has always had to.
    pub(crate) fn read<T>(&self, f: impl FnOnce(&World) -> T) -> Result<T, String> {
        let mut guard = self.world.lock().map_err(|_| "world state is poisoned".to_string())?;
        let open = guard.as_mut().ok_or_else(|| "no world is open".to_string())?;

        let current = wb_store::freshness::fingerprint(&open.world.root);
        if current != open.fingerprint {
            // A reload that fails is not fatal — a half-saved file in the writer's own
            // editor should not take the app down. The last good world keeps answering.
            if let Ok(world) = load(&open.world.root) {
                open.world = world;
            }
            open.fingerprint = current;
        }
        Ok(f(&open.world))
    }

    /// Act on the world's files, then reload from disk and swap the result in.
    ///
    /// The lock is held across the write and the reload, so no command can observe a
    /// world that disagrees with the disk. Deliberately not `write(|&mut World|)`: the
    /// world is *derived* from the files, and mutating it in memory and serializing
    /// afterwards would invert that.
    pub(crate) fn commit<T>(
        &self,
        f: impl FnOnce(&World) -> Result<T, String>,
    ) -> Result<(T, WorldSummary), String> {
        let mut guard = self.world.lock().map_err(|_| "world state is poisoned".to_string())?;
        let open = guard.as_mut().ok_or_else(|| "no world is open".to_string())?;

        let outcome = f(&open.world)?;

        let root = open.world.root.clone();
        let reloaded = load(&root).map_err(|e| {
            format!(
                "the change was written, but the world no longer loads: {e}\n\
                 Your files are on disk. Fix the error and reopen the world."
            )
        })?;
        let summary = WorldSummary::of(&reloaded);
        *open = Open { world: reloaded, fingerprint: wb_store::freshness::fingerprint(&root) };
        Ok((outcome, summary))
    }

    fn open(&self, world: World) -> Result<WorldSummary, String> {
        let mut guard = self.world.lock().map_err(|_| "world state is poisoned".to_string())?;
        let summary = WorldSummary::of(&world);
        let fingerprint = wb_store::freshness::fingerprint(&world.root);
        *guard = Some(Open { world, fingerprint });
        Ok(summary)
    }
}

fn certainty(c: Containment) -> &'static str {
    match c {
        Containment::Yes => "yes",
        Containment::Maybe => "maybe",
        Containment::No => "no",
    }
}

pub(crate) fn primitive_name(p: Primitive) -> &'static str {
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
    pub scene_count: usize,
    /// `unlinked` · `root_missing` · `linked`. What the story panel can honestly claim.
    pub manuscript: &'static str,
    /// Inclusive day range worth scrubbing, padded so the outermost events do not sit
    /// pinned against the ends of the track.
    pub span: [i64; 2],
    /// Every instant anything could change. The UI snaps to these and skips requerying
    /// between them.
    pub change_points: Vec<i64>,
    pub undeclared_types: Vec<String>,
    /// The type vocabulary, for the authoring form's type control. Open, not closed —
    /// an undeclared type still loads, so the control offers these and accepts others.
    pub types: Vec<TypeDto>,
    /// Every record in the world, entities, events and scenes together, because they
    /// share one id namespace.
    ///
    /// Deliberately not derived from a snapshot, which is filtered by date: checking a
    /// new id against that would let a writer create `place_marrow` on a day Marrow does
    /// not exist and only discover the clash when they pressed save.
    ///
    /// This replaced a bare list of ids. Every consumer wanted more than the id and had
    /// nowhere to get it: the reference boxes offered `act_aldric_vane` with no way to
    /// know that was Aldric, and there was no way at all to go to a record by the name
    /// the writer thinks of it by.
    pub records: Vec<RecordDto>,
    /// Every attribute any fact in this world asserts, most-used first.
    ///
    /// The world's own vocabulary, offered rather than enforced — the same stance
    /// [`Self::types`] takes, and for the same reason: a new attribute is an ordinary
    /// thing to write, and a form that only accepted the existing ones would be stricter
    /// than the data model underneath it.
    pub attrs: Vec<AttrDto>,
}

#[derive(Serialize)]
pub struct TypeDto {
    pub name: String,
    pub primitive: &'static str,
}

/// One record, named well enough to find it and to say what it is.
#[derive(Serialize)]
pub struct RecordDto {
    pub id: String,
    pub name: String,
    /// `entity` · `event` · `scene`. Settled here because the whole world is in front of
    /// us; an id alone cannot answer it.
    pub kind: &'static str,
    /// An entity's declared type or an event's kind. A scene has neither and sends "".
    #[serde(rename = "type")]
    pub type_name: String,
    /// What the prose calls it. Carried because a writer hunting for Aldric is at least
    /// as likely to type "the duke", and that is exactly the string the manuscript
    /// scanner already matches on.
    pub aka: Vec<String>,
}

#[derive(Serialize)]
pub struct AttrDto {
    pub name: String,
    /// How many records assert it. The order this list is sorted by, and the reason a
    /// world's real vocabulary rises above the one-off somebody typed once.
    pub count: usize,
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
            scene_count: world.scenes.len(),
            manuscript: match &world.manuscript {
                None => "unlinked",
                Some(m) if world.root.join(&m.root).is_dir() => "linked",
                Some(_) => "root_missing",
            },
            span: [lo - pad, hi + pad],
            change_points,
            undeclared_types: world.undeclared_types().keys().cloned().collect(),
            types: world
                .types
                .values()
                .map(|t| TypeDto { name: t.name.clone(), primitive: primitive_name(t.primitive) })
                .collect(),
            records: records_of(world),
            attrs: attrs_of(world),
        }
    }
}

fn records_of(world: &World) -> Vec<RecordDto> {
    let entities = world.entities.values().map(|e| RecordDto {
        id: e.id.clone(),
        name: e.name.clone(),
        kind: "entity",
        type_name: e.type_name.clone(),
        aka: e.aliases.clone(),
    });
    let events = world.events.values().map(|e| RecordDto {
        id: e.id.clone(),
        name: e.name.clone(),
        kind: "event",
        type_name: e.kind.clone(),
        aka: Vec::new(),
    });
    let scenes = world.scenes.values().map(|s| RecordDto {
        id: s.id.clone(),
        name: s.name.clone(),
        kind: "scene",
        type_name: String::new(),
        aka: Vec::new(),
    });
    entities.chain(events).chain(scenes).collect()
}

/// Counted by record rather than by fact, so an attribute split across six windows on one
/// entity — which is the shape every changing number takes here — does not outrank one
/// that half the world asserts once.
fn attrs_of(world: &World) -> Vec<AttrDto> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for entity in world.entities.values() {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for fact in &entity.facts {
            if seen.insert(fact.attr.as_str()) {
                *counts.entry(fact.attr.as_str()).or_default() += 1;
            }
        }
    }
    let mut attrs: Vec<AttrDto> =
        counts.into_iter().map(|(name, count)| AttrDto { name: name.to_string(), count }).collect();
    // Most-used first, then alphabetically, so the order is stable across loads and a
    // world's real vocabulary is what the writer sees before anything they typed once.
    attrs.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    attrs
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
    // `wb_story::check`, so the header count includes contradictions found in the prose.
    // The alternative is a Findings panel that says a world is clean while the story
    // panel shows a chapter naming somebody who was dead at the time.
    state.read(|world| wb_story::check(world).findings.iter().map(FindingDto::of).collect())
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
pub(crate) fn changed_lines(before: &str, after: &str) -> Vec<DiffLine> {
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
    state.read(|world| {
        let proposals = wb_propose::store::load_all(&world.root).map_err(|e| e.to_string())?;
        Ok(proposals.iter().map(|p| summarize(world, p)).collect())
    })?
}

#[tauri::command]
pub fn proposal_detail(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProposalDetailDto, String> {
    state.read(|world| detail_of(world, &id))?
}

fn detail_of(world: &World, id: &str) -> Result<ProposalDetailDto, String> {
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
    // Rejecting only rewrites the proposal's own status, so it needs no reload — but it
    // goes through `commit` anyway, because the queue's impact figures are computed
    // against the current world and a decision is exactly when they should be refreshed.
    state
        .commit(|world| {
            let mut proposal = wb_propose::store::load_all(&world.root)
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("no proposal `{id}`"))?;

            if accept {
                wb_propose::accept(world, &mut proposal).map_err(|e| e.to_string())?;
            } else {
                wb_propose::reject(&mut proposal).map_err(|e| e.to_string())?;
            }
            Ok(())
        })
        .map(|(_, summary)| summary)
}

#[tauri::command]
pub fn open_world(
    path: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<WorldSummary, String> {
    let root = PathBuf::from(path.trim());
    // Checked before the load, so a mistyped path says what is actually wrong instead of
    // surfacing whatever the loader tripped over three folders down.
    if !root.join("world.yaml").is_file() {
        return Err(format!("there is no `world.yaml` in {}", root.display()));
    }

    let world = load(&root).map_err(|e| e.to_string())?;
    let summary = state.open(world)?;
    remember(&app, &root);
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

/// What to open on launch: the last world that is still there, else the example.
///
/// A tool whose files are the writer's own should reopen the writer's own files. Until
/// this slice the app could only ever open the world it shipped with, which made every
/// other feature in it a demo.
#[tauri::command]
pub fn initial_world(app: tauri::AppHandle) -> Option<String> {
    recent_worlds(app).into_iter().next().or_else(example_world_path)
}

/// The last few world folders opened, newest first, skipping any that have moved.
///
/// Paths and nothing else. A recent list that cached names or counts would be a second
/// copy of the world's own truth, going stale in a config directory.
#[tauri::command]
pub fn recent_worlds(app: tauri::AppHandle) -> Vec<String> {
    let Some(file) = recent_file(&app) else { return Vec::new() };
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    serde_json::from_str::<Vec<String>>(&text)
        .unwrap_or_default()
        .into_iter()
        .filter(|p| Path::new(p).join("world.yaml").is_file())
        .take(RECENT)
        .collect()
}

const RECENT: usize = 8;

fn recent_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("recent.json"))
}

/// Failing to remember a world is never worth failing an open over — the world is loaded
/// and the writer is looking at it.
fn remember(app: &tauri::AppHandle, root: &Path) {
    let Some(file) = recent_file(app) else { return };
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let canonical = canonical.display().to_string();

    let mut list: Vec<String> = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    list.retain(|p| *p != canonical);
    list.insert(0, canonical);
    list.truncate(RECENT);

    if let Ok(text) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(file, text);
    }
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
