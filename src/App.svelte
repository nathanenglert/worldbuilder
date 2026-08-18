<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    api,
    inTauri,
    type Finding,
    type Lineage,
    type ProposalSummary,
    type Reference,
    type Snapshot,
    type Story,
    type StoryScene,
    type Surfacing,
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
  import LineageView from "./lib/LineageView.svelte";
  import VersionPanel from "./lib/VersionPanel.svelte";
  import ExportPanel from "./lib/ExportPanel.svelte";
  import GoTo from "./lib/GoTo.svelte";
  import {
    existenceWindow,
    idOf,
    nameOf,
    resolveLocally,
    type EditableKind,
    type Selection,
  } from "./lib/selection";

  let summary = $state<WorldSummary | null>(null);
  let events = $state<WorldEvent[]>([]);
  let snapshot = $state<Snapshot | null>(null);
  let terrain = $state<Terrain | null>(null);
  let backdrop = $state<string | null>(null);
  let findings = $state<Finding[]>([]);
  let proposals = $state<ProposalSummary[]>([]);
  let scenes = $state<StoryScene[]>([]);
  let story = $state<Story | null>(null);
  let lineage = $state<Lineage | null>(null);
  /** What the right-hand column is showing. Exactly one of them, always. */
  type Panel = "inspector" | "checks" | "proposals" | "story" | "version" | "export" | "edit";
  let panel = $state<Panel>("inspector");
  /**
   * The centre pane. Both are projections of the same timeline — one onto the ground,
   * one onto descent — so they share the scrubber underneath rather than the axis.
   */
  let view = $state<"map" | "lineage">("map");
  let day = $state(0);
  let label = $state("");
  /**
   * The address of what is selected. The map and the lineage chart highlight by it, and
   * `selection` below is what it turns out to mean.
   */
  let selected = $state<string | null>(null);
  let selection = $state<Selection>({ state: "none" });
  let error = $state<string | null>(null);
  let busy = $state(false);
  let jumpExpr = $state("");
  let rootPath = $state("");

  // ---- opening somebody else's world
  let opening = $state(false);
  let openPath = $state("");
  let recent = $state<string[]>([]);
  let version = $state<{ branch: string | null; dirty: number; kind: string } | null>(null);

  // ---- authoring
  let editTarget = $state<{
    kind: EditableKind;
    id: string | null;
    focus?: string;
    type?: string;
  } | null>(null);
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

  const definiteCount = $derived(findings.filter((f) => f.certainty === "definite").length);
  const openCount = $derived(findings.filter((f) => f.certainty === "possible").length);
  const pendingCount = $derived(proposals.filter((p) => p.status === "pending").length);
  /** Scenes whose location has a marker — the ones the map can actually draw a path through. */
  const placedScenes = $derived(scenes.filter((s) => s.point !== null));
  /**
   * The events, written the way a date box would hang a date off one of them.
   *
   * Derived here rather than in each panel that needs it, so there is one answer to what
   * an anchor is. `@evt_siege_of_marrow+2y` is the expression that makes a date *move*
   * when the siege does, and until now it required remembering the id exactly.
   */
  const anchors = $derived(
    (summary?.records ?? []).filter((r) => r.kind === "event").map((r) => `@${r.id}`),
  );

  // ---- everything the app already knows about a record, filed under its id
  //
  // The Inspector had one source — the snapshot — and so it could say what a record
  // asserts and nothing else. Everything else about it was already in this component and
  // reachable only through a panel that replaced the record on screen: the two findings
  // that name Aldric were in the checks panel, the five times the book names him were in
  // the story panel, and the three records that point at him could only be seen by
  // *proposing to delete him*. Three indexes, built where the data already lives.

  /**
   * Findings, by every record they name.
   *
   * Over `subject` *and* `related`, because the two halves of a finding are both records
   * it is about: the existence violation's subject is the siege and its related is
   * Aldric, and it belongs on both. Deduplicated per finding — a rule that names the same
   * record twice is one finding, not two.
   */
  const findingsBy = $derived.by(() => {
    const by = new Map<string, Finding[]>();
    for (const finding of findings) {
      for (const id of new Set([finding.subject, ...finding.related])) {
        const list = by.get(id);
        if (list) list.push(finding);
        else by.set(id, [finding]);
      }
    }
    return by;
  });

  /** Where a record sits in the book, by id. Entities only — the iceberg measures records. */
  const surfacingBy = $derived(
    new Map((story?.records ?? []).map((r) => [r.id, r] as [string, Surfacing])),
  );

  /**
   * Ids to names, and the answer to whether an id is a record at all.
   *
   * Step 4's `records[]` paying off twice: a finding's subject can be rendered as "The
   * Siege of Marrow" instead of `evt_siege_of_marrow`, and an id that is *not* in here is
   * one that resolves to nothing — which is worth showing differently rather than
   * hiding, because a reference going nowhere is a thing about the world.
   */
  const names = $derived(
    Object.fromEntries((summary?.records ?? []).map((r) => [r.id, r.name])) as Record<
      string,
      string
    >,
  );

  /**
   * What points at the selected record. The one of the three that is not already here.
   *
   * A whole-world index would mean asking `references_to` once per record, which is the
   * world scanned once per record — and a second copy of the world in the frontend to
   * hold it. One id at a time is what the panel actually shows, and it is the same shape
   * the record lookup above already takes.
   *
   * Carried with the id it answers for, so the panel never attributes one record's
   * referrers to another while a fetch is in flight, and can tell "nothing points here"
   * from "not answered yet".
   */
  let pointing = $state<{ id: string; refs: Reference[] } | null>(null);
  let pointingToken = 0;

  $effect(() => {
    const id = selected;
    // Re-read after every write: saving a record can add or drop an edge to any other,
    // and `summary` is the object every write hands back.
    void summary;

    const mine = ++pointingToken;
    if (id === null) {
      pointing = null;
      return;
    }
    void api
      .references(id)
      .then((refs) => {
        if (mine === pointingToken) pointing = { id, refs };
      })
      .catch(() => {
        if (mine === pointingToken) pointing = null;
      });
  });

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

  /**
   * Just enough version state for the header chip. The panel fetches the rest — a chip
   * that showed history would be a second copy of it going stale.
   */
  async function loadVersion() {
    try {
      const v = await api.versionStatus();
      version = { branch: v.branch, dirty: v.dirty.length, kind: v.standing.kind };
    } catch {
      version = null;
    }
  }

  /** Fetched on demand, because most sessions never open the lineage view. */
  async function loadLineage() {
    try {
      lineage = await api.lineage();
    } catch (e) {
      error = String(e);
    }
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

  // ---- the way back

  /**
   * Where the writer was before something moved them, and what to call it.
   *
   * One mark, not a stack. Going back leaves a mark where you were, so a second press
   * returns — and the label always names the destination, which is what stops a button
   * called "back" from being a lie the second time it is pressed.
   */
  let mark = $state<{ day: number; id: string | null; where: string | null; when: string } | null>(
    null,
  );

  function markHere() {
    mark = { day, id: selected, where: nameOf(selection), when: label };
  }

  /**
   * The chip's label: whichever of the two the writer actually left.
   *
   * A mark is a record *and* a day, and naming the wrong one is a small lie. Going to a
   * record's own existence window moves only the clock, and a chip reading "‹ Aldric Vane
   * III" while Aldric III is what is on screen says nothing about where it goes.
   */
  const backLabel = $derived.by(() => {
    if (!mark) return null;
    return mark.id !== null && mark.id !== selected ? (mark.where ?? mark.id) : mark.when;
  });

  /** A mark on the spot the writer is standing on is not a way back to anywhere. */
  const canGoBack = $derived(!!mark && (mark.id !== selected || mark.day !== day));

  /**
   * Go to a day the writer *named* rather than scrubbed to, leaving a mark behind.
   *
   * The line is whether they could see where they were going. Dragging the head moves
   * through everything in between and marking each step would fill the button with
   * places nobody chose; a typed date, a finding, or a record's own existence window is
   * a teleport, and a teleport needs a way back.
   */
  function jumpTo(d: number) {
    markHere();
    goto(d);
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
      lineage = null;
      view = "map";
      recent = await api.recentWorlds();
      void loadVersion();
      mapQueries = 0;
      scrubSteps = 0;
      selected = null;
      // A way back into a world that is no longer open leads nowhere.
      mark = null;
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
    const expr = jumpExpr.trim();
    if (!expr) return;
    try {
      const resolved = await api.resolveExpr(expr);
      if (resolved === null) {
        error = `"${expr}" has no position on the timeline.`;
      } else {
        error = null;
        jumpTo(resolved);
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
      if (lineage) await loadLineage();
      lastBucket = -1;
      await fetchSnapshot(day);
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * A branch switch rewrites every file in the world folder, so nothing derived from it
   * survives. The backend has already reloaded and handed back a fresh summary; this is
   * the same refetch a decided proposal does, for the same reason.
   */
  async function afterBranch(next: WorldSummary) {
    summary = next;
    try {
      events = await api.timeline();
      findings = await api.checkWorld();
      proposals = await api.listProposals();
      scenes = await api.scenes();
      story = await api.story();
      if (lineage) await loadLineage();
      void loadVersion();
      selected = null;
      // Every file in the folder was just rewritten; the record the mark named may not
      // be on this branch at all.
      mark = null;
      lastBucket = -1;
      await fetchSnapshot(day);
      terrain = await api.terrain();
      if (terrain) backdrop = await api.mapImage();
    } catch (e) {
      error = String(e);
    }
  }

  // ---- what the selected id turns out to be

  /**
   * Resolve the selection against everything the app is holding, and fall back to the
   * world itself when none of it knows the id.
   *
   * This is what stops the panel going quietly blank. Nothing here changes `selected`:
   * scrubbing past a record's lifespan leaves it selected and changes what the panel
   * *says* about it, because the writer chose that record and the clock moving is not
   * them changing their mind.
   */
  let selectionToken = 0;

  $effect(() => {
    const local = resolveLocally(selected, snapshot, events, scenes);
    const mine = ++selectionToken;

    if (local.state !== "looking") {
      selection = local;
      return;
    }

    // Hold the answer already on screen while a fresh one is fetched. Every change point
    // the scrubber crosses re-runs this, and a record that is off-window stays off-window
    // across most of them — showing "looking…" each time would flicker for no new answer.
    const held = untrack(() => selection);
    if (!("id" in held) || held.id !== local.id) selection = local;

    void (async () => {
      const found = await lookUp(local.id);
      if (mine === selectionToken) selection = found;
    })();
  });

  /**
   * The one question the snapshot cannot answer: is this a record at all, and if so, when?
   *
   * The window is reported as authored rather than as day numbers — `0811~` is the world's
   * own answer and resolving it to a date would overstate what is known. The day behind
   * the button is resolved, because a button has to go somewhere.
   */
  async function lookUp(id: string): Promise<Selection> {
    try {
      const record = await api.entityRecord(id);
      const anchor = record.existence_from ?? record.existence_to;
      let goto: { day: number; label: string } | null = null;
      if (anchor) {
        const d = await api.resolveExpr(anchor);
        if (d !== null) goto = { day: d, label: await api.formatDay(d) };
      }
      return {
        state: "elsewhere",
        id,
        kind: "entity",
        name: record.name,
        type: record.type,
        window: existenceWindow(record.existence_from, record.existence_to),
        goto,
      };
    } catch {
      // Not an entity, not an event, not a scene, and the world has never heard of it.
      return { state: "unknown", id, kind: null };
    }
  }

  // ---- one door out of the form

  /**
   * One thing the writer asked for.
   *
   * Choosing a record and going somewhere were the same call, and that was wrong in both
   * directions. It navigated when it should not: picking a record named in a version
   * comparison forced the panel to the inspector and threw the comparison away, and
   * getting it back meant re-running it. And most of the app never went through it at
   * all — the header chips, the edit button, the form's own back button and the world
   * opener each moved the panel themselves, so an unsaved draft could vanish with nobody
   * asked. Saying which of the two is meant is what lets one guard cover every route.
   */
  type Intent =
    | { act: "select"; id: string | null; show: boolean }
    /**
     * A teleport: show the record, drop a mark, and move the clock if a day came with
     * it. Separate from `select` because the two differ in what they cost the writer —
     * clicking a marker they can already see needs no way back, and arriving somewhere
     * they could not see does.
     */
    | { act: "visit"; id: string | null; day: number | null }
    | { act: "panel"; panel: Exclude<Panel, "edit"> }
    /**
     * `focus` is which attribute the form should open *at*. A fact read in the inspector
     * and the box that would change it are the same fact, and the trip between them was
     * "open the form, then find the row again" down a form that can run past a screen.
     *
     * `type` is what a new record starts out as, and is how "save & new" hands a run of
     * cities forward. The two never travel together: one is about a record that exists.
     */
    | { act: "edit"; kind: EditableKind; id: string | null; focus?: string; type?: string }
    | { act: "close" }
    | { act: "open"; path: string };

  /** What the form is holding back until the writer says what to do with it. */
  let held = $state<Intent | null>(null);

  /**
   * The one door. Everything that changes what is on screen comes through here, because
   * everything that changes what is on screen can cost an unsaved draft.
   */
  function intend(intent: Intent) {
    if (settled(intent)) return;
    if (costsTheDraft(intent)) {
      held = intent;
      return;
    }
    carry(intent);
  }

  /**
   * Asked for what is already the case.
   *
   * Worth its own arm because `carry` is not idempotent: re-opening the form on the
   * record it is already showing hands it a fresh `target`, which reloads the record and
   * takes the draft with it. That is reachable — a scene dot on the map opens the form,
   * and it is still drawn while that scene is being edited.
   *
   * `focus` is deliberately not compared. The same record asked for at a different
   * attribute is still the record already open, and reloading it to move the caret would
   * trade a draft for a scroll position.
   *
   * A *new* record is never already the case. There is no record to re-open, so asking
   * for one while a blank form is up means another one — which is what "save & new"
   * asks for, and what "+ record" over a half-written draft always meant. That one now
   * goes to the discard prompt like every other way of leaving, rather than to nothing.
   */
  function settled(intent: Intent): boolean {
    return (
      intent.act === "edit" &&
      intent.id !== null &&
      panel === "edit" &&
      editTarget?.kind === intent.kind &&
      editTarget?.id === intent.id
    );
  }

  /**
   * Would carrying this out take the form off the screen with unsaved work still in it?
   *
   * A selection only costs when it would navigate, which a deselection never does — so
   * clicking empty map to drop a highlight is free, as it was always meant to be. Note
   * there is no exemption for the record being edited: clicking it on the map still
   * swaps the form for the inspector, which is a loss like any other. The old guard let
   * exactly that one through.
   *
   * The clock alone is not here at all, and that is deliberate: the form edits the record
   * and not the snapshot, so scrubbing under an open draft changes what the map shows and
   * nothing the writer typed. A `visit` costs only because it also moves the panel.
   */
  function costsTheDraft(intent: Intent): boolean {
    if (panel !== "edit" || !editDirty) return false;
    if (intent.act === "select") return intent.show && intent.id !== null;
    return true;
  }


  function carry(intent: Intent) {
    if (intent.act === "select") {
      selected = intent.id;
      if (!intent.show || intent.id === null) return;
      if (panel === "edit") dropForm();
      panel = "inspector";
      return;
    }

    if (intent.act === "visit") {
      // The mark is dropped in here rather than at the call sites, so it records where
      // the writer actually was — not where they were when they asked, which is a
      // different place if the form held the question while they answered it.
      if (intent.day === null) markHere();
      else jumpTo(intent.day);
      selected = intent.id;
      if (panel === "edit") dropForm();
      panel = "inspector";
      return;
    }

    if (panel === "edit") dropForm();

    if (intent.act === "edit") {
      editTarget = { kind: intent.kind, id: intent.id, focus: intent.focus, type: intent.type };
      if (intent.id) selected = intent.id;
      panel = "edit";
    } else if (intent.act === "open") {
      // Set here rather than left to `open`, which only gets there after the load: a
      // panel that still said "edit" over a form that had just been dropped would be a
      // lie for as long as the world takes to read, and for good if the path is wrong.
      panel = "inspector";
      void open(intent.path);
    } else {
      panel = intent.act === "close" ? "inspector" : intent.panel;
    }
  }

  /**
   * Everything the form owns, dropped — including the geometry, which is the map's copy
   * of the draft and would otherwise be drawn over the next record edited.
   *
   * The form's state does not outlive its panel. It used to: `editDirty` left standing
   * after the writer navigated away was a discard prompt about a draft already gone.
   */
  function dropForm() {
    editTarget = null;
    editDirty = false;
    mapMode = "browse";
    editGeometry = { marker: null, shape: [] };
  }

  /** The writer has answered the form's question about what it was holding. */
  function resolveHeld(discard: boolean) {
    const intent = held;
    held = null;
    if (!discard || !intent) return;
    editDirty = false;
    carry(intent);
  }

  // ---- what the rest of the app calls

  /** Choose a record the writer could already see: the map, the lineage chart, the track. */
  const inspect = (id: string | null) => intend({ act: "select", id, show: true });
  /** Choose a record without moving the panel, for a list that must survive being read. */
  const pick = (id: string) => intend({ act: "select", id, show: false });
  /** Arrive somewhere the writer could not see, and leave a way back. */
  const visit = (id: string | null, day: number | null = null) =>
    intend({ act: "visit", id, day });
  /** A header chip: the panel it names, or back out of it if that is what is showing. */
  const toggle = (p: Exclude<Panel, "edit">) =>
    intend({ act: "panel", panel: panel === p ? "inspector" : p });
  const openEditor = (kind: EditableKind, id: string | null, focus?: string) =>
    intend({ act: "edit", kind, id, focus });

  /** A blank record of the same type as the one just saved. What "save & new" asks for. */
  const openLike = (kind: EditableKind, type: string) =>
    intend({ act: "edit", kind, id: null, type });
  const closePanel = () => intend({ act: "close" });

  /** Take the writer to the moment and the record a finding is about. */
  function inspectFinding(finding: Finding) {
    visit(finding.related[0] ?? finding.subject, finding.at);
  }

  // ---- going to a record by name

  let goingTo = $state(false);

  /**
   * Cmd/Ctrl-K, the one keyboard shortcut in the app.
   *
   * On `window` rather than on an element because it has to work from anywhere,
   * including from inside the form — where it is most useful, since checking what an id
   * refers to is exactly what a writer stops mid-record to do. Nothing is lost by
   * opening it: the pick goes through `intend` like every other way of choosing.
   */
  function hotkey(e: KeyboardEvent) {
    if (e.key !== "k" || !(e.metaKey || e.ctrlKey)) return;
    e.preventDefault();
    goingTo = !goingTo;
  }

  /**
   * What is being held, as the subject of "… does not keep them."
   *
   * The prompt used to be a bare "Discard your changes?" for the one intent that could
   * reach it. Now that every route arrives here, it has to say which one.
   */
  const panelNames: Record<Exclude<Panel, "edit">, string> = {
    inspector: "world",
    checks: "consistency checks",
    proposals: "review queue",
    story: "story panel",
    version: "version panel",
    export: "publish panel",
  };

  function describe(intent: Intent): string {
    switch (intent.act) {
      case "select":
        return `Opening ${intent.id}`;
      case "visit":
        return intent.id === null ? "Going back" : `Going to ${intent.id}`;
      case "panel":
        return `Opening the ${panelNames[intent.panel]}`;
      case "edit":
        return intent.id === null ? `Starting a new ${intent.kind}` : `Opening ${intent.id}`;
      case "open":
        return `Opening ${intent.path.split("/").pop()}`;
      case "close":
        return "Leaving this form";
    }
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
      // Only when it is on screen or has been: a parentage edge, a title window and a
      // date anchor can all move under a save, so it is never safe to keep a stale one.
      if (lineage) await loadLineage();
      void loadVersion();
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
      // The world this writer had open last, if it is still there, and the bundled
      // example otherwise. A local-first tool that could only ever open its own demo
      // was the oldest limitation in this app.
      const path = await api.initialWorld();
      if (path) await open(path);
      else error = "Could not locate a world to open.";
    } catch (e) {
      error = String(e);
    }
  });
</script>

<div class="app">
  <header>
    <div class="identity">
      <p class="eyebrow">
        {summary?.calendar ?? "Worldbuilder"}
        <button class="open" onclick={() => (opening = !opening)} title="Open another world folder">
          {opening ? "cancel" : "open…"}
        </button>
      </p>
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
        bind:value={jumpExpr}
        placeholder="0812-04  ·  812~  ·  @evt_siege_of_marrow+2y"
        spellcheck="false"
        aria-label="Jump to a date"
      />
      <button type="submit">go</button>
    </form>

    {#if summary}
      <div class="chips">
        <button class="chip find" onclick={() => (goingTo = true)} title="Go to a record (⌘K)">
          ⌘K go to
        </button>

        <!-- Only after something has moved the writer, and labelled with where it goes
             rather than "back": pressing it leaves a mark here, so the second press
             returns, and a button that said "back" both times would be lying once. -->
        {#if canGoBack && mark}
          <button
            class="chip back"
            onclick={() => visit(mark!.id, mark!.day)}
            title="Back to {mark.where ?? 'nothing in particular'} on {mark.when}"
          >
            ‹ {backLabel}
          </button>
        {/if}

        <button
          class="chip"
          class:bad={definiteCount > 0}
          class:note={definiteCount === 0 && openCount > 0}
          onclick={() => toggle("checks")}
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
          onclick={() => toggle("proposals")}
          title="Changes awaiting review"
        >
          {pendingCount} pending
        </button>

        {#if story}
          <button
            class="chip"
            class:live={story.standing === "linked"}
            class:bad={story.standing === "root_missing"}
            onclick={() => toggle("story")}
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

        {#if version && version.kind !== "none"}
          <button
            class="chip"
            class:live={version.dirty === 0}
            class:note={version.dirty > 0}
            onclick={() => toggle("version")}
            title="Save points and what-ifs"
          >
            {#if version.dirty > 0}
              {version.dirty} to save
            {:else if version.branch}
              on {version.branch}
            {:else}
              versions
            {/if}
          </button>
        {/if}

        <!-- Grouped so the row breaks between "look at" and "make", never mid-group. -->
        <span class="group">
          <button
            class="chip make"
            onclick={() => openEditor("entity", null)}
            title="Write a new record"
          >
            + record
          </button>
          <button
            class="chip make"
            onclick={() => openEditor("event", null)}
            title="Write a new event"
          >
            + event
          </button>
          <button
            class="chip make"
            onclick={() => openEditor("scene", null)}
            title="Write a new scene"
          >
            + scene
          </button>
          <button
            class="chip make"
            onclick={() => toggle("export")}
            title="Write this world out as one file"
          >
            ⤓ publish
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

  {#if opening}
    <!-- A path field rather than a native folder picker: no extra plugin, and a native
         modal cannot be driven by the automation that verifies everything else here. -->
    <form
      class="opener"
      onsubmit={(e) => {
        e.preventDefault();
        const path = openPath.trim();
        if (!path) return;
        opening = false;
        openPath = "";
        intend({ act: "open", path });
      }}
    >
      <input
        bind:value={openPath}
        placeholder="/path/to/your-world  (the folder with world.yaml in it)"
        spellcheck="false"
        aria-label="World folder to open"
      />
      <button type="submit">open</button>
      {#each recent.filter((p) => p !== rootPath).slice(0, 4) as p (p)}
        <button
          type="button"
          class="recent"
          title={p}
          onclick={() => {
            opening = false;
            intend({ act: "open", path: p });
          }}
        >
          {p.split("/").pop()}
        </button>
      {/each}
    </form>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="body" class:editing={panel === "edit"}>
    <div class="stage">
      <!-- Two projections of one timeline: the map onto the ground, the lineage onto
           descent. They swap here rather than sitting side by side, because both want
           the width and both are driven by the same scrubber underneath. -->
      {#if view === "map"}
        <MapView
          {snapshot}
          {terrain}
          {backdrop}
          {selected}
          onselect={inspect}
          mode={mapMode}
          draft={panel === "edit" ? editGeometry : null}
          onmarker={(p) => (editGeometry = { ...editGeometry, marker: p })}
          onshape={(points) => (editGeometry = { ...editGeometry, shape: points })}
          onmodedone={() => (mapMode = "browse")}
          scenes={placedScenes}
          activeScene={panel === "edit" && editTarget?.kind === "scene" ? editTarget.id : null}
          showStory={panel === "story" || (panel === "edit" && editTarget?.kind === "scene")}
          onscene={(id) => openEditor("scene", id)}
        />
      {:else}
        <LineageView {lineage} {day} {selected} onselect={inspect} onday={goto} />
      {/if}

      {#if summary}
        <div class="views">
          <button class:on={view === "map"} onclick={() => (view = "map")}>map</button>
          <button
            class:on={view === "lineage"}
            onclick={() => {
              view = "lineage";
              if (!lineage) void loadLineage();
            }}>lineage</button
          >
        </div>
      {/if}
    </div>
    {#if panel === "edit" && editTarget}
      <Editor
        target={editTarget}
        {summary}
        geometry={editGeometry}
        mode={mapMode}
        holding={held && describe(held)}
        {anchors}
        onmode={(m) => (mapMode = m)}
        ongeometry={(g) => (editGeometry = g)}
        ondirty={(d) => (editDirty = d)}
        onsaved={afterWrite}
        onnew={openLike}
        onclose={closePanel}
        onjump={jumpTo}
        onresolve={resolveHeld}
      />
    {:else if panel === "checks"}
      <Findings {findings} {names} onjump={inspectFinding} onselect={visit} onclose={closePanel} />
    {:else if panel === "proposals"}
      <Proposals {proposals} ondecided={afterDecision} onclose={closePanel} />
    {:else if panel === "story"}
      <StoryPanel
        {story}
        {scenes}
        {names}
        onselect={inspect}
        onscene={(id) => openEditor("scene", id)}
        onclose={closePanel}
      />
    {:else if panel === "version"}
      <VersionPanel
        onchanged={afterBranch}
        onstatus={(v) => (version = { branch: v.branch, dirty: v.dirty.length, kind: v.standing.kind })}
        onselect={pick}
        onclose={closePanel}
      />
    {:else if panel === "export"}
      <ExportPanel {anchors} onjump={jumpTo} onclose={closePanel} />
    {:else}
      <Inspector
        {snapshot}
        {terrain}
        {selection}
        {names}
        findings={findingsBy.get(idOf(selection) ?? "") ?? []}
        surfacing={surfacingBy.get(idOf(selection) ?? "") ?? null}
        references={pointing?.id === idOf(selection) ? pointing.refs : null}
        onselect={inspect}
        onedit={openEditor}
        onday={jumpTo}
      />
    {/if}
  </div>

  {#if summary}
    <Timeline
      span={summary.span}
      {scenes}
      onpick={inspect}
      {day}
      {events}
      changePoints={summary.change_points}
      onday={goto}
    />
  {:else}
    <div class="placeholder">{busy ? "Opening world…" : "Waiting for a world."}</div>
  {/if}
</div>

<svelte:window onkeydown={hotkey} />

{#if goingTo && summary}
  <GoTo
    records={summary.records}
    onclose={() => (goingTo = false)}
    onpick={(id) => {
      goingTo = false;
      visit(id);
    }}
  />
{/if}

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

  /* The two navigation chips, in the header's own quiet register: they move the writer
     rather than reporting on the world, so they must not read as another count. */
  .chip.find,
  .chip.back {
    color: var(--ink-3);
    border-color: var(--rule);
    text-transform: none;
    letter-spacing: 0.04em;
  }

  .chip.back {
    max-width: 20ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chip.find:hover,
  .chip.back:hover {
    color: var(--accent);
    border-color: var(--rule-strong);
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

  .eyebrow .open {
    margin-left: 8px;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--rule-strong);
  }

  .eyebrow .open:hover {
    color: var(--accent);
  }

  .opener {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    padding: 8px 20px;
    border-bottom: 1px solid var(--rule);
    background: var(--surface);
  }

  .opener input {
    flex: 1;
    min-width: 220px;
    background: var(--paper);
    border: 1px solid var(--rule);
    color: var(--ink);
    font-family: var(--f-mono);
    font-size: 11.5px;
    padding: 6px 9px;
  }

  .opener input::placeholder {
    color: var(--rule-strong);
  }

  .opener button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
    border: 1px solid var(--rule);
    padding: 5px 10px;
    white-space: nowrap;
  }

  .opener button:hover {
    color: var(--accent);
    border-color: var(--rule-strong);
  }

  .opener .recent {
    color: var(--rule-strong);
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

  .stage {
    position: relative;
    min-width: 0;
    min-height: 0;
    display: grid;
  }

  /* Top-right of the stage: the only corner the map does not already use. Layers are
     top-left, the legend bottom-left, the zoom readout bottom-right. */
  .views {
    position: absolute;
    right: 12px;
    top: 12px;
    display: flex;
    gap: 2px;
    padding: 3px;
    background: color-mix(in srgb, var(--paper) 86%, transparent);
    border: 1px solid var(--rule);
  }

  .views button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
    padding: 3px 9px;
    border: 1px solid transparent;
  }

  .views button:hover {
    color: var(--ink);
  }

  .views button.on {
    color: var(--accent);
    border-color: var(--rule-strong);
    background: var(--accent-soft);
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
