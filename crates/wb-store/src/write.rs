//! Writing a record back without disturbing the parts of the file nobody changed.
//!
//! The obvious implementation — deserialize, mutate, re-serialize — is what this replaces.
//! It works, and it costs the writer every comment they put in their own frontmatter, the
//! inline style they chose, and a readable diff. This tool holds people's life's work; a
//! one-word edit should read as a one-word edit.
//!
//! # The shape of it
//!
//! ```text
//! original bytes ──▶ gate ──▶ locate ──▶ diff the model ──▶ splice ──▶ verify ──┬─▶ keep
//!                     │                                                          │
//!                     └──────────────── any doubt ───────────────────────────────┴─▶ canonical
//! ```
//!
//! Every arrow pointing away from the happy path lands in the same place: a canonical
//! rewrite, reported as such. That is exactly today's behaviour, so the worst case of all
//! this machinery is the behaviour it was built to improve on — never a damaged file.
//!
//! # What survives, precisely
//!
//! Comments above a key, comments trailing a key on its own line, comments on facts you
//! did not touch, keys this version of the model has never heard of, inline flow style,
//! blank lines, line endings, a byte order mark, and the prose body.
//!
//! What does not: a comment *inside the exact value you changed*. Those bytes are the
//! ones being replaced, and there is nowhere honest to put the comment back.
//!
//! # Why it is safe to hand-roll the locating
//!
//! [`crate::yaml::scan`] is a line walker, not a parser, and libyaml is kept alongside it
//! as an oracle: the parse says *what* keys exist and in what order, the scanner says
//! *where* they are, and the two must agree before a single byte is spliced. A scanner
//! bug becomes a wide diff instead of a wrong file.

use std::ops::Range;
use std::path::Path;

use wb_core::DateExpr;

use crate::error::{Error, Result};
use crate::frontmatter;
use crate::model::{Entity, Event, Fact, Scene};
use crate::yaml::emit;
use crate::yaml::scan::{self, Entry, Style};

/// Frontmatter keys the entity model understands, in the order a fresh file writes them.
pub const ENTITY_KEYS: [&str; 9] =
    ["id", "name", "aka", "type", "existence", "parents", "facts", "marker", "shape"];
pub const EVENT_KEYS: [&str; 6] = ["id", "name", "kind", "date", "participants", "location"];
pub const SCENE_KEYS: [&str; 7] = ["id", "name", "date", "pov", "on_page", "location", "prose"];

/// How much of the writer's original text a render managed to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fidelity {
    /// Every byte not semantically changed is byte-identical to the original.
    Preserved,
    /// The file was rewritten canonically. Says why, and which comments that costs.
    Reformatted { reason: String, comments_lost: Vec<String> },
    /// There was no original: a new file.
    Created,
}

impl Fidelity {
    pub fn preserves_bytes(&self) -> bool {
        !matches!(self, Self::Reformatted { .. })
    }
}

#[derive(Debug, Clone)]
pub struct Rendered {
    pub text: String,
    pub fidelity: Fidelity,
}

/// The bytes to write for `desired`, given whatever is on disk now.
///
/// `original` is the whole file — BOM, fences, prose and all. Note there is no `before`
/// parameter: the writer parses the previous state out of `original` itself. Taking it
/// from an in-memory world instead would mean that a field someone changed in their own
/// editor, and which this draft never touched, would be silently reverted.
pub fn render_entity(path: &Path, original: Option<&str>, desired: &Entity) -> Result<Rendered> {
    let Some(original) = original else {
        return Ok(Rendered { text: fresh_entity(path, desired)?, fidelity: Fidelity::Created });
    };

    match patch_entity(path, original, desired) {
        Ok(text) => Ok(Rendered { text, fidelity: Fidelity::Preserved }),
        Err(reason) => Ok(Rendered {
            text: canonical_entity(path, desired)?,
            fidelity: Fidelity::Reformatted { reason, comments_lost: comments_in(original) },
        }),
    }
}

pub fn render_event(path: &Path, original: Option<&str>, desired: &Event) -> Result<Rendered> {
    let Some(original) = original else {
        return Ok(Rendered { text: fresh_event(path, desired)?, fidelity: Fidelity::Created });
    };

    match patch_event(path, original, desired) {
        Ok(text) => Ok(Rendered { text, fidelity: Fidelity::Preserved }),
        Err(reason) => Ok(Rendered {
            text: canonical_event(path, desired)?,
            fidelity: Fidelity::Reformatted { reason, comments_lost: comments_in(original) },
        }),
    }
}

pub fn render_scene(path: &Path, original: Option<&str>, desired: &Scene) -> Result<Rendered> {
    let Some(original) = original else {
        return Ok(Rendered { text: fresh_scene(path, desired)?, fidelity: Fidelity::Created });
    };

    match patch_scene(path, original, desired) {
        Ok(text) => Ok(Rendered { text, fidelity: Fidelity::Preserved }),
        Err(reason) => Ok(Rendered {
            text: canonical_scene(path, desired)?,
            fidelity: Fidelity::Reformatted { reason, comments_lost: comments_in(original) },
        }),
    }
}

// ---------------------------------------------------------------- the patch

fn patch_entity(
    path: &Path,
    original: &str,
    desired: &Entity,
) -> std::result::Result<String, String> {
    let region = region_of(path, original)?;
    let yaml = &original[region.start..region.end];
    let mapping = gate(yaml)?;

    let before: Entity = serde_yaml_bw::from_str(yaml)
        .map_err(|e| format!("the file no longer parses as a record: {e}"))?;

    let entries = locate(original, region.clone(), &mapping)?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    for key in ENTITY_KEYS {
        let existing = entries.iter().find(|e| e.key == key);
        if key == "facts" {
            patch_facts(original, existing, &before.facts, &desired.facts, &entries, &mut edits)?;
            continue;
        }
        // Compare the *model*, never the rendering. `marker: [0.43, 0.40]` and
        // `[0.43, 0.4]` are the same point, and a writer who did not touch their marker
        // should not find it reformatted because this code spells floats differently.
        if !entity_differs(key, &before, desired) {
            continue;
        }
        let field = entity_field(key, desired, existing);
        plan(original, key, field, existing, &entries, &ENTITY_KEYS, 0, &mut edits)?;
    }

    let candidate = splice(original, &mut edits)?;
    let mut parsed: Entity = parse_back(path, &candidate)?;
    parsed.body = desired.body.clone();
    parsed.source = desired.source.clone();
    if parsed != *desired {
        return Err("the patched file did not reparse to the intended record".into());
    }
    check_comments(original, &candidate, &edits)?;
    Ok(candidate)
}

fn patch_event(
    path: &Path,
    original: &str,
    desired: &Event,
) -> std::result::Result<String, String> {
    let region = region_of(path, original)?;
    let yaml = &original[region.start..region.end];
    let mapping = gate(yaml)?;

    let before: Event = serde_yaml_bw::from_str(yaml)
        .map_err(|e| format!("the file no longer parses as an event: {e}"))?;

    let entries = locate(original, region.clone(), &mapping)?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    for key in EVENT_KEYS {
        let existing = entries.iter().find(|e| e.key == key);
        if !event_differs(key, &before, desired) {
            continue;
        }
        let field = event_field(key, desired, existing);
        plan(original, key, field, existing, &entries, &EVENT_KEYS, 0, &mut edits)?;
    }

    let candidate = splice(original, &mut edits)?;
    let mut parsed: Event = parse_back(path, &candidate)?;
    parsed.body = desired.body.clone();
    parsed.source = desired.source.clone();
    if parsed != *desired {
        return Err("the patched file did not reparse to the intended record".into());
    }
    check_comments(original, &candidate, &edits)?;
    Ok(candidate)
}

fn patch_scene(
    path: &Path,
    original: &str,
    desired: &Scene,
) -> std::result::Result<String, String> {
    let region = region_of(path, original)?;
    let yaml = &original[region.start..region.end];
    let mapping = gate(yaml)?;

    let before: Scene = serde_yaml_bw::from_str(yaml)
        .map_err(|e| format!("the file no longer parses as a scene: {e}"))?;

    let entries = locate(original, region.clone(), &mapping)?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    for key in SCENE_KEYS {
        let existing = entries.iter().find(|e| e.key == key);
        if !scene_differs(key, &before, desired) {
            continue;
        }
        let field = scene_field(key, desired, existing);
        plan(original, key, field, existing, &entries, &SCENE_KEYS, 0, &mut edits)?;
    }

    let candidate = splice(original, &mut edits)?;
    let mut parsed: Scene = parse_back(path, &candidate)?;
    parsed.source = desired.source.clone();
    if parsed != *desired {
        return Err("the patched file did not reparse to the intended scene".into());
    }
    check_comments(original, &candidate, &edits)?;
    Ok(candidate)
}

/// What a key's value should become. `Absent` removes the key entirely.
enum Field {
    Absent,
    /// Inline text, going on the key's own line.
    Inline(String),
    /// Whole lines, going underneath the key.
    Block(String),
}

/// Has this field actually changed? Anything answering `false` is never re-rendered, so
/// however the writer chose to spell it stands.
fn entity_differs(key: &str, before: &Entity, after: &Entity) -> bool {
    match key {
        "id" => before.id != after.id,
        "name" => before.name != after.name,
        "aka" => before.aliases != after.aliases,
        "type" => before.type_name != after.type_name,
        "existence" => before.existence != after.existence,
        "parents" => before.parents != after.parents,
        "facts" => before.facts != after.facts,
        "marker" => before.marker != after.marker,
        "shape" => before.shape != after.shape,
        _ => false,
    }
}

fn event_differs(key: &str, before: &Event, after: &Event) -> bool {
    match key {
        "id" => before.id != after.id,
        "name" => before.name != after.name,
        "kind" => before.kind != after.kind,
        "date" => before.date != after.date,
        "participants" => before.participants != after.participants,
        "location" => before.location != after.location,
        _ => false,
    }
}

/// Every key in [`SCENE_KEYS`] must appear here and in [`scene_field`]. A key missing from
/// this one is never written; a key missing from that one is *deleted*, because `plan`
/// reads `(Some(entry), Field::Absent)` as a removal. `every_scene_key_is_wired_end_to_end`
/// is what keeps the three lists honest.
fn scene_differs(key: &str, before: &Scene, after: &Scene) -> bool {
    match key {
        "id" => before.id != after.id,
        "name" => before.name != after.name,
        "date" => before.date != after.date,
        "pov" => before.pov != after.pov,
        "on_page" => before.on_page != after.on_page,
        "location" => before.location != after.location,
        "prose" => before.prose != after.prose,
        _ => false,
    }
}

fn scene_field(key: &str, s: &Scene, existing: Option<&Entry>) -> Field {
    let flow = existing.map(|x| x.style == Style::Flow);
    match key {
        "id" => Field::Inline(emit::scalar(&s.id)),
        "name" => Field::Inline(emit::scalar(&s.name)),
        "date" => Field::Inline(emit::date(&s.date)),
        "pov" => match &s.pov {
            None => Field::Absent,
            Some(id) => Field::Inline(emit::scalar(id)),
        },
        "on_page" if s.on_page.is_empty() => Field::Absent,
        "on_page" if flow.unwrap_or(true) => Field::Inline(emit::ids(&s.on_page, Style::Flow, 0)),
        "on_page" => Field::Block(emit::ids(&s.on_page, Style::Block, 0)),
        "location" => match &s.location {
            None => Field::Absent,
            Some(id) => Field::Inline(emit::scalar(id)),
        },
        // Left bare: the `#` in `ch12.md#the-breach` only opens a comment when a space
        // precedes it, and quoting every link would make the common case look escaped.
        "prose" => match &s.prose {
            None => Field::Absent,
            Some(link) => Field::Inline(emit::scalar(link)),
        },
        _ => Field::Absent,
    }
}

fn entity_field(key: &str, e: &Entity, existing: Option<&Entry>) -> Field {
    let flow = existing.map(|x| x.style == Style::Flow);
    match key {
        "id" => Field::Inline(emit::scalar(&e.id)),
        "name" => Field::Inline(emit::scalar(&e.name)),
        "aka" if e.aliases.is_empty() => Field::Absent,
        "aka" if flow.unwrap_or(true) => Field::Inline(emit::ids(&e.aliases, Style::Flow, 0)),
        "aka" => Field::Block(emit::ids(&e.aliases, Style::Block, 0)),
        "type" => Field::Inline(emit::scalar(&e.type_name)),
        "existence" => match &e.existence {
            None => Field::Absent,
            // A span is written inline unless the file already spelled it out.
            Some(s) if flow.unwrap_or(true) => Field::Inline(emit::span(s, Style::Flow, 0)),
            Some(s) => Field::Block(emit::span(s, Style::Block, 0)),
        },
        "parents" if e.parents.is_empty() => Field::Absent,
        "parents" if flow.unwrap_or(true) => Field::Inline(emit::ids(&e.parents, Style::Flow, 0)),
        "parents" => Field::Block(emit::ids(&e.parents, Style::Block, 0)),
        "marker" => match e.marker {
            None => Field::Absent,
            Some(p) => Field::Inline(emit::point(p)),
        },
        "shape" if e.shape.is_empty() => Field::Absent,
        // A polygon goes one vertex to a line by default: a shape is read down a column,
        // and a git diff of a moved vertex should be a moved vertex.
        "shape" if flow.unwrap_or(false) => Field::Inline(emit::points(&e.shape, Style::Flow, 0)),
        "shape" => Field::Block(emit::points(&e.shape, Style::Block, 0)),
        _ => Field::Absent,
    }
}

fn event_field(key: &str, e: &Event, existing: Option<&Entry>) -> Field {
    let flow = existing.map(|x| x.style == Style::Flow);
    match key {
        "id" => Field::Inline(emit::scalar(&e.id)),
        "name" => Field::Inline(emit::scalar(&e.name)),
        "kind" if e.kind.is_empty() => Field::Absent,
        "kind" => Field::Inline(emit::scalar(&e.kind)),
        "date" => Field::Inline(emit::date(&e.date)),
        "participants" if e.participants.is_empty() => Field::Absent,
        "participants" if flow.unwrap_or(true) => {
            Field::Inline(emit::ids(&e.participants, Style::Flow, 0))
        }
        "participants" => Field::Block(emit::ids(&e.participants, Style::Block, 0)),
        "location" => match &e.location {
            None => Field::Absent,
            Some(id) => Field::Inline(emit::scalar(id)),
        },
        _ => Field::Absent,
    }
}

/// Turn one key's desired state into an edit, or into nothing at all.
#[allow(clippy::too_many_arguments)]
fn plan(
    text: &str,
    key: &str,
    field: Field,
    existing: Option<&Entry>,
    entries: &[Entry],
    order: &[&str],
    indent: usize,
    edits: &mut Vec<(Range<usize>, String)>,
) -> std::result::Result<(), String> {
    let pad = " ".repeat(indent);
    match (existing, field) {
        (None, Field::Absent) => {}
        (Some(e), Field::Absent) => edits.push((e.entry.clone(), String::new())),
        (Some(e), Field::Inline(v)) => {
            if inline_value(text, e) {
                if text[e.value.start..e.value.end] != v {
                    edits.push((e.value.clone(), v));
                }
            } else {
                // It was spelled out over several lines and now fits on one.
                edits.push((e.entry.clone(), format!("{pad}{key}: {v}\n")));
            }
        }
        (Some(e), Field::Block(v)) => {
            if inline_value(text, e) {
                edits.push((e.entry.clone(), format!("{pad}{key}:\n{v}")));
            } else if text[e.value.start..e.value.end] != v {
                edits.push((e.value.clone(), v));
            }
        }
        (None, Field::Inline(v)) => {
            edits.push((insertion_point(entries, order, key, indent), format!("{pad}{key}: {v}\n")))
        }
        (None, Field::Block(v)) => {
            edits.push((insertion_point(entries, order, key, indent), format!("{pad}{key}:\n{v}")))
        }
    }
    Ok(())
}

/// Is the value on the same line as its key?
fn inline_value(text: &str, e: &Entry) -> bool {
    !text[e.entry.start..e.value.start].contains('\n')
}

/// Where a key the file does not yet have should go: after the last key that precedes it
/// in the model's own order, so a file gains keys in a predictable place.
fn insertion_point(entries: &[Entry], order: &[&str], key: &str, _indent: usize) -> Range<usize> {
    let rank = |k: &str| order.iter().position(|o| *o == k).unwrap_or(usize::MAX);
    let mine = rank(key);

    let mut after: Option<usize> = None;
    let mut before: Option<usize> = None;
    for e in entries {
        if rank(&e.key) < mine {
            after = Some(e.entry.end);
        } else if rank(&e.key) > mine && before.is_none() {
            before = Some(e.entry.start);
        }
    }
    let at = after.or(before).unwrap_or_else(|| entries.last().map_or(0, |e| e.entry.end));
    at..at
}

// ---------------------------------------------------------------- facts

/// Facts get element-level treatment, because a fact list is where the comments are.
///
/// Matching is longest-common-subsequence on whole facts first, then positional pairing
/// inside the runs that did not match. Positional alone would misread a removal as an
/// edit to every fact after it; LCS alone would turn a one-word edit into a delete and an
/// insert, taking that fact's comment with it.
fn patch_facts(
    text: &str,
    existing: Option<&Entry>,
    before: &[Fact],
    desired: &[Fact],
    entries: &[Entry],
    edits: &mut Vec<(Range<usize>, String)>,
) -> std::result::Result<(), String> {
    if before == desired {
        return Ok(());
    }

    let Some(entry) = existing else {
        if desired.is_empty() {
            return Ok(());
        }
        let at = insertion_point(entries, &ENTITY_KEYS, "facts", 0);
        edits.push((at, format!("facts:\n{}", emit::facts(desired, 0))));
        return Ok(());
    };

    if desired.is_empty() {
        edits.push((entry.entry.clone(), String::new()));
        return Ok(());
    }
    if before.is_empty() {
        edits.push((entry.value.clone(), emit::facts(desired, 0)));
        return Ok(());
    }

    let seq_indent = first_column(text, &entry.value)
        .ok_or_else(|| "the fact list has no elements to line up with".to_string())?;
    let items = scan::index_items(text, entry.value.clone(), seq_indent)
        .ok_or_else(|| "the fact list could not be located".to_string())?;
    if items.len() != before.len() {
        return Err(format!(
            "found {} fact elements where the record has {}",
            items.len(),
            before.len()
        ));
    }

    let mut cursor = entry.value.start;
    for step in align(before, desired) {
        match step {
            (Some(i), Some(j)) => {
                if before[i] != desired[j] {
                    patch_fact(text, &items[i], &before[i], &desired[j], edits)?;
                }
                cursor = items[i].item.end;
            }
            (Some(i), None) => {
                edits.push((items[i].item.clone(), String::new()));
                cursor = items[i].item.end;
            }
            (None, Some(j)) => {
                edits.push((cursor..cursor, emit::fact_item(&desired[j], seq_indent)));
            }
            (None, None) => unreachable!("align never emits an empty step"),
        }
    }
    Ok(())
}

const FACT_KEYS: [&str; 4] = ["attr", "value", "from", "to"];

fn patch_fact(
    text: &str,
    item: &scan::Item,
    _before: &Fact,
    desired: &Fact,
    edits: &mut Vec<(Range<usize>, String)>,
) -> std::result::Result<(), String> {
    let entries = scan::index_block(text, item.content.clone(), item.indent)
        .ok_or_else(|| "a fact's keys could not be located".to_string())?;

    for key in FACT_KEYS {
        let existing = entries.iter().find(|e| e.key == key);
        let field = match key {
            "attr" => Field::Inline(emit::scalar(&desired.attr)),
            "value" => Field::Inline(emit::value(&desired.value)),
            "from" if desired.from == DateExpr::Unknown => Field::Absent,
            "from" => Field::Inline(emit::date(&desired.from)),
            "to" if desired.to == DateExpr::Unknown => Field::Absent,
            "to" => Field::Inline(emit::date(&desired.to)),
            _ => Field::Absent,
        };
        plan(text, key, field, existing, &entries, &FACT_KEYS, item.indent, edits)?;
    }
    Ok(())
}

/// Pair up two lists: exact matches first, then positionally inside the gaps.
fn align<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] =
                if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }

    let mut anchors = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            anchors.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    let mut out = Vec::new();
    let (mut pi, mut pj) = (0, 0);
    for (mi, mj) in anchors.into_iter().chain(std::iter::once((n, m))) {
        let (ga, gb) = (mi - pi, mj - pj);
        let paired = ga.min(gb);
        for k in 0..paired {
            out.push((Some(pi + k), Some(pj + k)));
        }
        for k in paired..ga {
            out.push((Some(pi + k), None));
        }
        for k in paired..gb {
            out.push((None, Some(pj + k)));
        }
        if mi < n && mj < m {
            out.push((Some(mi), Some(mj)));
        }
        pi = mi + 1;
        pj = mj + 1;
    }
    out
}

// ---------------------------------------------------------------- machinery

/// Refuse anything whose meaning depends on YAML this writer does not model.
///
/// An alias or a merge key means the bytes on the page are not the whole record, and a
/// patcher that rewrote one of them would be guessing about what the file means. Giving
/// up here is cheap; guessing is not.
fn gate(yaml: &str) -> std::result::Result<serde_yaml_bw::Mapping, String> {
    let value: serde_yaml_bw::Value = serde_yaml_bw::from_str_value_preserve(yaml)
        .map_err(|e| format!("the frontmatter does not parse: {e}"))?;

    fn unsupported(v: &serde_yaml_bw::Value) -> Option<&'static str> {
        use serde_yaml_bw::Value as V;
        match v {
            V::Alias(_) => Some("an alias"),
            V::Tagged(_) => Some("a tag"),
            V::Null(a) | V::Bool(_, a) | V::Number(_, a) | V::String(_, a) => {
                a.as_ref().map(|_| "an anchor")
            }
            V::Sequence(s) => {
                if s.anchor.is_some() {
                    return Some("an anchor");
                }
                s.iter().find_map(unsupported)
            }
            V::Mapping(m) => {
                if m.anchor.is_some() {
                    return Some("an anchor");
                }
                m.iter().find_map(|(k, v)| unsupported(k).or_else(|| unsupported(v)))
            }
        }
    }

    if let Some(what) = unsupported(&value) {
        return Err(format!("the frontmatter uses {what}, which this writer will not rewrite"));
    }

    let serde_yaml_bw::Value::Mapping(mapping) = value else {
        return Err("the frontmatter is not a mapping".into());
    };
    if mapping.keys().any(|k| !matches!(k, serde_yaml_bw::Value::String(_, _))) {
        return Err("the frontmatter has a key that is not plain text".into());
    }
    Ok(mapping)
}

/// Locate every key, and refuse unless the scanner and libyaml tell the same story.
fn locate(
    text: &str,
    region: Range<usize>,
    mapping: &serde_yaml_bw::Mapping,
) -> std::result::Result<Vec<Entry>, String> {
    let entries = scan::index_block(text, region, 0)
        .ok_or_else(|| "the frontmatter could not be indexed".to_string())?;

    let found: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    let truth: Vec<&str> = mapping
        .keys()
        .filter_map(|k| match k {
            serde_yaml_bw::Value::String(s, _) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    if found != truth {
        return Err(format!("read the keys as {found:?} but the file actually has {truth:?}"));
    }
    Ok(entries)
}

/// Apply the edits, back to front, refusing any pair that overlaps.
fn splice(text: &str, edits: &mut [(Range<usize>, String)]) -> std::result::Result<String, String> {
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    for pair in edits.windows(2) {
        if pair[0].0.end > pair[1].0.start {
            return Err("two edits landed on the same bytes".into());
        }
    }

    let mut out = String::with_capacity(text.len() + 64);
    let mut cursor = 0;
    for (range, replacement) in edits.iter() {
        if range.start < cursor {
            return Err("an edit ran backwards".into());
        }
        out.push_str(&text[cursor..range.start]);
        out.push_str(replacement);
        cursor = range.end;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

/// Every comment the original carried, outside the bytes an edit replaced, must still be
/// there. This cannot prove a comment did not *move*; it does prove none vanished.
fn check_comments(
    original: &str,
    candidate: &str,
    edits: &[(Range<usize>, String)],
) -> std::result::Result<(), String> {
    let mut at = 0;
    for line in original.split_inclusive('\n') {
        let range = at..at + line.len();
        at = range.end;
        let bare = line.trim_end_matches(['\n', '\r']);
        if !bare.trim_start().starts_with('#') {
            continue;
        }
        let touched = edits.iter().any(|(r, _)| r.start < range.end && range.start < r.end);
        if !touched && !candidate.contains(bare) {
            return Err(format!("the comment {bare:?} would have been lost"));
        }
    }
    Ok(())
}

fn comments_in(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| l.starts_with('#')).map(str::to_string).collect()
}

fn first_column(text: &str, region: &Range<usize>) -> Option<usize> {
    let slice = &text[region.start..region.end];
    let line = slice.lines().find(|l| !l.trim().is_empty())?;
    Some(line.len() - line.trim_start().len())
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "md" || e == "markdown")
}

fn region_of(path: &Path, text: &str) -> std::result::Result<Range<usize>, String> {
    if !is_markdown(path) {
        return Ok(0..text.len());
    }
    frontmatter::split_spans(text)
        .map(|s| s.frontmatter)
        .ok_or_else(|| "the file has no frontmatter block".to_string())
}

fn parse_back<T: serde::de::DeserializeOwned>(
    path: &Path,
    candidate: &str,
) -> std::result::Result<T, String> {
    let region = region_of(path, candidate)?;
    serde_yaml_bw::from_str(&candidate[region.start..region.end])
        .map_err(|e| format!("the patched file does not parse: {e}"))
}

// ---------------------------------------------------------------- canonical

fn to_yaml<T: serde::Serialize>(path: &Path, value: &T) -> Result<String> {
    serde_yaml_bw::to_string(value)
        .map_err(|e| Error::Yaml { path: path.to_path_buf(), message: e.to_string() })
}

fn canonical_entity(path: &Path, entity: &Entity) -> Result<String> {
    let yaml = to_yaml(path, entity)?;
    if !is_markdown(path) {
        return Ok(yaml);
    }
    let body = entity.body.trim_end();
    if body.is_empty() {
        return Ok(format!("---\n{yaml}---\n"));
    }
    Ok(format!("---\n{yaml}---\n\n{body}\n"))
}

fn canonical_event(path: &Path, event: &Event) -> Result<String> {
    to_yaml(path, event)
}

fn canonical_scene(path: &Path, scene: &Scene) -> Result<String> {
    to_yaml(path, scene)
}

// ---------------------------------------------------------------- new files

/// A brand-new record, written the way the ones already in the folder are written.
///
/// Serde's own output is correct and reads like machine output: `marker:` followed by a
/// two-element block sequence, where every record a human wrote says `marker: [x, y]`.
/// A world folder that is visibly two-tone depending on which records the app made is a
/// small thing that undermines a large claim — these are your files.
///
/// Verified the same way a patch is: if the result does not reparse to what was intended,
/// serde's version is used instead.
fn fresh_entity(path: &Path, e: &Entity) -> Result<String> {
    let mut yaml = String::new();
    yaml.push_str(&format!("id: {}\n", emit::scalar(&e.id)));
    yaml.push_str(&format!("name: {}\n", emit::scalar(&e.name)));
    if !e.aliases.is_empty() {
        yaml.push_str(&format!("aka: {}\n", emit::ids(&e.aliases, Style::Flow, 0)));
    }
    yaml.push_str(&format!("type: {}\n", emit::scalar(&e.type_name)));
    if let Some(span) = &e.existence {
        yaml.push_str(&format!("existence: {}\n", emit::span(span, Style::Flow, 0)));
    }
    if !e.parents.is_empty() {
        yaml.push_str(&format!("parents: {}\n", emit::ids(&e.parents, Style::Flow, 0)));
    }
    if !e.facts.is_empty() {
        yaml.push_str(&format!("facts:\n{}", emit::facts(&e.facts, 0)));
    }
    if let Some(p) = e.marker {
        yaml.push_str(&format!("marker: {}\n", emit::point(p)));
    }
    if !e.shape.is_empty() {
        yaml.push_str(&format!("shape:\n{}", emit::points(&e.shape, Style::Block, 0)));
    }

    let text = if is_markdown(path) {
        let body = e.body.trim_end();
        if body.is_empty() {
            format!("---\n{yaml}---\n")
        } else {
            format!("---\n{yaml}---\n\n{body}\n")
        }
    } else {
        yaml
    };

    match parse_back::<Entity>(path, &text) {
        Ok(mut back) => {
            back.body = e.body.clone();
            back.source = e.source.clone();
            if back == *e {
                return Ok(text);
            }
            canonical_entity(path, e)
        }
        Err(_) => canonical_entity(path, e),
    }
}

fn fresh_event(path: &Path, e: &Event) -> Result<String> {
    let mut yaml = String::new();
    yaml.push_str(&format!("id: {}\n", emit::scalar(&e.id)));
    yaml.push_str(&format!("name: {}\n", emit::scalar(&e.name)));
    if !e.kind.is_empty() {
        yaml.push_str(&format!("kind: {}\n", emit::scalar(&e.kind)));
    }
    yaml.push_str(&format!("date: {}\n", emit::date(&e.date)));
    if !e.participants.is_empty() {
        yaml.push_str(&format!("participants: {}\n", emit::ids(&e.participants, Style::Flow, 0)));
    }
    if let Some(loc) = &e.location {
        yaml.push_str(&format!("location: {}\n", emit::scalar(loc)));
    }

    match parse_back::<Event>(path, &yaml) {
        Ok(mut back) => {
            back.body = e.body.clone();
            back.source = e.source.clone();
            if back == *e {
                return Ok(yaml);
            }
            canonical_event(path, e)
        }
        Err(_) => canonical_event(path, e),
    }
}

fn fresh_scene(path: &Path, s: &Scene) -> Result<String> {
    let mut yaml = String::new();
    yaml.push_str(&format!("id: {}\n", emit::scalar(&s.id)));
    yaml.push_str(&format!("name: {}\n", emit::scalar(&s.name)));
    yaml.push_str(&format!("date: {}\n", emit::date(&s.date)));
    if let Some(pov) = &s.pov {
        yaml.push_str(&format!("pov: {}\n", emit::scalar(pov)));
    }
    if !s.on_page.is_empty() {
        yaml.push_str(&format!("on_page: {}\n", emit::ids(&s.on_page, Style::Flow, 0)));
    }
    if let Some(loc) = &s.location {
        yaml.push_str(&format!("location: {}\n", emit::scalar(loc)));
    }
    if let Some(link) = &s.prose {
        yaml.push_str(&format!("prose: {}\n", emit::scalar(link)));
    }

    match parse_back::<Scene>(path, &yaml) {
        Ok(mut back) => {
            back.source = s.source.clone();
            if back == *s {
                return Ok(yaml);
            }
            canonical_scene(path, s)
        }
        Err(_) => canonical_scene(path, s),
    }
}
