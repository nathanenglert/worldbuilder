import { invoke } from "@tauri-apps/api/core";

export type Certainty = "yes" | "maybe" | "no";

/** One live claim on a territory. Two at once means a vague handover date. */
export interface Claim {
  owner: string;
  name: string;
  color: string | null;
  certainty: Certainty;
}

export interface Fact {
  attr: string;
  value: string;
  certainty: Certainty;
}

export interface Entity {
  id: string;
  name: string;
  type: string;
  primitive: string | null;
  existence: Certainty;
  facts: Fact[];
  marker: [number, number] | null;
  shape: [number, number][];
  claims: Claim[];
}

export interface Snapshot {
  day: number;
  label: string;
  entities: Entity[];
}

export interface WorldSummary {
  name: string;
  calendar: string;
  months: string[];
  entity_count: number;
  event_count: number;
  scene_count: number;
  /** What the story panel can honestly claim before it claims anything. */
  manuscript: "unlinked" | "root_missing" | "linked";
  span: [number, number];
  change_points: number[];
  undeclared_types: string[];
  types: { name: string; primitive: string }[];
  /**
   * Every id in the world, entities and events together — they share one namespace.
   * Check a new id against this, never against `snapshot.entities`, which is filtered
   * by date and would let a clash go unnoticed until the save.
   */
  ids: string[];
}

export interface WorldEvent {
  id: string;
  name: string;
  kind: string;
  nominal: number | null;
  earliest: number | null;
  latest: number | null;
  label: string;
  participants: string[];
  location: string | null;
}

/**
 * A consistency finding. `definite` is wrong under every reading of every fuzzy date;
 * `possible` is where deliberate mysteries live, and is never presented as an error.
 */
export interface Finding {
  rule: string;
  title: string;
  certainty: "definite" | "possible";
  subject: string;
  related: string[];
  message: string;
  at: number | null;
  sources: string[];
}

/** A pending change and what accepting it would do to the world's consistency. */
export interface ProposalSummary {
  id: string;
  title: string;
  author: string;
  note: string;
  status: "pending" | "accepted" | "rejected";
  changes: string[];
  resolves: number;
  introduces: number;
  /** Accepting would add a contradiction wrong under every reading. */
  breaks: boolean;
}

export interface DiffLine {
  tag: "+" | "-";
  text: string;
}

export interface FileEdit {
  path: string;
  is_new: boolean;
  diff: DiffLine[];
}

export interface ProposalDetail extends ProposalSummary {
  resolved: Finding[];
  introduced: Finding[];
  files: FileEdit[];
  error: string | null;
}

/** A closed loop of the coastline, flattened to `[x0, y0, x1, y1, …]`. */
export interface Ring {
  points: number[];
  is_hole: boolean;
}

export interface River {
  points: number[];
  flux: number[];
  order: number;
  mouth: "sea" | "lake" | "sink";
}

export interface BiomeStyle {
  label: string;
  color: string;
  water: boolean;
}

/** What the ground under one entity is like. Terrain does not change with time, so this
 *  is computed once and never refetched. */
export interface Place {
  biome: string;
  color: string;
  /** `0` at the shore, `1` at the highest point on the map. */
  elevation: number;
  temperature: number;
  precipitation: number;
  on_river: boolean;
  coastal: boolean;
}

/**
 * The substrate the timeline is projected onto. Fetched once per world — unlike a
 * snapshot, nothing in here moves when the scrubber does.
 */
export interface Terrain {
  aspect: number;
  sea_level: number;
  coast: Ring[];
  /** One flat loop per cell, index-aligned with every array below. */
  cells: number[][];
  is_land: boolean[];
  lake: boolean[];
  height: number[];
  temperature: number[];
  precipitation: number[];
  /** Index into `palette`. */
  biome: number[];
  palette: BiomeStyle[];
  rivers: River[];
  places: Record<string, Place>;
  summary: {
    land_fraction: number;
    cells: number;
    islands: number;
    rivers: number;
    lake_cells: number;
    coast_points: number;
    temperature_min: number;
    temperature_max: number;
    biomes: [string, number][];
  };
}

/** Which quantity the terrain layer is shaded by. */
export type Layer = "biome" | "height" | "temperature" | "precipitation" | "none";

// ---------------------------------------------------------------- authoring

export type ValueKind = "text" | "int" | "float" | "bool";

/**
 * A fact as it was *authored*, not as it renders at a date.
 *
 * The difference is the whole reason these types exist. `Fact` — what the map receives —
 * has no `from`/`to` and a value stringified through `Display`, so a form bound to it and
 * saved back would resolve `@evt_siege_of_marrow` into nothing and turn 9000 into "9000".
 */
export interface FactRecord {
  attr: string;
  value: string | number | boolean;
  kind: ValueKind;
  from: string | null;
  to: string | null;
}

export interface EntityRecord {
  id: string;
  name: string;
  type: string;
  primitive: string | null;
  existence_from: string | null;
  existence_to: string | null;
  parents: string[];
  facts: FactRecord[];
  marker: [number, number] | null;
  shape: [number, number][];
  body: string;
  path: string;
  /** Content hash of the file as it was read. Sent back on save; a mismatch is refused. */
  revision: string | null;
}

export interface EventRecord {
  id: string;
  name: string;
  kind: string;
  date: string;
  participants: string[];
  location: string | null;
  path: string;
  revision: string | null;
}

/** What the writer sends back. Dates are strings exactly as typed. */
export interface EntityDraft {
  id: string;
  name: string;
  type: string;
  existence_from: string | null;
  existence_to: string | null;
  parents: string[];
  facts: { attr: string; value: string | number | boolean; from: string | null; to: string | null }[];
  marker: [number, number] | null;
  shape: [number, number][];
  /** `null` leaves the prose exactly as it is. */
  body: string | null;
}

export interface EventDraft {
  id: string;
  name: string;
  kind: string | null;
  date: string;
  participants: string[];
  location: string | null;
}

export interface Reference {
  by: string;
  name: string;
  how: string;
}

/** What a save would do, before it does it. */
export interface EditPreview {
  files: { path: string; is_new: boolean; diff: DiffLine[] }[];
  resolved: Finding[];
  introduced: Finding[];
  breaks: boolean;
  /** False when saving would rewrite the file rather than patch it — comments at risk. */
  preserves_bytes: boolean;
  reformat_reason: string | null;
  comments_at_risk: string[];
  references: Reference[];
  revision: string | null;
}

export interface SaveResult {
  summary: WorldSummary;
  written: string[];
  revision: string | null;
}

// ---------------------------------------------------------------- the story

/** What the app can honestly claim about the manuscript. */
export type Standing = "unlinked" | "root_missing" | "linked";

/** Where a record sits in the iceberg. `underbuilt` is the one to read first. */
export type Standing4 = "underbuilt" | "load-bearing" | "overbuilt" | "quiet";

/**
 * A scene, for the timeline band and the map's story path.
 *
 * `order` is reading order and `nominal` is when it happens, and they are allowed to
 * disagree — that disagreement is a flashback. Never sort one by the other.
 */
export interface StoryScene {
  id: string;
  name: string;
  nominal: number | null;
  earliest: number | null;
  latest: number | null;
  label: string;
  pov: string | null;
  on_page: string[];
  location: string | null;
  prose: string | null;
  order: number;
  /** From the location's record marker, so it survives dates the location does not. */
  point: [number, number] | null;
  unreadable: string | null;
  words: number | null;
  names: string[];
}

export interface Surfacing {
  id: string;
  name: string;
  standing: Standing4;
  mentions: number;
  scenes: string[];
  referenced_by: number;
  appears_in: number;
  cast_in: number;
  facts: number;
  prose_bytes: number;
  first_seen: string | null;
}

export interface Story {
  standing: Standing;
  scenes_read: number;
  surfaced: number;
  total: number;
  /** Rounded percentage, or null for an empty world — 0% of nothing says nothing. */
  percent: number | null;
  records: Surfacing[];
  unreadable: { scene: string; reason: string }[];
  root: string | null;
}

export interface Passage {
  scene: string;
  file: string;
  heading: string | null;
  text: string;
  words: number;
  truncated: boolean;
}

export interface SceneRecord {
  id: string;
  name: string;
  date: string;
  pov: string | null;
  on_page: string[];
  location: string | null;
  prose: string | null;
  path: string;
  revision: string | null;
}

export interface SceneDraft {
  id: string;
  name: string;
  date: string;
  pov: string | null;
  on_page: string[];
  location: string | null;
  prose: string | null;
}

// ------------------------------------------------------------- descent

/** One row of the lineage chart: a record with a lifespan. */
export interface Life {
  id: string;
  name: string;
  type: string;
  primitive: string | null;
  /** Steps down from the oldest recorded forebear. Zero is nobody's child. */
  generation: number;
  parents: string[];
  /** The certain core of the lifespan… */
  from: number | null;
  to: number | null;
  /** …and the possible window around it, which is what the feathered ends draw. */
  earliest: number | null;
  latest: number | null;
  label: string;
}

export interface Tenure {
  holder: string;
  name: string;
  from: number | null;
  to: number | null;
  earliest: number | null;
  latest: number | null;
}

/**
 * A thing passed from one record to the next.
 *
 * `title` is one value with many holders — the Duke of Corrath, held by Maren and then
 * Aldric. `office` is one record's attribute with many values — the Vale's owner, held
 * by the duchy and then the empire. Both draw the same way.
 */
export interface Succession {
  key: string;
  label: string;
  attr: string;
  kind: "title" | "office";
  holders: Tenure[];
  gaps: [number, number][];
  overlaps: [number, number][];
}

export interface Lineage {
  lives: Life[];
  successions: Succession[];
}

// ------------------------------------------------------------- versions

/** `none` — nothing tracks this folder. `nested` — read-only. `root` — everything. */
export type RepoStanding = "none" | "nested" | "root";

export interface Commit {
  id: string;
  full: string;
  summary: string;
  author: string;
  /** Unix seconds. Real-world time, emphatically not the world's own calendar. */
  when: number;
}

export interface Version {
  standing: { kind: RepoStanding; repo: string | null; world: string; note: string | null };
  branch: string | null;
  canon: string | null;
  head: Commit | null;
  dirty: { path: string; state: "new" | "modified" | "deleted" }[];
  unborn: boolean;
}

export interface Branch {
  name: string;
  is_head: boolean;
  /** What deleting it would make unreachable — say this before the second click. */
  ahead: number;
  /** Non-zero means merging is not a fast-forward, and will be refused. */
  behind: number;
  tip: Commit | null;
}

export interface History {
  commits: Commit[];
  scanned: number;
  truncated: boolean;
}

export interface RecordDiff {
  id: string;
  name: string;
  kind: string;
  /** Field names, never line numbers: `existence`, `facts +1 −1`. */
  fields: string[];
  moved: { what: string; from: number | null; to: number | null; days: number }[];
}

export interface Compare {
  rev: string;
  label: string;
  added: RecordDiff[];
  removed: RecordDiff[];
  changed: RecordDiff[];
  resolved: Finding[];
  introduced: Finding[];
  breaks: boolean;
  files: { path: string; diff: DiffLine[] }[];
  more_files: number;
}

// -------------------------------------------------------------- publishing

export type ExportScope = "everything" | "as-of" | "on-the-page";

export interface ExportPreview {
  caption: string;
  bytes: number;
  records: number;
  omitted: number;
  links: number;
  suggested: string;
}

/** False when the page is open in a plain browser rather than the desktop shell. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  openWorld: (path: string) => invoke<WorldSummary>("open_world", { path }),
  examplePath: () => invoke<string | null>("example_world_path"),
  snapshot: (day: number) => invoke<Snapshot>("snapshot", { day }),
  terrain: () => invoke<Terrain | null>("terrain"),
  mapImage: () => invoke<string | null>("map_image"),
  timeline: () => invoke<WorldEvent[]>("timeline"),
  resolveExpr: (expr: string) => invoke<number | null>("resolve_expr", { expr }),
  formatDay: (day: number) => invoke<string>("format_day", { day }),
  checkWorld: () => invoke<Finding[]>("check_world"),
  listProposals: () => invoke<ProposalSummary[]>("list_proposals"),
  proposalDetail: (id: string) => invoke<ProposalDetail>("proposal_detail", { id }),
  decideProposal: (id: string, accept: boolean) =>
    invoke<WorldSummary>("decide_proposal", { id, accept }),

  entityRecord: (id: string) => invoke<EntityRecord>("entity_record", { id }),
  eventRecord: (id: string) => invoke<EventRecord>("event_record", { id }),
  previewEntity: (draft: EntityDraft) => invoke<EditPreview>("preview_entity", { draft }),
  previewEvent: (draft: EventDraft) => invoke<EditPreview>("preview_event", { draft }),
  previewDelete: (id: string) => invoke<EditPreview>("preview_delete", { id }),
  saveEntity: (draft: EntityDraft, revision: string | null, allowReformat = false) =>
    invoke<SaveResult>("save_entity", { draft, revision, allowReformat }),
  saveEvent: (draft: EventDraft, revision: string | null, allowReformat = false) =>
    invoke<SaveResult>("save_event", { draft, revision, allowReformat }),
  saveGeometry: (
    id: string,
    marker: [number, number] | null,
    shape: [number, number][],
    revision: string | null,
  ) => invoke<SaveResult>("save_geometry", { id, marker, shape, revision }),
  deleteRecord: (id: string, revision: string | null) =>
    invoke<SaveResult>("delete_record", { id, revision }),
  terrainPlaces: () => invoke<Record<string, Place>>("terrain_places"),

  scenes: () => invoke<StoryScene[]>("scenes"),
  story: () => invoke<Story>("story"),
  passage: (scene: string) => invoke<Passage>("passage", { scene }),
  resolveProse: (link: string) => invoke<Passage>("resolve_prose", { link }),
  chapters: () => invoke<string[]>("chapters"),
  sceneRecord: (id: string) => invoke<SceneRecord>("scene_record", { id }),
  previewScene: (draft: SceneDraft) => invoke<EditPreview>("preview_scene", { draft }),
  saveScene: (draft: SceneDraft, revision: string | null, allowReformat = false) =>
    invoke<SaveResult>("save_scene", { draft, revision, allowReformat }),

  lineage: () => invoke<Lineage>("lineage"),

  initialWorld: () => invoke<string | null>("initial_world"),
  recentWorlds: () => invoke<string[]>("recent_worlds"),

  versionStatus: () => invoke<Version>("version_status"),
  versionHistory: (limit = 30) => invoke<History>("version_history", { limit }),
  versionBranches: () => invoke<Branch[]>("version_branches"),
  versionCompare: (rev: string) => invoke<Compare>("version_compare", { rev }),
  versionCommit: (message: string) => invoke<Commit>("version_commit", { message }),
  versionBranch: (name: string, switchTo: boolean) =>
    invoke<WorldSummary>("version_branch", { name, switch: switchTo }),
  versionSwitch: (name: string) => invoke<WorldSummary>("version_switch", { name }),
  versionMerge: (target: string) => invoke<string>("version_merge", { target }),
  versionDelete: (name: string) => invoke<void>("version_delete", { name }),
  versionDiscard: () => invoke<[number, WorldSummary]>("version_discard"),

  previewExport: (scope: ExportScope, at: string | null) =>
    invoke<ExportPreview>("preview_export", { scope, at }),
  writeExport: (scope: ExportScope, at: string | null, path: string, overwrite: boolean) =>
    invoke<{ path: string; bytes: number }>("write_export", { scope, at, path, overwrite }),
};
