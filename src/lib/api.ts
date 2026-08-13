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

/** False when the page is open in a plain browser rather than the desktop shell. */
export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const api = {
  openWorld: (path: string) => invoke<WorldSummary>("open_world", { path }),
  examplePath: () => invoke<string | null>("example_world_path"),
  snapshot: (day: number) => invoke<Snapshot>("snapshot", { day }),
  timeline: () => invoke<WorldEvent[]>("timeline"),
  resolveExpr: (expr: string) => invoke<number | null>("resolve_expr", { expr }),
  formatDay: (day: number) => invoke<string>("format_day", { day }),
  checkWorld: () => invoke<Finding[]>("check_world"),
  listProposals: () => invoke<ProposalSummary[]>("list_proposals"),
  proposalDetail: (id: string) => invoke<ProposalDetail>("proposal_detail", { id }),
  decideProposal: (id: string, accept: boolean) =>
    invoke<WorldSummary>("decide_proposal", { id, accept }),
};
