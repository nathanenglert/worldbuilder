<script lang="ts">
  import type { Entity, Snapshot, Terrain } from "./api";

  let {
    snapshot,
    terrain = null,
    selected = null,
    onselect,
  }: {
    snapshot: Snapshot | null;
    terrain: Terrain | null;
    selected: string | null;
    onselect: (id: string | null) => void;
  } = $props();

  const entity = $derived(snapshot?.entities.find((e) => e.id === selected) ?? null);

  /** The ground under the selected place. Time-invariant, so it is read from the terrain
   *  that was fetched once rather than from the snapshot. */
  const ground = $derived(entity && terrain ? (terrain.places[entity.id] ?? null) : null);

  /** Grouped for the overview list, so a snapshot reads as a world and not a dump. */
  const grouped = $derived.by(() => {
    const groups = new Map<string, Entity[]>();
    for (const e of snapshot?.entities ?? []) {
      const key = e.primitive ?? "other";
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(e);
    }
    return [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  });
</script>

<aside>
  {#if entity}
    <button class="back" onclick={() => onselect(null)}>‹ all present</button>

    <header>
      <p class="kind">{entity.type}{entity.primitive ? ` · ${entity.primitive}` : ""}</p>
      <h2>{entity.name}</h2>
      <p class="id">{entity.id}</p>
    </header>

    {#if entity.existence === "maybe"}
      <p class="caution">
        Existence is uncertain here — the dates bracketing it are vague enough that this
        moment falls inside the doubt.
      </p>
    {/if}

    {#if entity.claims.length > 1}
      <div class="claims">
        <p class="label">Contested</p>
        {#each entity.claims as c (c.owner)}
          <div class="claim">
            <span class="chip" style="background:{c.color ?? 'var(--ink-3)'}"></span>
            <span>{c.name}</span>
            <span class="cert">{c.certainty}</span>
          </div>
        {/each}
      </div>
    {/if}

    {#if ground}
      <p class="label">Ground</p>
      <div class="ground">
        <span class="chip" style="background:{ground.color}"></span>
        <span class="biome">{ground.biome}</span>
        <span class="tags">
          {#if ground.on_river}<em>on a river</em>{/if}
          {#if ground.coastal}<em>coastal</em>{/if}
        </span>
      </div>
      <dl class="measures">
        <div class="fact">
          <dt>elevation</dt>
          <dd>{(ground.elevation * 100).toFixed(0)}% of the range</dd>
        </div>
        <div class="fact">
          <dt>temperature</dt>
          <dd>{ground.temperature.toFixed(1)} °C</dd>
        </div>
        <div class="fact">
          <dt>rainfall</dt>
          <dd>{(ground.precipitation * 100).toFixed(0)}% of the wettest</dd>
        </div>
      </dl>
    {/if}

    {#if entity.facts.length}
      <p class="label">Facts here</p>
      <dl>
        {#each entity.facts as f (f.attr + f.value)}
          <div class="fact" class:maybe={f.certainty === "maybe"}>
            <dt>{f.attr}</dt>
            <dd>
              {f.value}
              {#if f.certainty === "maybe"}<em>possibly</em>{/if}
            </dd>
          </div>
        {/each}
      </dl>
    {:else}
      <p class="empty">No facts recorded at this moment.</p>
    {/if}
  {:else if snapshot}
    <header>
      <p class="kind">Present at this moment</p>
      <h2>{snapshot.entities.length} things</h2>
      <p class="id">{snapshot.label}</p>
    </header>

    {#each grouped as [primitive, list] (primitive)}
      <p class="label">{primitive}</p>
      <ul>
        {#each list as e (e.id)}
          <li>
            <button onclick={() => onselect(e.id)} class:maybe={e.existence === "maybe"}>
              <span>{e.name}</span>
              {#if e.claims.length > 1}<em>contested</em>{/if}
              {#if e.existence === "maybe"}<em>uncertain</em>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/each}
  {:else}
    <p class="empty">No world open.</p>
  {/if}
</aside>

<style>
  aside {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px;
    overflow-y: auto;
    border-left: 1px solid var(--rule);
    background: var(--paper);
  }

  header {
    display: grid;
    gap: 2px;
    padding-bottom: 6px;
  }

  h2 {
    margin: 0;
    font-size: 19px;
    font-weight: 600;
    line-height: 1.2;
  }

  .kind,
  .id {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .id {
    text-transform: none;
    letter-spacing: 0;
    color: var(--rule-strong);
  }

  .label {
    margin: 10px 0 0;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .back {
    align-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .back:hover {
    color: var(--accent);
  }

  .caution {
    margin: 0;
    padding: 9px 11px;
    background: var(--surface-2);
    border-left: 2px solid var(--warn);
    font-size: 12.5px;
    color: var(--ink-2);
  }

  .claims {
    display: grid;
    gap: 5px;
  }

  .claim {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }

  .chip {
    width: 11px;
    height: 11px;
    flex: none;
  }

  .cert {
    margin-left: auto;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--warn);
  }

  dl {
    margin: 0;
    display: grid;
    gap: 1px;
    background: var(--rule);
    border: 1px solid var(--rule);
  }

  .fact {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: 10px;
    padding: 7px 10px;
    background: var(--surface);
  }

  .fact.maybe {
    background: var(--surface-2);
  }

  dt {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
    padding-top: 2px;
  }

  dd {
    margin: 0;
    font-size: 13px;
    word-break: break-word;
  }

  dd em,
  li em {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    font-style: normal;
    color: var(--warn);
    margin-left: 6px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 1px;
  }

  li button {
    width: 100%;
    text-align: left;
    display: flex;
    align-items: baseline;
    padding: 5px 8px;
    font-size: 13px;
    color: var(--ink-2);
    border-left: 2px solid transparent;
  }

  li button:hover {
    background: var(--surface);
    color: var(--ink);
    border-left-color: var(--accent);
  }

  .empty {
    margin: 0;
    font-size: 13px;
    color: var(--ink-3);
  }

  .ground {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .biome {
    font-weight: 600;
  }

  .tags {
    display: flex;
    gap: 6px;
  }

  .tags em {
    font-style: normal;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
    border: 1px solid var(--rule);
    padding: 1px 5px;
  }

  .measures {
    margin-bottom: 18px;
  }
</style>
