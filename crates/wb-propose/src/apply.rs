//! Simulating a proposal, rendering the files it would write, and writing them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use wb_store::write::Fidelity;
use wb_store::{Entity, Event, Fact, World};

use crate::error::{Error, Result};
use crate::model::{Change, Proposal};

/// Fields the model round-trips. Anything else in a file is the writer's, and refusing
/// to drop it is the whole point of [`Error::WouldDropKey`].
const ENTITY_KEYS: [&str; 8] =
    ["id", "name", "type", "existence", "parents", "facts", "marker", "shape"];
const EVENT_KEYS: [&str; 6] = ["id", "name", "kind", "date", "participants", "location"];

/// A file the proposal would write, with its current contents for diffing.
#[derive(Debug, Clone)]
pub struct FileEdit {
    pub path: PathBuf,
    pub before: Option<String>,
    pub after: String,
    /// How much of the writer's own formatting this render keeps. Carried out to the
    /// review UI, because a reviewer deciding whether to accept should know when the
    /// answer is "this reformats your file" before they find out from the diff.
    pub fidelity: Fidelity,
}

impl FileEdit {
    pub fn is_new(&self) -> bool {
        self.before.is_none()
    }

    /// True when accepting would rewrite the file rather than patch it.
    pub fn reformats(&self) -> bool {
        !self.fidelity.preserves_bytes()
    }

    pub fn changes_anything(&self) -> bool {
        self.before.as_deref() != Some(self.after.as_str())
    }
}

/// The world as it would be if this proposal were accepted. Nothing touches disk.
pub fn simulate(world: &World, proposal: &Proposal) -> Result<World> {
    let mut entities: BTreeMap<String, Entity> = world.entities.clone();
    let mut events: BTreeMap<String, Event> = world.events.clone();

    apply_changes(&proposal.id, &mut entities, &mut events, &proposal.changes)?;

    Ok(World::assemble(
        world.root.clone(),
        world.definition(),
        entities.into_values().collect(),
        events.into_values().collect(),
    )?)
}

fn apply_changes(
    proposal: &str,
    entities: &mut BTreeMap<String, Entity>,
    events: &mut BTreeMap<String, Event>,
    changes: &[Change],
) -> Result<()> {
    let unknown = |id: &str| Error::UnknownTarget { proposal: proposal.into(), id: id.into() };

    for change in changes {
        match change {
            Change::CreateEntity { id, name, type_name, existence, parents, facts } => {
                if entities.contains_key(id) || events.contains_key(id) {
                    return Err(Error::AlreadyExists { proposal: proposal.into(), id: id.clone() });
                }
                entities.insert(
                    id.clone(),
                    Entity {
                        id: id.clone(),
                        name: name.clone(),
                        type_name: type_name.clone(),
                        existence: existence.clone(),
                        parents: parents.clone(),
                        facts: facts.clone(),
                        marker: None,
                        shape: Vec::new(),
                        body: String::new(),
                        source: PathBuf::new(),
                    },
                );
            }

            Change::CreateEvent { id, name, kind, date, participants, location } => {
                if entities.contains_key(id) || events.contains_key(id) {
                    return Err(Error::AlreadyExists { proposal: proposal.into(), id: id.clone() });
                }
                events.insert(
                    id.clone(),
                    Event {
                        id: id.clone(),
                        name: name.clone(),
                        kind: kind.clone(),
                        date: date.clone(),
                        participants: participants.clone(),
                        location: location.clone(),
                        body: String::new(),
                        source: PathBuf::new(),
                    },
                );
            }

            Change::AddFact { entity, attr, value, from, to } => {
                let target = entities.get_mut(entity).ok_or_else(|| unknown(entity))?;
                target.facts.push(Fact {
                    attr: attr.clone(),
                    value: value.clone(),
                    from: from.clone(),
                    to: to.clone(),
                });
            }

            Change::RemoveFact { entity, attr, value } => {
                let target = entities.get_mut(entity).ok_or_else(|| unknown(entity))?;
                let before = target.facts.len();
                target.facts.retain(|f| !(f.attr == *attr && f.value == *value));
                if target.facts.len() == before {
                    return Err(Error::NoSuchFact {
                        proposal: proposal.into(),
                        id: entity.clone(),
                        attr: attr.clone(),
                        value: value.to_string(),
                    });
                }
            }

            Change::SetExistence { entity, from, to } => {
                let target = entities.get_mut(entity).ok_or_else(|| unknown(entity))?;
                let mut span = target.existence.clone().unwrap_or_default();
                if let Some(from) = from {
                    span.from = from.clone();
                }
                if let Some(to) = to {
                    span.to = to.clone();
                }
                target.existence = Some(span);
            }

            Change::SetEventDate { event, date } => {
                let target = events.get_mut(event).ok_or_else(|| unknown(event))?;
                target.date = date.clone();
            }
        }
    }
    Ok(())
}

/// The files this proposal would write, rendered but not saved.
pub fn preview(world: &World, proposal: &Proposal) -> Result<Vec<FileEdit>> {
    let after = simulate(world, proposal)?;
    let touched: BTreeSet<&str> = proposal.changes.iter().map(|c| c.target()).collect();

    let mut edits = Vec::new();
    for id in touched {
        // The writer patches in place where it can, so a key this version does not model
        // survives instead of blocking the write. `guard_unknown_keys` still stands
        // behind the canonical fallback, which is the only path that could drop one.
        if let Some(entity) = after.entities.get(id) {
            let path = entity_path(&after, entity);
            let before = read_if_present(&path)?;
            let out = wb_store::write::render_entity(&path, before.as_deref(), entity)?;
            if !out.fidelity.preserves_bytes() {
                guard_unknown_keys(&path, before.as_deref(), &ENTITY_KEYS, true)?;
            }
            edits.push(FileEdit { after: out.text, fidelity: out.fidelity, path, before });
        } else if let Some(event) = after.events.get(id) {
            let path = event_path(&after, event);
            let before = read_if_present(&path)?;
            let out = wb_store::write::render_event(&path, before.as_deref(), event)?;
            if !out.fidelity.preserves_bytes() {
                guard_unknown_keys(&path, before.as_deref(), &EVENT_KEYS, false)?;
            }
            edits.push(FileEdit { after: out.text, fidelity: out.fidelity, path, before });
        }
    }

    edits.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(edits)
}

/// Write the proposal's files and mark it accepted. Returns what was written.
pub fn accept(world: &World, proposal: &Proposal) -> Result<Vec<PathBuf>> {
    if !proposal.is_pending() {
        return Err(Error::NotPending {
            proposal: proposal.id.clone(),
            status: proposal.status.slug(),
        });
    }

    // Render everything first: a proposal that cannot be written in full is not
    // written in part.
    let edits = preview(world, proposal)?;

    let mut written = Vec::new();
    for edit in edits.iter().filter(|e| e.changes_anything()) {
        // Through a temp file and a rename: the MCP server re-reads this tree on every
        // call, so a torn write is a file another process will genuinely try to parse.
        wb_store::atomic::write(&edit.path, &edit.after)?;
        written.push(edit.path.clone());
    }
    Ok(written)
}

// ---------------------------------------------------------------- rendering

fn read_if_present(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|source| Error::Io { path: path.to_path_buf(), source })
}

/// Refuse to rewrite a file carrying keys this version would silently discard.
fn guard_unknown_keys(
    path: &Path,
    before: Option<&str>,
    known: &[&str],
    markdown: bool,
) -> Result<()> {
    let Some(text) = before else { return Ok(()) };
    let yaml = if markdown {
        match wb_store::frontmatter::split(text) {
            Some(doc) => doc.frontmatter.to_string(),
            None => return Ok(()),
        }
    } else {
        text.to_string()
    };

    let parsed: serde_yaml_bw::Value = serde_yaml_bw::from_str(&yaml)
        .map_err(|e| Error::Yaml { path: path.to_path_buf(), message: e.to_string() })?;
    let Some(mapping) = parsed.as_mapping() else { return Ok(()) };

    for key in mapping.keys().filter_map(|k| k.as_str()) {
        if !known.contains(&key) {
            return Err(Error::WouldDropKey { path: path.to_path_buf(), key: key.to_string() });
        }
    }
    Ok(())
}

// Rendering and path derivation both moved to `wb-store` in the authoring slice. The
// queue and the app's own writes have to produce the same bytes for the same record and
// put them in the same place, and two implementations of that would eventually disagree.
use wb_store::paths::{entity_path, event_path};
