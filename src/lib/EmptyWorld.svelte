<script lang="ts">
  /**
   * A world before there is anything in it.
   *
   * The map draws its grid and the inspector says "0 things" — both true, and neither any
   * use on the first morning of a world. This says the same thing and then says what to do
   * about it, which on an empty stage costs nothing: there is provably nothing underneath
   * to cover up.
   *
   * It names `world.yaml` on purpose. The two things a writer is most likely to want next
   * — the map they have already drawn, and the book this is all for — are lines in that
   * file rather than buttons anywhere in this app, and nothing else would ever tell them.
   */
  let { path, onnew }: { path: string; onnew: (kind: "entity" | "event") => void } = $props();
</script>

<div class="barren">
  <div class="sheet">
    <p class="kind">nothing in this world yet</p>

    <div class="acts">
      <button onclick={() => onnew("entity")}>+ record</button>
      <span>the people, places and powers in it</span>
      <button onclick={() => onnew("event")}>+ event</button>
      <span>the things that happen, which every date can hang off</span>
    </div>

    <p class="explain">
      Both are files, written into <code>{path}</code> as Markdown you can open in any
      editor. Nothing about this world lives anywhere else.
    </p>
    <p class="explain">
      The rest of what this app can do is declared in <code>world.yaml</code>: a map you
      have drawn, the book the world is for, and your own words for the things in it — a
      duchy, a hive, a covenant. It was written out with all of that commented in place,
      so opening it is the documentation.
    </p>
  </div>
</div>

<style>
  .barren {
    display: grid;
    align-content: center;
    justify-items: center;
    padding: var(--s-8);
    overflow-y: auto;
    background: var(--paper);
  }

  .sheet {
    display: grid;
    gap: var(--s-6);
    max-width: 460px;
  }

  .kind {
    margin: 0;
    font-family: var(--f-mono);
    font-size: var(--caps);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--ink-3);
  }

  /* The control and what it is for, on one line each. A column of buttons with the
     explanations underneath reads as a list of features; beside them it reads as an
     answer to "what do I do now". */
  .acts {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: var(--s-4) var(--s-6);
  }

  .acts button {
    padding: 5px 10px;
    font-family: var(--f-mono);
    font-size: 11px;
    letter-spacing: 0.06em;
    color: var(--accent);
    background: var(--accent-soft);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
  }

  .acts button:hover {
    color: var(--paper);
    background: var(--accent);
  }

  .acts span {
    font-size: 12.5px;
    color: var(--ink-2);
  }

  .explain {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--ink-3);
  }

  code {
    font-family: var(--f-mono);
    font-size: 11px;
    color: var(--ink-2);
  }
</style>
