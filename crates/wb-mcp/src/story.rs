//! The manuscript, shaped for an agent.
//!
//! Until this existed, `chapter-canon-check` had to tell an agent to go find the prose
//! on the filesystem itself, and `iceberg-check` shipped with a written apology for
//! measuring internal connectedness instead of what reaches the page. Both are now
//! answerable through the server, which means they are answerable by an agent that has
//! nothing but this server attached.
//!
//! Read-only, like everything else here except the proposal queue — and more strictly
//! so: there is no write path to the manuscript anywhere in the workspace to expose.

use schemars::JsonSchema;
use serde::Serialize;
use wb_store::World;
use wb_story::{Story, iceberg};

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SceneOut {
    pub id: String,
    pub name: String,
    /// As the writer typed it, e.g. `0812-04~` or `@evt_siege_of_marrow`.
    pub date: String,
    /// Resolved to a day, when it resolves at all.
    pub nominal: Option<i64>,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pov: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub on_page: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Pass this to `read_scene`. Absent means the scene is not linked to prose yet,
    /// which is a normal state and not a fault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prose: Option<String>,
    /// Position in the book, counting from one. Derived from the manuscript itself, so
    /// it is reading order and may disagree with the dates — a flashback is exactly that
    /// disagreement, and it is not a mistake.
    pub reading_order: usize,
    /// How many records the prose of this scene names.
    pub names_records: usize,
    /// Why the prose could not be read, when it could not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unreadable: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PassageOut {
    pub scene: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    pub text: String,
    pub words: usize,
    pub truncated: bool,
    /// Every record this passage names, so the world can be lined up against it without
    /// a second pass and without guessing at the names.
    pub names: Vec<MentionOut>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MentionOut {
    pub id: String,
    pub name: String,
    pub times: usize,
    /// One sentence it appears in. A count nobody can check is a count nobody should act
    /// on, and this is what makes it checkable.
    pub first_seen: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct IcebergOut {
    /// `unlinked` — no manuscript declared, and that is fine. `root_missing` — declared
    /// and not found. `linked` — read.
    pub standing: String,
    pub scenes_read: usize,
    pub surfaced: usize,
    pub total: usize,
    /// Percentage of records the prose names, or `null` for an empty world.
    pub surfaced_percent: Option<u32>,
    pub records: Vec<SurfacingOut>,
    /// Scenes whose link went nowhere, and why.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unreadable: Vec<UnreadableOut>,
    /// What this measurement does and does not mean, carried on the payload so the
    /// caveat travels with the number rather than living only in a skill.
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SurfacingOut {
    pub id: String,
    pub name: String,
    /// `underbuilt` · `load-bearing` · `overbuilt` · `quiet`. Underbuilt comes first.
    pub standing: String,
    pub mentions: usize,
    pub scenes: Vec<String>,
    pub referenced_by: usize,
    pub appears_in: usize,
    pub cast_in: usize,
    pub facts: usize,
    pub prose_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UnreadableOut {
    pub scene: String,
    pub reason: String,
}

const NOTE: &str = "Surfacing counts a record's name, its declared `aka` spellings, and \
[[wikilinks]] — matched on whole words. A character the prose only ever calls by a first \
name will read as absent until that spelling is added to their `aka`, so a low ratio can \
mean the world is not on the page or that the world has not been told what the page calls \
it. Check `first_seen` before drawing a conclusion. Nothing here is waste: the submerged \
part of a world is what makes the visible part feel solid.";

pub fn list(world: &World, story: &Story) -> Vec<SceneOut> {
    let mut out: Vec<SceneOut> = story
        .reads
        .iter()
        .filter_map(|read| {
            let scene = world.scenes.get(&read.scene)?;
            let resolved = world.resolved_node(&scene.id);
            let nominal = resolved.and_then(|r| r.nominal);

            Some(SceneOut {
                id: scene.id.clone(),
                name: scene.name.clone(),
                date: scene.date.to_string(),
                nominal: nominal.map(|d| d.0),
                label: nominal
                    .map(|d| world.calendar.format_long(d))
                    .unwrap_or_else(|| "undated".into()),
                pov: scene.pov.clone(),
                on_page: scene.on_page.clone(),
                location: scene.location.clone(),
                prose: scene.prose.clone(),
                reading_order: story.order.get(&scene.id).map_or(0, |i| i + 1),
                names_records: wb_story::mentions::distinct(&read.mentions).len(),
                unreadable: read.passage.as_ref().err().cloned(),
            })
        })
        .collect();

    out.sort_by_key(|s| s.reading_order);
    out
}

pub fn read(world: &World, story: &Story, scene_id: &str) -> Result<PassageOut, String> {
    let read = story
        .get(scene_id)
        .ok_or_else(|| format!("no scene `{scene_id}`. Use `list_scenes` to see what is there."))?;
    let passage = read.passage.as_ref().map_err(Clone::clone)?;

    let mut tally: std::collections::BTreeMap<&str, (usize, Option<String>)> = Default::default();
    for mention in &read.mentions {
        let entry = tally.entry(&mention.id).or_insert((0, None));
        entry.0 += 1;
        if entry.1.is_none() {
            entry.1 = Some(wb_story::mentions::excerpt(&passage.text, mention.at, mention.len));
        }
    }

    let mut names: Vec<MentionOut> = tally
        .into_iter()
        .map(|(id, (times, first_seen))| MentionOut {
            name: world.entities.get(id).map_or_else(|| id.to_string(), |e| e.name.clone()),
            id: id.to_string(),
            times,
            first_seen: first_seen.unwrap_or_default(),
        })
        .collect();
    names.sort_by(|a, b| b.times.cmp(&a.times).then_with(|| a.name.cmp(&b.name)));

    Ok(PassageOut {
        scene: scene_id.to_string(),
        file: passage.file.clone(),
        heading: passage.heading.clone(),
        text: passage.text.clone(),
        words: passage.words,
        truncated: passage.truncated,
        names,
    })
}

pub fn report(world: &World, story: &Story) -> IcebergOut {
    let report = iceberg::report(world, story);

    IcebergOut {
        standing: match report.standing {
            wb_story::Standing::Unlinked => "unlinked",
            wb_story::Standing::RootMissing => "root_missing",
            wb_story::Standing::Linked => "linked",
        }
        .to_string(),
        scenes_read: report.scenes_read,
        surfaced: report.surfaced,
        total: report.total,
        surfaced_percent: report.ratio(),
        records: report
            .entries
            .iter()
            .map(|e| SurfacingOut {
                id: e.id.clone(),
                name: e.name.clone(),
                standing: e.quadrant.slug().to_string(),
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
            .map(|(scene, reason)| UnreadableOut { scene: scene.clone(), reason: reason.clone() })
            .collect(),
        note: NOTE,
    }
}
