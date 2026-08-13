<script lang="ts">
  import type { Finding } from "./api";

  let {
    findings,
    onjump,
    onclose,
  }: {
    findings: Finding[];
    onjump: (finding: Finding) => void;
    onclose: () => void;
  } = $props();

  const definite = $derived(findings.filter((f) => f.certainty === "definite"));
  const possible = $derived(findings.filter((f) => f.certainty === "possible"));

  const fileOf = (path: string) => path.split("/").pop() ?? path;
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

  {#if definite.length}
    <p class="label definite">Wrong under every reading</p>
    <ul>
      {#each definite as f, i (i)}
        <li>
          <button class="finding definite" onclick={() => onjump(f)}>
            <span class="rule">{f.title}</span>
            <span class="msg">{f.message}</span>
            {#if f.sources.length}
              <span class="src">{f.sources.map(fileOf).join(" · ")}</span>
            {/if}
          </button>
        </li>
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
        <li>
          <button class="finding" onclick={() => onjump(f)}>
            <span class="rule">{f.title}</span>
            <span class="msg">{f.message}</span>
            {#if f.sources.length}
              <span class="src">{f.sources.map(fileOf).join(" · ")}</span>
            {/if}
          </button>
        </li>
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
    width: 100%;
    text-align: left;
    display: grid;
    gap: 3px;
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
