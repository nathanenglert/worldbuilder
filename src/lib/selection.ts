/**
 * What a selected id actually is.
 *
 * Selection used to be a bare string, and the Inspector resolved it against
 * `snapshot.entities` alone — which is filtered by date. Everything that was not an
 * entity alive on the scrubbed day resolved to nothing, and the panel fell back to the
 * overview list without a word: scrubbing past a lifespan silently deselected, clicking a
 * finding about an event landed nowhere, and an id that named no record at all looked
 * exactly the same as having clicked "‹ all present". One blank stood for four different
 * answers, and none of them was given.
 *
 * So resolution is a value, and it has a state. The panel can then say which of the four
 * it is looking at.
 *
 * The kind is derived rather than carried. Ids are unique across the whole world —
 * entities and events share one namespace, and `guard_rename` refuses a new record that
 * collides with anything `world.knows` — so what a record *is* follows from its id. A
 * `kind` passed in beside it would be a second copy of that fact, free to go stale, and
 * most callers do not have it anyway: a finding knows a subject, a comparison row knows a
 * path, and neither knows what it named.
 *
 * Plain functions with no Svelte in sight, for the reason `draft.ts` gives: the thing
 * that must not go wrong here is best verified by reading.
 */

import type { Entity, Snapshot, StoryScene, WorldEvent } from "./api";

export type Selection =
  /** Nothing is selected. The panel shows the world, which is the right default. */
  | { state: "none" }
  /** An id the snapshot did not know, whose record is being fetched. */
  | { state: "looking"; id: string }
  /** An entity, alive on the scrubbed day. The only state the panel used to have. */
  | { state: "present"; id: string; kind: "entity"; entity: Entity }
  /** A real record, on a day it does not reach. */
  | {
      state: "elsewhere";
      id: string;
      kind: "entity";
      name: string;
      type: string;
      /** Its existence as authored — `0771-06-12 to 0811~` — not resolved to numbers. */
      window: string;
      /** Somewhere inside that window, so the panel can offer to take the writer there. */
      goto: { day: number; label: string } | null;
    }
  | { state: "event"; id: string; kind: "event"; event: WorldEvent }
  | { state: "scene"; id: string; kind: "scene"; scene: StoryScene }
  /** The id names nothing. A reference to a record never written, or one since removed. */
  | { state: "unknown"; id: string; kind: null };

/** Which record kind an editor should be opened for, when there is one. */
export type EditableKind = "entity" | "event" | "scene";

/**
 * Everything answerable from what the app already has in hand.
 *
 * Order matters and matches `plan_delete`: entities, then events, then scenes. `looking`
 * is not a failure — it is the one question this cannot answer, because the snapshot is
 * date-filtered and the timeline is not the whole world.
 */
export function resolveLocally(
  id: string | null,
  snapshot: Snapshot | null,
  events: WorldEvent[],
  scenes: StoryScene[],
): Selection {
  if (id === null) return { state: "none" };

  const entity = snapshot?.entities.find((e) => e.id === id);
  if (entity) return { state: "present", id, kind: "entity", entity };

  const event = events.find((e) => e.id === id);
  if (event) return { state: "event", id, kind: "event", event };

  const scene = scenes.find((s) => s.id === id);
  if (scene) return { state: "scene", id, kind: "scene", scene };

  return { state: "looking", id };
}

/** The id under a selection, or `null` when there is nothing to point at. */
export function idOf(selection: Selection): string | null {
  return selection.state === "none" ? null : selection.id;
}

/**
 * What to call this in one phrase, or `null` when it has no name to call it by.
 *
 * `unknown` and `looking` deliberately return nothing rather than the id: an id is what
 * the writer would be shown *instead* of a name, and somewhere that matters — the back
 * mark's label — a bare id would read as a place they had never been.
 */
export function nameOf(selection: Selection): string | null {
  switch (selection.state) {
    case "present":
      return selection.entity.name;
    case "elsewhere":
      return selection.name;
    case "event":
      return selection.event.name;
    case "scene":
      return selection.scene.name;
    default:
      return null;
  }
}

/** What the `edit` button should open, or `null` when there is nothing to edit yet. */
export function editableKind(selection: Selection): EditableKind | null {
  switch (selection.state) {
    case "present":
    case "elsewhere":
      return "entity";
    case "event":
      return "event";
    case "scene":
      return "scene";
    default:
      return null;
  }
}

/**
 * An existence span the way it was written down.
 *
 * `?` rather than a blank or a guess: it is what the app calls an unstated date
 * everywhere else, and an unstated one is a perfectly good answer.
 */
export function existenceWindow(from: string | null, to: string | null): string {
  return `${from ?? "?"} to ${to ?? "?"}`;
}
