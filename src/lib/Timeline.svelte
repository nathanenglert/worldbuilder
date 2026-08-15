<script lang="ts">
  import type { WorldEvent } from "./api";

  let {
    span,
    day,
    events,
    changePoints,
    onday,
  }: {
    span: [number, number];
    day: number;
    events: WorldEvent[];
    changePoints: number[];
    onday: (day: number) => void;
  } = $props();

  let trackEl: HTMLDivElement;
  let dragging = $state(false);
  let hovered = $state<WorldEvent | null>(null);

  const width = $derived(Math.max(1, span[1] - span[0]));
  const pct = (d: number) => ((d - span[0]) / width) * 100;

  /** Change points are dense at this zoom; thin them so ticks stay legible. */
  const ticks = $derived(
    changePoints.filter((p, i, all) => i === 0 || p - all[i - 1] > width / 400),
  );

  function dayAt(clientX: number): number {
    const rect = trackEl.getBoundingClientRect();
    const t = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    return Math.round(span[0] + t * width);
  }

  function pointerdown(e: PointerEvent) {
    // A press on an event marker belongs to the marker. Capturing the pointer here
    // would retarget its click to the track, trading the exact jump to that event
    // for the approximate one that scrubbing gives.
    if ((e.target as Element).closest(".event")) return;
    dragging = true;
    trackEl.setPointerCapture(e.pointerId);
    onday(dayAt(e.clientX));
  }

  function pointermove(e: PointerEvent) {
    if (dragging) onday(dayAt(e.clientX));
  }

  function pointerup(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    trackEl.releasePointerCapture(e.pointerId);
  }

  function keydown(e: KeyboardEvent) {
    const year = Math.round(width / 200);
    const step = e.shiftKey ? year * 10 : e.altKey ? 1 : year;
    if (e.key === "ArrowLeft") {
      onday(Math.max(span[0], day - step));
      e.preventDefault();
    } else if (e.key === "ArrowRight") {
      onday(Math.min(span[1], day + step));
      e.preventDefault();
    } else if (e.key === "Home") {
      onday(span[0]);
      e.preventDefault();
    } else if (e.key === "End") {
      onday(span[1]);
      e.preventDefault();
    }
  }

  /** Step to the next instant where anything actually changes. */
  function jump(direction: 1 | -1) {
    const candidates =
      direction > 0 ? changePoints.filter((p) => p > day) : changePoints.filter((p) => p < day).reverse();
    if (candidates.length) onday(candidates[0]);
  }
</script>

<div class="timeline">
  <div class="controls">
    <button onclick={() => jump(-1)} title="Previous change">‹ change</button>
    <button onclick={() => jump(1)} title="Next change">change ›</button>
  </div>

  <div
    class="track"
    bind:this={trackEl}
    role="slider"
    tabindex="0"
    aria-label="Timeline position"
    aria-valuemin={span[0]}
    aria-valuemax={span[1]}
    aria-valuenow={day}
    onpointerdown={pointerdown}
    onpointermove={pointermove}
    onpointerup={pointerup}
    onpointercancel={pointerup}
    onkeydown={keydown}
  >
    <div class="rail"></div>

    {#each ticks as t (t)}
      <div class="tick" style="left:{pct(t)}%"></div>
    {/each}

    {#each events as e (e.id)}
      {#if e.nominal !== null}
        {@const lo = e.earliest ?? e.nominal}
        {@const hi = e.latest ?? e.nominal}
        <div
          class="event"
          class:fuzzy={hi > lo}
          style="left:{pct(lo)}%; width:{Math.max(0.35, pct(hi) - pct(lo))}%"
          role="button"
          tabindex="-1"
          onmouseenter={() => (hovered = e)}
          onmouseleave={() => (hovered = null)}
          onclick={(ev) => {
            ev.stopPropagation();
            onday(e.nominal!);
          }}
          onkeydown={(ev) => ev.key === "Enter" && onday(e.nominal!)}
        >
          <span class="bar"></span>
          <span class="pip"></span>
        </div>
      {/if}
    {/each}

    <div class="head" class:dragging style="left:{pct(day)}%">
      <span class="stem"></span>
      <span class="grip"></span>
    </div>
  </div>

  {#if hovered}
    <div class="tip" style="left:{pct(hovered.nominal ?? span[0])}%">
      <strong>{hovered.name}</strong>
      <span>{hovered.label}</span>
    </div>
  {/if}
</div>

<style>
  .timeline {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 16px;
    align-items: center;
    padding: 18px 20px 22px;
    border-top: 1px solid var(--rule);
    background: var(--paper);
  }

  .controls {
    display: flex;
    gap: 6px;
  }

  .controls button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
    border: 1px solid var(--rule);
    padding: 4px 8px;
    white-space: nowrap;
  }

  .controls button:hover {
    color: var(--accent);
    border-color: var(--rule-strong);
  }

  .track {
    position: relative;
    height: 42px;
    cursor: pointer;
    touch-action: none;
  }

  .rail {
    position: absolute;
    top: 27px;
    left: 0;
    right: 0;
    height: 1px;
    background: var(--rule-strong);
  }

  .tick {
    position: absolute;
    top: 23px;
    width: 1px;
    height: 5px;
    background: var(--rule-strong);
  }

  .event {
    position: absolute;
    top: 8px;
    height: 26px;
    cursor: pointer;
  }

  .event .bar {
    position: absolute;
    top: 15px;
    left: 0;
    right: 0;
    height: 5px;
    background: var(--accent);
    opacity: 0.32;
  }

  /* A wide bar is the span of days the event could have fallen on. */
  .event.fuzzy .bar {
    background: repeating-linear-gradient(
      90deg,
      var(--accent) 0 3px,
      transparent 3px 6px
    );
    opacity: 0.5;
  }

  .event .pip {
    position: absolute;
    top: 12.5px;
    left: 0;
    width: 9px;
    height: 9px;
    margin-left: -4px;
    background: var(--accent);
    transform: rotate(45deg);
  }

  .event:hover .pip {
    background: var(--ink);
  }

  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 0;
    pointer-events: none;
  }

  .head .stem {
    position: absolute;
    top: 2px;
    bottom: 4px;
    left: -1px;
    width: 2px;
    background: var(--ink);
  }

  .head .grip {
    position: absolute;
    top: -3px;
    left: -6px;
    width: 12px;
    height: 12px;
    background: var(--ink);
  }

  .head.dragging .stem,
  .head.dragging .grip {
    background: var(--accent);
  }

  .tip {
    position: absolute;
    bottom: 2px;
    transform: translateX(-50%);
    display: flex;
    gap: 8px;
    align-items: baseline;
    white-space: nowrap;
    background: var(--surface-2);
    border: 1px solid var(--rule-strong);
    padding: 3px 8px;
    font-size: 11.5px;
    pointer-events: none;
  }

  .tip span {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }
</style>
