//! The tool surface.
//!
//! Read tools are unrestricted. Write tools cannot reach canon at all — they file
//! proposals, and only a human accepts one in the app. There is deliberately no accept
//! tool: an agent that could approve its own changes is an agent with write access to
//! the writer's world, and the first bad session costs them their trust in the tool.
//!
//! Two habits run through every tool here. Uncertainty is passed through rather than
//! resolved — a `maybe` stays `maybe`, because the writer's vagueness is data. And
//! nothing is silently truncated: every capped list reports what it dropped.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use wb_core::Day;
use wb_store::World;

use crate::change::ChangeInput;
use crate::dto::{
    EntityBrief, EntityDto, EventDto, FindingDto, HitDto, NamedRef, certainty, primitive_name,
    relative,
};
use crate::handle::WorldHandle;
use crate::notes;
use crate::overview::WorldOverview;

/// Enough to answer any single question without flooding a context window. Every tool
/// that caps its output says so in the payload.
const DEFAULT_LIMIT: usize = 200;

pub const INSTRUCTIONS: &str = "\
Worldbuilder holds a fictional world as time-indexed facts: nothing is a scalar, \
everything is an assertion that held over a window of days. `describe_world` first — it \
carries the calendar, the date syntax, and the attribute vocabulary this world already \
uses, none of which you can guess.

Three things to hold onto:

1. `certainty: \"maybe\"` means the world's own fuzzy dates leave a question genuinely \
open. Report it as open. Resolving it yourself is inventing canon.
2. A `possible` consistency finding is often a deliberate mystery, not a bug. `definite` \
means wrong under every reading of every date. Judge the difference; do not assume it.
3. You cannot write to this world. `propose_changes` files a proposal the writer accepts \
or rejects in the app. Use `check_changes` first to see what a change would settle and \
what it would break.";

#[derive(Clone)]
pub struct WorldServer {
    world: Arc<WorldHandle>,
    tool_router: ToolRouter<Self>,
}

impl WorldServer {
    pub fn new(world: Arc<WorldHandle>) -> Self {
        Self { world, tool_router: Self::tool_router() }
    }

    pub fn open(root: impl AsRef<Path>) -> wb_store::Result<Self> {
        Ok(Self::new(Arc::new(WorldHandle::open(root)?)))
    }

    fn read<T>(&self, f: impl FnOnce(&World) -> Result<T, String>) -> Result<T, String> {
        self.world.with(f)?
    }

    fn root(&self) -> &Path {
        self.world.root()
    }

    /// Every tool this server advertises. Available without a world or a connection, so
    /// documentation and the shipped skills can be checked against the real surface.
    pub fn tool_names() -> Vec<String> {
        Self::tool_router().list_all().iter().map(|t| t.name.to_string()).collect()
    }
}

// ------------------------------------------------------------------ arguments

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AtArgs {
    /// A date expression: `0812`, `0812-04-17`, `0812~`, `@evt_siege_of_marrow+1y`.
    /// See `date_syntax` from `describe_world`.
    pub date: String,
    /// Only records of this declared type.
    #[serde(default)]
    pub r#type: Option<String>,
    /// Only records behaving like this engine role: actor, polity, place, thing.
    #[serde(default)]
    pub primitive: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntityArgs {
    pub id: String,
    /// Optional date. Supplied, the record also reports which facts were live then and
    /// whether it existed at all.
    #[serde(default)]
    pub at: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryArgs {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub primitive: Option<String>,
    /// Substring of the name or id, case-insensitive.
    #[serde(default)]
    pub name_contains: Option<String>,
    /// Only records that existed on this date.
    #[serde(default)]
    pub alive_at: Option<String>,
    /// Only records carrying this attribute at some point.
    #[serde(default)]
    pub has_attr: Option<String>,
    /// Only records with geometry — a marker or a polygon.
    #[serde(default)]
    pub on_map: Option<bool>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TimelineArgs {
    /// Start of the window. Omit for the beginning of recorded history.
    #[serde(default)]
    pub from: Option<String>,
    /// End of the window. Omit for the end of it.
    #[serde(default)]
    pub to: Option<String>,
    /// Only events of this kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Only events naming this id as a participant or location.
    #[serde(default)]
    pub involving: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LineageArgs {
    pub id: String,
    /// Generations to walk in each direction. Default 3.
    #[serde(default)]
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckArgs {
    /// Only findings about this record, or naming it.
    #[serde(default)]
    pub subject: Option<String>,
    /// Only findings from this rule, by slug.
    #[serde(default)]
    pub rule: Option<String>,
    /// `definite` to see only what is wrong under every reading; `possible` for what the
    /// world's vagueness leaves open. Omit for both.
    #[serde(default)]
    pub certainty: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Matched against ids, names, types, fact values, and prose.
    pub text: String,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DateArgs {
    pub date: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteArgs {
    /// Path relative to the world folder, as returned by `list_notes`.
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ChangesArgs {
    pub changes: Vec<ChangeInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeArgs {
    /// One line a writer can decide from — the claim, not the mechanism. "Aldric died of
    /// his wounds within the year", not "set existence.to".
    pub title: String,
    /// Why. Cite what led here: a note, a chapter, a finding. This is the part a writer
    /// actually reads before accepting.
    #[serde(default)]
    pub note: Option<String>,
    /// Who is asking. Defaults to the connected client's name.
    #[serde(default)]
    pub author: Option<String>,
    pub changes: Vec<ChangeInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposalListArgs {
    /// `pending`, `accepted`, or `rejected`. Omit for pending only.
    #[serde(default)]
    pub status: Option<String>,
}

// ------------------------------------------------------------------ results

#[derive(Debug, Serialize, JsonSchema)]
pub struct SnapshotOut {
    pub day: i64,
    pub label: String,
    pub entities: Vec<EntityBrief>,
    /// Set when the result was capped — never a silent truncation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omitted: Option<usize>,
    /// The change points bracketing this date. Nothing differs anywhere between them, so
    /// sampling inside the window tells you nothing new.
    pub unchanged_from: Option<i64>,
    pub unchanged_until: Option<i64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListOut<T> {
    pub matched: usize,
    pub items: Vec<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub omitted: Option<usize>,
}

impl<T> ListOut<T> {
    fn new(items: Vec<T>, matched: usize, limit: usize) -> Self {
        let omitted = matched.saturating_sub(limit.min(matched));
        Self { matched, items, omitted: (omitted > 0).then_some(omitted) }
    }
}

/// Map geometry at a date.
///
/// GeoJSON-shaped for the sake of tools that already read it, but the coordinates are
/// **not geographic**: they are normalized 0..1 image space with the origin top-left, so
/// `y` increases *southward*. Plotting them on a real map without flipping `y` will
/// mirror the world. `coordinate_space` says so on every response.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TerritoryOut {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub coordinate_space: &'static str,
    pub note: &'static str,
    pub day: i64,
    pub label: String,
    pub features: Vec<Feature>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Feature {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub geometry: Geometry,
    pub properties: FeatureProps,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Geometry {
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// `[x, y]` for a Point; `[[[x, y], …]]` for a Polygon. Serialized untyped because
    /// the two shapes differ in nesting depth.
    pub coordinates: serde_json::Value,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FeatureProps {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// `yes` or `maybe`. A `maybe` border is drawn dashed in the app rather than picked.
    pub existence: &'static str,
    /// Live `owner` claims. Two at once is a vague handover, not a bug.
    pub claims: Vec<Claim>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Claim {
    pub owner: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    pub certainty: &'static str,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct LineageOut {
    pub subject: NamedRef,
    /// Ancestors, nearest generation first.
    pub ancestors: Vec<Kin>,
    pub descendants: Vec<Kin>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Kin {
    pub id: String,
    pub name: String,
    /// 1 is a parent or child, 2 a grandparent or grandchild.
    pub generation: usize,
    pub lifespan: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct CheckOut {
    pub definite: usize,
    pub possible: usize,
    /// Findings, worst first. `definite` ones are wrong under every reading of every
    /// fuzzy date; `possible` ones are where deliberate mysteries live.
    pub findings: Vec<FindingDto>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResolvedDateOut {
    pub expr: String,
    pub day: Option<i64>,
    pub label: String,
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
    pub exact: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ImpactOut {
    /// Findings this would clear.
    pub resolves: Vec<FindingDto>,
    /// Findings this would create. A `definite` one here is a reason not to file it.
    pub introduces: Vec<FindingDto>,
    pub definite_before: usize,
    pub definite_after: usize,
    pub possible_before: usize,
    pub possible_after: usize,
    /// True when accepting would add a contradiction wrong under every reading.
    pub breaks_something: bool,
    /// Files that would change, and whether each is new.
    pub files: Vec<FileOut>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FileOut {
    pub path: String,
    pub is_new: bool,
    pub changed_lines: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProposedOut {
    pub id: String,
    pub path: String,
    pub status: &'static str,
    /// What accepting it would do. Filing is not accepting.
    pub impact: ImpactOut,
    pub next_step: &'static str,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProposalOut {
    pub id: String,
    pub title: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub changes: Vec<String>,
    pub resolves: usize,
    pub introduces: usize,
    pub breaks_something: bool,
    /// Set when the proposal no longer simulates — usually because canon moved under it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ------------------------------------------------------------------ tools

#[tool_router(router = tool_router)]
impl WorldServer {
    /// Orientation. Read this before anything else: it carries the calendar, the date
    /// syntax, the type and attribute vocabulary already in use, and where consistency
    /// currently stands. None of it is guessable, and getting it wrong produces dates
    /// that resolve to the wrong day and facts that duplicate existing ones.
    #[tool(annotations(title = "Describe world", read_only_hint = true))]
    async fn describe_world(&self) -> Result<Json<WorldOverview>, String> {
        let pending = self.pending_count();
        let mut overview = self.read(|world| Ok(WorldOverview::of(world, pending, 0)))?;
        // Read *after* the query, because the query is what performs the reload. Asking
        // first reports the count from before the reload it just caused.
        overview.reloads = self.world.reloads();
        Ok(Json(overview))
    }

    /// Everything true at one instant — the map's own query. Facts whose window does not
    /// cover the date are absent; records that did not exist are omitted entirely.
    #[tool(annotations(title = "World at a date", read_only_hint = true))]
    async fn world_at(
        &self,
        Parameters(args): Parameters<AtArgs>,
    ) -> Result<Json<SnapshotOut>, String> {
        self.read(|world| {
            let day = resolve_day(world, &args.date)?;
            let limit = args.limit.unwrap_or(DEFAULT_LIMIT);

            let snapshot = world.at(day);
            let matching: Vec<_> = snapshot
                .entities
                .iter()
                .filter(|v| {
                    matches_type(world, v.entity, args.r#type.as_deref(), args.primitive.as_deref())
                })
                .collect();

            let matched = matching.len();
            let entities = matching.iter().take(limit).map(|v| EntityBrief::at(world, v)).collect();

            let points = world.change_points();
            Ok(Json(SnapshotOut {
                day: day.0,
                label: world.calendar.format_long(day),
                entities,
                omitted: (matched > limit).then(|| matched - limit),
                unchanged_from: points.iter().rev().find(|p| p.0 <= day.0).map(|d| d.0),
                unchanged_until: points.iter().find(|p| p.0 > day.0).map(|d| d.0),
            }))
        })
    }

    /// One whole record: every fact with the window it holds over, lineage, geometry,
    /// prose, and — which no file states — what points back at it.
    #[tool(annotations(title = "Get entity", read_only_hint = true))]
    async fn get_entity(
        &self,
        Parameters(args): Parameters<EntityArgs>,
    ) -> Result<Json<EntityDto>, String> {
        self.read(|world| {
            let entity = world.entities.get(&args.id).ok_or_else(|| unknown_id(world, &args.id))?;
            let at = match &args.at {
                None => None,
                Some(expr) => Some(resolve_day(world, expr)?),
            };
            Ok(Json(EntityDto::of(world, entity, at)))
        })
    }

    /// Find records by type, name, geometry, or whether they existed on a date.
    #[tool(annotations(title = "Query entities", read_only_hint = true))]
    async fn query_entities(
        &self,
        Parameters(args): Parameters<QueryArgs>,
    ) -> Result<Json<ListOut<EntityBrief>>, String> {
        self.read(|world| {
            let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
            let alive_at = match &args.alive_at {
                None => None,
                Some(expr) => Some(resolve_day(world, expr)?),
            };
            let needle = args.name_contains.as_ref().map(|s| s.to_lowercase());

            let hits: Vec<&wb_store::Entity> = world
                .entities
                .values()
                .filter(|e| {
                    matches_type(world, e, args.r#type.as_deref(), args.primitive.as_deref())
                })
                .filter(|e| {
                    needle.as_ref().is_none_or(|n| {
                        e.name.to_lowercase().contains(n) || e.id.to_lowercase().contains(n)
                    })
                })
                .filter(|e| {
                    args.has_attr.as_ref().is_none_or(|a| e.facts.iter().any(|f| &f.attr == a))
                })
                .filter(|e| {
                    args.on_map
                        .is_none_or(|want| (e.marker.is_some() || !e.shape.is_empty()) == want)
                })
                .filter(|e| alive_at.is_none_or(|day| world.entity_at(&e.id, day).is_some()))
                .collect();

            let matched = hits.len();
            let items = hits
                .iter()
                .take(limit)
                .map(|e| match alive_at.and_then(|day| world.entity_at(&e.id, day)) {
                    Some(view) => EntityBrief::at(world, &view),
                    None => EntityBrief::of(world, e),
                })
                .collect();
            Ok(Json(ListOut::new(items, matched, limit)))
        })
    }

    /// Events in a window, chronologically. Undated events are omitted — they have
    /// nowhere on the timeline to sit.
    #[tool(annotations(title = "Timeline", read_only_hint = true))]
    async fn timeline(
        &self,
        Parameters(args): Parameters<TimelineArgs>,
    ) -> Result<Json<ListOut<EventDto>>, String> {
        self.read(|world| {
            let limit = args.limit.unwrap_or(DEFAULT_LIMIT);
            let points = world.change_points();
            let from = match &args.from {
                Some(expr) => resolve_edge(world, expr, Edge::Start)?,
                None => points.first().copied().unwrap_or(Day(0)),
            };
            let to = match &args.to {
                Some(expr) => resolve_edge(world, expr, Edge::End)?,
                None => points.last().copied().unwrap_or(Day(0)),
            };
            if to.0 < from.0 {
                return Err(format!(
                    "`to` ({}) is before `from` ({}) — the window is empty",
                    world.calendar.format_numeric(to),
                    world.calendar.format_numeric(from)
                ));
            }

            let events: Vec<&wb_store::Event> = world
                .events_between(from, to)
                .into_iter()
                .filter(|e| args.kind.as_ref().is_none_or(|k| &e.kind == k))
                .filter(|e| {
                    args.involving.as_ref().is_none_or(|id| {
                        e.participants.iter().any(|p| p == id)
                            || e.location.as_deref() == Some(id.as_str())
                    })
                })
                .collect();

            let matched = events.len();
            let items = events.iter().take(limit).map(|e| EventDto::of(world, e)).collect();
            Ok(Json(ListOut::new(items, matched, limit)))
        })
    }

    /// Map geometry at a date, with each region's live ownership claims. Coordinates are
    /// normalized image space, not longitude and latitude — see `coordinate_space`.
    #[tool(annotations(title = "Territory at a date", read_only_hint = true))]
    async fn territory_at(
        &self,
        Parameters(args): Parameters<DateArgs>,
    ) -> Result<Json<TerritoryOut>, String> {
        self.read(|world| {
            let day = resolve_day(world, &args.date)?;
            let features = world
                .at(day)
                .entities
                .iter()
                .filter(|v| v.entity.marker.is_some() || !v.entity.shape.is_empty())
                .map(|view| {
                    let entity = view.entity;
                    let geometry = if entity.shape.is_empty() {
                        Geometry {
                            kind: "Point",
                            coordinates: serde_json::json!(entity.marker.unwrap_or_default()),
                        }
                    } else {
                        Geometry {
                            kind: "Polygon",
                            coordinates: serde_json::json!([closed(&entity.shape)]),
                        }
                    };

                    let claims = view
                        .facts
                        .iter()
                        .filter(|f| f.attr == "owner")
                        .map(|f| {
                            let owner = f.value.to_string();
                            Claim {
                                name: world
                                    .entities
                                    .get(&owner)
                                    .map_or_else(|| owner.clone(), |o| o.name.clone()),
                                color: world
                                    .value_at(&owner, "color", day)
                                    .map(|c| c.value.to_string()),
                                certainty: certainty(f.certainty),
                                owner,
                            }
                        })
                        .collect();

                    Feature {
                        kind: "Feature",
                        geometry,
                        properties: FeatureProps {
                            id: entity.id.clone(),
                            name: entity.name.clone(),
                            type_name: entity.type_name.clone(),
                            existence: certainty(view.existence),
                            claims,
                        },
                    }
                })
                .collect();

            Ok(Json(TerritoryOut {
                kind: "FeatureCollection",
                coordinate_space: "normalized-image",
                note: "Coordinates are [x, y] in 0..1 with the origin top-left; y increases southward. Not longitude and latitude.",
                day: day.0,
                label: world.calendar.format_long(day),
                features,
            }))
        })
    }

    /// Ancestors and descendants, with lifespans. Lineage is parentage edges over
    /// records that already carry existence intervals — there is no separate family tree.
    #[tool(annotations(title = "Lineage", read_only_hint = true))]
    async fn lineage(
        &self,
        Parameters(args): Parameters<LineageArgs>,
    ) -> Result<Json<LineageOut>, String> {
        self.read(|world| {
            let entity = world.entities.get(&args.id).ok_or_else(|| unknown_id(world, &args.id))?;
            let depth = args.depth.unwrap_or(3);

            let mut ancestors = Vec::new();
            let mut frontier = vec![args.id.clone()];
            let mut seen = vec![args.id.clone()];
            for generation in 1..=depth {
                let mut next = Vec::new();
                for id in &frontier {
                    let Some(e) = world.entities.get(id) else { continue };
                    for parent in &e.parents {
                        if seen.contains(parent) {
                            continue;
                        }
                        seen.push(parent.clone());
                        if let Some(p) = world.entities.get(parent) {
                            ancestors.push(kin(world, p, generation));
                            next.push(parent.clone());
                        }
                    }
                }
                if next.is_empty() {
                    break;
                }
                frontier = next;
            }

            let mut descendants = Vec::new();
            let mut frontier = vec![args.id.clone()];
            let mut seen = vec![args.id.clone()];
            for generation in 1..=depth {
                let mut next = Vec::new();
                for e in world.entities.values() {
                    if seen.contains(&e.id) || !e.parents.iter().any(|p| frontier.contains(p)) {
                        continue;
                    }
                    seen.push(e.id.clone());
                    descendants.push(kin(world, e, generation));
                    next.push(e.id.clone());
                }
                if next.is_empty() {
                    break;
                }
                frontier = next;
            }

            Ok(Json(LineageOut {
                subject: NamedRef::known(&entity.id, &entity.name),
                ancestors,
                descendants,
            }))
        })
    }

    /// Run every consistency rule. Deterministic and offline: it cannot invent a
    /// contradiction that is not in the data, and it cannot tell a bug from a mystery.
    /// That judgement is the part worth your attention.
    #[tool(annotations(title = "Check consistency", read_only_hint = true))]
    async fn check_consistency(
        &self,
        Parameters(args): Parameters<CheckArgs>,
    ) -> Result<Json<CheckOut>, String> {
        self.read(|world| {
            let report = wb_check::check(world);
            let findings: Vec<FindingDto> = report
                .findings
                .iter()
                .filter(|f| {
                    args.subject
                        .as_ref()
                        .is_none_or(|s| &f.subject == s || f.related.iter().any(|r| r == s))
                })
                .filter(|f| args.rule.as_ref().is_none_or(|r| f.rule.slug() == r))
                .filter(|f| args.certainty.as_ref().is_none_or(|c| f.certainty.slug() == c))
                .map(|f| FindingDto::of(world, f))
                .collect();

            let definite = findings.iter().filter(|f| f.certainty == "definite").count();
            Ok(Json(CheckOut { definite, possible: findings.len() - definite, findings }))
        })
    }

    /// Free-text search over ids, names, types, fact values, and prose. Ranked, so the
    /// record named "Marrow" comes before the paragraphs mentioning it.
    #[tool(annotations(title = "Search", read_only_hint = true))]
    async fn search(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<ListOut<HitDto>>, String> {
        self.read(|world| {
            let limit = args.limit.unwrap_or(25);
            // One over the limit, so "there are more" can be reported honestly without
            // ranking the whole world.
            let hits = world.search(&args.text, limit + 1);
            let matched = hits.len();
            let items = hits.iter().take(limit).map(HitDto::of).collect();
            Ok(Json(ListOut::new(items, matched, limit)))
        })
    }

    /// Resolve a date expression to a day number and a label. Use it to check an anchor
    /// before writing it into a proposal.
    #[tool(annotations(title = "Resolve date", read_only_hint = true))]
    async fn resolve_date(
        &self,
        Parameters(args): Parameters<DateArgs>,
    ) -> Result<Json<ResolvedDateOut>, String> {
        self.read(|world| {
            let expr = wb_core::parse_date(&args.date)
                .map_err(|e| format!("bad date {:?}: {e}", args.date))?;
            let resolved = world
                .resolve_in("query", &expr)
                .map_err(|e| format!("{:?} does not resolve in this world: {e}", args.date))?;
            Ok(Json(ResolvedDateOut {
                expr: expr.to_string(),
                day: resolved.nominal.map(|d| d.0),
                label: resolved
                    .nominal
                    .map(|d| world.calendar.format_long(d))
                    .unwrap_or_else(|| "unknown".to_string()),
                earliest: resolved.earliest.map(|d| d.0),
                latest: resolved.latest.map(|d| d.0),
                exact: resolved.is_exact(),
            }))
        })
    }

    /// Source documents in the world's `notes/` folder — the raw material a world gets
    /// built from. Nothing outside that folder is readable through this server.
    #[tool(annotations(title = "List notes", read_only_hint = true))]
    async fn list_notes(&self) -> Result<Json<ListOut<notes::NoteDto>>, String> {
        let items = notes::list(self.root())?;
        let matched = items.len();
        Ok(Json(ListOut::new(items, matched, matched)))
    }

    /// Read one note, by the path `list_notes` gave.
    #[tool(annotations(title = "Read note", read_only_hint = true))]
    async fn read_note(
        &self,
        Parameters(args): Parameters<NoteArgs>,
    ) -> Result<Json<notes::NoteBody>, String> {
        Ok(Json(notes::read(self.root(), &args.path)?))
    }

    /// Dry-run a set of changes: what they would settle, what they would break, and
    /// which files they would touch. Writes nothing, files nothing. Use this to check
    /// your own work before spending a writer's attention on a review.
    #[tool(annotations(title = "Check changes", read_only_hint = true))]
    async fn check_changes(
        &self,
        Parameters(args): Parameters<ChangesArgs>,
    ) -> Result<Json<ImpactOut>, String> {
        let proposal = draft("prp_dry_run", "dry run", None, None, args.changes)?;
        self.read(|world| Ok(Json(measure(world, &proposal)?)))
    }

    /// File a proposal for the writer to accept or reject in the app.
    ///
    /// This does **not** change the world. It writes one YAML file to `proposals/`, and
    /// a human decides. Prefer one proposal per idea a writer can say yes or no to as a
    /// whole — an ingestion pass over a chapter of notes is one proposal with many
    /// changes, not forty proposals.
    #[tool(annotations(
        title = "Propose changes",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false
    ))]
    async fn propose_changes(
        &self,
        Parameters(args): Parameters<ProposeArgs>,
    ) -> Result<Json<ProposedOut>, String> {
        if args.changes.is_empty() {
            return Err("a proposal with no changes asks the writer to decide nothing".into());
        }

        let id = self.next_proposal_id(&args.title);
        let mut proposal = draft(
            &id,
            &args.title,
            args.note.as_deref(),
            args.author.as_deref().or(Some("agent")),
            args.changes,
        )?;

        // Simulated before it is written: a proposal that cannot even be applied is a
        // bug report, not a suggestion, and should not reach the queue.
        let impact = self.read(|world| measure(world, &proposal))?;

        proposal.source = self.root().join(wb_propose::store::DIR).join(format!("{id}.yaml"));
        let path = wb_propose::store::write(self.root(), &proposal).map_err(|e| e.to_string())?;

        Ok(Json(ProposedOut {
            id,
            path: path.strip_prefix(self.root()).unwrap_or(&path).display().to_string(),
            status: "pending",
            impact,
            next_step: "Filed. Nothing has changed in the world yet — the writer accepts or rejects this in the app's review queue.",
        }))
    }

    /// The review queue, with what accepting each proposal would do.
    #[tool(annotations(title = "List proposals", read_only_hint = true))]
    async fn list_proposals(
        &self,
        Parameters(args): Parameters<ProposalListArgs>,
    ) -> Result<Json<ListOut<ProposalOut>>, String> {
        let wanted = args.status.as_deref().unwrap_or("pending");
        let all = wb_propose::store::load_all(self.root()).map_err(|e| e.to_string())?;

        self.read(|world| {
            let items: Vec<ProposalOut> = all
                .iter()
                .filter(|p| wanted == "all" || p.status.slug() == wanted)
                .map(|p| {
                    let effect = wb_propose::impact(world, p);
                    ProposalOut {
                        id: p.id.clone(),
                        title: p.title.clone(),
                        status: p.status.slug(),
                        author: p.author.clone(),
                        note: p.note.trim().to_string(),
                        changes: p.changes.iter().map(|c| c.summary()).collect(),
                        resolves: effect.as_ref().map_or(0, |e| e.resolved.len()),
                        introduces: effect.as_ref().map_or(0, |e| e.introduced.len()),
                        breaks_something: effect
                            .as_ref()
                            .is_ok_and(wb_propose::Impact::breaks_something),
                        error: effect.err().map(|e| e.to_string()),
                    }
                })
                .collect();
            let matched = items.len();
            Ok(Json(ListOut::new(items, matched, matched)))
        })
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for WorldServer {
    fn get_info(&self) -> ServerInfo {
        let mut identity = Implementation::new("worldbuilder", env!("CARGO_PKG_VERSION"));
        identity.title = Some("Worldbuilder".into());
        identity.description = Some(
            "A fictional world as time-indexed facts, with a review queue for changes.".into(),
        );

        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = identity;
        info.instructions = Some(INSTRUCTIONS.to_string());
        info
    }
}

// ------------------------------------------------------------------ helpers

impl WorldServer {
    fn pending_count(&self) -> usize {
        wb_propose::store::load_all(self.root())
            .map(|all| all.iter().filter(|p| p.is_pending()).count())
            .unwrap_or(0)
    }

    /// A readable id derived from the title, suffixed only if it would collide. An agent
    /// filing three proposals in a session should not produce `prp_1`, `prp_2`, `prp_3`.
    fn next_proposal_id(&self, title: &str) -> String {
        let stem: String = title
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let stem: Vec<&str> = stem.split('_').filter(|s| !s.is_empty()).take(6).collect();
        let base = format!("prp_{}", stem.join("_"));

        let taken: Vec<String> = wb_propose::store::load_all(self.root())
            .map(|all| all.into_iter().map(|p| p.id).collect())
            .unwrap_or_default();

        if !taken.contains(&base) {
            return base;
        }
        (2..).map(|n| format!("{base}_{n}")).find(|id| !taken.contains(id)).unwrap_or(base)
    }
}

fn draft(
    id: &str,
    title: &str,
    note: Option<&str>,
    author: Option<&str>,
    changes: Vec<ChangeInput>,
) -> Result<wb_propose::Proposal, String> {
    Ok(wb_propose::Proposal {
        id: id.to_string(),
        title: title.to_string(),
        author: author.unwrap_or_default().to_string(),
        note: note.unwrap_or_default().to_string(),
        status: wb_propose::Status::Pending,
        changes: changes
            .into_iter()
            .map(ChangeInput::into_change)
            .collect::<Result<Vec<_>, _>>()?,
        source: PathBuf::new(),
    })
}

fn measure(world: &World, proposal: &wb_propose::Proposal) -> Result<ImpactOut, String> {
    let effect = wb_propose::impact(world, proposal).map_err(|e| e.to_string())?;
    let files = wb_propose::preview(world, proposal)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|edit| FileOut {
            path: relative(world, &edit.path),
            is_new: edit.is_new(),
            changed_lines: changed_lines(edit.before.as_deref().unwrap_or(""), &edit.after),
        })
        .collect();

    Ok(ImpactOut {
        resolves: effect.resolved.iter().map(|f| FindingDto::of(world, f)).collect(),
        introduces: effect.introduced.iter().map(|f| FindingDto::of(world, f)).collect(),
        definite_before: effect.before.0,
        definite_after: effect.after.0,
        possible_before: effect.before.1,
        possible_after: effect.after.1,
        breaks_something: effect.breaks_something(),
        files,
    })
}

/// Line count rather than the diff itself: an agent filing a proposal does not need to
/// read back the YAML it just described, and the writer sees the real diff in the app.
fn changed_lines(before: &str, after: &str) -> usize {
    let before: Vec<&str> = before.lines().collect();
    after.lines().filter(|line| !before.contains(line)).count()
}

fn kin(world: &World, entity: &wb_store::Entity, generation: usize) -> Kin {
    let lifespan = entity
        .existence
        .as_ref()
        .map(|span| {
            let label = |expr: &wb_core::DateExpr| {
                world
                    .resolve_in(&entity.id, expr)
                    .ok()
                    .and_then(|r| r.nominal)
                    .map(|d| world.calendar.format_numeric(d))
                    .unwrap_or_else(|| expr.to_string())
            };
            format!("{} → {}", label(&span.from), label(&span.to))
        })
        .unwrap_or_else(|| "undated".to_string());

    Kin { id: entity.id.clone(), name: entity.name.clone(), generation, lifespan }
}

fn matches_type(
    world: &World,
    entity: &wb_store::Entity,
    type_name: Option<&str>,
    primitive: Option<&str>,
) -> bool {
    type_name.is_none_or(|t| entity.type_name == t)
        && primitive.is_none_or(|p| world.primitive_of(entity).map(primitive_name) == Some(p))
}

/// Which end of a fuzzy date a window bound should take.
#[derive(Clone, Copy)]
enum Edge {
    Start,
    End,
}

/// Resolve a window endpoint, widening rather than collapsing.
///
/// `from` takes a date's *earliest* possible day and `to` its *latest*, so
/// `from: "0806", to: "0806"` is the whole of that year — which is what anyone typing a
/// year means. Collapsing both to the nominal day makes the obvious query return
/// nothing at all, since a year's nominal day is its first instant and no event sits
/// exactly there.
fn resolve_edge(world: &World, expr: &str, edge: Edge) -> Result<Day, String> {
    let parsed = wb_core::parse_date(expr).map_err(|e| format!("bad date {expr:?}: {e}"))?;
    let resolved = world
        .resolve_in("query", &parsed)
        .map_err(|e| format!("{expr:?} does not resolve in this world: {e}"))?;

    // An open-ended date (`>0812` as a `to`) has no bound on that side. Falling back to
    // the world's own extreme is what "after 812, however long that runs" means.
    let fallback = || match edge {
        Edge::Start => world.change_points().first().copied(),
        Edge::End => world.change_points().last().copied(),
    };

    match edge {
        Edge::Start => resolved.earliest.or(resolved.nominal).or_else(fallback),
        Edge::End => resolved.latest.or(resolved.nominal).or_else(fallback),
    }
    .ok_or_else(|| {
        format!("{expr:?} has no position on the timeline, and this world has no dates to bound it")
    })
}

/// A date that cannot be placed on the timeline is an error, not an empty result — an
/// agent that mistyped an anchor should be told, not handed the whole world back.
fn resolve_day(world: &World, expr: &str) -> Result<Day, String> {
    match world.day_of(expr) {
        Ok(Some(day)) => Ok(day),
        Ok(None) => Err(format!(
            "{expr:?} parses but does not resolve to a day — `?` and open-ended dates \
             have no position. Give a date, or an anchor that has one."
        )),
        Err(e) => Err(format!("bad date {expr:?}: {e}")),
    }
}

fn unknown_id(world: &World, id: &str) -> String {
    let near: Vec<&str> = world
        .entities
        .keys()
        .chain(world.events.keys())
        .filter(|k| {
            let (k, id) = (k.to_lowercase(), id.to_lowercase());
            k.contains(&id) || id.contains(&k)
        })
        .map(String::as_str)
        .take(5)
        .collect();

    if near.is_empty() {
        format!("no record `{id}`. Use `search` or `query_entities` to find the right id.")
    } else {
        format!("no record `{id}`. Did you mean: {}?", near.join(", "))
    }
}

/// GeoJSON polygons must repeat their first point as the last. World files do not, since
/// a writer drawing a region should not have to close it by hand.
fn closed(shape: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut ring = shape.to_vec();
    if let (Some(&first), Some(&last)) = (shape.first(), shape.last())
        && first != last
    {
        ring.push(first);
    }
    ring
}
