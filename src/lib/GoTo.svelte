<script lang="ts">
  /**
   * Go to any record by what you call it.
   *
   * Until this, the only way to reach a record was to find it on the map — which draws
   * entities with a marker or a shape, alive on the scrubbed day, and nothing else. A
   * polity, an undated thing, anyone dead by the day you happened to be on: unreachable,
   * in a tool whose whole promise is that the world is one addressable thing.
   *
   * It does not move the clock. Picking a record the writer has scrubbed past selects it
   * and lets the panel say so — "not present on this day, it exists 0771 to 0811~", with
   * the day to go to. Teleporting silently would hide exactly the fact worth knowing.
   */
  import type { WorldRecord } from "./api";
  import { rank, type Hit } from "./search";

  let {
    records,
    onpick,
    onclose,
  }: {
    records: WorldRecord[];
    onpick: (id: string) => void;
    onclose: () => void;
  } = $props();

  /** Enough to scroll, few enough that ranking still means something. */
  const SHOWN = 40;

  let query = $state("");
  let cursor = $state(0);
  let box: HTMLInputElement | undefined = $state();
  let list: HTMLUListElement | undefined = $state();

  const hits = $derived(rank(query, records));
  const shown = $derived<Hit[]>(hits.slice(0, SHOWN));

  // Typing moves the answer out from under the cursor. Anywhere but the top would be a
  // guess about which of the new hits the writer meant.
  $effect(() => {
    query;
    cursor = 0;
  });

  $effect(() => {
    box?.focus();
  });

  /** Keep the highlighted row in view when the keyboard is what is moving it. */
  $effect(() => {
    const el = list?.children[cursor] as HTMLElement | undefined;
    el?.scrollIntoView({ block: "nearest" });
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onclose();
    } else if (e.key === "ArrowDown") {
      cursor = Math.min(shown.length - 1, cursor + 1);
    } else if (e.key === "ArrowUp") {
      cursor = Math.max(0, cursor - 1);
    } else if (e.key === "Enter") {
      const hit = shown[cursor];
      if (hit) onpick(hit.record.id);
    } else {
      return;
    }
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<!-- Clicking away closes it, the way the map's background click deselects. The backdrop
     is a plain div with a click handler rather than a dialog, because the app has one
     window and no modals anywhere else in it. -->
<div
  class="scrim"
  role="presentation"
  onclick={onclose}
  onkeydown={keydown}
>
  <div class="palette" role="presentation" onclick={(e) => e.stopPropagation()}>
    <input
      bind:this={box}
      bind:value={query}
      onkeydown={keydown}
      placeholder="a name, an alias, or an id"
      spellcheck="false"
      aria-label="Go to a record"
    />

    {#if shown.length === 0}
      <p class="empty">Nothing in this world is called that.</p>
    {:else}
      <ul bind:this={list}>
        {#each shown as hit, i (hit.record.id)}
          <li>
            <button
              class="hit"
              class:on={i === cursor}
              onmouseenter={() => (cursor = i)}
              onclick={() => onpick(hit.record.id)}
            >
              <span class="name">{hit.record.name}</span>
              <span class="kind">
                {hit.record.type || hit.record.kind}
              </span>
              <span class="id">{hit.record.id}</span>
              <!-- Say why a row is here when the name alone does not explain it. -->
              {#if hit.via}
                <span class="via">also “{hit.via}”</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <!-- Never silently drop what is off the edge, the rule the timeline's scene band
         follows: a shorter list would read as a world with fewer records in it. -->
    <p class="foot">
      {#if hits.length > SHOWN}
        {shown.length} of {hits.length} matches · keep typing
      {:else}
        {hits.length} of {records.length} records
      {/if}
      <span class="keys">↑↓ enter · esc</span>
    </p>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    justify-content: center;
    align-items: start;
    padding-top: 12vh;
    background: color-mix(in srgb, var(--paper) 62%, transparent);
  }

  .palette {
    width: min(560px, 88vw);
    max-height: 66vh;
    display: flex;
    flex-direction: column;
    background: var(--paper);
    border: 1px solid var(--rule-strong);
    box-shadow: 0 18px 50px rgb(0 0 0 / 0.45);
  }

  input {
    width: 100%;
    padding: 12px 14px;
    background: var(--surface);
    border: none;
    border-bottom: 1px solid var(--rule);
    color: var(--ink);
    font-family: var(--f-body);
    font-size: 15px;
  }

  input::placeholder {
    color: var(--rule-strong);
  }

  input:focus {
    outline: none;
  }

  ul {
    margin: 0;
    padding: 4px;
    list-style: none;
    overflow-y: auto;
  }

  .hit {
    width: 100%;
    text-align: left;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 1px 10px;
    padding: 7px 10px;
    border-left: 2px solid transparent;
  }

  .hit.on {
    background: var(--surface);
    border-left-color: var(--accent);
  }

  .name {
    font-size: 13.5px;
    color: var(--ink);
  }

  .kind {
    justify-self: end;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .id,
  .via {
    grid-column: 1 / -1;
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--rule-strong);
  }

  .hit.on .kind {
    color: var(--accent);
  }

  .empty {
    margin: 0;
    padding: 16px 14px;
    font-size: 12.5px;
    color: var(--ink-3);
  }

  .foot {
    margin: 0;
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 12px;
    border-top: 1px solid var(--rule);
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--rule-strong);
  }
</style>
