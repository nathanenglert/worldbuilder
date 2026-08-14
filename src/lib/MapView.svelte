<script lang="ts">
  import type { Entity, Layer, Snapshot, Terrain } from "./api";

  let {
    snapshot,
    terrain = null,
    backdrop = null,
    selected = null,
    onselect,
  }: {
    snapshot: Snapshot | null;
    terrain: Terrain | null;
    backdrop: string | null;
    selected: string | null;
    onselect: (id: string | null) => void;
  } = $props();

  // Normalized 0..1 world coordinates scale into this box. The height follows the map
  // image's own aspect when there is one, so the backdrop is never stretched.
  const W = 1000;
  const H = $derived(terrain ? Math.round(W / terrain.aspect) : 700);
  const NEUTRAL = "#7b8b84";

  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);
  let layer = $state<Layer>("biome");
  let showBackdrop = $state(false);

  let wrapEl: HTMLDivElement;
  let svgEl: SVGSVGElement;
  let dragging = false;
  let panned = false;
  let lastX = 0;
  let lastY = 0;

  const regions = $derived((snapshot?.entities ?? []).filter((e) => e.shape.length > 2));
  const markers = $derived((snapshot?.entities ?? []).filter((e) => e.marker !== null));
  const contested = $derived(regions.filter((r) => r.claims.length > 1));

  /**
   * Banded rather than smoothly interpolated, on purpose. A banded ramp reads like a
   * contour map — you can see where a threshold is — and it matches how the biome table
   * decides: by crossing an edge, not by shading toward one.
   */
  const RAMPS: Record<string, string[]> = {
    height: [
      "#3F6247", "#4E7049", "#647C4B", "#7F8850", "#9A8E58",
      "#AE8C5F", "#A9836A", "#9C8C88", "#D4D6D5",
    ],
    temperature: [
      "#5B7FA6", "#6E93AE", "#83A5AC", "#9BB09B", "#B3AE83",
      "#C0A06B", "#C08658", "#B36A4B", "#9E4F43",
    ],
    precipitation: [
      "#C9BE93", "#BBB783", "#A6B078", "#8CA875", "#6E9E7C",
      "#519183", "#3B8184", "#2E6C7C", "#28556B",
    ],
  };

  const LAYERS: { key: Layer; label: string }[] = [
    { key: "biome", label: "biome" },
    { key: "height", label: "height" },
    { key: "temperature", label: "temp" },
    { key: "precipitation", label: "rain" },
    { key: "none", label: "off" },
  ];

  const band = (t: number, stops: string[]) =>
    stops[Math.min(stops.length - 1, Math.max(0, Math.floor(t * stops.length)))];

  /** The observed range of a quantity over land, so a ramp spends its whole travel on
   *  ground that exists rather than on a theoretical 0..1. */
  function landRange(values: number[], isLand: boolean[]): [number, number] {
    let lo = Infinity;
    let hi = -Infinity;
    for (let i = 0; i < values.length; i++) {
      if (!isLand[i]) continue;
      if (values[i] < lo) lo = values[i];
      if (values[i] > hi) hi = values[i];
    }
    return lo <= hi ? [lo, hi] : [0, 1];
  }

  const quantity = $derived.by(() => {
    if (!terrain || layer === "none" || layer === "biome") return null;
    const values = terrain[layer];
    const [lo, hi] = landRange(values, terrain.is_land);
    return { values, lo, hi, span: hi - lo || 1, stops: RAMPS[layer] };
  });

  /**
   * Every cell, bucketed by the colour it ends up. A few thousand polygons become a
   * handful of paths, which is the difference between a map that pans and one that does
   * not — and it costs nothing, because none of this changes when the scrubber moves.
   */
  const cellPaths = $derived.by(() => {
    if (!terrain || layer === "none") return [];
    const q = quantity;
    const buckets = new Map<string, string[]>();

    for (let i = 0; i < terrain.cells.length; i++) {
      // Water keeps its own colour in every layer. Shading the sea by rainfall makes the
      // coastline vanish, and the coastline is the thing the writer drew.
      const color =
        !terrain.is_land[i] || terrain.lake[i] || !q
          ? terrain.palette[terrain.biome[i]].color
          : band((q.values[i] - q.lo) / q.span, q.stops);

      const flat = terrain.cells[i];
      let d = "";
      for (let k = 0; k < flat.length; k += 2) {
        d += `${k ? "L" : "M"}${(flat[k] * W).toFixed(1)},${(flat[k + 1] * H).toFixed(1)}`;
      }
      const bucket = buckets.get(color);
      if (bucket) bucket.push(d + "Z");
      else buckets.set(color, [d + "Z"]);
    }

    return [...buckets].map(([color, parts]) => ({ color, d: parts.join("") }));
  });

  /** The coastline as one path. Holes punch through under `evenodd`. */
  const coastPath = $derived.by(() => {
    if (!terrain) return "";
    return terrain.coast
      .map((ring) => {
        let d = "";
        for (let k = 0; k < ring.points.length; k += 2) {
          d += `${k ? "L" : "M"}${(ring.points[k] * W).toFixed(1)},${(ring.points[k + 1] * H).toFixed(1)}`;
        }
        return d + "Z";
      })
      .join("");
  });

  /**
   * Rivers, one segment per pair of points, so a channel can widen downstream. A single
   * stroked polyline cannot taper, and the taper is most of what makes a river read as
   * one rather than as a contour.
   */
  const riverSegments = $derived.by(() => {
    if (!terrain) return [];
    const peak = Math.max(...terrain.rivers.flatMap((r) => r.flux), 1e-9);
    const out: { d: string; w: number }[] = [];
    for (const river of terrain.rivers) {
      for (let k = 0; k + 3 < river.points.length; k += 2) {
        const [x0, y0, x1, y1] = river.points.slice(k, k + 4);
        out.push({
          d: `M${(x0 * W).toFixed(1)},${(y0 * H).toFixed(1)}L${(x1 * W).toFixed(1)},${(y1 * H).toFixed(1)}`,
          // Square root, because flux grows with catchment area and width grows with
          // discharge — a linear map turns the main stem into a lake.
          w: 0.7 + 3.1 * Math.sqrt(Math.min(1, (river.flux[k / 2] ?? 0) / peak)),
        });
      }
    }
    return out;
  });

  /** The biomes actually present, commonest first, for the legend. */
  const biomeLegend = $derived.by(() => {
    if (!terrain) return [];
    const style = new Map(terrain.palette.map((p) => [p.label, p.color]));
    return terrain.summary.biomes
      .slice(0, 7)
      .map(([label, cells]) => ({ label, cells, color: style.get(label) ?? NEUTRAL }));
  });

  /**
   * What the ends of a ramp say. Degrees are a real unit and get printed; the other two
   * are normalized, so a number there would be a false precision — `0.88 rain` is not a
   * quantity of anything. Name the ends instead.
   */
  const legend = $derived.by(() => {
    if (!quantity) return null;
    if (layer === "temperature") {
      const round = (v: number) => `${v.toFixed(0)}°`;
      return { title: "temperature", lo: round(quantity.lo), hi: round(quantity.hi) };
    }
    if (layer === "height") return { title: "elevation", lo: "shore", hi: "highest" };
    return { title: "rainfall", lo: "driest", hi: "wettest" };
  });

  function pathOf(shape: [number, number][]): string {
    const points = shape.map(
      ([x, y], i) => `${i ? "L" : "M"}${(x * W).toFixed(1)},${(y * H).toFixed(1)}`,
    );
    return points.join(" ") + " Z";
  }

  function centroid(shape: [number, number][]): [number, number] {
    const sx = shape.reduce((a, p) => a + p[0], 0) / shape.length;
    const sy = shape.reduce((a, p) => a + p[1], 0) / shape.length;
    return [sx * W, sy * H];
  }

  const colorOf = (e: Entity) => e.claims[0]?.color ?? NEUTRAL;

  /**
   * Screen pixels to viewBox units. Uses the SVG's own matrix rather than element-rect
   * ratios, which are wrong whenever `preserveAspectRatio` letterboxes the drawing.
   */
  function toViewBox(clientX: number, clientY: number): { x: number; y: number } {
    const ctm = svgEl?.getScreenCTM();
    if (!ctm) return { x: 0, y: 0 };
    const p = new DOMPoint(clientX, clientY).matrixTransform(ctm.inverse());
    return { x: p.x, y: p.y };
  }

  function zoomAbout(x: number, y: number, factor: number) {
    const next = Math.min(9, Math.max(0.55, scale * factor));
    // Keep whatever sits under (x, y) pinned there.
    tx = x - (x - tx) * (next / scale);
    ty = y - (y - ty) * (next / scale);
    scale = next;
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    const { x, y } = toViewBox(e.clientX, e.clientY);
    zoomAbout(x, y, Math.exp(-e.deltaY * 0.0016));
  }

  // Attached by hand because framework-added wheel listeners are often passive,
  // which would make preventDefault a no-op and let the window scroll instead.
  $effect(() => {
    const el = wrapEl;
    if (!el) return;
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });

  function pointerdown(e: PointerEvent) {
    dragging = true;
    panned = false;
    lastX = e.clientX;
    lastY = e.clientY;
    wrapEl.setPointerCapture(e.pointerId);
  }

  function pointermove(e: PointerEvent) {
    if (!dragging) return;
    const from = toViewBox(lastX, lastY);
    const to = toViewBox(e.clientX, e.clientY);
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    if (Math.abs(dx) + Math.abs(dy) > 0.5) panned = true;
    tx += dx;
    ty += dy;
    lastX = e.clientX;
    lastY = e.clientY;
  }

  function pointerup(e: PointerEvent) {
    dragging = false;
    wrapEl.releasePointerCapture?.(e.pointerId);
  }

  /** A click that wasn't the tail of a pan clears the selection. */
  function backgroundClick() {
    if (!panned) onselect(null);
  }

  function keydown(e: KeyboardEvent) {
    const step = 40 / scale;
    switch (e.key) {
      case "Escape":
        onselect(null);
        break;
      case "+":
      case "=":
        zoomAbout(W / 2, H / 2, 1.2);
        break;
      case "-":
        zoomAbout(W / 2, H / 2, 1 / 1.2);
        break;
      case "0":
        reset();
        break;
      case "ArrowLeft":
        tx += step;
        break;
      case "ArrowRight":
        tx -= step;
        break;
      case "ArrowUp":
        ty += step;
        break;
      case "ArrowDown":
        ty -= step;
        break;
      default:
        return;
    }
    e.preventDefault();
  }

  function reset() {
    scale = 1;
    tx = 0;
    ty = 0;
  }
</script>

<!--
  The interaction surface is the wrapper; the SVG itself is just what gets drawn.

  `role="application"` is correct for a pan/zoom canvas — it tells assistive tech to
  pass keystrokes through to us rather than intercepting them for browse mode. Svelte's
  a11y rules follow aria-query, which files `application` under structure rather than
  widget roles, so the interaction warnings here are a taxonomy gap and not a defect.
  Keyboard parity is real: arrows pan, +/- zoom, 0 resets, Escape clears the selection.
-->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="wrap"
  bind:this={wrapEl}
  role="application"
  tabindex="0"
  aria-label="World map. Drag to pan, scroll to zoom, escape to clear selection."
  onpointerdown={pointerdown}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointercancel={pointerup}
  onclick={backgroundClick}
  onkeydown={keydown}
>
  <svg bind:this={svgEl} viewBox="0 0 {W} {H}" preserveAspectRatio="xMidYMid meet" aria-hidden="true">
    <defs>
      <pattern id="grid" width="50" height="50" patternUnits="userSpaceOnUse">
        <path d="M50 0 L0 0 0 50" fill="none" stroke="var(--rule)" stroke-width="1" />
      </pattern>

      <!-- One hatch per contested region, weaving both claimants' colours together. -->
      {#each contested as r (r.id)}
        <pattern
          id="hatch-{r.id}"
          width="16"
          height="16"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect width="16" height="16" fill={r.claims[0]?.color ?? NEUTRAL} opacity="0.30" />
          <rect width="8" height="16" fill={r.claims[1]?.color ?? NEUTRAL} opacity="0.38" />
        </pattern>
      {/each}
    </defs>

    <rect width={W} height={H} fill="var(--surface)" />
    {#if !terrain || (layer === "none" && !showBackdrop)}
      <rect width={W} height={H} fill="url(#grid)" opacity="0.5" />
    {/if}

    <!-- Terrain: fetched once, and never touched again by the scrubber. -->
    <g class="terrain" transform="translate({tx} {ty}) scale({scale})">
      {#if backdrop && showBackdrop}
        <!-- Stage 1 of the pipeline stays on the map as a display layer. The vectors are
             what is queryable; the writer's own art is what they recognise. -->
        <image
          href={backdrop}
          x="0"
          y="0"
          width={W}
          height={H}
          preserveAspectRatio="none"
          opacity={layer === "none" ? 0.9 : 0.35}
        />
      {/if}

      {#each cellPaths as p (p.color)}
        <path d={p.d} fill={p.color} fill-opacity={showBackdrop ? 0.6 : 0.92} />
      {/each}

      {#if terrain}
        {#if layer === "none" && !showBackdrop}
          <path d={coastPath} fill="var(--surface-2)" fill-rule="evenodd" />
        {/if}
        <path
          d={coastPath}
          fill="none"
          stroke="var(--ink-2)"
          stroke-width={1.2 / scale}
          stroke-opacity="0.7"
          stroke-linejoin="round"
        />
        {#each riverSegments as s, i (i)}
          <path
            d={s.d}
            fill="none"
            stroke="#4E86A0"
            stroke-width={s.w / scale}
            stroke-linecap="round"
          />
        {/each}
      {/if}
    </g>

    <g transform="translate({tx} {ty}) scale({scale})">
      {#each regions as r (r.id)}
        {@const uncertain = r.claims.length > 1}
        {@const c = colorOf(r)}
        {@const [cx, cy] = centroid(r.shape)}
        <g class="region" class:selected={selected === r.id}>
          <path
            d={pathOf(r.shape)}
            fill={uncertain ? `url(#hatch-${r.id})` : c}
            fill-opacity={uncertain ? 1 : 0.34}
            stroke={uncertain ? "var(--ink-2)" : c}
            stroke-width={(selected === r.id ? 3.2 : 1.8) / scale}
            stroke-dasharray={uncertain ? `${9 / scale} ${6 / scale}` : undefined}
            stroke-linejoin="round"
            onclick={(e) => {
              e.stopPropagation();
              if (!panned) onselect(r.id);
            }}
            role="button"
            tabindex="-1"
            onkeydown={(e) => e.key === "Enter" && onselect(r.id)}
          />
          <!-- Sits above the name, where it cannot collide with a settlement label
               that happens to fall near the centroid. -->
          {#if uncertain}
            <text x={cx} y={cy - 15 / scale} class="disputed" font-size={10.5 / scale}>
              CONTESTED
            </text>
          {/if}
          <text x={cx} y={cy} class="region-name" font-size={15 / scale}>{r.name}</text>
        </g>
      {/each}

      {#each markers as m (m.id)}
        {@const [x, y] = m.marker ?? [0, 0]}
        <g class="marker" class:selected={selected === m.id} class:faint={m.existence === "maybe"}>
          <circle
            cx={x * W}
            cy={y * H}
            r={(selected === m.id ? 7 : 5) / scale}
            fill="var(--paper)"
            stroke={selected === m.id ? "var(--accent)" : "var(--ink)"}
            stroke-width={2 / scale}
            onclick={(e) => {
              e.stopPropagation();
              if (!panned) onselect(m.id);
            }}
            role="button"
            tabindex="-1"
            onkeydown={(e) => e.key === "Enter" && onselect(m.id)}
          />
          <text x={x * W + 11 / scale} y={y * H + 4 / scale} font-size={13 / scale}>{m.name}</text>
        </g>
      {/each}
    </g>
  </svg>

  {#if terrain}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="layers" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <span class="cap">terrain</span>
      <div class="pills">
        {#each LAYERS as l (l.key)}
          <button class:on={layer === l.key} onclick={() => (layer = l.key)}>{l.label}</button>
        {/each}
      </div>
      {#if backdrop}
        <button class="wide" class:on={showBackdrop} onclick={() => (showBackdrop = !showBackdrop)}>
          imported map
        </button>
      {/if}
    </div>
  {/if}

  <div class="hud">
    <button
      onclick={(e) => {
        e.stopPropagation();
        reset();
      }}>reset view</button
    >
    <span>{(scale * 100).toFixed(0)}%</span>
  </div>

  <div class="legends">
    {#if terrain && layer === "biome"}
      <div class="legend wrapped">
        {#each biomeLegend as b (b.label)}
          <span class="key"><i style="background:{b.color}"></i>{b.label}</span>
        {/each}
      </div>
    {:else if quantity && legend}
      <div class="legend">
        <span class="cap">{legend.title}</span>
        <span class="num">{legend.lo}</span>
        <span class="ramp">
          {#each quantity.stops as c, i (i)}
            <i style="background:{c}"></i>
          {/each}
        </span>
        <span class="num">{legend.hi}</span>
      </div>
    {/if}

    {#if terrain && terrain.summary.rivers > 0 && layer !== "none"}
      <div class="legend">
        <span class="key"><i class="river"></i>{terrain.summary.rivers} rivers, widening with flow</span>
      </div>
    {/if}

    {#if contested.length > 0}
      <div class="legend">
        <span class="swatch"></span>
        hatched + dashed = the handover date is vague, so both claims are live
      </div>
    {/if}
  </div>
</div>

<style>
  .wrap {
    position: relative;
    height: 100%;
    min-height: 0;
    background: var(--surface);
    touch-action: none;
    cursor: grab;
  }

  .wrap:active {
    cursor: grabbing;
  }

  svg {
    display: block;
    width: 100%;
    height: 100%;
  }

  .region path,
  .marker circle {
    cursor: pointer;
  }

  .region path {
    transition:
      fill-opacity 140ms ease,
      stroke-width 140ms ease;
  }

  .region:hover path {
    fill-opacity: 0.5;
  }

  .region-name {
    fill: var(--ink);
    font-family: var(--f-body);
    font-weight: 600;
    text-anchor: middle;
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
  }

  .disputed {
    fill: var(--ink-2);
    font-family: var(--f-mono);
    letter-spacing: 0.14em;
    text-anchor: middle;
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
  }

  .marker text {
    fill: var(--ink-2);
    font-family: var(--f-body);
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
  }

  .marker.faint circle {
    stroke-dasharray: 3 2;
  }

  .marker.selected text,
  .marker:hover text {
    fill: var(--ink);
  }

  .terrain {
    pointer-events: none;
  }

  .hud {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 10px;
    font-family: var(--f-mono);
    font-size: 11px;
    color: var(--ink-3);
    background: color-mix(in srgb, var(--paper) 82%, transparent);
    border: 1px solid var(--rule);
    padding: 5px 9px;
  }

  .hud button:hover {
    color: var(--accent);
  }

  .layers {
    position: absolute;
    left: 12px;
    top: 12px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-family: var(--f-mono);
    font-size: 10.5px;
    background: color-mix(in srgb, var(--paper) 86%, transparent);
    border: 1px solid var(--rule);
    padding: 6px 7px;
  }

  .cap {
    color: var(--ink-3);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    font-size: 9.5px;
  }

  .pills {
    display: flex;
    gap: 2px;
  }

  .layers button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
    border: 1px solid transparent;
    padding: 2px 6px;
  }

  .layers button:hover {
    color: var(--ink);
  }

  .layers button.on {
    color: var(--accent);
    border-color: var(--rule-strong);
    background: var(--accent-soft);
  }

  .layers .wide {
    text-align: left;
  }

  .legends {
    position: absolute;
    left: 12px;
    bottom: 12px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
  }

  .legend {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
    color: var(--ink-3);
    background: color-mix(in srgb, var(--paper) 82%, transparent);
    border: 1px solid var(--rule);
    padding: 5px 9px;
  }

  .legend.wrapped {
    flex-wrap: wrap;
    max-width: 34em;
    gap: 4px 11px;
  }

  .key {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .key i {
    width: 11px;
    height: 11px;
    border: 1px solid color-mix(in srgb, var(--ink) 18%, transparent);
  }

  .key i.river {
    width: 16px;
    height: 3px;
    border: 0;
    background: #4e86a0;
  }

  .ramp {
    display: inline-flex;
  }

  .ramp i {
    width: 15px;
    height: 11px;
  }

  .num {
    font-family: var(--f-mono);
    font-size: 10.5px;
    font-variant-numeric: tabular-nums;
  }

  .swatch {
    width: 22px;
    height: 11px;
    border: 1px dashed var(--ink-2);
    background: repeating-linear-gradient(45deg, var(--warn) 0 4px, var(--accent) 4px 8px);
    opacity: 0.55;
  }
</style>
