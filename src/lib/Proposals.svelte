<script lang="ts">
  import { api, type ProposalDetail, type ProposalSummary } from "./api";

  let {
    proposals,
    ondecided,
    onclose,
  }: {
    proposals: ProposalSummary[];
    ondecided: () => void;
    onclose: () => void;
  } = $props();

  let openId = $state<string | null>(null);
  let detail = $state<ProposalDetail | null>(null);
  let busy = $state(false);
  let failure = $state<string | null>(null);

  const pending = $derived(proposals.filter((p) => p.status === "pending"));
  const decided = $derived(proposals.filter((p) => p.status !== "pending"));

  async function show(id: string) {
    openId = id;
    detail = null;
    failure = null;
    try {
      detail = await api.proposalDetail(id);
    } catch (e) {
      failure = String(e);
      openId = null;
      console.error("proposal_detail failed", e);
    }
  }

  async function decide(accept: boolean) {
    if (!openId) return;
    busy = true;
    failure = null;
    try {
      await api.decideProposal(openId, accept);
      openId = null;
      detail = null;
      ondecided();
    } catch (e) {
      failure = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<!-- Focusable so the keyboard has somewhere to land when a panel closes and the
     button that closed it goes with it. App.svelte does the placing. -->
<aside tabindex="-1">
  {#if openId && detail}
    <button class="back" onclick={() => ((openId = null), (detail = null))}>‹ all proposals</button>

    <header>
      <p class="kind">{detail.author || "unattributed"}</p>
      <h2>{detail.title}</h2>
    </header>

    {#if detail.note}
      <p class="note">{detail.note}</p>
    {/if}

    <p class="label">Changes</p>
    <!-- Keyed by index throughout this panel: diff lines, change summaries and finding
         messages all repeat legitimately, and content keys make Svelte throw. -->
    <ul class="changes">
      {#each detail.changes as change, i (i)}
        <li>{change}</li>
      {/each}
    </ul>

    {#if detail.error}
      <p class="failure">{detail.error}</p>
    {:else}
      <p class="label">If accepted</p>
      {#if detail.resolved.length === 0 && detail.introduced.length === 0}
        <p class="explain">Nothing about the world's consistency changes.</p>
      {/if}

      {#each detail.resolved as f, i (i)}
        <div class="effect good">
          <span class="tag">settles</span>
          <span>{f.message}</span>
        </div>
      {/each}

      {#each detail.introduced as f, i (i)}
        <div class="effect" class:bad={f.certainty === "definite"}>
          <span class="tag">{f.certainty === "definite" ? "breaks" : "opens"}</span>
          <span>{f.message}</span>
        </div>
      {/each}

      {#if detail.files.length}
        <p class="label">Files</p>
        <p class="explain">
          Frontmatter is rewritten in canonical form, so a one-line change can produce a
          wide diff. Prose bodies are untouched.
        </p>
        {#each detail.files as file (file.path)}
          <div class="file">
            <p class="path">{file.path}{file.is_new ? " · new" : ""}</p>
            <pre>{#each file.diff as line, i (i)}<span class={line.tag === "+"
                    ? "add"
                    : "del"}>{line.tag} {line.text}</span>{"\n"}{/each}</pre>
          </div>
        {/each}
      {/if}
    {/if}

    {#if failure}
      <p class="failure">{failure}</p>
    {/if}

    <div class="actions">
      <button class="accept" disabled={busy || !!detail.error} onclick={() => decide(true)}>
        {busy ? "working…" : "Accept"}
      </button>
      <button disabled={busy} onclick={() => decide(false)}>Reject</button>
    </div>
    {#if detail.breaks}
      <p class="explain warn">
        This adds a contradiction that no reading of the dates rescues. Accepting is still
        allowed — it is your world — but the consistency panel will report it.
      </p>
    {/if}
  {:else}
    <button class="back" onclick={onclose}>‹ back to the world</button>

    <header>
      <p class="kind">Review queue</p>
      <h2>{pending.length} pending</h2>
      <p class="id">nothing reaches canon unreviewed</p>
    </header>

    {#if failure}
      <p class="failure">{failure}</p>
    {/if}

    {#if pending.length === 0}
      <p class="explain">
        No changes are waiting. Agents connected over MCP will land their suggestions here
        rather than writing to your world directly.
      </p>
    {/if}

    <ul>
      {#each pending as p (p.id)}
        <li>
          <button class="row" class:bad={p.breaks} onclick={() => show(p.id)}>
            <span class="title">{p.title}</span>
            <span class="meta">
              {#if p.resolves}<em class="good">settles {p.resolves}</em>{/if}
              {#if p.introduces}<em class:bad={p.breaks}>adds {p.introduces}</em>{/if}
              {#if !p.resolves && !p.introduces}<em class="quiet">no effect on checks</em>{/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>

    {#if decided.length}
      <p class="label">Decided</p>
      <ul>
        {#each decided as p (p.id)}
          <li><span class="done">{p.title} <em>{p.status}</em></span></li>
        {/each}
      </ul>
    {/if}
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

  .note {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--ink-2);
    border-left: 2px solid var(--rule-strong);
    padding-left: 10px;
  }

  .explain {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-3);
  }

  .explain.warn {
    color: var(--warn);
  }

  .failure {
    margin: 0;
    padding: 8px 10px;
    background: var(--surface-2);
    border-left: 2px solid var(--warn);
    color: var(--warn);
    font-size: 12px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 5px;
  }

  ul.changes {
    gap: 3px;
  }

  ul.changes li {
    font-family: var(--f-mono);
    font-size: 11px;
    line-height: 1.5;
    color: var(--ink-2);
    word-break: break-word;
  }

  .row {
    width: 100%;
    text-align: left;
    display: grid;
    gap: 3px;
    padding: 9px 11px;
    background: var(--surface);
    border-left: 2px solid var(--rule-strong);
  }

  .row:hover {
    background: var(--surface-2);
    border-left-color: var(--accent);
  }

  .row.bad {
    border-left-color: var(--warn);
  }

  .title {
    font-size: 13px;
    color: var(--ink);
  }

  .meta {
    display: flex;
    gap: 10px;
  }

  em {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    font-style: normal;
    color: var(--ink-3);
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

  .effect {
    display: grid;
    grid-template-columns: 54px 1fr;
    gap: 8px;
    padding: 7px 10px;
    background: var(--surface);
    font-size: 12px;
    line-height: 1.45;
    border-left: 2px solid var(--era);
  }

  .effect.good {
    border-left-color: var(--accent);
  }

  .effect.bad {
    border-left-color: var(--warn);
  }

  .tag {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ink-3);
    padding-top: 2px;
  }

  .file {
    display: grid;
    gap: 4px;
  }

  .path {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  pre {
    margin: 0;
    overflow-x: auto;
    background: var(--surface);
    border: 1px solid var(--rule);
    padding: 8px 10px;
    font-family: var(--f-mono);
    font-size: 10.5px;
    line-height: 1.55;
  }

  pre .add {
    color: var(--accent);
  }

  pre .del {
    color: var(--warn);
  }

  .actions {
    display: flex;
    gap: 8px;
    padding-top: 6px;
  }

  .actions button {
    flex: 1;
    font-family: var(--f-mono);
    font-size: 11px;
    letter-spacing: 0.08em;
    padding: 8px;
    border: 1px solid var(--rule-strong);
    color: var(--ink-2);
  }

  .actions button:hover:not(:disabled) {
    border-color: var(--ink-3);
    color: var(--ink);
  }

  .actions .accept {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    color: var(--accent);
  }

  .actions button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .done {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-size: 12.5px;
    color: var(--ink-3);
  }
</style>
