<script lang="ts">
  import type { Finding } from "./api";

  let {
    findings,
    names,
    onjump,
    onselect,
    onclose,
  }: {
    findings: Finding[];
    /** Ids to names. An id absent from it is one no record answers to. */
    names: Record<string, string>;
    /** The whole finding: its moment and the record it leans on. */
    onjump: (finding: Finding) => void;
    /** One named record, on its own. */
    onselect: (id: string) => void;
    onclose: () => void;
  } = $props();

  const definite = $derived(findings.filter((f) => f.certainty === "definite"));
  const possible = $derived(findings.filter((f) => f.certainty === "possible"));

  const fileOf = (path: string) => path.split("/").pop() ?? path;

  /**
   * Every record a finding is about, subject first and each named once.
   *
   * A finding has two ends and only one of them was reachable: the row went to
   * `related[0]`, so the existence violation between the siege and Aldric could take a
   * writer to Aldric and never to the siege — which is the record whose dates are more
   * likely to be the thing that moves.
   */
  const about = (f: Finding) => [...new Set([f.subject, ...f.related])];
</script>

<aside>
  <button class="back" onclick={onclose}>‹ back to the world</button>

  <header>
    <p class="kind">Consistency</p>
    <h2>
      {#if findings.length === 0}
        Nothing to report
      {:else}
        {definite.length} definite · {possible.length} open
      {/if}
    </h2>
    <p class="id">interval arithmetic, no model involved</p>
  </header>

  <!-- The row reads the finding; the chips under it are the records it is about. Two
       different moves, and they were one button: "take me to when this happened" and
       "take me to this record" are not the same trip, and only the first one existed. -->
  {#snippet row(f: Finding)}
    <li>
      <div class="finding" class:definite={f.certainty === "definite"}>
        <button class="body" onclick={() => onjump(f)}>
          <span class="rule">{f.title}</span>
          <span class="msg">{f.message}</span>
          {#if f.sources.length}
            <span class="src">{f.sources.map(fileOf).join(" · ")}</span>
          {/if}
        </button>
        <div class="who">
          {#each about(f) as id (id)}
            <!-- Struck through when nothing answers to the id, which is the whole of what
                 a `reference-to-nothing` finding is reporting: the panel should not have
                 to say in prose what the chip can show. -->
            <button class="chip" class:gone={!names[id]} title={id} onclick={() => onselect(id)}>
              {names[id] ?? id}
            </button>
          {/each}
        </div>
      </div>
    </li>
  {/snippet}

  {#if definite.length}
    <p class="label definite">Wrong under every reading</p>
    <ul>
      {#each definite as f, i (i)}
        {@render row(f)}
      {/each}
    </ul>
  {/if}

  {#if possible.length}
    <p class="label">Open questions</p>
    <p class="explain">
      The world's own vagueness leaves room for these. A deliberate mystery looks exactly
      like this, so nothing here is presented as an error.
    </p>
    <ul>
      {#each possible as f, i (i)}
        {@render row(f)}
      {/each}
    </ul>
  {/if}

  {#if findings.length === 0}
    <p class="explain">
      Every event falls inside its participants' lifetimes, every reference resolves, no
      attribute is asserted two ways at once, and no lineage runs backwards.
    </p>
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
    padding-bottom: 4px;
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

  .back {
    align-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .back:hover {
    color: var(--accent);
  }

  .label {
    margin: 8px 0 0;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .label.definite {
    color: var(--warn);
  }

  .explain {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-3);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 6px;
  }

  .finding {
    display: grid;
    gap: 6px;
    padding: 9px 11px;
    background: var(--surface);
    border-left: 2px solid var(--rule-strong);
  }

  .finding:hover {
    background: var(--surface-2);
    border-left-color: var(--accent);
  }

  .finding.definite {
    border-left-color: var(--warn);
  }

  .body {
    width: 100%;
    text-align: left;
    display: grid;
    gap: 3px;
  }

  .who {
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

  .chip.gone {
    color: var(--warn);
    text-decoration: line-through;
    text-decoration-color: var(--rule-strong);
  }

  .rule {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .msg {
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--ink);
  }

  .src {
    font-family: var(--f-mono);
    font-size: 10px;
    color: var(--rule-strong);
    word-break: break-all;
  }
</style>
