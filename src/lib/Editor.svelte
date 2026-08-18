<script lang="ts">
  /**
   * Writing to the world, with what the change would do shown first.
   *
   * Two decisions shape everything here.
   *
   * **This edits the record, not the snapshot.** The map's `Entity` is a rendered view at
   * a date: its facts have lost their `from`/`to` to a certainty flag, its values have
   * been stringified, and anything not valid at the scrubber's day is missing entirely.
   * Binding a form to that and saving would resolve `@evt_siege_of_marrow` into nothing
   * and turn every number into text. So the panel fetches the raw record and holds it
   * independently — which is also why scrubbing the timeline while editing is harmless
   * and needs no prompt.
   *
   * **Save is unreachable until the impact has been shown.** Not a warning to scroll
   * past: the button is disabled outside the `reviewed` phase. What it never does is
   * *refuse* — a contradiction relabels the button and explains itself, because it is
   * the writer's world and a deliberate mystery is a plot, not a bug.
   */
  import { untrack } from "svelte";

  import { api } from "./api";
  import type { EditPreview, Finding, Passage, WorldSummary } from "./api";
  import {
    blankDraft,
    blankEventDraft,
    blankFact,
    blankSceneDraft,
    deriveId,
    draftOf,
    eventDraftOf,
    eventPayloadOf,
    payloadOf,
    same,
    sceneDraftOf,
    scenePayloadOf,
    type Draft,
    type EventDraftState,
    type SceneDraftState,
  } from "./draft";
  import DateField from "./form/DateField.svelte";
  import FactRow from "./form/FactRow.svelte";
  import Field from "./form/Field.svelte";
  import IdField from "./form/IdField.svelte";
  import ProseField from "./form/ProseField.svelte";
  import RefField from "./form/RefField.svelte";
  import SuggestField from "./form/SuggestField.svelte";
  import TextInput from "./form/TextInput.svelte";

  let {
    target,
    summary,
    geometry,
    mode,
    holding,
    anchors,
    onmode,
    ongeometry,
    ondirty,
    onsaved,
    onnew,
    onclose,
    onjump,
    onresolve,
  }: {
    /**
     * `focus` is the attribute the form was opened *at*, when the writer arrived by
     * clicking a fact rather than an edit button. `type` is what a *new* record starts
     * out as, which is how "save & new" carries a run of cities forward; it is ignored
     * when there is an id, because an existing record already knows what it is.
     */
    target: {
      kind: "entity" | "event" | "scene";
      id: string | null;
      focus?: string;
      type?: string;
    };
    summary: WorldSummary | null;
    geometry: { marker: [number, number] | null; shape: [number, number][] };
    mode: "browse" | "marker" | "shape";
    /**
     * What the app is holding back rather than carry out over an unsaved draft, said as
     * the subject of a sentence — "Opening the consistency checks". A phrase rather than
     * the intent itself: the form has to *say* what is waiting, not decide about it.
     */
    holding: string | null;
    /** Event expressions for every date box in here. See `DateField`. */
    anchors: string[];
    onmode: (mode: "browse" | "marker" | "shape") => void;
    ongeometry: (g: { marker: [number, number] | null; shape: [number, number][] }) => void;
    ondirty: (dirty: boolean) => void;
    onsaved: (summary: WorldSummary, markerChanged: boolean) => void;
    /** Saved, and start another one like it. The type is the only thing carried over. */
    onnew: (kind: "entity" | "event" | "scene", type: string) => void;
    onclose: () => void;
    onjump: (day: number) => void;
    onresolve: (discard: boolean) => void;
  } = $props();

  type Phase = "loading" | "clean" | "dirty" | "validating" | "reviewed" | "saving" | "failed";

  let phase = $state<Phase>("loading");
  let draft = $state<Draft | null>(null);
  let eventDraft = $state<EventDraftState | null>(null);
  let sceneDraft = $state<SceneDraftState | null>(null);
  /** What the current `prose:` string resolves to, or why it does not. */
  let passage = $state<{ ok: Passage } | { err: string } | null>(null);
  let chapters = $state<string[]>([]);
  let pristine = $state<string>("");
  let pristineMarker = $state<[number, number] | null>(null);
  let revision = $state<string | null>(null);
  let check = $state<EditPreview | null>(null);
  let failure = $state<string | null>(null);
  let allowReformat = $state(false);
  /**
   * The files the last save wrote, or null.
   *
   * The one thing the form never said was that it had worked. Save greyed itself out and
   * that was the whole of it, nine hundred pixels below the fold — so this says which
   * files landed, which is also the local-first promise stated rather than implied.
   */
  let saved = $state<string[] | null>(null);
  /**
   * The box the writer should be typing in, as `list:index`.
   *
   * One key rather than an index per list, because the rule is the same for all of them:
   * a row that was just added is a row nobody asked to look at — they asked to *write*
   * in it, and the caret was still wherever the button was.
   */
  let fresh = $state<string | null>(null);

  const types = $derived(summary?.types ?? []);
  const records = $derived(summary?.records ?? []);
  const ids = $derived(records.map((r) => r.id));
  /**
   * Ids to names, for the reference lists.
   *
   * Those lists offered bare ids until the world summary started carrying names, which
   * asked a writer to recognise `act_maren_vane` while they were typing a parent. The
   * name is the thing they know it by.
   */
  const names = $derived(Object.fromEntries(records.map((r) => [r.id, r.name])));
  /**
   * What this world's facts already call things, most-used first.
   *
   * Offered, never required — the stance `types` takes, for the same reason: writing a
   * new attribute is an ordinary thing to do, and a form that only accepted the existing
   * ones would be stricter than the data model underneath it.
   */
  const attrs = $derived((summary?.attrs ?? []).map((a) => a.name));
  const creating = $derived(target.id === null);
  /**
   * Which box has the writer's attention, as `list:index`.
   *
   * Two ways to earn it and one answer, because they are the same request: a row just
   * added, and — for facts — the row the writer clicked in the inspector. That second one
   * lands on the *first* window of the attribute, not the one in force on the scrubbed
   * day: the form does not know the day, and knowing it would not help, since
   * `to: @evt_siege_of_marrow` is an expression and resolving six of them to find one row
   * would be a round trip per fact. The windows sit together anyway.
   */
  const attention = $derived(
    fresh ??
      (target.focus && draft
        ? `facts:${draft.facts.findIndex((f) => f.attr === target.focus)}`
        : null),
  );
  const primitive = $derived(types.find((t) => t.name === draft?.type)?.primitive ?? null);

  /**
   * Add a row and put the caret in it.
   *
   * Every one of these lists is a stack of identical boxes, and the button that grows one
   * is at the bottom of it. Clicking `+ parent` and then having to find the box it made
   * is the kind of small tax that adds up over a record with six of them.
   */
  function add<T>(list: T[], row: T, key: string) {
    list.push(row);
    fresh = `${key}:${list.length - 1}`;
  }

  /**
   * A slow answer for an early keystroke must not land on a later one — the idiom the
   * header's date box uses, except that here it also guards the save gate, so anything
   * that makes the draft different retires the answer that is out for the old one.
   */
  let token = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  // ---- loading

  $effect(() => {
    const t = target;
    phase = "loading";
    check = null;
    // Same retirement as below, for the other way a draft stops existing: a check still
    // out for the record being left must not answer for the one arriving.
    token += 1;
    failure = null;
    allowReformat = false;
    saved = null;
    fresh = null;
    void load(t);
  });

  async function load(t: {
    kind: "entity" | "event" | "scene";
    id: string | null;
    type?: string;
  }) {
    try {
      if (t.kind === "scene") {
        const record = t.id ? await api.sceneRecord(t.id) : null;
        const d = record ? sceneDraftOf(record) : blankSceneDraft();
        revision = record?.revision ?? null;
        sceneDraft = d;
        draft = null;
        eventDraft = null;
        pristine = JSON.stringify(d);
        chapters = await api.chapters();
        void resolveProse(d.prose);
      } else if (t.kind === "event") {
        const d = t.id ? eventDraftOf(await api.eventRecord(t.id)) : blankEventDraft(t.type);
        revision = t.id ? (await api.eventRecord(t.id)).revision : null;
        eventDraft = d;
        draft = null;
        sceneDraft = null;
        pristine = JSON.stringify(d);
      } else {
        const record = t.id ? await api.entityRecord(t.id) : null;
        const d = record ? draftOf(record) : blankDraft(t.type || types[0]?.name || "place");
        revision = record?.revision ?? null;
        draft = d;
        eventDraft = null;
        sceneDraft = null;
        pristine = JSON.stringify(d);
        pristineMarker = d.marker;
        ongeometry({ marker: d.marker, shape: d.shape });
      }
      phase = "clean";
      ondirty(false);
    } catch (e) {
      failure = String(e).replace(/^Error:\s*/, "");
      phase = "failed";
    }
  }

  // ---- dirty tracking and validation

  const current = $derived.by(() => {
    if (sceneDraft) return JSON.stringify(sceneDraft);
    if (eventDraft) return JSON.stringify(eventDraft);
    if (!draft) return "";
    return JSON.stringify({ ...draft, marker: geometry.marker, shape: geometry.shape });
  });

  $effect(() => {
    // `current` is the *only* thing this watches. Reading `phase` here as well would
    // make the effect depend on state it also writes: reviewed → dirty → validating →
    // reviewed, forever, and the panel never leaves "checking…".
    const now = current;
    untrack(() => {
      if (phase === "loading" || phase === "saving") return;
      const dirty = now !== pristine || creating;
      ondirty(dirty);
      // Whatever answer is in flight was asked about a draft that no longer exists, so
      // it is retired here rather than left to land. It used to land: a blur starts a
      // check at once, and typing through the next keystroke left that check to arrive
      // and set `reviewed` — the one phase that unlocks Save — over a draft it had never
      // seen. The rule is that the impact is shown before the save, and an impact
      // computed for the previous keystroke does not satisfy it.
      token += 1;
      check = null;
      if (!dirty) {
        phase = "clean";
        return;
      }
      phase = "dirty";
      // The last save is still on disk and still true, but it is no longer what is in
      // the boxes, and "written · marrow.md" over a changed draft reads as "no need".
      saved = null;
      schedule();
    });
  });

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(() => void validate(), 400);
  }

  /** Called on blur and on a committed map gesture: no reason to wait out the debounce. */
  function settle() {
    clearTimeout(timer);
    void validate();
  }

  /**
   * Show what the link points at, the way `DateField` shows what a date expression means.
   *
   * Separate from `validate` because it answers a different question and fails for
   * different reasons: a link that goes nowhere is not a reason to refuse a save. A scene
   * whose chapter has not been written yet is an ordinary state of a book.
   */
  let proseToken = 0;
  async function resolveProse(link: string) {
    const mine = ++proseToken;
    const trimmed = link.trim();
    if (trimmed === "") {
      passage = null;
      return;
    }
    try {
      const p = await api.resolveProse(trimmed);
      if (mine === proseToken) passage = { ok: p };
    } catch (e) {
      if (mine === proseToken) passage = { err: String(e).replace(/^Error:\s*/, "") };
    }
  }

  async function validate() {
    if (!draft && !eventDraft && !sceneDraft) return;
    const mine = ++token;
    phase = "validating";
    failure = null;
    try {
      const result = sceneDraft
        ? await api.previewScene(scenePayloadOf(sceneDraft))
        : eventDraft
        ? await api.previewEvent(eventPayloadOf(eventDraft))
        : await api.previewEntity(
            payloadOf({ ...draft!, marker: geometry.marker, shape: geometry.shape }, true),
          );
      if (mine !== token) return;
      check = result;
      phase = "reviewed";
    } catch (e) {
      if (mine !== token) return;
      failure = String(e).replace(/^Error:\s*/, "");
      check = null;
      phase = "failed";
    }
  }

  // ---- saving

  /**
   * `then: "again"` is the "save & new" half: write this, then start another of the same
   * type. It is a run of records that makes it worth having — five cities in a duchy, or
   * the four scenes of a chapter — and every one of them used to cost a trip back out to
   * the header.
   */
  async function save(then: "stay" | "again" = "stay") {
    if (!draft && !eventDraft && !sceneDraft) return;
    phase = "saving";
    failure = null;
    try {
      const result = sceneDraft
        ? await api.saveScene(scenePayloadOf(sceneDraft), revision, allowReformat)
        : eventDraft
        ? await api.saveEvent(eventPayloadOf(eventDraft), revision, allowReformat)
        : await api.saveEntity(
            payloadOf({ ...draft!, marker: geometry.marker, shape: geometry.shape }, true),
            revision,
            allowReformat,
          );
      const markerChanged = !same(pristineMarker, geometry.marker);
      revision = result.revision;
      pristine = current;
      pristineMarker = geometry.marker;
      phase = "clean";
      // The impact preview is written in the conditional — "what this *would* do" — and
      // it outlived the deed. A diff of a change already on disk is not a preview of
      // anything; it is the panel describing the past in the future tense.
      check = null;
      saved = result.written;
      ondirty(false);
      onmode("browse");
      onsaved(result.summary, markerChanged);
      if (then === "again") onnew(target.kind, carried());
    } catch (e) {
      failure = String(e).replace(/^Error:\s*/, "");
      phase = "failed";
    }
  }

  function revert() {
    const back = JSON.parse(pristine);
    if (sceneDraft) {
      sceneDraft = back;
      void resolveProse(back.prose);
    } else if (eventDraft) {
      eventDraft = back;
    } else {
      draft = back;
      ongeometry({ marker: back.marker, shape: back.shape });
    }
    check = null;
    phase = "clean";
    ondirty(false);
  }

  // ---- removing

  /**
   * Deleting asks a different question from editing — not "what does this settle" but
   * "what still points at this" — so it gets its own confirmation rather than a mode of
   * the save button. If anything anchors a date to the record, the backend refuses and
   * says so; a world that no longer resolves its own dates is not an outcome to offer.
   */
  let removing = $state(false);
  let removal = $state<EditPreview | null>(null);

  async function askToRemove() {
    if (!target.id) return;
    failure = null;
    try {
      removal = await api.previewDelete(target.id);
      removing = true;
    } catch (e) {
      failure = String(e).replace(/^Error:\s*/, "");
    }
  }

  async function confirmRemove() {
    if (!target.id) return;
    phase = "saving";
    try {
      const result = await api.deleteRecord(target.id, revision);
      removing = false;
      ondirty(false);
      onsaved(result.summary, true);
      onclose();
    } catch (e) {
      failure = String(e).replace(/^Error:\s*/, "");
      phase = "failed";
      removing = false;
    }
  }

  // ---- the form's own behaviour

  /**
   * A new id follows the name until the writer edits it. Renaming an existing record is
   * out of scope, so this only ever runs while creating.
   */
  $effect(() => {
    if (!draft || !creating || draft.idPinned) return;
    const suggested = deriveId(draft.name, primitive);
    if (draft.id !== suggested) draft.id = suggested;
  });

  $effect(() => {
    if (!eventDraft || !creating || eventDraft.idPinned) return;
    const suggested = deriveId(eventDraft.name, "event");
    if (eventDraft.id !== suggested) eventDraft.id = suggested;
  });

  $effect(() => {
    if (!sceneDraft || !creating || sceneDraft.idPinned) return;
    const suggested = deriveId(sceneDraft.name, "scene");
    if (sceneDraft.id !== suggested) sceneDraft.id = suggested;
  });

  /**
   * Close this fact's window and open the next one with the same attribute.
   *
   * This is the shape almost every real edit takes — a population changes, a title
   * passes, a border moves — and doing it by hand means remembering not to just
   * overwrite the number. Nothing is ever silently overwritten: new truth closes an
   * interval and opens another, so the form offers exactly that in one click.
   */
  function split(i: number) {
    if (!draft) return;
    const f = draft.facts[i];
    const at = f.to.trim() === "" ? "" : f.to;
    draft.facts.splice(i + 1, 0, {
      attr: f.attr,
      value: f.value,
      kind: f.kind,
      pinned: f.pinned,
      from: at,
      to: "",
    });
  }

  const breaksDefinitely = $derived((check?.introduced ?? []).some((f) => f.certainty === "definite"));
  const savable = $derived(
    (phase === "reviewed" || phase === "failed") &&
      !!check &&
      (check.preserves_bytes || allowReformat),
  );

  function tagOf(f: Finding): string {
    return f.certainty === "definite" ? "breaks" : "opens";
  }

  /** What a record of this kind is, in the one word "save & new" hands to the next one. */
  function carried(): string {
    if (draft) return draft.type;
    if (eventDraft) return eventDraft.kind;
    // A scene has no type. The button still earns its place — scenes come in runs.
    return "";
  }

  // ---- the bar

  /** What the form is doing, in a word. */
  const doing = $derived.by(() => {
    if (phase === "saving") return "saving";
    if (phase === "dirty" || phase === "validating") return "checking";
    if (phase === "failed") return "stuck";
    if (saved) return "saved";
    if (phase === "clean") return creating ? "unwritten" : "clean";
    return breaksDefinitely ? "breaks" : "ready";
  });

  /**
   * One line for what saving would do, beside the button that would do it.
   *
   * The panel has always answered this at length, and it answered it a full screen below
   * the last field — so the writer scrolled past the form to read the verdict and back up
   * to keep typing. The long answer stays where it is; this is the same thing said in the
   * one place a decision gets made.
   */
  const verdict = $derived.by(() => {
    if (phase === "saving") return "writing to disk";
    if (phase === "dirty" || phase === "validating") return "working out what this would do";
    if (failure) return failure;
    if (saved) return saved.join(" · ");
    if (phase === "clean") {
      return creating ? "nothing written yet" : "nothing to save · this matches the file";
    }
    if (!check) return "";
    if (!check.preserves_bytes && !allowReformat) {
      return "saving would reformat the file — allow it above";
    }
    const settles = check.resolved.length;
    const opens = check.introduced.length;
    if (settles === 0 && opens === 0) return "changes nothing about the world's consistency";
    const said: string[] = [];
    if (settles) said.push(`settles ${settles}`);
    if (opens) said.push(breaksDefinitely ? `breaks ${opens}` : `opens ${opens}`);
    return said.join(" · ");
  });

  /**
   * Cmd/Ctrl-S, which is the one key a writer will try without being told.
   *
   * It does not reach through the gate. The rule that save is unreachable until the
   * impact has been shown is the whole design of this panel, so a press while the check
   * is still pending brings the check *forward* — the same thing a blur does — and the
   * next press saves. Two presses, and the writer saw what they were saving.
   *
   * `preventDefault` regardless: whatever the webview does with Cmd-S, it is not this.
   */
  function hotkey(e: KeyboardEvent) {
    if (e.key !== "s" || !(e.metaKey || e.ctrlKey)) return;
    e.preventDefault();
    if (savable) void save();
    else if (phase === "dirty" || phase === "validating") settle();
  }
</script>

<svelte:window onkeydown={hotkey} />

<!-- Focusable so the keyboard has somewhere to land when a panel closes and the
     button that closed it goes with it. App.svelte does the placing. -->
<aside class="panel" tabindex="-1">
  <div class="bar">
    <button class="back" onclick={onclose}>‹ back to the world</button>
    {#if !creating}
      <button class="remove" onclick={askToRemove}>remove</button>
    {/if}
  </div>

  {#if removing && removal}
    <div class="caution confirm">
      <p><strong>Remove this record?</strong></p>
      {#if removal.references.length}
        <p class="lost">
          {removal.references.length} other record{removal.references.length === 1 ? "" : "s"}
          still point{removal.references.length === 1 ? "s" : ""} at it:
        </p>
        <ul class="refs">
          {#each removal.references as r, i (i)}
            <li>{r.name} <em>{r.how}</em></li>
          {/each}
        </ul>
      {:else}
        <p class="lost">Nothing else points at it.</p>
      {/if}
      <div class="row">
        <button class="danger" onclick={confirmRemove}>remove it</button>
        <button onclick={() => (removing = false)}>keep it</button>
      </div>
    </div>
  {/if}

  {#if holding !== null}
    <!-- The app is holding something back rather than silently throwing away an edit.
         Naming it matters now that every way out of this form arrives here: "discard"
         means something different when the alternative is a new blank record than when
         it is the panel you were reading a minute ago. -->
    <div class="caution confirm">
      <p><strong>Discard your changes?</strong></p>
      <p class="lost">{holding} does not keep them.</p>
      <div class="row">
        <button class="danger" onclick={() => onresolve(true)}>discard</button>
        <button onclick={() => onresolve(false)}>keep editing</button>
      </div>
    </div>
  {/if}

  {#if phase === "loading"}
    <p class="empty">Loading…</p>
  {:else if draft}
    <header>
      <p class="kind">{creating ? "new record" : `editing · ${draft.type}`}</p>
      <h2>{draft.name.trim() === "" ? "Untitled" : draft.name}</h2>
      <p class="id">{draft.id || "—"}</p>
    </header>

    <p class="note">editing the record · the date only changes what the map shows</p>

    <Field label="name">
      <TextInput bind:value={draft.name} onblur={settle} />
    </Field>

    <IdField
      bind:value={draft.id}
      locked={!creating}
      taken={ids}
      onedit={() => (draft!.idPinned = true)}
    />

    <Field label="type" hint="an undeclared type still loads · it is only reported">
      <SuggestField
        bind:value={draft.type}
        listId="entity-types"
        options={types.map((t) => t.name)}
        onsettled={settle}
      />
    </Field>

    <!-- Both of these lists were on the record and not on the form, which meant a save
         wrote them away. They are here first because they must be *sent*, and second
         because they are worth editing: the aliases are what the book is scanned for,
         and the parents are the whole of what the lineage view draws. -->
    <p class="label">Also called</p>
    <p class="aside-note">what the prose calls it · the manuscript is scanned for these</p>
    <div class="facts">
      {#each draft.aka as _, i (i)}
        <div class="participant">
          <TextInput
            bind:value={draft.aka[i]}
            takeFocus={attention === `aka:${i}`}
            onblur={settle}
          />
          <button class="drop" onclick={() => draft!.aka.splice(i, 1)}>remove</button>
        </div>
      {/each}
      <button class="add" onclick={() => add(draft!.aka, "", "aka")}>+ another name</button>
    </div>

    <div class="two">
      <DateField
        bind:value={draft.existence_from}
        label="exists from"
        {anchors}
        {onjump}
        onsettled={settle}
      />
      <DateField bind:value={draft.existence_to} label="until" {anchors} {onjump} onsettled={settle} />
    </div>
    <p class="aside-note">Leave these empty if nobody knows. `?` is a perfectly good answer.</p>

    <p class="label">Descends from</p>
    <p class="aside-note">parentage · a bloodline is these edges and nothing else</p>
    <div class="facts">
      {#each draft.parents as _, i (i)}
        <div class="participant">
          <RefField
            bind:value={draft.parents[i]}
            {ids}
            {names}
            listId="entity-parents"
            takeFocus={attention === `parents:${i}`}
            onsettled={settle}
          />
          <button class="drop" onclick={() => draft!.parents.splice(i, 1)}>remove</button>
        </div>
      {/each}
      <button class="add" onclick={() => add(draft!.parents, "", "parents")}>+ parent</button>
    </div>

    <p class="label">Facts here</p>
    <div class="facts">
      {#each draft.facts as _, i (i)}
        <FactRow
          bind:fact={draft.facts[i]}
          {attrs}
          {anchors}
          {ids}
          {names}
          sought={attention === `facts:${i}`}
          onremove={() => draft!.facts.splice(i, 1)}
          onsplit={() => split(i)}
          {onjump}
          onsettled={settle}
        />
      {/each}
      <button class="add" onclick={() => add(draft!.facts, blankFact(), "facts")}>+ fact</button>
    </div>

    <p class="label">On the map</p>
    <div class="geometry">
      <div class="geo-row">
        <span class="geo-what"
          >{geometry.marker
            ? `marker at ${geometry.marker[0].toFixed(3)}, ${geometry.marker[1].toFixed(3)}`
            : "no marker"}</span
        >
        <button class:on={mode === "marker"} onclick={() => onmode(mode === "marker" ? "browse" : "marker")}>
          {mode === "marker" ? "placing…" : geometry.marker ? "move" : "place"}
        </button>
        {#if geometry.marker}
          <button
            class="drop"
            onclick={() => {
              ongeometry({ marker: null, shape: geometry.shape });
              settle();
            }}>clear</button
          >
        {/if}
      </div>
      <div class="geo-row">
        <span class="geo-what"
          >{geometry.shape.length > 2
            ? `outline of ${geometry.shape.length} points`
            : "no outline"}</span
        >
        <button class:on={mode === "shape"} onclick={() => onmode(mode === "shape" ? "browse" : "shape")}>
          {mode === "shape" ? "drawing…" : geometry.shape.length > 2 ? "adjust" : "draw"}
        </button>
        {#if geometry.shape.length > 0}
          <button
            class="drop"
            onclick={() => {
              ongeometry({ marker: geometry.marker, shape: [] });
              settle();
            }}>clear</button
          >
        {/if}
      </div>
    </div>

    <Field label="prose" hint="never parsed · write freely">
      <ProseField
        bind:value={draft.body}
        placeholder="What is this, in a sentence or two?"
        onsettled={settle}
      />
    </Field>
  {:else if eventDraft}
    <header>
      <p class="kind">{creating ? "new event" : "editing · event"}</p>
      <h2>{eventDraft.name.trim() === "" ? "Untitled" : eventDraft.name}</h2>
      <p class="id">{eventDraft.id || "—"}</p>
    </header>

    <Field label="name">
      <TextInput bind:value={eventDraft.name} onblur={settle} />
    </Field>

    <IdField
      bind:value={eventDraft.id}
      locked={!creating}
      taken={ids}
      onedit={() => (eventDraft!.idPinned = true)}
    />

    <Field label="kind" hint="battle · conquest · oath · anything you like">
      <TextInput bind:value={eventDraft.kind} mono onblur={settle} />
    </Field>

    <DateField bind:value={eventDraft.date} label="date" {anchors} {onjump} onsettled={settle} />

    <Field label="where">
      <RefField bind:value={eventDraft.location} {ids} {names} listId="event-location" onsettled={settle} />
    </Field>

    <p class="label">Who was there</p>
    <div class="facts">
      {#each eventDraft.participants as _, i (i)}
        <div class="participant">
          <RefField
            bind:value={eventDraft.participants[i]}
            {ids}
            {names}
            listId="event-participants"
            takeFocus={attention === `participants:${i}`}
            onsettled={settle}
          />
          <button class="drop" onclick={() => eventDraft!.participants.splice(i, 1)}>remove</button>
        </div>
      {/each}
      <button class="add" onclick={() => add(eventDraft!.participants, "", "participants")}>
        + participant
      </button>
    </div>
  {:else if sceneDraft}
    <header>
      <p class="kind">{creating ? "new scene" : "editing · scene"}</p>
      <h2>{sceneDraft.name.trim() === "" ? "Untitled" : sceneDraft.name}</h2>
      <p class="id">{sceneDraft.id || "—"}</p>
    </header>

    <Field label="name">
      <TextInput bind:value={sceneDraft.name} onblur={settle} />
    </Field>

    <IdField
      bind:value={sceneDraft.id}
      locked={!creating}
      taken={ids}
      onedit={() => (sceneDraft!.idPinned = true)}
    />

    <DateField
      bind:value={sceneDraft.date}
      label="when it is set"
      {anchors}
      {onjump}
      onsettled={settle}
    />

    <Field label="point of view" hint="whose eyes · optional">
      <RefField bind:value={sceneDraft.pov} {ids} {names} listId="scene-pov" onsettled={settle} />
    </Field>

    <Field label="where">
      <RefField bind:value={sceneDraft.location} {ids} {names} listId="scene-location" onsettled={settle} />
    </Field>

    <!-- The link, with what it resolves to underneath — the same move `DateField` makes
         for a date expression. A writer learns the grammar by watching it answer. -->
    <Field label="prose" hint="ch12.md#the-breach · relative to the manuscript root">
      <SuggestField
        bind:value={sceneDraft.prose}
        options={chapters}
        listId="scene-prose"
        placeholder="chapter.md#heading"
        onsettled={() => {
          void resolveProse(sceneDraft!.prose);
          settle();
        }}
      />
    </Field>

    {#if passage}
      {#if "ok" in passage}
        <p class="resolved">
          → {passage.ok.file}{passage.ok.heading ? ` · “${passage.ok.heading}”` : ""} · {passage
            .ok.words} words
        </p>
        <!-- The heading is already named on the line above, so the preview starts at the
             prose. Repeating it would waste the only three lines this gets. -->
        <p class="quote">
          {passage.ok.text.replace(/^#{1,6} .*\n+/, "").slice(0, 220).trim()}…
        </p>
      {:else}
        <!-- Not an error state for the form: a chapter that is not written yet is a
             normal thing for a scene to be pointed at. Save stays reachable. -->
        <p class="resolved warn">→ {passage.err}</p>
      {/if}
    {:else if sceneDraft.prose.trim() === ""}
      <p class="resolved quiet">→ not linked to any prose yet</p>
    {/if}

    <p class="label">Who is on the page</p>
    <div class="facts">
      {#each sceneDraft.onPage as _, i (i)}
        <div class="participant">
          <RefField
            bind:value={sceneDraft.onPage[i]}
            {ids}
            {names}
            listId="scene-on-page"
            takeFocus={attention === `onPage:${i}`}
            onsettled={settle}
          />
          <button class="drop" onclick={() => sceneDraft!.onPage.splice(i, 1)}>remove</button>
        </div>
      {/each}
      <button class="add" onclick={() => add(sceneDraft!.onPage, "", "onPage")}>+ name</button>
    </div>
  {/if}

  <!-- ---- what this would do -->

  {#if failure}
    <p class="caution bad failure">{failure}</p>
  {/if}

  {#if check && !check.preserves_bytes}
    <div class="caution">
      <p><strong>Saving would reformat this file.</strong> {check.reformat_reason}</p>
      {#if check.comments_at_risk.length}
        <p class="lost">{check.comments_at_risk.length} comment(s) would not survive.</p>
      {/if}
      <label class="allow">
        <input type="checkbox" bind:checked={allowReformat} />
        rewrite it anyway
      </label>
    </div>
  {/if}

  {#if check}
    <p class="label">What this would do</p>
    {#if check.resolved.length === 0 && check.introduced.length === 0}
      <p class="empty">Nothing about the world's consistency changes.</p>
    {:else}
      <div class="effects">
        {#each check.resolved as f, i (i)}
          <div class="effect good">
            <span class="tag">settles</span>
            <p>{f.message}</p>
          </div>
        {/each}
        {#each check.introduced as f, i (i)}
          <div class="effect" class:bad={f.certainty === "definite"}>
            <span class="tag">{tagOf(f)}</span>
            <p>{f.message}</p>
          </div>
        {/each}
      </div>
    {/if}

    {#each check.files as file, i (i)}
      <p class="path">{file.path}{file.is_new ? " · new" : ""}</p>
      {#if file.diff.length}
        <pre>{#each file.diff as line, j (j)}<span class={line.tag === "+" ? "add" : "del"}
              >{line.tag} {line.text}</span
            >{"\n"}{/each}</pre>
      {:else}
        <p class="empty">No change to this file.</p>
      {/if}
    {/each}
  {/if}

  {#if breaksDefinitely}
    <p class="explain warn">
      This adds a contradiction that no reading of the dates rescues. Saving is still
      allowed — it is your world — but the consistency panel will report it.
    </p>
  {/if}

  {#if draft || eventDraft || sceneDraft}
    <!-- Pinned, because the decision it carries is made from wherever the writer happens
         to be. This form runs to fifteen hundred pixels on a record with six facts, and
         the save button used to sit nine hundred below the last thing on screen — so the
         only way to find out whether a change was safe was to leave the field you were
         typing in. -->
    <div class="actions">
      <button class="save" disabled={!savable || phase === "saving"} onclick={() => save()}>
        {phase === "saving" ? "saving…" : breaksDefinitely ? "Save anyway" : "Save"}
      </button>
      {#if creating}
        <!-- Only while creating. Coming back to an existing record to change one fact is
             not the start of a run, and a button that offers to make another Marrow there
             reads as a mode rather than a shortcut. -->
        <button disabled={!savable || phase === "saving"} onclick={() => save("again")}>
          Save &amp; new
        </button>
      {/if}
      <button disabled={phase === "clean" || phase === "saving"} onclick={revert}>Revert</button>
      <span class="doing" class:warn={phase === "failed" || breaksDefinitely}>{doing}</span>
      <span class="verdict" class:warn={phase === "failed"}>{verdict}</span>
    </div>
  {/if}
</aside>

<style>
  /* The one thing this panel does not share: the sticky action bar owns the bottom
     padding, so it can sit flush against the bottom of the scrollport rather than
     floating twenty pixels above it. */
  aside.panel {
    padding-bottom: 0;
  }

  /* What the link means, in `DateField`'s voice: an answer, not a validation message. */
  .resolved {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--accent);
    margin-top: -4px;
  }

  .resolved.warn {
    color: var(--warn);
  }

  .resolved.quiet {
    color: var(--ink-3);
  }

  .quote {
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-3);
    padding: 8px 10px;
    background: var(--surface);
    border-left: 2px solid var(--rule-strong);
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .remove {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .remove:hover {
    color: var(--warn);
  }

  .refs {
    margin: 0 0 8px;
    padding-left: 16px;
    font-size: 12px;
  }

  .refs em {
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--ink-3);
    font-style: normal;
  }

  .note,
  .aside-note {
    margin: 0;
    padding-left: 11px;
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--ink-3);
  }

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .facts {
    display: grid;
    gap: 8px;
  }

  .participant {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .add {
    justify-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .add:hover {
    color: var(--accent);
  }

  .drop {
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--ink-3);
  }

  .drop:hover {
    color: var(--warn);
  }

  .geometry {
    display: grid;
    gap: 1px;
    background: var(--rule);
    border: 1px solid var(--rule);
  }

  .geo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    background: var(--surface);
  }

  .geo-what {
    flex: 1;
    font-family: var(--f-mono);
    font-size: 11px;
    color: var(--ink-2);
  }

  .geo-row button {
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    padding: 2px 7px;
    border: 1px solid var(--rule-strong);
    color: var(--ink-3);
  }

  .geo-row button:hover {
    color: var(--accent);
  }

  .geo-row button.on {
    color: var(--accent);
    background: var(--accent-soft);
  }

  .geo-row .drop {
    border: none;
  }

  .caution p {
    margin: 0 0 6px;
  }

  .confirm .row {
    display: flex;
    gap: 10px;
  }

  .confirm button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  /* The one button in the form that loses what the writer typed. */
  .confirm .danger:hover {
    color: var(--danger);
  }

  .confirm button:hover {
    color: var(--accent);
  }

  .lost {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--warn);
  }

  .allow {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-2);
  }

  .effects {
    display: grid;
    gap: 6px;
  }

  .effect {
    display: grid;
    gap: 3px;
    padding: 8px 10px;
    background: var(--surface);
    border-left: 2px solid var(--era);
  }

  .effect.good {
    border-left-color: var(--accent);
  }

  .effect.bad {
    border-left-color: var(--warn);
  }

  .effect p {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-2);
  }

  .tag {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .path {
    margin: 6px 0 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  pre {
    margin: 0;
    padding: 8px 10px;
    overflow-x: auto;
    background: var(--surface);
    border: 1px solid var(--rule);
    font-family: var(--f-mono);
    font-size: 11px;
    line-height: 1.5;
  }

  .add {
    color: var(--ink-3);
  }

  pre .del {
    color: var(--warn);
  }

  .actions {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 10px;
    margin: 6px -20px 0;
    padding: 11px 20px 14px;
    background: var(--paper);
    border-top: 1px solid var(--rule);
  }

  .actions button {
    padding: 6px 14px;
    border: 1px solid var(--rule-strong);
    font-family: var(--f-mono);
    font-size: 11px;
    letter-spacing: 0.06em;
    color: var(--ink-2);
  }

  .actions button:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
  }

  .actions button:disabled {
    color: var(--rule-strong);
    border-color: var(--rule);
    cursor: default;
  }

  .actions .save {
    color: var(--accent);
  }

  /* The phase, in the header's chip voice — one word, read before the sentence. */
  .doing {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .verdict {
    flex: 1;
    min-width: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    line-height: 1.4;
    color: var(--ink-3);
  }

  .doing.warn,
  .verdict.warn {
    color: var(--warn);
  }

  .explain.warn {
    color: var(--warn);
  }

  /* The engine's own words rather than this panel's, so they are set in the mono the
     rest of the engine's words are set in. Everything else comes from `.caution.bad`. */
  .failure {
    font-family: var(--f-mono);
    font-size: 11.5px;
  }

  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--ink-3);
  }

</style>
