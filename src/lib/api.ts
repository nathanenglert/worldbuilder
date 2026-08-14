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
  span: [number, number];
  change_points: number[];
  undeclared_types: string[];
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
};
