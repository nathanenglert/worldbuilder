<script lang="ts">
  import type { Story, StoryScene, Surfacing } from "./api";

  let {
    kept,
    story,
    scenes,
    names,
    onselect,
    onscene,
    onclose,
  }: {
    /**
     * Which record is expanded — held by the app, because the two things this panel's
     * detail offers both destroy the panel. Following a scene chip or opening the record
     * is the *point* of expanding a row, and coming back closed it again.
     */
    kept: { open: string | null };
    story: Story | null;
    scenes: StoryScene[];
    /** Ids to names, for the scenes a record was found in. */
    names: Record<string, string>;
    onselect: (id: string) => void;
    onscene: (id: string) => void;
    onclose: () => void;
  } = $props();

  /**
   * The four quadrants, in the order `iceberg-check` says to read them.
   *
   * Underbuilt first and never anywhere else: it is the only one that names work worth
   * doing. Everything below it is description, and the last two are explicitly *not*
   * problems — an unused culture is the iceberg doing its job.
   */
  const groups: { key: Surfacing["standing"]; label: string; note: string }[] = [
    {
      key: "underbuilt",
      label: "the story leans here",
      note: "named often, with little behind it — where the next hour goes",
    },
    { key: "load-bearing", label: "load-bearing", note: "the spine; know before you change it" },
    { key: "overbuilt", label: "below the waterline", note: "built, unseen, doing its job" },
    { key: "quiet", label: "quiet", note: "stubs, and stubs are not debt" },
  ];

  const of = (key: Surfacing["standing"]) =>
    (story?.records ?? []).filter((r) => r.standing === key);

  const open = $derived(kept.open);
</script>

<!-- Focusable so the keyboard has somewhere to land when a panel closes and the
     button that closed it goes with it. App.svelte does the placing. -->
<aside class="panel" tabindex="-1">
  <button class="back" onclick={onclose}>‹ back to the world</button>

  {#if !story}
    <p class="empty">Reading the book…</p>
  {:else if story.standing === "unlinked"}
    <header>
      <p class="kind">the iceberg</p>
      <h2>No manuscript linked</h2>
    </header>
    <!-- Not an error and not a nag. Most worlds are like this, and the honest thing is to
         say what would make the question answerable rather than to report 0%. -->
    <p class="explain">
      Every record is below the waterline, because there is nothing to be above it. Point
      <code>manuscript.root</code> in <code>world.yaml</code> at the folder your chapters live
      in, then give a scene a <code>prose:</code> link, and this becomes a measurement.
    </p>
    <p class="explain">
      The book is never copied here and never edited here — the link is one-way.
    </p>
  {:else if story.standing === "root_missing"}
    <header>
      <p class="kind">the iceberg</p>
      <h2>The manuscript moved</h2>
    </header>
    <p class="caution bad">
      <code>{story.root}</code> is not there. Nothing was read, so nothing below would mean
      anything — this is not a report that your world is 0% on the page.
    </p>
  {:else}
    <header>
      <p class="kind">the iceberg</p>
      <h2>{story.percent}% of this world reaches the page</h2>
      <p class="id">
        {story.surfaced} of {story.total} records · {story.scenes_read} scene{story.scenes_read ===
        1
          ? ""
          : "s"} read
      </p>
    </header>

    <p class="label">the book</p>
    <ul class="scenes">
      {#each scenes as s (s.id)}
        <button class="row" onclick={() => onscene(s.id)}>
          <span class="ord">{s.order + 1}</span>
          <span class="name">{s.name}</span>
          <span class="when">{s.label}</span>
          {#if s.unreadable}
            <em class="bad">no prose</em>
          {:else if s.words !== null}
            <em class="quiet">{s.words}w</em>
          {/if}
        </button>
      {/each}
    </ul>

    {#each groups as g (g.key)}
      {@const rows = of(g.key)}
      {#if rows.length}
        <p class="label" class:lead={g.key === "underbuilt"}>{g.label}</p>
        <p class="explain">{g.note}</p>
        <ul>
          {#each rows as r (r.id)}
            <li>
              <button
                class="row record"
                class:lit={r.mentions > 0}
                onclick={() => (kept.open = open === r.id ? null : r.id)}
              >
                <span class="name">{r.name}</span>
                <span class="counts">
                  {#if r.mentions > 0}
                    <em class="good">{r.mentions} on the page</em>
                  {/if}
                  <em class="quiet">{r.facts} fact{r.facts === 1 ? "" : "s"}</em>
                </span>
              </button>

              {#if open === r.id}
                <!-- The count, with the sentence behind it. A number nobody can check is a
                     number nobody should act on, and a wrong iceberg ratio is worse than
                     none — a writer will spend a week on what it points at. -->
                <div class="detail">
                  {#if r.first_seen}
                    <p class="quote">“{r.first_seen}”</p>
                  {:else}
                    <p class="explain">
                      The prose never names this. If your book calls it something shorter, add
                      that spelling to its <code>aka</code> and it will count.
                    </p>
                  {/if}
                  <!-- Which chapters the count came from. The report has carried this
                       since the iceberg was written and nothing has ever shown it, so
                       the number could be read and not followed. -->
                  {#if r.scenes.length}
                    <div class="where">
                      {#each r.scenes as id (id)}
                        <button class="chip" title={id} onclick={() => onscene(id)}>
                          {names[id] ?? id}
                        </button>
                      {/each}
                    </div>
                  {/if}
                  <dl>
                    <div><dt>referenced by</dt><dd>{r.referenced_by}</dd></div>
                    <div><dt>in events</dt><dd>{r.appears_in}</dd></div>
                    <div><dt>in scene casts</dt><dd>{r.cast_in}</dd></div>
                    <div><dt>prose</dt><dd>{r.prose_bytes} bytes</dd></div>
                  </dl>
                  <button class="go" onclick={() => onselect(r.id)}>open the record ›</button>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/each}

    {#if of("underbuilt").length === 0}
      <p class="explain good">
        Nothing is underbuilt — the story is not reaching for anything that is not there.
      </p>
    {/if}

    {#if story.unreadable.length}
      <p class="label definite">links that went nowhere</p>
      {#each story.unreadable as u (u.scene)}
        <p class="caution bad">{u.reason}</p>
      {/each}
    {/if}
  {/if}
</aside>

<style>
  .label.lead {
    color: var(--warn);
  }

  .label.definite {
    color: var(--warn);
  }

  .empty {
    font-size: 13px;
    color: var(--ink-3);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 1px;
  }

  .row {
    width: 100%;
    text-align: left;
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 8px 11px;
    background: var(--surface);
    border-left: 2px solid var(--rule-strong);
  }

  .row:hover {
    background: var(--surface-2);
    border-left-color: var(--accent);
  }

  .row.lit {
    border-left-color: color-mix(in srgb, var(--accent) 45%, var(--rule-strong));
  }

  .scenes .row {
    border-left-color: var(--era);
  }

  .ord {
    font-family: var(--f-mono);
    font-size: 9.5px;
    color: var(--era);
    min-width: 12px;
  }

  .name {
    flex: 1;
    font-size: 13px;
  }

  .when,
  .counts {
    display: flex;
    gap: 8px;
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--rule-strong);
    white-space: nowrap;
  }

  em {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    font-style: normal;
  }

  em.good {
    color: var(--accent);
  }

  em.bad {
    color: var(--warn);
  }

  em.quiet {
    color: var(--rule-strong);
  }

  .detail {
    display: grid;
    gap: 8px;
    padding: 10px 11px 11px;
    background: var(--surface-2);
    border-left: 2px solid var(--rule);
  }

  .where {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip {
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.05em;
    padding: 2px 7px;
    color: var(--ink-3);
    border: 1px solid var(--rule);
  }

  .chip:hover {
    color: var(--accent);
    border-color: var(--accent);
  }

  .quote {
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--ink-2);
  }

  dl {
    display: grid;
    gap: 1px;
    background: var(--rule);
    border: 1px solid var(--rule);
  }

  dl div {
    display: grid;
    grid-template-columns: 110px 1fr;
    padding: 5px 9px;
    background: var(--surface);
  }

  dt {
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
  }

  dd {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-2);
  }

  .go {
    justify-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .go:hover {
    color: var(--accent);
  }
</style>
