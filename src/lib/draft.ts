/**
 * Turning a record into something a form can hold, and back again.
 *
 * All of this is deliberately plain functions with no Svelte in sight, because the one
 * thing that must not go wrong here — losing the authored form of a date, or the type of
 * a value — is the thing easiest to verify by reading.
 */

import type {
  EntityDraft,
  EntityRecord,
  EventDraft,
  EventRecord,
  FactRecord,
  SceneDraft,
  SceneRecord,
  ValueKind,
} from "./api";

/** A fact while it is being edited: everything is a string except the kind. */
export interface DraftFact {
  attr: string;
  value: string;
  kind: ValueKind;
  from: string;
  to: string;
  /** Set once the writer picks a kind by hand, so inference stops guessing over them. */
  pinned: boolean;
}

export interface Draft {
  id: string;
  name: string;
  type: string;
  existence_from: string;
  existence_to: string;
  facts: DraftFact[];
  marker: [number, number] | null;
  shape: [number, number][];
  body: string;
  /** Set once the writer edits the id by hand, so it stops following the name. */
  idPinned: boolean;
}

export interface EventDraftState {
  id: string;
  name: string;
  kind: string;
  date: string;
  participants: string[];
  location: string;
  idPinned: boolean;
}

/**
 * A scene, as the form holds it.
 *
 * `aka` has no analogue here and `prose` is a plain string rather than a path type: the
 * writer types what their editor's "copy link to heading" gave them, and the app resolves
 * it late rather than parsing it into pieces it would then have to reassemble.
 */
export interface SceneDraftState {
  id: string;
  name: string;
  date: string;
  pov: string;
  onPage: string[];
  location: string;
  prose: string;
  idPinned: boolean;
}

/** An empty box means `?`, which is a perfectly good answer. */
const shown = (v: string | null): string => v ?? "";
const sent = (v: string): string | null => (v.trim() === "" ? null : v.trim());

export function factOf(f: FactRecord): DraftFact {
  return {
    attr: f.attr,
    value: String(f.value),
    kind: f.kind,
    from: shown(f.from),
    to: shown(f.to),
    pinned: true,
  };
}

export function blankFact(): DraftFact {
  // One empty row rather than none: an empty list with an "add" button reads as "facts
  // are unusual here", and a blank row reads as "this is where facts go".
  return { attr: "", value: "", kind: "text", from: "", to: "", pinned: false };
}

export function draftOf(record: EntityRecord): Draft {
  return {
    id: record.id,
    name: record.name,
    type: record.type,
    existence_from: shown(record.existence_from),
    existence_to: shown(record.existence_to),
    facts: record.facts.map(factOf),
    marker: record.marker,
    shape: record.shape,
    body: record.body,
    idPinned: true,
  };
}

export function blankDraft(type: string): Draft {
  return {
    id: "",
    name: "",
    type,
    existence_from: "",
    existence_to: "",
    facts: [blankFact()],
    marker: null,
    shape: [],
    body: "",
    idPinned: false,
  };
}

export function eventDraftOf(record: EventRecord): EventDraftState {
  return {
    id: record.id,
    name: record.name,
    kind: record.kind,
    date: record.date,
    participants: record.participants,
    location: shown(record.location),
    idPinned: true,
  };
}

export function blankEventDraft(): EventDraftState {
  return {
    id: "",
    name: "",
    kind: "",
    date: "",
    participants: [],
    location: "",
    idPinned: false,
  };
}

/**
 * A fact value as the type it is meant to be.
 *
 * This is the one place the wire format carries real types rather than strings, and
 * getting it wrong is quiet: a population written as text sorts and compares as text
 * forever after. Hence a visible kind control rather than inference at save time.
 */
function typed(f: DraftFact): string | number | boolean {
  if (f.kind === "bool") return f.value.trim().toLowerCase() === "true";
  if (f.kind === "int") {
    const n = Number.parseInt(f.value, 10);
    return Number.isFinite(n) ? n : 0;
  }
  if (f.kind === "float") {
    const n = Number.parseFloat(f.value);
    return Number.isFinite(n) ? n : 0;
  }
  return f.value;
}

export function payloadOf(d: Draft, includeBody: boolean): EntityDraft {
  return {
    id: d.id.trim(),
    name: d.name.trim(),
    type: d.type.trim(),
    existence_from: sent(d.existence_from),
    existence_to: sent(d.existence_to),
    parents: [],
    // A row nobody filled in is not a fact. Dropping it here is what lets the form open
    // with a blank one without that blank becoming an error the writer has to clear.
    facts: d.facts
      .filter((f) => f.attr.trim() !== "")
      .map((f) => ({ attr: f.attr.trim(), value: typed(f), from: sent(f.from), to: sent(f.to) })),
    marker: d.marker,
    shape: d.shape,
    body: includeBody ? d.body : null,
  };
}

export function sceneDraftOf(record: SceneRecord): SceneDraftState {
  return {
    id: record.id,
    name: record.name,
    date: record.date,
    pov: shown(record.pov),
    onPage: record.on_page,
    location: shown(record.location),
    prose: shown(record.prose),
    idPinned: true,
  };
}

export function blankSceneDraft(): SceneDraftState {
  return {
    id: "",
    name: "",
    date: "",
    pov: "",
    onPage: [],
    location: "",
    prose: "",
    idPinned: false,
  };
}

export function scenePayloadOf(d: SceneDraftState): SceneDraft {
  return {
    id: d.id.trim(),
    name: d.name.trim(),
    date: d.date.trim(),
    pov: sent(d.pov),
    on_page: d.onPage.filter((p) => p.trim() !== ""),
    location: sent(d.location),
    prose: sent(d.prose),
  };
}

export function eventPayloadOf(d: EventDraftState): EventDraft {
  return {
    id: d.id.trim(),
    name: d.name.trim(),
    kind: sent(d.kind),
    date: d.date.trim(),
    participants: d.participants.filter((p) => p.trim() !== ""),
    location: sent(d.location),
  };
}

/** Cheap structural comparison, so "is this dirty" needs no change tracking. */
export function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Guess a value's kind from what has been typed, until the writer says otherwise.
 *
 * `9000` becomes an integer because that is nearly always what is meant. `"9000"` as
 * deliberate text is one click away, and once clicked this stops second-guessing it.
 */
export function inferKind(text: string): ValueKind {
  const t = text.trim();
  if (t === "") return "text";
  if (t === "true" || t === "false") return "bool";
  if (/^-?\d+$/.test(t)) return "int";
  if (/^-?\d*\.\d+$/.test(t)) return "float";
  return "text";
}

const PREFIX: Record<string, string> = {
  actor: "act_",
  polity: "pol_",
  place: "place_",
  thing: "thing_",
  event: "evt_",
  scene: "scn_",
};

/**
 * Suggest an id from a name.
 *
 * Note the world's own convention is not clean — `place_marrow` and `ter_vale_of_corrath`
 * are both the `place` primitive, split on whether the thing has a point or an outline.
 * Guessing that would produce ids that quietly disagree with the ones already there, so
 * this derives from the primitive and leaves the rest to the writer.
 */
export function deriveId(name: string, primitive: string | null): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  if (slug === "") return "";
  return (PREFIX[primitive ?? ""] ?? "") + slug;
}
