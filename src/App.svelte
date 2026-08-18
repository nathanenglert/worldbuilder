<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    api,
    inTauri,
    type Compare,
    type Finding,
    type Layer,
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
  import PillGroup from "./lib/PillGroup.svelte";
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
   * Where closing this panel puts the writer.
   *
   * "‹ back to the world" went to the inspector from everywhere, which is right when the
   * inspector is where you came from and wrong the rest of the time: starting a record
   * from the checks panel and closing the form left the writer looking at the record they
   * had just written, with the list they were working down gone. A panel is somewhere you
   * were passing through, and closing what you opened should leave you where you opened
   * it from.
   *
   * Never `edit`, because leaving the form drops it — there would be nothing to return to.
   */
  let lastPanel = $state<Exclude<Panel, "edit">>("inspector");
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
  /**
   * Version control did not answer.
   *
   * The chip used to disappear on a failure, which reads as "this world is not under
   * version control" — the one thing it is not evidence of. What it is evidence of is
   * that the app does not know, so the chip stays and says as much.
   */
  let versionFailed = $state(false);

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

  // ---- what each view keeps while it is off screen
  //
  // The centre pane and the right-hand column are both `{#if}` chains, so leaving a view
  // destroys it and coming back builds a new one. That is right for everything a view can
  // work out again for nothing — and these four cannot. A comparison is a revision
  // materialized out of the object store into a scratch directory with two worlds loaded
  // and checked against each other; a half-typed save point message is words the writer
  // wrote; the map's corner is where they had got to after a minute of panning, thrown
  // away by a single glance at the lineage chart.
  //
  // Held here rather than in the components for the reason `editGeometry` is held here:
  // the thing that outlives a view is the only thing that can keep what the view keeps.
  // The price is invalidation, which a destroyed component could not have done either —
  // see `worldMoved`.

  type Kept = {
    /** Pan, zoom, and which ground is drawn. */
    map: { scale: number; tx: number; ty: number; layer: Layer; showBackdrop: boolean };
    /** Which baton the lineage chart is following, or `all` for plain descent. */
    lineage: { chosen: string };
    version: {
      comparison: Compare | null;
      /** The world moved after this was worked out. It says so rather than disappearing. */
      stale: boolean;
      message: string;
      branch: string;
      /** The what-if whose actions are showing. */
      open: string | null;
    };
    /** The record whose iceberg detail is expanded. */
    story: { open: string | null };
  };

  const blankKept = (): Kept => ({
    map: { scale: 1, tx: 0, ty: 0, layer: "biome", showBackdrop: false },
    lineage: { chosen: "all" },
    version: { comparison: null, stale: false, message: "", branch: "", open: null },
    story: { open: null },
  });

  let kept = $state<Kept>(blankKept());

  /**
   * The world is no longer the one a held comparison was worked out against.
   *
   * Nothing here recomputes it. Re-materializing a revision and checking both worlds is
   * work the writer asked for once, and doing it again behind their back on every save is
   * not what they asked. The comparison stays where it was and says it is out of date,
   * which is the difference between a stale number and a lie — and is a question at all
   * only because it now outlives the panel that worked it out.
   */
  function worldMoved() {
    if (kept.version.comparison) kept.version.stale = true;
  }

  /** The folder being read, by the only name there is before it has been read: its own. */
  const opened = $derived(rootPath.split("/").filter(Boolean).pop() ?? "a world");

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
      versionFailed = false;
    } catch {
      // `version` is left standing. What it says is the last thing that was true, which
      // is worth more than nothing as long as the chip admits that is what it is.
      versionFailed = true;
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

  // ---- noticing that the world moved without us
  //
  // The writer's own editor is the other half of this application, and so is the agent
  // holding the MCP server. They will rename a place in Obsidian, pull a branch in a
  // terminal, or have a proposal accepted from outside — and the header went on showing
  // the counts it read at launch. A wrong number looks exactly like a right one, and this
  // one is load-bearing: "2 open questions" is what a writer decides their afternoon on.
  //
  // The backend has always had to solve this — every query reloads if the tree moved
  // (`AppState::read`) — so nothing here is ever *wrong*, only old. This makes the old
  // visible rather than pretending it is current.

  /** The stamp of the files everything on screen was read from. */
  let readAt = $state<string | null>(null);
  /** They have moved since. Nothing on screen is a lie yet; it is out of date. */
  let stale = $state(false);

  /** Take the world's stamp, and let what is on screen answer for it. */
  async function markFresh() {
    try {
      readAt = await api.worldStamp();
      stale = false;
    } catch {
      // No world, or it went out from under us. Nothing to compare against, so nothing
      // to claim: the next successful read sets the mark again.
      readAt = null;
    }
  }

  /** There is a world to watch. Kept apart from `summary` so a save does not restart it. */
  const watching = $derived(!!summary);

  /**
   * Every few seconds, and only ever a stamp.
   *
   * The stamp is a walk of the tree without a parse — the same thing the backend does on
   * every single query — so this is the cheapest question the app can ask, and it stops
   * asking the moment the answer is yes. There is no watcher: a filesystem watch across
   * three platforms, an editor's atomic-rename dance and a network share is a great deal
   * of machinery to be told something a poll notices within three seconds.
   */
  $effect(() => {
    if (!watching) return;
    const timer = setInterval(() => {
      if (stale) return;
      void api
        .worldStamp()
        .then((now) => {
          if (readAt !== null && now !== readAt) stale = true;
        })
        .catch(() => {});
    }, 3000);
    return () => clearInterval(timer);
  });

  /**
   * The error bar is cleared by whatever the *writer* started — scrubbing, opening,
   * saving, rereading — and never by a step inside one of those.
   *
   * The obvious rule is "clear at the top of every fetch", and it eats its own errors:
   * a reread whose lineage fetch fails goes on to fetch the snapshot, which clears the
   * bar on its way past, and the failure the writer needed to see never lands.
   */
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
    error = null;
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

    // Emptied before the load, not after it. Opening a world used to leave every derived
    // view of the *last* one on screen while the new one was read — for a second or two
    // the header carried the new world's name over the old world's findings, the old
    // world's chapters and the old world's coastline, which is the single most misleading
    // state this app can be in. And if the path is wrong it never resolves at all: the
    // error appears above a world the app is no longer holding.
    summary = null;
    events = [];
    snapshot = null;
    terrain = null;
    backdrop = null;
    findings = [];
    proposals = [];
    scenes = [];
    story = null;
    lineage = null;
    version = null;
    versionFailed = false;
    readAt = null;
    stale = false;
    day = 0;
    label = "";
    view = "map";
    mapQueries = 0;
    scrubSteps = 0;
    selected = null;
    // A way back into a world that is no longer open leads nowhere.
    mark = null;
    // Nor does anything kept about the last one: a comparison against a revision of a
    // repository we are not in, a baton nothing in this world holds, and — the visible
    // one — a map panned to a corner of a coastline that is not there any more.
    kept = blankKept();
    panel = "inspector";
    lastPanel = "inspector";

    try {
      summary = await api.openWorld(path);
      events = await api.timeline();
      findings = await api.checkWorld();
      proposals = await api.listProposals();
      scenes = await api.scenes();
      story = await api.story();
      recent = await api.recentWorlds();
      void loadVersion();
      await markFresh();

      const [lo, hi] = summary.span;
      const start = Math.round(lo + (hi - lo) * 0.62);
      day = start;
      lastBucket = bucketOf(start);
      await fetchSnapshot(start);

      // Terrain last, and awaited separately: it is the one fetch that can take a second,
      // and the timeline is usable long before the ground under it has been drawn. It is
      // also fetched exactly once — nothing in it moves when the scrubber does.
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
    error = null;
    if (!expr) return;
    try {
      const resolved = await api.resolveExpr(expr);
      if (resolved === null) {
        error = `"${expr}" has no position on the timeline.`;
      } else {
        jumpTo(resolved);
      }
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * Read the whole world again.
   *
   * Written for a decided proposal, which may have rewritten any file — the backend has
   * already reloaded, so this is only the derived views catching up. It is now also what
   * the writer presses when the files moved underneath them, which is the same job asked
   * by a different party, and one refetch is easier to keep honest than two.
   */
  async function refresh() {
    error = null;
    try {
      summary = await api.openWorld(rootPath);
      events = await api.timeline();
      findings = await api.checkWorld();
      proposals = await api.listProposals();
      scenes = await api.scenes();
      story = await api.story();
      if (lineage) await loadLineage();
      void loadVersion();
      worldMoved();
      lastBucket = -1;
      await fetchSnapshot(day);
      await markFresh();
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
    error = null;
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
      worldMoved();
      lastBucket = -1;
      await fetchSnapshot(day);
      terrain = await api.terrain();
      if (terrain) backdrop = await api.mapImage();
      await markFresh();
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


  /**
   * Move the panel, leaving a mark on the one being left.
   *
   * Everything that *opens* something goes through here, which is what makes `lastPanel`
   * worth anything: a route that set `panel` for itself would be a route the way back had
   * never heard of, which is the failure the one door was written to end. The two that do
   * not come through are the two that are not openings — a close spends the mark instead
   * of leaving one, and opening a world resets both.
   */
  function show(next: Panel) {
    if (next !== panel && panel !== "edit") lastPanel = panel;
    panel = next;
  }

  function carry(intent: Intent) {
    if (intent.act === "select") {
      selected = intent.id;
      if (!intent.show || intent.id === null) return;
      if (panel === "edit") dropForm();
      show("inspector");
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
      show("inspector");
      return;
    }

    if (panel === "edit") dropForm();

    if (intent.act === "edit") {
      editTarget = { kind: intent.kind, id: intent.id, focus: intent.focus, type: intent.type };
      if (intent.id) selected = intent.id;
      show("edit");
    } else if (intent.act === "open") {
      // Set here rather than left to `open`, which only gets there after the load: a
      // panel that still said "edit" over a form that had just been dropped would be a
      // lie for as long as the world takes to read, and for good if the path is wrong.
      show("inspector");
      void open(intent.path);
    } else if (intent.act === "close") {
      // Spent on the way through. A mark that survived being used would make the panel it
      // returns to its own way back — close the form from the story panel, and the story
      // panel's own "‹ back to the world" would then do nothing at all, because it would
      // be asking to go where it already was. Closing walks out, one door at a time.
      const back = lastPanel;
      lastPanel = "inspector";
      panel = back;
    } else {
      show(intent.panel);
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

  // ---- where the keyboard goes when a panel closes

  let bodyEl = $state<HTMLDivElement | null>(null);
  /**
   * Skipped once. Nothing has been closed at the first render, and an app that grabs the
   * keyboard on launch is answering a question nobody asked.
   */
  let panelSettled = false;

  /**
   * Put focus back in the panel column when the last thing that had it was destroyed.
   *
   * Closing a panel is the one move that reliably drops the keyboard on the floor: the
   * button that did it was the panel's own "‹ back to the world", so it goes with the
   * panel and `document.activeElement` falls to `<body>`. From there the next Tab starts
   * at the top of the window — past the header, past the jump field — which is how a
   * keyboard writer loses their place for the price of closing something.
   *
   * Only when focus was *lost*. Focus that landed somewhere on purpose — the chip that
   * opened this panel, still on screen and still focused — is not ours to move.
   */
  $effect(() => {
    void panel;
    if (!panelSettled) {
      panelSettled = true;
      return;
    }
    // After the effects, not among them, because a panel that means to place the caret
    // itself has to be allowed to win: the form opening on the fact the writer clicked
    // focuses that box, and this would otherwise take it straight back. By then focus is
    // no longer on `<body>` and this does nothing at all.
    //
    // A timeout rather than `requestAnimationFrame`, which is the obvious way to write
    // this and does not work: a window nobody is looking at does not paint, so the frame
    // never comes and the keyboard stays on the floor until the writer clicks the app.
    // Worth knowing generally — nothing that must happen can be hung off a frame.
    const soon = setTimeout(() => {
      if (document.activeElement !== document.body) return;
      bodyEl?.querySelector("aside")?.focus({ preventScroll: true });
    });
    return () => clearTimeout(soon);
  });

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
    error = null;
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
      worldMoved();
      lastBucket = -1;
      await fetchSnapshot(day);
      if (markerChanged && terrain) terrain.places = await api.terrainPlaces();
      // Our own write moved the files, so the mark moves with it. Without this every save
      // would raise the "the files moved" flag against the writer who just saved.
      await markFresh();
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
        <!-- Beside "open…" rather than out among the counts. Both are operations on the
             world as a folder of files rather than on anything inside it, and publish had
             been sitting in the row of things you make, which it is not one of. -->
        {#if summary}
          <button
            class="open"
            aria-pressed={panel === "export"}
            onclick={() => toggle("export")}
            title="Write this world out as one file">publish…</button
          >
        {/if}
      </p>
      <h1>{summary?.name ?? "No world open"}</h1>
    </div>

    <div class="readout">
      <div class="when">
        <p class="date">{label || "—"}</p>
        <p class="daynum">day {day.toLocaleString()}</p>
      </div>

      <!-- The one thing in this header that is a *problem*, at the one size nothing else
           in the header is. It is here and not in the chip row because a header of eleven
           equal-weight controls has no answer to "what should I look at first", and this
           is the answer whenever there is one: a definite finding is the world contradicting
           itself, and it is worth more of the writer's attention than the scene count.

           When there are none, nothing is sized to be read first, which is also correct. -->
      {#if definiteCount > 0}
        <button
          class="broken"
          class:stale
          aria-pressed={panel === "checks"}
          onclick={() => toggle("checks")}
          title="Contradictions this world cannot be right about"
        >
          <span class="count">{definiteCount}</span>
          <span class="cap">definite</span>
        </button>
      {/if}
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

        <!-- Only when the files moved. It is not a status light that is usually green:
             a control that is nearly always in one state teaches the eye to stop seeing
             it, and this one has something to say on the few occasions it appears. -->
        {#if stale}
          <button
            class="chip moved"
            onclick={() => void refresh()}
            title="The world folder changed outside this window — read it again"
          >
            ↻ the files moved
          </button>
        {/if}

        <button
          class="chip"
          class:on={panel === "checks"}
          class:stale
          class:bad={definiteCount > 0}
          class:note={definiteCount === 0 && openCount > 0}
          aria-pressed={panel === "checks"}
          onclick={() => toggle("checks")}
          title="Deterministic consistency rules"
        >
          <!-- No count when there are definite findings: the readout above is carrying
               that number at four times the size, and saying it twice in one header is
               how a header stops being read. -->
          {#if definiteCount > 0}
            checks
          {:else if openCount > 0}
            {openCount} open question{openCount === 1 ? "" : "s"}
          {:else}
            consistent
          {/if}
        </button>

        <button
          class="chip"
          class:on={panel === "proposals"}
          class:stale
          class:live={pendingCount > 0}
          aria-pressed={panel === "proposals"}
          onclick={() => toggle("proposals")}
          title="Changes awaiting review"
        >
          {pendingCount} pending
        </button>

        {#if story}
          <button
            class="chip"
            class:on={panel === "story"}
            class:stale
            class:live={story.standing === "linked"}
            class:bad={story.standing === "root_missing"}
            aria-pressed={panel === "story"}
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

        {#if versionFailed || (version && version.kind !== "none")}
          <button
            class="chip"
            class:on={panel === "version"}
            class:off={versionFailed}
            class:stale
            class:live={!versionFailed && version?.dirty === 0}
            class:note={!versionFailed && (version?.dirty ?? 0) > 0}
            aria-pressed={panel === "version"}
            onclick={() => toggle("version")}
            title={versionFailed
              ? "Version control did not answer just now — this is the last thing it said"
              : "Save points and what-ifs"}
          >
            {#if version && version.dirty > 0}
              {version.dirty} to save
            {:else if version?.branch}
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
        </span>
      </div>

      <!-- One title each. The whole block used to carry a single tooltip about the last
           stat in it, so the four counts a writer would actually wonder about explained
           nothing, and the one that did was explained by hovering somewhere else. -->
      <dl class="stats" class:stale>
        <div title="Records with a lifespan: people, places, polities, things">
          <dt>entities</dt>
          <dd>{summary.entity_count}</dd>
        </div>
        <div title="Dated happenings, which is what everything else hangs its dates off">
          <dt>events</dt>
          <dd>{summary.event_count}</dd>
        </div>
        {#if summary.scene_count > 0}
          <div title="Chapters of the book placed in the world, in reading order">
            <dt>scenes</dt>
            <dd>{summary.scene_count}</dd>
          </div>
        {/if}
        <!-- Not "changes". These are the only instants at which this world is different
             from the instant before, which is why dragging across three centuries costs a
             handful of queries and not three centuries of them. -->
        <div title="Days on which anything about this world changes — the whole timeline is flat between them">
          <dt>turning points</dt>
          <dd>{summary.change_points.length}</dd>
        </div>
        <!-- The change-point premise with its workings shown: snapshots actually fetched
             against scrubber movements. It is a claim about the engine being demonstrated
             live, which belongs in front of whoever is building it and nobody else. -->
        {#if import.meta.env.DEV}
          <div class="hot" title="Snapshots fetched / scrub movements — dev builds only">
            <dt>queries</dt>
            <dd>{mapQueries} <span>/ {scrubSteps}</span></dd>
          </div>
        {/if}
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
    <!-- Dismissable, because an error bar with no way out is furniture. Most of these
         clear themselves on the next fetch; the ones that do not are the ones the writer
         has read, understood and would like to stop looking at. -->
    <p class="error">
      <span>{error}</span>
      <button onclick={() => (error = null)} title="Dismiss">×</button>
    </p>
  {/if}

  <div class="body" class:editing={panel === "edit"} bind:this={bodyEl}>
    <div class="stage">
      <!-- Two projections of one timeline: the map onto the ground, the lineage onto
           descent. They swap here rather than sitting side by side, because both want
           the width and both are driven by the same scrubber underneath. -->
      {#if view === "map"}
        <MapView
          kept={kept.map}
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
        <LineageView kept={kept.lineage} {lineage} {day} {selected} onselect={inspect} onday={goto} />
      {/if}

      <!-- Over the stage rather than in place of it: the map is where the writer is
           looking, so that is where "this is going to take a moment" belongs. -->
      {#if busy}
        <div class="opening">Opening {opened}…</div>
      {/if}

      {#if summary}
        <div class="views">
          <PillGroup
            options={[
              { value: "map", label: "map" },
              { value: "lineage", label: "lineage" },
            ]}
            value={view}
            onpick={(v: string) => {
              view = v as "map" | "lineage";
              if (view === "lineage" && !lineage) void loadLineage();
            }}
          />
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
      <Proposals {proposals} ondecided={refresh} onclose={closePanel} />
    {:else if panel === "story"}
      <StoryPanel
        kept={kept.story}
        {story}
        {scenes}
        {names}
        onselect={inspect}
        onscene={(id) => openEditor("scene", id)}
        onclose={closePanel}
      />
    {:else if panel === "version"}
      <VersionPanel
        kept={kept.version}
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

  /* The only creative actions in a header full of counts, so they read as one — and
     filled rather than outlined, because everything else in this row is a *reading* and
     these three are the only things you can press to make something exist. Small enough
     at 10.5px that three of them do not compete with the one element sized to be read
     first. */
  .chip.make {
    background: var(--accent);
    color: var(--paper);
    border-color: var(--accent);
  }

  .chip.make:hover {
    background: color-mix(in srgb, var(--accent) 82%, var(--paper));
    border-color: color-mix(in srgb, var(--accent) 82%, var(--paper));
    color: var(--paper);
  }

  .chip:hover {
    border-color: var(--rule-strong);
    color: var(--ink-2);
  }

  /* The same treatment the map/lineage toggle uses, because it answers the same question:
     which of these is showing. Deliberately background and border only — the text colour
     stays whatever the count means, so a chip does not stop being red for the duration of
     the panel that would tell you why it is red. */
  .chip.on {
    background: var(--accent-soft);
    border-color: var(--rule-strong);
  }

  .chip.moved {
    color: var(--era);
    border-color: color-mix(in srgb, var(--era) 45%, transparent);
    text-transform: none;
    letter-spacing: 0.04em;
  }

  .chip.moved:hover {
    color: var(--accent);
    border-color: var(--accent);
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

  /* Last of the chip rules, and that is the whole point: staleness outranks every state
     a chip can be in, because the state is exactly what is no longer known. An amber
     "2 open questions" that may now be four is a worse thing to show than a grey one
     admitting it has not looked since the files changed. */
  .chip.stale,
  .stats.stale div:not(.hot) dd,
  .broken.stale .count {
    color: var(--rule-strong);
  }
  /* `.hot` is excluded on purpose: the query counter is a reading of what this session
     did, and the files moving on disk does not make it any less true. */

  .chip.stale {
    border-style: dashed;
  }

  /* Version control did not answer. Not hidden, not alarming: unknown. */
  .chip.off {
    color: var(--rule-strong);
    border-style: dotted;
  }

  .eyebrow {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--accent);
  }

  /* Down from 17px, and on purpose. The world's name is the one thing in this header the
     writer already knows — they chose the folder — so it does not need to be the largest
     thing on the screen, and while it was, nothing else could be. */
  h1 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .readout {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
  }

  .when {
    text-align: center;
  }

  .date {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    white-space: nowrap;
  }

  /* The one element in the header sized to be read first, and only ever present when
     there is something to read first about. */
  .broken {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 2px 9px 3px;
    border-left: 2px solid var(--warn);
    background: color-mix(in srgb, var(--warn) 9%, transparent);
  }

  .broken .count {
    font-size: 17px;
    font-weight: 600;
    line-height: 1.1;
    color: var(--warn);
    font-variant-numeric: tabular-nums;
  }

  .broken .cap {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: color-mix(in srgb, var(--warn) 75%, var(--ink-3));
  }

  .broken:hover {
    background: color-mix(in srgb, var(--warn) 16%, transparent);
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

  .eyebrow .open:hover,
  .eyebrow .open[aria-pressed="true"] {
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
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin: 0;
    padding: 8px 20px;
    background: var(--surface-2);
    border-bottom: 1px solid var(--warn);
    color: var(--warn);
    font-size: 12.5px;
  }

  .error span {
    flex: 1;
    min-width: 0;
  }

  .error button {
    font-family: var(--f-mono);
    font-size: 14px;
    line-height: 1;
    color: color-mix(in srgb, var(--warn) 65%, transparent);
  }

  .error button:hover {
    color: var(--warn);
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

  .opening {
    position: absolute;
    inset: 0;
    z-index: 4;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--paper) 72%, transparent);
    font-family: var(--f-mono);
    font-size: 11.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-3);
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
