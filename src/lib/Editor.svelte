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
    pendingSelect,
    onmode,
    ongeometry,
    ondirty,
    onsaved,
    onclose,
    onjump,
    onresolveselect,
  }: {
    target: { kind: "entity" | "event" | "scene"; id: string | null };
    summary: WorldSummary | null;
    geometry: { marker: [number, number] | null; shape: [number, number][] };
    mode: "browse" | "marker" | "shape";
    pendingSelect: string | null;
    onmode: (mode: "browse" | "marker" | "shape") => void;
    ongeometry: (g: { marker: [number, number] | null; shape: [number, number][] }) => void;
    ondirty: (dirty: boolean) => void;
    onsaved: (summary: WorldSummary, markerChanged: boolean) => void;
    onclose: () => void;
    onjump: (day: number) => void;
    onresolveselect: (discard: boolean) => void;
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

  const types = $derived(summary?.types ?? []);
  const ids = $derived(summary?.ids ?? []);
  const creating = $derived(target.id === null);
  const primitive = $derived(types.find((t) => t.name === draft?.type)?.primitive ?? null);

  // ---- loading

  $effect(() => {
    const t = target;
    phase = "loading";
    check = null;
    failure = null;
    allowReformat = false;
    void load(t);
  });

  async function load(t: { kind: "entity" | "event" | "scene"; id: string | null }) {
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
        const d = t.id ? eventDraftOf(await api.eventRecord(t.id)) : blankEventDraft();
        revision = t.id ? (await api.eventRecord(t.id)).revision : null;
        eventDraft = d;
        draft = null;
        sceneDraft = null;
        pristine = JSON.stringify(d);
      } else {
        const record = t.id ? await api.entityRecord(t.id) : null;
        const d = record ? draftOf(record) : blankDraft(types[0]?.name ?? "place");
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
      if (!dirty) {
        phase = "clean";
        check = null;
        return;
      }
      phase = "dirty";
      check = null;
      schedule();
    });
  });

  // A slow answer for an early keystroke must not land on a later one. Same idiom the
  // header's date box uses.
  let token = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

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

  async function save() {
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
      ondirty(false);
      onmode("browse");
      onsaved(result.summary, markerChanged);
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
</script>

<aside>
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

  {#if pendingSelect !== null}
    <!-- The app is holding a selection back rather than silently throwing away an edit. -->
    <div class="caution confirm">
      <p>Discard your changes?</p>
      <div class="row">
        <button class="danger" onclick={() => onresolveselect(true)}>discard</button>
        <button onclick={() => onresolveselect(false)}>keep editing</button>
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
    />
    {#if creating}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="pin" onfocusin={() => (draft!.idPinned = true)}></div>
    {/if}

    <Field label="type" hint="an undeclared type still loads · it is only reported">
      <SuggestField
        bind:value={draft.type}
        listId="entity-types"
        options={types.map((t) => t.name)}
        onsettled={settle}
      />
    </Field>

    <div class="two">
      <DateField bind:value={draft.existence_from} label="exists from" {onjump} onsettled={settle} />
      <DateField bind:value={draft.existence_to} label="until" {onjump} onsettled={settle} />
    </div>
    <p class="aside-note">Leave these empty if nobody knows. `?` is a perfectly good answer.</p>

    <p class="label">Facts here</p>
    <div class="facts">
      {#each draft.facts as _, i (i)}
        <FactRow
          bind:fact={draft.facts[i]}
          onremove={() => draft!.facts.splice(i, 1)}
          onsplit={() => split(i)}
          {onjump}
          onsettled={settle}
        />
      {/each}
      <button class="add" onclick={() => draft!.facts.push(blankFact())}>+ fact</button>
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

    <IdField bind:value={eventDraft.id} locked={!creating} taken={ids} />

    <Field label="kind" hint="battle · conquest · oath · anything you like">
      <TextInput bind:value={eventDraft.kind} mono onblur={settle} />
    </Field>

    <DateField bind:value={eventDraft.date} label="date" {onjump} onsettled={settle} />

    <Field label="where">
      <RefField bind:value={eventDraft.location} {ids} listId="event-location" onsettled={settle} />
    </Field>

    <p class="label">Who was there</p>
    <div class="facts">
      {#each eventDraft.participants as _, i (i)}
        <div class="participant">
          <RefField
            bind:value={eventDraft.participants[i]}
            {ids}
            listId="event-participants"
            onsettled={settle}
          />
          <button class="drop" onclick={() => eventDraft!.participants.splice(i, 1)}>remove</button>
        </div>
      {/each}
      <button class="add" onclick={() => eventDraft!.participants.push("")}>+ participant</button>
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

    <IdField bind:value={sceneDraft.id} locked={!creating} taken={ids} />

    <DateField bind:value={sceneDraft.date} label="when it is set" {onjump} onsettled={settle} />

    <Field label="point of view" hint="whose eyes · optional">
      <RefField bind:value={sceneDraft.pov} {ids} listId="scene-pov" onsettled={settle} />
    </Field>

    <Field label="where">
      <RefField bind:value={sceneDraft.location} {ids} listId="scene-location" onsettled={settle} />
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
            listId="scene-on-page"
            onsettled={settle}
          />
          <button class="drop" onclick={() => sceneDraft!.onPage.splice(i, 1)}>remove</button>
        </div>
      {/each}
      <button class="add" onclick={() => sceneDraft!.onPage.push("")}>+ name</button>
    </div>
  {/if}

  <!-- ---- what this would do -->

  {#if failure}
    <p class="failure">{failure}</p>
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

  {#if draft || eventDraft || sceneDraft}
    <div class="actions">
      <button class="save" disabled={!savable || phase === "saving"} onclick={save}>
        {phase === "saving" ? "saving…" : breaksDefinitely ? "Save anyway" : "Save"}
      </button>
      <button disabled={phase === "clean" || phase === "saving"} onclick={revert}>Revert</button>
      {#if phase === "dirty" || phase === "validating"}
        <span class="working">checking…</span>
      {/if}
    </div>
    {#if breaksDefinitely}
      <p class="explain warn">
        This adds a contradiction that no reading of the dates rescues. Saving is still
        allowed — it is your world — but the consistency panel will report it.
      </p>
    {/if}
  {/if}
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px;
    overflow-y: auto;
    border-left: 1px solid var(--rule);
    background: var(--paper);
  }

  header {
    display: grid;
    gap: 2px;
    padding-bottom: 6px;
  }

  h2 {
    margin: 0;
    font-size: 19px;
    font-weight: 600;
    line-height: 1.2;
  }

  .kind,
  .id {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .id {
    text-transform: none;
    letter-spacing: 0;
    color: var(--rule-strong);
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

  .label {
    margin: 10px 0 0;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--accent);
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

  .back {
    align-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .back:hover {
    color: var(--accent);
  }

  .note,
  .aside-note {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--ink-3);
  }

  .aside-note {
    padding-left: 11px;
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

  .caution {
    margin: 0;
    padding: 9px 11px;
    background: var(--surface-2);
    border-left: 2px solid var(--warn);
    font-size: 12.5px;
    color: var(--ink-2);
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

  .confirm .danger:hover {
    color: var(--warn);
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
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 6px;
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

  .working {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .explain {
    margin: 0;
    font-size: 12px;
    color: var(--ink-3);
  }

  .explain.warn {
    color: var(--warn);
  }

  .failure {
    margin: 0;
    padding: 9px 11px;
    background: var(--surface-2);
    border-left: 2px solid var(--warn);
    font-family: var(--f-mono);
    font-size: 11.5px;
    color: var(--warn);
  }

  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--ink-3);
  }

  .pin {
    display: none;
  }
</style>
