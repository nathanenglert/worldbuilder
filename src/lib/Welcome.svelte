<script lang="ts">
  import type { WorldPlan } from "./api";
  import NewWorld from "./NewWorld.svelte";

  /**
   * The first screen, and the only one that has to explain itself.
   *
   * What was here before was nothing: with no world remembered, the app silently opened
   * the duchy it ships with, so the first thing a local-first tool did was show somebody
   * else's fiction and let the writer work out from the header that it was not theirs.
   * The example is still one press away and now says what it is.
   *
   * Three doors, in the order a first run wants them: make one, open one you already
   * have, or look at the example. Making one is first and is the only one given room,
   * because it is the only one a writer arriving here for the first time can take.
   */
  let {
    busy = false,
    oncreate,
    onopen,
    onexample,
  }: {
    busy?: boolean;
    oncreate: (plan: WorldPlan) => void;
    onopen: (path: string) => void;
    onexample: () => void;
  } = $props();

  let path = $state("");
</script>

<div class="welcome">
  <div class="sheet">
    <p class="kind">worldbuilder</p>
    <h1>A world is a folder of files you own.</h1>
    <p class="lede">
      Records in Markdown and YAML, in a folder you choose, on this machine. Every fact is
      dated — including the ones you have not pinned down yet — so the map can be drawn as
      it stood on any day, and a contradiction can be told apart from a decision you have
      not made.
    </p>

    <section>
      <h2>Start a world of your own</h2>
      <NewWorld {busy} {oncreate} />
    </section>

    <section>
      <h2>Open one you already have</h2>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          const p = path.trim();
          if (p) onopen(p);
        }}
      >
        <input
          bind:value={path}
          placeholder="the folder with world.yaml in it"
          spellcheck="false"
          autocomplete="off"
          aria-label="World folder to open"
        />
        <button type="submit" disabled={!path.trim() || busy}>open</button>
      </form>
    </section>

    <p class="example">
      Not sure yet? <button onclick={onexample}>Look at the example world</button> — an
      invented duchy on the wrong side of a mountain wall, its map, four centuries of its
      dates, and the book it is for.
    </p>
  </div>
</div>

<style>
  .welcome {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* Off-centre vertically, because the sheet is tall and a first run should open with
       its top edge where the eye already is rather than pushed down by the leftovers. */
    display: grid;
    align-content: start;
    justify-items: center;
    padding: 8vh var(--s-8) var(--s-8);
    background: var(--paper);
  }

  .sheet {
    width: 100%;
    max-width: 560px;
    display: grid;
    gap: var(--s-6);
  }

  .kind {
    margin: 0;
    font-family: var(--f-mono);
    font-size: var(--caps);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--accent);
  }

  h1 {
    margin: 0;
    font-size: 25px;
    font-weight: 600;
    line-height: 1.25;
    text-wrap: balance;
  }

  .lede {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.6;
    color: var(--ink-2);
  }

  section {
    display: grid;
    gap: var(--s-5);
    padding-top: var(--s-6);
    border-top: 1px solid var(--rule);
  }

  h2 {
    margin: 0;
    font-family: var(--f-mono);
    font-size: var(--caps);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--accent);
  }

  form {
    display: flex;
    gap: var(--s-4);
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 7px 9px;
    font-family: var(--f-mono);
    font-size: 12px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: 2px;
  }

  input:focus {
    outline: none;
    border-color: var(--rule-strong);
  }

  input::placeholder {
    color: var(--rule-strong);
  }

  form button {
    padding: 6px 12px;
    font-family: var(--f-mono);
    font-size: 11px;
    letter-spacing: 0.06em;
    color: var(--ink-2);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
  }

  form button:hover:not(:disabled) {
    color: var(--accent);
    background: var(--accent-soft);
  }

  form button:disabled {
    color: var(--ink-3);
    border-color: var(--rule);
    cursor: default;
  }

  /* Deliberately not a third section. The example is a way of finding out what this is,
     which is a different kind of thing from the two decisions above it. */
  .example {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--ink-3);
  }

  .example button {
    padding: 0;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
</style>
