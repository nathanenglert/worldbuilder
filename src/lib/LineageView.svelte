<script lang="ts">
  import type { Lineage, Life, Succession } from "./api";
  import PillGroup from "./PillGroup.svelte";

  let {
    kept,
    lineage,
    day,
    selected,
    onselect,
    onday,
  }: {
    /**
     * Which baton is being followed — held by the app, because this view is the other
     * arm of the same `{#if}` the map is in. Picking a succession is a choice about what
     * to look at, and flicking to the map to check where a duchy is put it back to plain
     * descent.
     */
    kept: { chosen: string };
    lineage: Lineage | null;
    day: number;
    selected: string | null;
    onselect: (id: string) => void;
    onday: (day: number) => void;
  } = $props();

  /** `all` shows descent; a key shows one baton and only the records that held it. */
  const chosen = $derived(kept.chosen);

  const succession = $derived<Succession | null>(
    lineage?.successions.find((s) => s.key === chosen) ?? null,
  );

  /**
   * The rows, in the order the chosen view wants them.
   *
   * Descent sorts by generation, which is what the backend already did. A succession
   * sorts by *tenure* instead — the order the thing was held in, which is the only order
   * that makes the baton read left to right down the chart.
   */
  const rows = $derived.by((): Life[] => {
    const lives = lineage?.lives ?? [];
    if (!succession) return lives;
    const order = succession.holders.map((h) => h.holder);
    return order
      .map((id) => lives.find((l) => l.id === id))
      .filter((l): l is Life => l !== undefined);
  });

  /**
   * The axis, fitted to what is on screen rather than shared with the timeline below.
   *
   * Three actors over four thousand years of recorded history occupy two percent of the
   * main track, and a chart nobody can read is not a projection of anything. The two are
   * coupled by the *day* instead: the scrubber's position is drawn here, and clicking
   * here moves it.
   */
  const span = $derived.by((): [number, number] => {
    const points: number[] = [];
    for (const life of rows) {
      if (life.earliest !== null) points.push(life.earliest);
      if (life.latest !== null) points.push(life.latest);
    }
    for (const t of succession?.holders ?? []) {
      if (t.earliest !== null) points.push(t.earliest);
      if (t.latest !== null) points.push(t.latest);
    }
    if (points.length === 0) return [day - 1000, day + 1000];
    const lo = Math.min(...points);
    const hi = Math.max(...points);
    const pad = Math.max(360, Math.round((hi - lo) / 12));
    return [lo - pad, hi + pad];
  });

  const width = $derived(Math.max(1, span[1] - span[0]));
  const pct = (d: number) => ((d - span[0]) / width) * 100;
  /** Clamped, so a life running past the window draws to the edge instead of off it. */
  const clamped = (d: number | null, fallback: number) =>
    Math.min(100, Math.max(0, pct(d ?? fallback)));

  /** Year ticks, thinned to something a reader can count. */
  const ticks = $derived.by(() => {
    const years = width / 360;
    const step = [1, 2, 5, 10, 25, 50, 100, 250, 500, 1000].find((s) => years / s <= 9) ?? 2000;
    const out: number[] = [];
    const first = Math.ceil(span[0] / 360 / step) * step;
    for (let y = first; y * 360 <= span[1]; y += step) out.push(y * 360);
    return out;
  });

  /** Which row each record is on, so a parentage line knows where to land. */
  const rowOf = $derived(new Map(rows.map((life, i) => [life.id, i])));

  const ROW = 30;

  function tenuresOf(id: string) {
    // In the descent view a record shows every baton it ever held, so a title is visible
    // without having to go looking for which succession it belongs to.
    const from = succession ? [succession] : (lineage?.successions ?? []);
    return from.flatMap((s) =>
      s.holders.filter((h) => h.holder === id).map((h) => ({ ...h, label: s.label })),
    );
  }

  function scrub(e: MouseEvent) {
    const track = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const t = Math.min(1, Math.max(0, (e.clientX - track.left) / track.width));
    onday(Math.round(span[0] + t * width));
  }
</script>

<div class="lineage">
  {#if !lineage}
    <p class="empty">Working out who is related to whom…</p>
  {:else if lineage.lives.length === 0}
    <p class="empty">
      Nothing in this world has a parent or holds anything that changed hands. Give a
      record <code>parents:</code>, or two records the same <code>title</code>, and a line
      appears here.
    </p>
  {:else}
    <div class="picker">
      <span class="cap">showing</span>
      <PillGroup
        options={[
          { value: "all", label: "descent" },
          ...lineage.successions.map((s) => ({
            value: s.key,
            label: s.label,
            title: `${s.holders.length} holders · ${s.kind}`,
          })),
        ]}
        value={chosen}
        onpick={(v) => (kept.chosen = v)}
      />
    </div>

    {#if succession}
      <p class="note">
        {succession.holders.length} held it.
        {#if succession.gaps.length}
          <em class="bad"
            >{succession.gaps.length} stretch{succession.gaps.length === 1 ? "" : "es"} nobody
            did.</em
          >
        {/if}
        {#if succession.overlaps.length}
          <!-- Not "contested". Two tenures meeting at a date written `0768~` overlap
               because nobody wrote the day down, which is uncertainty and not a rival
               claim — the same distinction the map draws by hatching a border. -->
          <em class="warn"
            >{succession.overlaps.length} stretch{succession.overlaps.length === 1 ? "" : "es"} the
            world does not settle.</em
          >
        {/if}
      </p>
    {/if}

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="chart" style="height:{rows.length * ROW + 34}px" onclick={scrub}>
      {#each ticks as t (t)}
        <div class="tick" style="left:{pct(t)}%">
          <span>{Math.round(t / 360)}</span>
        </div>
      {/each}

      {#if succession}
        {#each succession.gaps as g (g[0])}
          <div class="gap" style="left:{clamped(g[0], 0)}%; width:{clamped(g[1], 0) - clamped(g[0], 0)}%"></div>
        {/each}
        {#each succession.overlaps as o (o[0])}
          <div class="overlap" style="left:{clamped(o[0], 0)}%; width:{clamped(o[1], 0) - clamped(o[0], 0)}%"></div>
        {/each}
      {/if}

      <!-- Drawn as positioned rules rather than an SVG path: path data has no percentage
           units, and every other x in this chart is a percentage of a fitted axis. -->
      {#each rows as life, i (life.id)}
        {#each life.parents as parent (parent)}
          {#if rowOf.has(parent) && rowOf.get(parent)! < i}
            {@const pr = rowOf.get(parent)!}
            {@const x = clamped(life.earliest, span[0])}
            <div
              class="descent"
              style="left:{x}%; top:{pr * ROW + 28}px; height:{(i - pr) * ROW - 7}px"
            ></div>
            <div class="descent tick-in" style="left:{x}%; top:{i * ROW + 21}px"></div>
          {/if}
        {/each}
      {/each}

      {#each rows as life, i (life.id)}
        {@const lo = clamped(life.earliest, span[0])}
        {@const hi = clamped(life.latest, span[1])}
        {@const core = clamped(life.from, life.earliest ?? span[0])}
        {@const coreEnd = clamped(life.to, life.latest ?? span[1])}
        <div class="row" style="top:{i * ROW + 14}px" class:lit={selected === life.id}>
          <!-- The possible window, drawn faint, with the certain core solid inside it.
               A life that begins `0749~` gets a feathered start rather than a hard edge
               on a year the chapterhouse openly guessed at. -->
          <div class="life" style="left:{lo}%; width:{Math.max(0.4, hi - lo)}%"></div>
          <div
            class="core"
            style="left:{core}%; width:{Math.max(0.4, coreEnd - core)}%"
          ></div>

          <!-- Keyed by index: one record can hold two different things over the same
               days — Marrow is a duchy's capital and a duke's seat from the same oath —
               so holder-and-date is a legitimate duplicate, and a content key throws. -->
          {#each tenuresOf(life.id) as t, ti (ti)}
            {@const tl = clamped(t.earliest, span[0])}
            {@const th = clamped(t.latest, span[1])}
            <div
              class="held"
              style="left:{tl}%; width:{Math.max(0.4, th - tl)}%"
              title={t.label}
            ></div>
          {/each}

          <button class="name" onclick={(e) => { e.stopPropagation(); onselect(life.id); }}>
            {life.name}
          </button>
          <span class="when">{life.label}</span>
        </div>
      {/each}

      <div class="now" style="left:{Math.min(100, Math.max(0, pct(day)))}%"></div>
    </div>

    <div class="legend">
      <span class="key"><i class="k-life"></i>possible</span>
      <span class="key"><i class="k-core"></i>certain</span>
      <span class="key"><i class="k-held"></i>held it</span>
      {#if succession?.gaps.length}
        <span class="key"><i class="k-gap"></i>nobody did</span>
      {/if}
      {#if succession?.overlaps.length}
        <span class="key"><i class="k-unsettled"></i>the dates do not say who</span>
      {/if}
      <span class="key rule">the vertical line is the scrubbed day · click to move it</span>
    </div>
  {/if}
</div>

<style>
  .lineage {
    position: relative;
    overflow: auto;
    padding: 14px 18px 20px;
    background: var(--paper);
  }

  .empty {
    max-width: 44ch;
    font-size: 13px;
    line-height: 1.6;
    color: var(--ink-3);
  }

  code {
    font-family: var(--f-mono);
    font-size: 11.5px;
    color: var(--ink-2);
  }

  .picker {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    margin-bottom: 8px;
  }

  .cap,
  .note,
  .legend {
    font-family: var(--f-mono);
    font-size: 10.5px;
  }

  .cap {
    color: var(--ink-3);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    font-size: 9.5px;
    margin-right: 3px;
  }




  .note {
    margin: 0 0 8px;
    color: var(--ink-3);
  }

  .note em {
    font-style: normal;
    margin-left: 6px;
  }

  .note .bad {
    color: var(--warn);
  }

  .note .warn {
    color: var(--era);
  }

  .chart {
    position: relative;
    min-height: 80px;
    cursor: crosshair;
    border-top: 1px solid var(--rule);
    padding-top: 4px;
  }

  .tick {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--rule);
  }

  .tick span {
    position: absolute;
    top: 0;
    left: 3px;
    font-family: var(--f-mono);
    font-size: 9.5px;
    color: var(--rule-strong);
  }

  .descent {
    position: absolute;
    width: 1px;
    background: var(--rule-strong);
    pointer-events: none;
  }

  .descent.tick-in {
    width: 7px;
    height: 1px;
  }

  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: 26px;
  }

  .life,
  .core,
  .held {
    position: absolute;
    top: 11px;
    pointer-events: none;
  }

  .life {
    height: 6px;
    background: var(--rule-strong);
    opacity: 0.55;
  }

  .core {
    height: 6px;
    background: var(--ink-3);
  }

  .held {
    top: 9px;
    height: 10px;
    background: color-mix(in srgb, var(--accent) 60%, transparent);
    border-left: 1px solid var(--accent);
    border-right: 1px solid var(--accent);
  }

  .row.lit .core {
    background: var(--accent);
  }

  .name {
    position: absolute;
    left: 0;
    top: 0;
    padding: 2px 6px 2px 0;
    font-size: 12.5px;
    color: var(--ink-2);
    text-shadow:
      0 0 4px var(--paper),
      0 0 4px var(--paper);
  }

  .name:hover {
    color: var(--accent);
  }

  .row.lit .name {
    color: var(--accent);
  }

  .when {
    position: absolute;
    right: 0;
    top: 3px;
    font-family: var(--f-mono);
    font-size: 9.5px;
    color: var(--rule-strong);
    text-shadow:
      0 0 4px var(--paper),
      0 0 4px var(--paper);
  }

  /* A hole in a succession is drawn full height and behind everything: it is a statement
     about the office, not about any one row. */
  .gap,
  .overlap {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
  }

  .gap {
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    border-left: 1px dashed color-mix(in srgb, var(--warn) 60%, transparent);
    border-right: 1px dashed color-mix(in srgb, var(--warn) 60%, transparent);
  }

  .overlap {
    background: color-mix(in srgb, var(--era) 14%, transparent);
  }

  .now {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--accent);
    pointer-events: none;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    margin-top: 12px;
    color: var(--ink-3);
  }

  .key {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .key i {
    display: inline-block;
    width: 16px;
    height: 6px;
  }

  .k-life {
    background: var(--rule-strong);
  }

  .k-core {
    background: var(--ink-3);
  }

  .k-held {
    background: color-mix(in srgb, var(--accent) 60%, transparent);
  }

  .k-gap {
    background: color-mix(in srgb, var(--warn) 40%, transparent);
  }

  .k-unsettled {
    background: color-mix(in srgb, var(--era) 40%, transparent);
  }

  .key.rule {
    color: var(--rule-strong);
  }
</style>
