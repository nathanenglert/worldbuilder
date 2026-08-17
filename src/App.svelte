<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    inTauri,
    type Finding,
    type ProposalSummary,
    type Snapshot,
    type Story,
    type StoryScene,
    type Terrain,
    type WorldEvent,
    type WorldSummary,
  } from "./lib/api";
  import MapView from "./lib/MapView.svelte";
  import Timeline from "./lib/Timeline.svelte";
  import Inspector from "./lib/Inspector.svelte";
  import Findings from "./lib/Findings.svelte";
  import Proposals from "./lib/Proposals.svelte";
  import Editor from "./lib/Editor.svelte";
  import StoryPanel from "./lib/StoryPanel.svelte";

  let summary = $state<WorldSummary | null>(null);
  let events = $state<WorldEvent[]>([]);
  let snapshot = $state<Snapshot | null>(null);
  let terrain = $state<Terrain | null>(null);
  let backdrop = $state<string | null>(null);
  let findings = $state<Finding[]>([]);
  let proposals = $state<ProposalSummary[]>([]);
  let scenes = $state<StoryScene[]>([]);
  let story = $state<Story | null>(null);
  let panel = $state<"inspector" | "checks" | "proposals" | "story" | "edit">("inspector");
  let day = $state(0);
  let label = $state("");
  let selected = $state<string | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let jumpTo = $state("");
  let rootPath = $state("");

  // ---- authoring
  let editTarget = $state<{ kind: "entity" | "event" | "scene"; id: string | null } | null>(null);
  let editDirty = $state(false);
  let mapMode = $state<"browse" | "marker" | "shape">("browse");
  /**
   * The geometry being edited, held here rather than in the panel so the map can draw it
   * and change it while the panel owns everything else about the record.
   */
  let editGeometry = $state<{ marker: [number, number] | null; shape: [number, number][] }>({
    marker: null,
    shape: [],
  });
  /** A selection the editor is holding back until the writer says what to do with it. */
  let pendingSelect = $state<string | null>(null);

  const definiteCount = $derived(findings.filter((f) => f.certainty === "definite").length);
  const openCount = $derived(findings.filter((f) => f.certainty === "possible").length);
  const pendingCount = $derived(proposals.filter((p) => p.status === "pending").length);
  /** Scenes whose location has a marker — the ones the map can actually draw a path through. */
  const placedScenes = $derived(scenes.filter((s) => s.point !== null));

  // Makes the change-point premise visible: drag across three centuries and the query
  // count barely moves, because the world only changes at a handful of instants.
  let mapQueries = $state(0);
  let scrubSteps = $state(0);

  let lastBucket = -1;
  let labelToken = 0;

  /** Index of the interval between change points that `d` falls in. */
  function bucketOf(d: number): number {
    const points = summary?.change_points ?? [];
    let lo = 0;
    let hi = points.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (points[mid] <= d) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  async function fetchSnapshot(d: number) {
    try {
      snapshot = await api.snapshot(d);
      label = snapshot.label;
      mapQueries += 1;
    } catch (e) {
      error = String(e);
    }
  }

  async function refreshLabel(d: number) {
    const token = ++labelToken;
    try {
      const next = await api.formatDay(d);
      if (token === labelToken) label = next;
    } catch {
      // The next snapshot carries an authoritative label anyway.
    }
  }

  function goto(d: number) {
    day = d;
    scrubSteps += 1;
    void refreshLabel(d);

    const bucket = bucketOf(d);
    if (bucket !== lastBucket) {
      lastBucket = bucket;
      void fetchSnapshot(d);
    }
  }

  async function open(path: string) {
    busy = true;
    error = null;
    rootPath = path;
    try {
      summary = await api.openWorld(path);
      events = await api.timeline();
      findings = await api.checkWorld();
      proposals = await api.listProposals();
      scenes = await api.scenes();
      story = await api.story();
      mapQueries = 0;
      scrubSteps = 0;
      selected = null;
      panel = "inspector";

      const [lo, hi] = summary.span;
      const start = Math.round(lo + (hi - lo) * 0.62);
      day = start;
      lastBucket = bucketOf(start);
      await fetchSnapshot(start);

      // Terrain last, and awaited separately: it is the one fetch that can take a second,
      // and the timeline is usable long before the ground under it has been drawn. It is
      // also fetched exactly once — nothing in it moves when the scrubber does.
      terrain = null;
      backdrop = null;
      terrain = await api.terrain();
      if (terrain) backdrop = await api.mapImage();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function jump() {
    const expr = jumpTo.trim();
    if (!expr) return;
    try {
      const resolved = await api.resolveExpr(expr);
      if (resolved === null) {
        error = `"${expr}" has no position on the timeline.`;
      } else {
        error = null;
        goto(resolved);
      }
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * A decided proposal may have rewritten files. The backend has already reloaded the
   * world and returned a fresh summary, so only the derived views need refetching.
   */
  async function afterDecision() {
    try {
      summary = await api.openWorld(rootPath);
      events = await api.timeline();
      findings = await api.checkWorld();
      proposals = await api.listProposals();
      scenes = await api.scenes();
      story = await api.story();
      lastBucket = -1;
      await fetchSnapshot(day);
    } catch (e) {
      error = String(e);
    }
  }

  /** Take the writer to the moment and the record a finding is about. */
  function inspectFinding(finding: Finding) {
    if (finding.at !== null) goto(finding.at);
    selected = finding.related[0] ?? finding.subject;
  }

  /** Open a scene from the timeline band or the story panel. */
  function openScene(id: string) {
    if (panel === "edit" && editDirty && id !== editTarget?.id) {
      pendingSelect = id;
      return;
    }
    edit("scene", id);
  }

  function select(id: string | null) {
    // The single choke point for every way of choosing a record — the map, the inspector
    // list, a finding. Guarding here is what stops an unsaved edit disappearing without
    // anybody being asked.
    if (panel === "edit" && editDirty && id !== editTarget?.id) {
      pendingSelect = id;
      return;
    }
    selected = id;
    if (id) panel = "inspector";
  }

  function resolvePendingSelect(discard: boolean) {
    const held = pendingSelect;
    pendingSelect = null;
    if (!discard) return;
    editDirty = false;
    closeEditor();
    selected = held;
    if (held) panel = "inspector";
  }

  function edit(kind: "entity" | "event" | "scene", id: string | null) {
    editTarget = { kind, id };
    editGeometry = { marker: null, shape: [] };
    panel = "edit";
    if (id) selected = id;
  }

  function closeEditor() {
    editTarget = null;
    editDirty = false;
    mapMode = "browse";
    editGeometry = { marker: null, shape: [] };
    panel = "inspector";
  }

  /**
   * A direct write, unlike a decided proposal, hands back a fresh summary already — the
   * backend reloaded before returning. So no second `openWorld`, and terrain is refetched
   * only when a marker moved, since `places` is a join of markers against ground that
   * itself has not changed.
   */
  async function afterWrite(next: WorldSummary, markerChanged: boolean) {
    try {
      summary = next;
      events = await api.timeline();
      findings = await api.checkWorld();
      // Every pending proposal's impact is measured against the current world, so a
      // direct write changes all of their arithmetic.
      proposals = await api.listProposals();
      // Both, always. A moved marker moves a scene's dot; an edited alias changes what
      // the prose is found to name; a re-dated event drags every scene anchored to it.
      // There is no write in this app that provably touches neither.
      scenes = await api.scenes();
      story = await api.story();
      lastBucket = -1;
      await fetchSnapshot(day);
      if (markerChanged && terrain) terrain.places = await api.terrainPlaces();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(async () => {
    if (!inTauri) {
      error = "Not running in the desktop shell. Start it with `pnpm tauri dev`.";
      return;
    }
    try {
      const path = await api.examplePath();
      if (path) await open(path);
      else error = "Could not locate the example world.";
    } catch (e) {
      error = String(e);
    }
  });
</script>

<div class="app">
  <header>
    <div class="identity">
      <p class="eyebrow">{summary?.calendar ?? "Worldbuilder"}</p>
      <h1>{summary?.name ?? "No world open"}</h1>
    </div>

    <div class="readout">
      <p class="date">{label || "—"}</p>
      <p class="daynum">day {day.toLocaleString()}</p>
    </div>

    <form
      class="jump"
      onsubmit={(e) => {
        e.preventDefault();
        void jump();
      }}
    >
      <input
        bind:value={jumpTo}
        placeholder="0812-04  ·  812~  ·  @evt_siege_of_marrow+2y"
        spellcheck="false"
        aria-label="Jump to a date"
      />
      <button type="submit">go</button>
    </form>

    {#if summary}
      <div class="chips">
        <button
          class="chip"
          class:bad={definiteCount > 0}
          class:note={definiteCount === 0 && openCount > 0}
          onclick={() => (panel = panel === "checks" ? "inspector" : "checks")}
          title="Deterministic consistency rules"
        >
          {#if definiteCount > 0}
            {definiteCount} definite
          {:else if openCount > 0}
            {openCount} open question{openCount === 1 ? "" : "s"}
          {:else}
            consistent
          {/if}
        </button>

        <button
          class="chip"
          class:live={pendingCount > 0}
          onclick={() => (panel = panel === "proposals" ? "inspector" : "proposals")}
          title="Changes awaiting review"
        >
          {pendingCount} pending
        </button>

        {#if story}
          <button
            class="chip"
            class:live={story.standing === "linked"}
            class:bad={story.standing === "root_missing"}
            onclick={() => (panel = panel === "story" ? "inspector" : "story")}
            title="What of this world reaches the page"
          >
            {#if story.standing === "linked"}
              {story.percent}% on the page
            {:else if story.standing === "root_missing"}
              manuscript missing
            {:else}
              no manuscript
            {/if}
          </button>
        {/if}

        <!-- Grouped so the row breaks between "look at" and "make", never mid-group. -->
        <span class="group">
          <button class="chip make" onclick={() => edit("entity", null)} title="Write a new record">
            + record
          </button>
          <button class="chip make" onclick={() => edit("event", null)} title="Write a new event">
            + event
          </button>
          <button class="chip make" onclick={() => edit("scene", null)} title="Write a new scene">
            + scene
          </button>
        </span>
      </div>

      <dl class="stats" title="Snapshot queries versus scrub movements">
        <div><dt>entities</dt><dd>{summary.entity_count}</dd></div>
        <div><dt>events</dt><dd>{summary.event_count}</dd></div>
        {#if summary.scene_count > 0}
          <div><dt>scenes</dt><dd>{summary.scene_count}</dd></div>
        {/if}
        <div><dt>changes</dt><dd>{summary.change_points.length}</dd></div>
        <div class="hot"><dt>queries</dt><dd>{mapQueries} <span>/ {scrubSteps}</span></dd></div>
      </dl>
    {/if}
  </header>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="body" class:editing={panel === "edit"}>
    <MapView
      {snapshot}
      {terrain}
      {backdrop}
      {selected}
      onselect={select}
      mode={mapMode}
      draft={panel === "edit" ? editGeometry : null}
      onmarker={(p) => (editGeometry = { ...editGeometry, marker: p })}
      onshape={(points) => (editGeometry = { ...editGeometry, shape: points })}
      onmodedone={() => (mapMode = "browse")}
      scenes={placedScenes}
      activeScene={panel === "edit" && editTarget?.kind === "scene" ? editTarget.id : null}
      showStory={panel === "story" || (panel === "edit" && editTarget?.kind === "scene")}
      onscene={openScene}
    />
    {#if panel === "edit" && editTarget}
      <Editor
        target={editTarget}
        {summary}
        geometry={editGeometry}
        mode={mapMode}
        {pendingSelect}
        onmode={(m) => (mapMode = m)}
        ongeometry={(g) => (editGeometry = g)}
        ondirty={(d) => (editDirty = d)}
        onsaved={afterWrite}
        onclose={closeEditor}
        onjump={goto}
        onresolveselect={resolvePendingSelect}
      />
    {:else if panel === "checks"}
      <Findings {findings} onjump={inspectFinding} onclose={() => (panel = "inspector")} />
    {:else if panel === "proposals"}
      <Proposals
        {proposals}
        ondecided={afterDecision}
        onclose={() => (panel = "inspector")}
      />
    {:else if panel === "story"}
      <StoryPanel
        {story}
        {scenes}
        onselect={select}
        onscene={openScene}
        onclose={() => (panel = "inspector")}
      />
    {:else}
      <Inspector {snapshot} {terrain} {selected} onselect={select} onedit={edit} />
    {/if}
  </div>

  {#if summary}
    <Timeline
      span={summary.span}
      {scenes}
      onscene={openScene}
      {day}
      {events}
      changePoints={summary.change_points}
      onday={goto}
    />
  {:else}
    <div class="placeholder">{busy ? "Opening world…" : "Waiting for a world."}</div>
  {/if}
</div>

<style>
  /*
   * Flex, not grid rows: the error bar is conditional, and with grid auto-placement its
   * absence shifted every later child up a row — handing the `1fr` to the timeline and
   * squeezing the map into an `auto` row. Flex sizes by the children that exist.
   */
  .app {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  header {
    display: grid;
    grid-template-columns: minmax(140px, 1fr) auto minmax(170px, 1fr) auto auto;
    align-items: center;
    gap: 14px;
    padding: 14px 20px;
    border-bottom: 1px solid var(--rule);
    background: var(--paper);
  }

  /* Every grid child may shrink. Without this a `1fr` column refuses to go below its
     content's intrinsic width, and the last thing in the row — the counts — is pushed
     off the edge rather than the row getting tighter. */
  header > * {
    min-width: 0;
  }

  /* Six chips is more than one row can promise to hold on a narrow window, and a chip
     silently off the edge is worse than a second row. */
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chips .group {
    display: flex;
    flex-wrap: nowrap;
    gap: 6px;
  }

  .chip {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    white-space: nowrap;
    padding: 5px 10px;
    color: var(--ink-3);
    border: 1px solid var(--rule);
  }

  /* The only creative actions in a header full of counts, so they read as one. */
  .chip.make {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  .chip:hover {
    border-color: var(--rule-strong);
    color: var(--ink-2);
  }

  .chip.note {
    color: var(--era);
    border-color: color-mix(in srgb, var(--era) 40%, transparent);
  }

  .chip.bad {
    color: var(--warn);
    border-color: color-mix(in srgb, var(--warn) 55%, transparent);
  }

  .chip.live {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .eyebrow {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--accent);
  }

  h1 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .readout {
    text-align: center;
  }

  .date {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
  }

  .daynum,
  .stats dt {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .jump {
    display: flex;
    gap: 0;
    border: 1px solid var(--rule);
  }

  .jump input {
    flex: 1;
    min-width: 0;
    background: var(--surface);
    border: none;
    color: var(--ink);
    font-family: var(--f-mono);
    font-size: 11.5px;
    padding: 6px 9px;
  }

  .jump input::placeholder {
    color: var(--rule-strong);
  }

  .jump button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.08em;
    color: var(--ink-3);
    padding: 0 11px;
    border-left: 1px solid var(--rule);
  }

  .jump button:hover {
    color: var(--accent);
  }

  .stats {
    display: flex;
    gap: 18px;
    margin: 0;
  }

  .stats div {
    display: grid;
    gap: 1px;
    text-align: right;
  }

  .stats dd {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .stats .hot dd {
    color: var(--accent);
  }

  .stats .hot span {
    color: var(--ink-3);
  }

  .error {
    margin: 0;
    padding: 8px 20px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--warn);
    color: var(--warn);
    font-size: 12.5px;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr minmax(280px, 350px);
  }

  /*
   * A form needs more room than a reading panel. Safe with the map because the SVG uses
   * `preserveAspectRatio="xMidYMid meet"` — narrowing letterboxes rather than distorts —
   * and screen-to-world conversion reads `getScreenCTM()` fresh on every call, so the
   * coordinate maths re-derives itself with no cache to invalidate.
   */
  .body.editing {
    grid-template-columns: 1fr minmax(420px, 520px);
  }

  .placeholder {
    padding: 22px 20px;
    border-top: 1px solid var(--rule);
    font-family: var(--f-mono);
    font-size: 11.5px;
    color: var(--ink-3);
  }
</style>
