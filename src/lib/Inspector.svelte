<script lang="ts">
  /**
   * What is selected, said out loud.
   *
   * The panel used to resolve a bare id against `snapshot.entities` and render the
   * overview list whenever that missed — so "you scrubbed past its lifespan", "that is an
   * event", "that is a scene" and "no such record" all looked like nothing being
   * selected. `Selection` is resolved upstream against the whole world; this renders
   * whichever of those it turns out to be.
   *
   * What it says about a record used to be its facts and nothing else, and everything
   * else the app knew about that record lived in a panel that replaced it: the findings
   * that name it, the times the book names it, and what points at it. All three arrive
   * here now, so that reading a record is one place rather than four.
   */
  import type { Entity, Finding, Reference, Snapshot, Surfacing, Terrain } from "./api";
  import { editableKind, type EditableKind, type Selection } from "./selection";

  let {
    snapshot,
    terrain = null,
    selection,
    names,
    findings,
    surfacing,
    references,
    onselect,
    onedit,
    onday,
  }: {
    snapshot: Snapshot | null;
    terrain: Terrain | null;
    selection: Selection;
    /** Ids to names. An id absent from it is one no record answers to. */
    names: Record<string, string>;
    /** Every finding naming this record, whichever end of it this record is. */
    findings: Finding[];
    /** Where it sits in the book, when there is a book and it is an entity. */
    surfacing: Surfacing | null;
    /**
     * What points here — `null` while the question is still out.
     *
     * The distinction is load-bearing: "nothing points at this" is a real answer about a
     * record and must not be shown before it has been given.
     */
    references: Reference[] | null;
    onselect: (id: string | null) => void;
    /** `focus` is the attribute to open the form at, when the click was about one. */
    onedit: (kind: EditableKind, id: string | null, focus?: string) => void;
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

  const broken = $derived(findings.filter((f) => f.certainty === "definite"));
  const open = $derived(findings.filter((f) => f.certainty === "possible"));

  /**
   * One row per record that points here, however many ways it does.
   *
   * `references_to` answers per way, so a scene that both sets a record as its point of
   * view and names it on the page arrives twice. Two rows with the same name reads as a
   * duplicate; one row saying "pov · on the page" reads as the truth.
   *
   * And the same way twice is counted rather than repeated — Marrow hangs two of its
   * facts off the siege, which is "fact anchor ×2" and never "fact anchor · fact anchor".
   */
  const pointers = $derived.by(() => {
    const by = new Map<string, { by: string; name: string; hows: Map<string, number> }>();
    for (const r of references ?? []) {
      const row = by.get(r.by) ?? { by: r.by, name: r.name, hows: new Map<string, number>() };
      row.hows.set(r.how, (row.hows.get(r.how) ?? 0) + 1);
      by.set(r.by, row);
    }
    return [...by.values()].map((row) => ({
      ...row,
      how: [...row.hows].map(([how, n]) => (n > 1 ? `${how} ×${n}` : how)).join(" · "),
    }));
  });

  const plural = (n: number, one: string, many = `${one}s`) => (n === 1 ? one : many);
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

<!-- What the consistency engine has to say about this record, in the two voices the
     checks panel uses — because they are two different claims and always were. A
     definite finding is wrong under every reading of every fuzzy date. A possible one is
     what a deliberate mystery looks like from the outside, and presenting it as an error
     would be the app telling a writer to fix their plot. -->
<!-- One record naming another, inside a row. The name is what the writer knows it by and
     the id is what the file says, so the name shows and the id is one hover away. An id
     no record answers to is marked rather than hidden — a link going nowhere is a fact
     about the world, and following it now lands somewhere that says so. -->
{#snippet ref(id: string)}
  <button class="ref" class:gone={!names[id]} title={id} onclick={() => onselect(id)}>
    {names[id] ?? id}
  </button>
{/snippet}

{#snippet reports()}
  {#each broken as f, i (i)}
    <p class="caution"><strong>{f.title}.</strong> {f.message}</p>
  {/each}

  {#if open.length}
    <p class="label">{open.length === 1 ? "Open question" : "Open questions"}</p>
    {#each open as f, i (i)}
      <p class="explain">{f.message}</p>
    {/each}
  {/if}
{/snippet}

<!-- The two questions a record cannot answer about itself: who names it, and whether the
     book does. Both were in the app already and both were somewhere else. -->
{#snippet links()}
  {#if pointers.length}
    <p class="label">What points here</p>
    <ul>
      {#each pointers as p (p.by)}
        <li>
          <button onclick={() => onselect(p.by)}>
            <span>{p.name}</span>
            <em class="how">{p.how}</em>
          </button>
        </li>
      {/each}
    </ul>
  {:else if references !== null}
    <p class="label">What points here</p>
    <!-- Not a warning. Most of a world is leaves, and a record nothing points at is the
         ordinary case rather than an orphan. -->
    <p class="empty">Nothing else names it.</p>
  {/if}

  {#if surfacing}
    {@const s = surfacing}
    <p class="label">On the page</p>
    {#if s.mentions > 0}
      <p class="explain">
        The prose names it {s.mentions}
        {plural(s.mentions, "time")} across {s.scenes.length}
        {plural(s.scenes.length, "scene")} · <em class="standing">{s.standing}</em>
      </p>
      {#if s.first_seen}
        <!-- The sentence behind the count, for the reason the story panel gives: a
             number nobody can check is a number nobody should act on. -->
        <p class="quote">“{s.first_seen}”</p>
      {/if}
      <ul>
        {#each s.scenes as id (id)}
          <li><button onclick={() => onselect(id)}>{names[id] ?? id}</button></li>
        {/each}
      </ul>
    {:else}
      <p class="empty">
        The book does not name it yet. That is the iceberg working, not a gap to fill.
      </p>
    {/if}
  {/if}
{/snippet}

<!-- Focusable so the keyboard has somewhere to land when a panel closes and the
     button that closed it goes with it. App.svelte does the placing. -->
<aside tabindex="-1">
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

    {@render reports()}

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
      <!-- Rows rather than a `<dl>`, because each one now goes somewhere: the fact a
           writer is reading and the box that would change it are the same fact, and the
           trip between them used to be "open the form, then find the row again". A list
           of buttons is also the honest markup for a list of buttons. -->
      <ul class="facts">
        {#each entity.facts as f (f.attr + f.value)}
          <li>
            <button
              class="fact"
              class:maybe={f.certainty === "maybe"}
              title="Edit {f.attr}"
              onclick={() => onedit("entity", entity.id, f.attr)}
            >
              <span class="attr">{f.attr}</span>
              <span class="val">
                {f.value}
                {#if f.certainty === "maybe"}<em>possibly</em>{/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty">No facts recorded at this moment.</p>
    {/if}

    {@render links()}

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

    {@render reports()}
    {@render links()}
  {:else if selection.state === "event"}
    {@const ev = selection.event}
    {@render bar()}

    <header>
      <p class="kind">{ev.kind || "event"}</p>
      <h2>{ev.name}</h2>
      <p class="id">{ev.id}</p>
    </header>

    {@render reports()}

    <dl>
      <div class="fact">
        <dt>when</dt>
        <dd>{ev.label}</dd>
      </div>
      {#if ev.location}
        {@const at = ev.location}
        <div class="fact">
          <dt>where</dt>
          <dd>{@render ref(at)}</dd>
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
          <li>
            <button class:gone={!names[p]} onclick={() => onselect(p)}>{names[p] ?? p}</button>
          </li>
        {/each}
      </ul>
    {/if}

    {@render links()}
  {:else if selection.state === "scene"}
    {@const sc = selection.scene}
    {@render bar()}

    <header>
      <p class="kind">scene</p>
      <h2>{sc.name}</h2>
      <p class="id">{sc.id}</p>
    </header>

    {@render reports()}

    <dl>
      <div class="fact">
        <dt>set on</dt>
        <dd>{sc.label}</dd>
      </div>
      {#if sc.pov}
        {@const through = sc.pov}
        <div class="fact">
          <dt>through</dt>
          <dd>{@render ref(through)}</dd>
        </div>
      {/if}
      {#if sc.location}
        {@const at = sc.location}
        <div class="fact">
          <dt>where</dt>
          <dd>{@render ref(at)}</dd>
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
      <!-- The cast the *scene* claims, which is not the same list as the prose's — see
           `Surfacing.cast_in`. Named "who it names" so the two never read as one. -->
      <p class="label">Who it names</p>
      <ul>
        {#each sc.on_page as p (p)}
          <li>
            <button class:gone={!names[p]} onclick={() => onselect(p)}>{names[p] ?? p}</button>
          </li>
        {/each}
      </ul>
    {/if}

    {@render links()}

    <!-- Not an error. Something pointed here, and what it pointed at was never written —
         which is an ordinary state of a world and worth saying rather than swallowing. -->
  {:else if selection.state === "unknown"}
    {@render bar()}

    <header>
      <p class="kind">no such record</p>
      <h2>Nothing is filed under this id</h2>
      <p class="id">{selection.id}</p>
    </header>

    <!-- "Whatever pointed here" was as far as this could go before the panel could ask.
         Now it can, so the sentence stops hedging and points at the answer. -->
    <p class="caution">
      No record answers to this id — either it has been removed, or it has not been
      written yet. What still names it is below, and that list is the whole of what a
      rename or a delete left behind.
    </p>

    {@render reports()}
    {@render links()}
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

  /* The fact list, which is buttons rather than a `<dl>` — same grid, so a record's facts
     and the ground under it still line up down the panel. */
  ul.facts {
    gap: 1px;
    background: var(--rule);
    border: 1px solid var(--rule);
  }

  button.fact {
    width: 100%;
    text-align: left;
    align-items: baseline;
    border-left: 2px solid transparent;
  }

  button.fact:hover {
    background: var(--surface-2);
    border-left-color: var(--accent);
  }

  button.fact .attr {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
  }

  button.fact:hover .attr {
    color: var(--accent);
  }

  button.fact .val {
    font-size: 13px;
    color: var(--ink);
    word-break: break-word;
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

  .val em,
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

  /* The findings panel's own quiet voice, for the same sentences. A possible finding is
     what a mystery looks like from outside, and it is not styled as a problem. */
  .explain {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-3);
  }

  .standing {
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    font-style: normal;
    color: var(--accent);
  }

  .quote {
    margin: 0;
    padding-left: 9px;
    border-left: 2px solid var(--rule);
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-2);
  }

  /* What one record calls its relationship to another — `pov`, `fact anchor`. Machine
     vocabulary, in the register the rest of the app gives machine vocabulary. */
  .how {
    margin-left: auto;
    padding-left: 10px;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.09em;
    font-style: normal;
    text-transform: none;
    color: var(--rule-strong);
    white-space: nowrap;
  }

  /* An id no record answers to. Still a button, because following it is how the writer
     finds out what happened to it. */
  .gone {
    color: var(--warn);
    text-decoration: line-through;
    text-decoration-color: var(--rule-strong);
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
