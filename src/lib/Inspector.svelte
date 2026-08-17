<script lang="ts">
  /**
   * What is selected, said out loud.
   *
   * The panel used to resolve a bare id against `snapshot.entities` and render the
   * overview list whenever that missed — so "you scrubbed past its lifespan", "that is an
   * event", "that is a scene" and "no such record" all looked like nothing being
   * selected. `Selection` is resolved upstream against the whole world; this renders
   * whichever of those it turns out to be.
   */
  import type { Entity, Snapshot, Terrain } from "./api";
  import { editableKind, type EditableKind, type Selection } from "./selection";

  let {
    snapshot,
    terrain = null,
    selection,
    onselect,
    onedit,
    onday,
  }: {
    snapshot: Snapshot | null;
    terrain: Terrain | null;
    selection: Selection;
    onselect: (id: string | null) => void;
    onedit: (kind: EditableKind, id: string | null) => void;
    /** Scrub, for the records whose answer is "not here — there". */
    onday: (day: number) => void;
  } = $props();

  const entity = $derived(selection.state === "present" ? selection.entity : null);

  /** The edit button's target, resolved once so the template does no narrowing. */
  const editing = $derived.by(() => {
    const kind = editableKind(selection);
    return kind && selection.state !== "none" ? { kind, id: selection.id } : null;
  });

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

<!-- One bar for every selected state, so "go back" and "edit this" are in the same place
     whatever kind of thing was chosen. -->
{#snippet bar()}
  <div class="bar">
    <button class="back" onclick={() => onselect(null)}>‹ all present</button>
    {#if editing}
      {@const e = editing}
      <button class="edit" onclick={() => onedit(e.kind, e.id)}>edit</button>
    {/if}
  </div>
{/snippet}

<aside>
  {#if entity}
    {@render bar()}

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

    <!-- A record whose life the scrubber has left. It stays selected, because the clock
         moving is not the writer changing their mind about what they were reading. -->
  {:else if selection.state === "elsewhere"}
    {@const away = selection}
    {@render bar()}

    <header>
      <p class="kind">{away.type}</p>
      <h2>{away.name}</h2>
      <p class="id">{away.id}</p>
    </header>

    <p class="caution">
      Not present on this day. It exists <strong>{away.window}</strong>.
    </p>

    {#if away.goto}
      {@const g = away.goto}
      <button class="go" onclick={() => onday(g.day)}>go to {g.label} ›</button>
    {/if}

    <p class="empty">
      Editing it does not need the scrubber — the form edits the record, not the moment.
    </p>
  {:else if selection.state === "event"}
    {@const ev = selection.event}
    {@render bar()}

    <header>
      <p class="kind">{ev.kind || "event"}</p>
      <h2>{ev.name}</h2>
      <p class="id">{ev.id}</p>
    </header>

    <dl>
      <div class="fact">
        <dt>when</dt>
        <dd>{ev.label}</dd>
      </div>
      {#if ev.location}
        <div class="fact">
          <dt>where</dt>
          <dd><button class="ref" onclick={() => onselect(ev.location)}>{ev.location}</button></dd>
        </div>
      {/if}
    </dl>

    {#if ev.nominal !== null}
      {@const at = ev.nominal}
      <button class="go" onclick={() => onday(at)}>go to it ›</button>
    {/if}

    {#if ev.participants.length}
      <p class="label">Who was there</p>
      <ul>
        {#each ev.participants as p (p)}
          <li><button onclick={() => onselect(p)}>{p}</button></li>
        {/each}
      </ul>
    {/if}
  {:else if selection.state === "scene"}
    {@const sc = selection.scene}
    {@render bar()}

    <header>
      <p class="kind">scene</p>
      <h2>{sc.name}</h2>
      <p class="id">{sc.id}</p>
    </header>

    <dl>
      <div class="fact">
        <dt>set on</dt>
        <dd>{sc.label}</dd>
      </div>
      {#if sc.pov}
        <div class="fact">
          <dt>through</dt>
          <dd><button class="ref" onclick={() => onselect(sc.pov)}>{sc.pov}</button></dd>
        </div>
      {/if}
      {#if sc.location}
        <div class="fact">
          <dt>where</dt>
          <dd><button class="ref" onclick={() => onselect(sc.location)}>{sc.location}</button></dd>
        </div>
      {/if}
      <div class="fact">
        <dt>prose</dt>
        <dd>{sc.prose ?? "not linked to any chapter yet"}</dd>
      </div>
    </dl>

    {#if sc.unreadable}
      <p class="caution">{sc.unreadable}</p>
    {/if}

    {#if sc.nominal !== null}
      {@const at = sc.nominal}
      <button class="go" onclick={() => onday(at)}>go to it ›</button>
    {/if}

    {#if sc.on_page.length}
      <p class="label">On the page</p>
      <ul>
        {#each sc.on_page as p (p)}
          <li><button onclick={() => onselect(p)}>{p}</button></li>
        {/each}
      </ul>
    {/if}

    <!-- Not an error. Something pointed here, and what it pointed at was never written —
         which is an ordinary state of a world and worth saying rather than swallowing. -->
  {:else if selection.state === "unknown"}
    {@render bar()}

    <header>
      <p class="kind">no such record</p>
      <h2>Nothing is filed under this id</h2>
      <p class="id">{selection.id}</p>
    </header>

    <p class="caution">
      Whatever pointed here names a record the world does not have — either it has been
      removed, or it has not been written yet.
    </p>
  {:else if selection.state === "looking"}
    <p class="empty">Looking for {selection.id}…</p>
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

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .edit {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    padding: 3px 9px;
    border: 1px solid var(--rule);
    color: var(--ink-3);
  }

  .edit:hover {
    color: var(--accent);
    border-color: var(--accent);
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

  /* Takes the writer somewhere, so it reads as a move rather than a label. */
  .go {
    align-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    padding: 4px 10px;
    border: 1px solid var(--rule);
    color: var(--accent);
  }

  .go:hover {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  /* An id inside a fact row. Monospace, because it is an id and not a name. */
  .ref {
    font-family: var(--f-mono);
    font-size: 11.5px;
    color: var(--ink);
    text-align: left;
  }

  .ref:hover {
    color: var(--accent);
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
