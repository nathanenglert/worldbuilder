<script lang="ts">
  import type { Entity, Layer, Snapshot, StoryScene, Terrain } from "./api";
  import {
    clamp01,
    dropVertex,
    insertVertex,
    midpoint,
    moveVertex,
    round4,
    viewToWorld,
    type Point,
  } from "./geometry";

  export type MapMode = "browse" | "marker" | "shape";

  let {
    kept,
    snapshot,
    terrain = null,
    backdrop = null,
    selected = null,
    onselect,
    mode = "browse",
    draft = null,
    onmarker,
    onshape,
    onmodedone,
    scenes = [],
    activeScene = null,
    showStory = false,
    onscene,
  }: {
    /**
     * Where the map is looking, and what it is drawing — held by the app.
     *
     * This view is one arm of an `{#if}`, so a glance at the lineage chart destroyed it
     * and coming back rebuilt it at 100% showing the whole map. Panning and zooming is a
     * minute of work and it is *positional*: the writer is not looking at the map, they
     * are looking at the eastern march, and there was no way to ask for that back.
     */
    kept: { scale: number; tx: number; ty: number; layer: Layer; showBackdrop: boolean };
    snapshot: Snapshot | null;
    terrain: Terrain | null;
    backdrop: string | null;
    selected: string | null;
    onselect: (id: string | null) => void;
    mode?: MapMode;
    /**
     * Geometry being edited right now, drawn on top of everything.
     *
     * Sourced from the app rather than from `snapshot`, which is what lets the writer
     * scrub the timeline mid-edit, and what lets a record that does not exist yet show
     * its marker.
     */
    draft?: { marker: Point | null; shape: Point[] } | null;
    onmarker?: (p: Point) => void;
    onshape?: (points: Point[]) => void;
    onmodedone?: () => void;
    /**
     * Scenes with a resolvable location, in reading order.
     *
     * Their points come from the location's *record* marker, not from the snapshot, so
     * the book's route does not vanish in stretches where its settings had not been
     * founded yet — which is most of a prequel.
     */
    scenes?: StoryScene[];
    activeScene?: string | null;
    showStory?: boolean;
    onscene?: (id: string) => void;
  } = $props();

  // Normalized 0..1 world coordinates scale into this box. The height follows the map
  // image's own aspect when there is one, so the backdrop is never stretched.
  const W = 1000;
  const H = $derived(terrain ? Math.round(W / terrain.aspect) : 700);
  const NEUTRAL = "#7b8b84";

  // Read here, written through `kept`. The drawing divides by `scale` forty-two times —
  // every stroke width, every dash, every label — and not one of them reads better as
  // `kept.scale`, so the five names stay exactly what they were and the six places that
  // *move* the map are the ones that say whose state it is.
  const scale = $derived(kept.scale);
  const tx = $derived(kept.tx);
  const ty = $derived(kept.ty);
  const layer = $derived(kept.layer);
  const showBackdrop = $derived(kept.showBackdrop);

  let wrapEl: HTMLDivElement;
  let svgEl: SVGSVGElement;
  let dragging = false;
  let panned = false;
  let captured = false;
  /** Which vertex a drag has hold of, if any. */
  let grabbed = $state<number | null>(null);
  let lastX = 0;
  let lastY = 0;

  const regions = $derived((snapshot?.entities ?? []).filter((e) => e.shape.length > 2));
  const markers = $derived((snapshot?.entities ?? []).filter((e) => e.marker !== null));

  /** The route, in reading order. Consecutive scenes in one place collapse to one point. */
  const path = $derived(
    scenes
      .map((s) => s.point!)
      .filter((p, i, all) => i === 0 || p[0] !== all[i - 1][0] || p[1] !== all[i - 1][1]),
  );

  /**
   * One circle per *place* the book visits, not per scene.
   *
   * A story returns to the same room, and two scenes at one settlement drew two circles
   * on the same pixel — the second silently hiding the first, so a book that visited
   * Marrow twice looked like it visited once. The stop carries every reading position
   * that happens there instead.
   */
  const stops = $derived.by(() => {
    const out: { point: Point; at: StoryScene[] }[] = [];
    for (const s of scenes) {
      const here = out.find((o) => o.point[0] === s.point![0] && o.point[1] === s.point![1]);
      if (here) here.at.push(s);
      else out.push({ point: s.point!, at: [s] });
    }
    return out;
  });
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
    // Keep whatever sits under (x, y) pinned there. Both worked out before either is
    // written, because `tx` and `ty` now read back through `kept` — an assignment
    // half-way down would leave the second line dividing by the zoom it just changed.
    const nx = x - (x - tx) * (next / scale);
    const ny = y - (y - ty) * (next / scale);
    kept.scale = next;
    kept.tx = nx;
    kept.ty = ny;
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
  }

  function pointermove(e: PointerEvent) {
    if (!dragging) return;
    const from = toViewBox(lastX, lastY);
    const to = toViewBox(e.clientX, e.clientY);
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    if (Math.abs(dx) + Math.abs(dy) > 0.5) {
      panned = true;
      // Capture only once the drag is real. Capturing on pointerdown retargets the
      // eventual click to the wrap, so every button and marker inside the map goes
      // dead to a real mouse — while still responding to a synthetic .click().
      if (!captured) {
        wrapEl.setPointerCapture(e.pointerId);
        captured = true;
      }
    }
    kept.tx += dx;
    kept.ty += dy;
    lastX = e.clientX;
    lastY = e.clientY;
  }

  function pointerup(e: PointerEvent) {
    dragging = false;
    if (captured) {
      wrapEl.releasePointerCapture(e.pointerId);
      captured = false;
    }
  }

  /** The current pan and zoom, for converting a click into a world coordinate. */
  const view = $derived({ tx, ty, scale, W, H });

  /** Where on the map, in normalized coordinates, a click landed. */
  function worldAt(e: MouseEvent): Point {
    return round4(clamp01(viewToWorld(toViewBox(e.clientX, e.clientY), view)));
  }

  /**
   * A click that wasn't the tail of a pan.
   *
   * In browse mode it clears the selection, as it always has. In a placement mode it is
   * the placement — free-riding on machinery that already works, since `click` fires
   * after `pointerup` and `panned` is authoritative by then. Pan and zoom keep working
   * in every mode, which is most of what makes drawing a polygon bearable: you can zoom
   * in to put a vertex exactly where you meant.
   */
  function backgroundClick(e: MouseEvent) {
    if (panned) return;
    if (mode === "marker") {
      onmarker?.(worldAt(e));
      return;
    }
    if (mode === "shape") {
      onshape?.([...(draft?.shape ?? []), worldAt(e)]);
      return;
    }
    onselect(null);
  }

  /**
   * Keys land only on a focused element, and entering a mode from the panel does not
   * focus the map. Without this, escape goes to whatever input was last touched.
   */
  $effect(() => {
    if (mode !== "browse") wrapEl?.focus();
  });

  function keydown(e: KeyboardEvent) {
    const step = 40 / scale;
    const shape = draft?.shape ?? [];
    switch (e.key) {
      case "Escape":
        // Leaving a mode keeps whatever was placed. Discarding is the panel's explicit
        // revert — escape-destroys-work is a bad bargain in an authoring tool.
        if (mode !== "browse") {
          onmodedone?.();
          break;
        }
        onselect(null);
        break;
      case "Backspace":
      case "Delete":
        if (mode !== "shape" || shape.length === 0) return;
        onshape?.(shape.slice(0, -1));
        break;
      case "Enter":
        if (mode !== "shape" || shape.length < 3) return;
        onmodedone?.();
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
        kept.tx += step;
        break;
      case "ArrowRight":
        kept.tx -= step;
        break;
      case "ArrowUp":
        kept.ty += step;
        break;
      case "ArrowDown":
        kept.ty -= step;
        break;
      default:
        return;
    }
    e.preventDefault();
  }

  function reset() {
    kept.scale = 1;
    kept.tx = 0;
    kept.ty = 0;
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
  class:editing={mode !== "browse"}
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

    <g class="entities" class:editing={mode !== "browse"} transform="translate({tx} {ty}) scale({scale})">
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

    <!-- The book's route through the world. Between the entity layer and the draft: over
         the places it visits, under anything being edited. Note `.entities` dims to .55 in
         edit mode and a sibling group does not inherit that — which is wanted here, since
         the path is the thing being looked at while a scene is open. -->
    {#if showStory && path.length > 1}
      <g class="story" transform="translate({tx} {ty}) scale({scale})">
        <polyline
          points={path.map(([x, y]) => `${x * W},${y * H}`).join(" ")}
          fill="none"
          stroke="var(--era)"
          stroke-width={2 / scale}
          stroke-opacity="0.55"
          stroke-dasharray="{6 / scale} {4 / scale}"
        />
      </g>
    {/if}

    {#if showStory}
      <g class="scenes" transform="translate({tx} {ty}) scale({scale})">
        {#each stops as stop (stop.at[0].id)}
          {@const [x, y] = stop.point}
          {@const here = stop.at.some((s) => s.id === activeScene)}
          {@const open = stop.at.find((s) => s.id === activeScene) ?? stop.at[0]}
          {@const label = stop.at.map((s) => s.order + 1).join("·")}
          <g class="scene" class:active={here}>
            <circle
              cx={x * W}
              cy={y * H}
              r={(here ? 11 : 8.5) / scale}
              fill="var(--paper)"
              stroke={here ? "var(--accent)" : "var(--era)"}
              stroke-width={1.6 / scale}
              role="button"
              tabindex="-1"
              aria-label={stop.at.map((s) => s.name).join(", ")}
              onclick={(e) => {
                e.stopPropagation();
                if (!panned) onscene?.(open.id);
              }}
              onkeydown={(e) => e.key === "Enter" && onscene?.(open.id)}
            >
              <title>{stop.at.map((s) => `${s.order + 1}. ${s.name}`).join("\n")}</title>
            </circle>
            <!-- Reading position, not date. On a book with a flashback these run out of
                 chronological order on purpose, and `1·3` means the story comes back. -->
            <text
              x={x * W}
              y={y * H + 3.4 / scale}
              font-size={(label.length > 3 ? 8 : 9.5) / scale}
              text-anchor="middle"
              class="ordinal"
            >{label}</text>
          </g>
        {/each}
      </g>
    {/if}

    <!-- The record being edited, drawn last so it is never dimmed with the rest. -->
    {#if draft}
      <g class="draft" transform="translate({tx} {ty}) scale({scale})">
        {#if draft.shape.length > 1}
          <path
            d={pathOf(draft.shape) + (draft.shape.length > 2 ? "" : "")}
            fill="var(--accent)"
            fill-opacity={draft.shape.length > 2 ? 0.14 : 0}
            stroke="var(--accent)"
            stroke-width={1.6 / scale}
            stroke-dasharray={mode === "shape" ? `${6 / scale} ${4 / scale}` : undefined}
            stroke-linejoin="round"
          />
        {/if}

        {#if mode === "shape"}
          {#each draft.shape as p, i (i)}
            <!-- A transparent hit area, because a nine-pixel target is impossible with a
                 trackpad. `transparent` and not `none`: `none` is not hit-testable. -->
            <rect
              class="grab"
              x={p[0] * W - 9 / scale}
              y={p[1] * H - 9 / scale}
              width={18 / scale}
              height={18 / scale}
              fill="transparent"
              role="button"
              tabindex="-1"
              onpointerdown={(e) => {
                // Mandatory. Without it the wrap starts a pan and dragging a vertex
                // drags the whole map. Capturing here is safe in a way capturing on the
                // wrap was not: this retargets one gesture, not every click on the map.
                e.stopPropagation();
                (e.currentTarget as Element).setPointerCapture(e.pointerId);
                grabbed = i;
              }}
              onpointermove={(e) => {
                if (grabbed !== i) return;
                onshape?.(moveVertex(draft!.shape, i, worldAt(e)));
              }}
              onpointerup={(e) => {
                if (grabbed !== i) return;
                (e.currentTarget as Element).releasePointerCapture(e.pointerId);
                grabbed = null;
                // Deliberately *not* `onmodedone`: finishing one vertex is not finishing
                // the outline, and leaving the mode here would take the handles away
                // after every single adjustment. The panel revalidates on its own.
              }}
              onclick={(e) => e.stopPropagation()}
              onkeydown={(e) => {
                if (e.key !== "Backspace" && e.key !== "Delete") return;
                e.stopPropagation();
                onshape?.(dropVertex(draft!.shape, i));
              }}
            />
            <!-- Squares, not circles: the app has no rounded corners anywhere, and a
                 circle already means a settlement. -->
            <rect
              x={p[0] * W - 4.5 / scale}
              y={p[1] * H - 4.5 / scale}
              width={9 / scale}
              height={9 / scale}
              fill="var(--paper)"
              stroke="var(--accent)"
              stroke-width={1.6 / scale}
              pointer-events="none"
            />
          {/each}

          <!-- Midpoints, so a coarse outline can be refined instead of redrawn. -->
          {#if draft.shape.length > 2}
            {#each draft.shape as _, i (`mid-${i}`)}
              {@const m = midpoint(draft.shape, i)}
              <rect
                class="mid"
                x={m[0] * W - 4 / scale}
                y={m[1] * H - 4 / scale}
                width={8 / scale}
                height={8 / scale}
                fill="var(--surface)"
                stroke="var(--accent)"
                stroke-width={1.2 / scale}
                role="button"
                tabindex="-1"
                onclick={(e) => {
                  e.stopPropagation();
                  onshape?.(insertVertex(draft!.shape, i, m));
                }}
                onkeydown={(e) => e.key === "Enter" && onshape?.(insertVertex(draft!.shape, i, m))}
              />
            {/each}
          {/if}
        {/if}

        {#if draft.marker}
          <circle
            cx={draft.marker[0] * W}
            cy={draft.marker[1] * H}
            r={7 / scale}
            fill="var(--accent-soft)"
            stroke="var(--accent)"
            stroke-width={2 / scale}
            pointer-events="none"
          />
        {/if}
      </g>
    {/if}
  </svg>

  {#if mode !== "browse"}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="banner" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      {#if mode === "marker"}
        placing · click the map · esc to stop
      {:else}
        drawing · {draft?.shape.length ?? 0} points · ⌫ undo · ↵ finish
      {/if}
    </div>
  {/if}

  {#if terrain}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="layers" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <span class="cap">terrain</span>
      <div class="pills">
        {#each LAYERS as l (l.key)}
          <button class:on={layer === l.key} onclick={() => (kept.layer = l.key)}>{l.label}</button>
        {/each}
      </div>
      {#if backdrop}
        <button
          class="wide"
          class:on={showBackdrop}
          onclick={() => (kept.showBackdrop = !showBackdrop)}
        >
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
  .wrap.editing,
  .wrap.editing:active {
    cursor: crosshair;
  }

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

  /* Scene circles sit above the places they mark, so they must not swallow a click meant
     for the region underneath when the story layer is merely being shown. */
  .story {
    pointer-events: none;
  }

  .scenes circle {
    cursor: pointer;
  }

  .scenes .ordinal {
    font-family: var(--f-mono);
    fill: var(--era);
    pointer-events: none;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
  }

  .scenes .scene.active .ordinal {
    fill: var(--accent);
  }

  .entities.editing {
    pointer-events: none;
    opacity: 0.55;
  }

  .grab {
    cursor: move;
  }

  .mid {
    cursor: copy;
    opacity: 0.45;
  }

  .mid:hover {
    opacity: 1;
  }

  .banner {
    position: absolute;
    top: 14px;
    left: 270px;
    padding: 5px 11px;
    background: color-mix(in srgb, var(--paper) 86%, transparent);
    border: 1px solid var(--rule);
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    color: var(--ink-2);
    pointer-events: auto;
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
