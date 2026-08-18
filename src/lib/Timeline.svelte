<script lang="ts">
  import type { StoryScene, WorldEvent } from "./api";

  let {
    span,
    day,
    events,
    scenes,
    changePoints,
    onday,
    onpick,
  }: {
    span: [number, number];
    day: number;
    events: WorldEvent[];
    scenes: StoryScene[];
    changePoints: number[];
    onday: (day: number) => void;
    /**
     * Select what was clicked, as well as going to it.
     *
     * The track is the only place in the app that draws events and scenes, and it used to
     * do nothing with them but move the clock — so an event could be created and never
     * reopened, and a scene dot went straight into a form. Selecting first means the
     * writer reads the record before deciding to change it.
     */
    onpick: (id: string) => void;
  } = $props();

  let trackEl: HTMLDivElement;
  let dragging = $state(false);
  let hovered = $state<{ name: string; label: string; at: number } | null>(null);

  /**
   * Whole history, or just the stretch the book covers.
   *
   * §12.4's problem, half-solved: four thousand years and a six-week story do not share
   * an axis usefully. This is a two-position toggle, not the era→century→year→day zoom
   * that question really wants — but it is the half that makes the scene band readable,
   * and it costs nothing because every position here is already a percentage of `span`.
   */
  let windowed = $state(false);

  const dated = $derived(scenes.filter((s) => s.nominal !== null));

  /** The book's own extent, padded so the first and last scenes are not against the ends. */
  const storySpan = $derived.by((): [number, number] | null => {
    if (dated.length === 0) return null;
    const lo = Math.min(...dated.map((s) => s.earliest ?? s.nominal!));
    const hi = Math.max(...dated.map((s) => s.latest ?? s.nominal!));
    const pad = Math.max(1, Math.round((hi - lo) / 10));
    return [lo - pad, hi + pad];
  });

  const view = $derived<[number, number]>(windowed && storySpan ? storySpan : span);
  const width = $derived(Math.max(1, view[1] - view[0]));
  const pct = (d: number) => ((d - view[0]) / width) * 100;

  /** Off-screen at this zoom, so the band can say how many rather than lie by omission. */
  const offscreen = $derived(
    dated.filter((s) => s.nominal! < view[0] || s.nominal! > view[1]).length,
  );

  /** Change points are dense at this zoom; thin them so ticks stay legible. */
  const ticks = $derived(
    changePoints.filter((p, i, all) => i === 0 || p - all[i - 1] > width / 400),
  );

  function dayAt(clientX: number): number {
    const rect = trackEl.getBoundingClientRect();
    const t = Math.min(1, Math.max(0, (clientX - rect.left) / rect.width));
    return Math.round(view[0] + t * width);
  }

  function pointerdown(e: PointerEvent) {
    // A press on an event marker belongs to the marker. Capturing the pointer here
    // would retarget its click to the track, trading the exact jump to that event
    // for the approximate one that scrubbing gives.
    if ((e.target as Element).closest(".event, .scene")) return;
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
      onday(Math.max(view[0], day - step));
      e.preventDefault();
    } else if (e.key === "ArrowRight") {
      onday(Math.min(view[1], day + step));
      e.preventDefault();
    } else if (e.key === "Home") {
      onday(view[0]);
      e.preventDefault();
    } else if (e.key === "End") {
      onday(view[1]);
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
    {#if storySpan}
      <button
        class:on={windowed}
        onclick={() => (windowed = !windowed)}
        title={windowed ? "Show the whole of recorded history" : "Clamp the axis to the book"}
      >
        <!-- Labelled with what pressing it does, not with what is currently showing.
             The terrain control can label state because it is a row and the live one is
             lit; a lone button saying "whole history" reads as an offer, not a status. -->
        {windowed ? "whole history" : "just the story"}
      </button>
    {/if}
  </div>

  <div
    class="track"
    bind:this={trackEl}
    role="slider"
    tabindex="0"
    aria-label="Timeline position"
    aria-valuemin={view[0]}
    aria-valuemax={view[1]}
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
          onmouseenter={() => (hovered = { name: e.name, label: e.label, at: e.nominal! })}
          onmouseleave={() => (hovered = null)}
          onclick={(ev) => {
            ev.stopPropagation();
            onday(e.nominal!);
            onpick(e.id);
          }}
          onkeydown={(ev) => ev.key === "Enter" && (onday(e.nominal!), onpick(e.id))}
        >
          <span class="bar"></span>
          <span class="pip"></span>
        </div>
      {/if}
    {/each}

    {#each dated as s (s.id)}
      <div
        class="scene"
        class:offscreen={s.nominal! < view[0] || s.nominal! > view[1]}
        style="left:{pct(s.nominal!)}%"
        role="button"
        tabindex="-1"
        title="{s.name} · {s.label}"
        onmouseenter={() => (hovered = { name: s.name, label: s.label, at: s.nominal! })}
        onmouseleave={() => (hovered = null)}
        onclick={(ev) => {
          ev.stopPropagation();
          onday(s.nominal!);
          onpick(s.id);
        }}
        onkeydown={(ev) => ev.key === "Enter" && onpick(s.id)}
      >
        <span class="dot"></span>
      </div>
    {/each}

    <div class="head" class:dragging style="left:{pct(day)}%">
      <span class="stem"></span>
      <span class="grip"></span>
    </div>
  </div>

  {#if hovered}
    <div class="tip" style="left:{pct(hovered.at)}%">
      <strong>{hovered.name}</strong>
      <span>{hovered.label}</span>
    </div>
  {/if}

  <!-- Never silently drop what is off the edge: a band that just looked emptier would
       read as a book with fewer scenes in it. -->
  {#if offscreen > 0}
    <div class="elsewhere">{offscreen} scene{offscreen === 1 ? "" : "s"} outside this window</div>
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

  .controls button.on {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
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

  /*
   * The target, which is not the same thing as the diamond.
   *
   * Nine pixels rotated forty-five degrees is a small thing to ask a pointer to find, and
   * it is the only way to reach an event on the track. The padding-and-negative-margin
   * trick the rest of the app uses will not work on the pip itself — it has a painted
   * background, so padding would just draw a bigger diamond — and a pseudo-element on the
   * pip inherits the rotation, so it would grow diagonally and reach sideways into the
   * next event. So the target hangs off `.event`, which is not transformed.
   *
   * Taller, and no wider than the diamond already was. `.event` has zero width on purpose:
   * two events a year apart sit three pixels apart on a track spanning four thousand
   * years, and a wider box would have them stealing each other's clicks.
   */
  .event::after {
    content: "";
    position: absolute;
    top: 2px;
    height: 24px;
    left: -5px;
    width: 10px;
  }

  .event:hover .pip {
    background: var(--ink);
  }

  /* Scenes are round; events are diamonds. A scene is a point in the telling, and it
     carries no doubt bar — the doubt belongs to the date it anchors to, which is already
     drawn as the event's bar directly above it. */
  .scene {
    position: absolute;
    top: 30px;
    height: 12px;
    width: 0;
    cursor: pointer;
  }

  .scene .dot {
    position: absolute;
    left: -3.5px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--paper);
    border: 1.5px solid var(--era);
  }

  .scene:hover .dot {
    background: var(--era);
  }

  .scene.offscreen {
    display: none;
  }

  .elsewhere {
    position: absolute;
    right: 20px;
    bottom: 4px;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.08em;
    color: var(--rule-strong);
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
