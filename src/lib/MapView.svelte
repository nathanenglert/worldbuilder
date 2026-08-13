<script lang="ts">
  import type { Entity, Snapshot } from "./api";

  let {
    snapshot,
    selected = null,
    onselect,
  }: {
    snapshot: Snapshot | null;
    selected: string | null;
    onselect: (id: string | null) => void;
  } = $props();

  // Normalized 0..1 world coordinates scale into this box.
  const W = 1000;
  const H = 700;
  const NEUTRAL = "#7b8b84";

  let scale = $state(1);
  let tx = $state(0);
  let ty = $state(0);

  let wrapEl: HTMLDivElement;
  let svgEl: SVGSVGElement;
  let dragging = false;
  let panned = false;
  let lastX = 0;
  let lastY = 0;

  const regions = $derived((snapshot?.entities ?? []).filter((e) => e.shape.length > 2));
  const markers = $derived((snapshot?.entities ?? []).filter((e) => e.marker !== null));
  const contested = $derived(regions.filter((r) => r.claims.length > 1));

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
    <rect width={W} height={H} fill="url(#grid)" opacity="0.5" />

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

  <div class="hud">
    <button
      onclick={(e) => {
        e.stopPropagation();
        reset();
      }}>reset view</button
    >
    <span>{(scale * 100).toFixed(0)}%</span>
  </div>

  {#if contested.length > 0}
    <div class="legend">
      <span class="swatch"></span>
      hatched + dashed = the handover date is vague, so both claims are live
    </div>
  {/if}
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

  .legend {
    position: absolute;
    left: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11.5px;
    color: var(--ink-3);
    background: color-mix(in srgb, var(--paper) 82%, transparent);
    border: 1px solid var(--rule);
    padding: 5px 9px;
  }

  .swatch {
    width: 22px;
    height: 11px;
    border: 1px dashed var(--ink-2);
    background: repeating-linear-gradient(45deg, var(--warn) 0 4px, var(--accent) 4px 8px);
    opacity: 0.55;
  }
</style>
