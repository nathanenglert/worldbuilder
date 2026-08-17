<script lang="ts">
  import {
    api,
    type Branch,
    type Compare,
    type History,
    type Version,
    type WorldSummary,
  } from "./api";

  let {
    onchanged,
    onstatus,
    onselect,
    onclose,
  }: {
    /** A branch switch rewrites the world's files, so the app has to refetch everything. */
    onchanged: (summary: WorldSummary) => void;
    /**
     * Handed up so the header chip and this panel are the same fetch rather than two.
     * A world edited in the writer's own editor changes what git says without the app
     * doing anything, and a chip with its own idea of the answer would go quietly stale.
     */
    onstatus: (version: Version) => void;
    onselect: (id: string) => void;
    onclose: () => void;
  } = $props();

  let version = $state<Version | null>(null);
  let history = $state<History | null>(null);
  let branches = $state<Branch[]>([]);
  let comparison = $state<Compare | null>(null);
  let busy = $state(false);
  let failure = $state<string | null>(null);
  let said = $state<string | null>(null);

  let message = $state("");
  let newBranch = $state("");
  /** Set to a branch name by the first click; the second click is the one that deletes. */
  let confirmDelete = $state<string | null>(null);
  let confirmDiscard = $state(false);

  const readOnly = $derived(version?.standing.kind !== "root");
  const openRow = $state<{ id: string | null }>({ id: null });

  async function load() {
    failure = null;
    try {
      version = await api.versionStatus();
      onstatus(version);
      if (version.standing.kind === "none") return;
      history = await api.versionHistory(25);
      branches = await api.versionBranches();
    } catch (e) {
      failure = String(e);
    }
  }

  /** Every action here can fail on purpose, and the refusal is the useful part. */
  async function act(what: () => Promise<string | null>) {
    busy = true;
    failure = null;
    said = null;
    try {
      said = await what();
      await load();
    } catch (e) {
      failure = String(e);
    } finally {
      busy = false;
    }
  }

  async function compare(rev: string) {
    comparison = null;
    failure = null;
    busy = true;
    try {
      comparison = await api.versionCompare(rev);
    } catch (e) {
      failure = String(e);
    } finally {
      busy = false;
    }
  }

  const when = (seconds: number) => {
    const ago = Math.max(0, Date.now() / 1000 - seconds);
    if (ago < 90) return "just now";
    if (ago < 5400) return `${Math.round(ago / 60)}m ago`;
    if (ago < 172800) return `${Math.round(ago / 3600)}h ago`;
    return `${Math.round(ago / 86400)}d ago`;
  };

  const moved = (days: number) =>
    days === 0 ? "moves" : days > 0 ? `${days} days later` : `${-days} days earlier`;

  void load();
</script>

<aside>
  <button class="back" onclick={onclose}>‹ back to the world</button>

  {#if comparison}
    <button class="back" onclick={() => (comparison = null)}> ‹ all versions </button>

    <header>
      <p class="kind">compared against</p>
      <h2>{comparison.rev}</h2>
      <p class="id">{comparison.label}</p>
    </header>

    {#if comparison.added.length === 0 && comparison.removed.length === 0 && comparison.changed.length === 0}
      <p class="explain">
        Nothing differs. Not one record, not one date — this revision and the world as it
        stands now are the same world.
      </p>
    {/if}

    {#each [["added", comparison.added], ["removed", comparison.removed], ["changed", comparison.changed]] as const as [label, rows] (label)}
      {#if rows.length}
        <p class="label">{label} · {rows.length}</p>
        <ul>
          {#each rows as r (r.id)}
            <li>
              <button class="row" onclick={() => onselect(r.id)}>
                <span class="title">{r.name}</span>
                <span class="meta">
                  <em class="quiet">{r.kind}</em>
                  {#each r.fields as f (f)}<em>{f}</em>{/each}
                  <!-- A record whose own file did not change and whose date moved anyway:
                       an anchor upstream of it did. This is the line a `git diff` cannot
                       produce, and the reason the comparison is worth having. -->
                  {#each r.moved as m (m.what)}<em class="era">{m.what} {moved(m.days)}</em>{/each}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/each}

    {#if comparison.resolved.length || comparison.introduced.length}
      <p class="label">What moving here would do</p>
      {#each comparison.resolved as f, i (i)}
        <div class="effect good"><span class="tag">settles</span><span>{f.message}</span></div>
      {/each}
      {#each comparison.introduced as f, i (i)}
        <div class="effect" class:bad={f.certainty === "definite"}>
          <span class="tag">{f.certainty === "definite" ? "breaks" : "opens"}</span>
          <span>{f.message}</span>
        </div>
      {/each}
    {/if}

    {#if comparison.files.length}
      <p class="label">Files</p>
      {#each comparison.files as file, i (i)}
        <div class="file">
          <p class="path">{file.path}</p>
          <pre>{#each file.diff as line, j (j)}<span
                class={line.tag === "+" ? "add" : "del"}>{line.tag} {line.text}</span
              >{"\n"}{/each}</pre>
        </div>
      {/each}
      {#if comparison.more_files > 0}
        <p class="explain">
          {comparison.more_files} more file{comparison.more_files === 1 ? "" : "s"} differ. The
          list above is complete; only the line-by-line reading is cut.
        </p>
      {/if}
    {/if}
  {:else if !version}
    <p class="explain">Looking for version control…</p>
  {:else if version.standing.kind === "none"}
    <header>
      <p class="kind">versions</p>
      <h2>Not under version control</h2>
    </header>
    <p class="explain">
      Nothing is tracking <code>{version.standing.world}</code>. Your world is plain files,
      so <code>git init</code> in that folder is all it takes — and then save points,
      what-ifs and comparing two versions of the world all become available here.
    </p>
  {:else}
    <header>
      <p class="kind">versions</p>
      <h2>
        {#if version.branch}on {version.branch}{:else}not on a branch{/if}
      </h2>
      <p class="id">
        {#if version.canon && version.canon !== version.branch}canon is {version.canon} ·{/if}
        {version.dirty.length === 0 ? "everything saved" : `${version.dirty.length} unsaved`}
      </p>
    </header>

    {#if version.standing.note}
      <!-- Not a greyed-out button with no explanation. The one thing a writer needs to
           know is which repository the buttons would have moved. -->
      <p class="caution">{version.standing.note}</p>
    {/if}

    {#if version.dirty.length}
      <p class="label">Unsaved</p>
      <ul class="paths">
        {#each version.dirty as c (c.path)}
          <li><em class={c.state}>{c.state}</em> {c.path}</li>
        {/each}
      </ul>

      {#if !readOnly}
        <div class="compose">
          <input
            bind:value={message}
            placeholder="what changed, in your words"
            spellcheck="false"
            aria-label="Save point message"
          />
          <button
            disabled={busy}
            onclick={() =>
              act(async () => {
                const c = await api.versionCommit(message);
                message = "";
                return `saved as ${c.id}`;
              })}
          >
            save point
          </button>
        </div>
        <button
          class="danger"
          disabled={busy}
          onclick={() => {
            if (!confirmDiscard) {
              confirmDiscard = true;
              return;
            }
            confirmDiscard = false;
            void act(async () => {
              const [count, summary] = await api.versionDiscard();
              onchanged(summary);
              return `threw away ${count} change${count === 1 ? "" : "s"}`;
            });
          }}
        >
          {confirmDiscard
            ? `really throw away ${version.dirty.length} change${version.dirty.length === 1 ? "" : "s"}?`
            : "throw the changes away"}
        </button>
      {/if}
    {/if}

    {#if said}<p class="explain good">{said}</p>{/if}
    {#if failure}<p class="failure">{failure}</p>{/if}

    <p class="label">History</p>
    {#if version.unborn}
      <p class="explain">
        No save points yet. The first one is the thing every what-if branches from.
      </p>
    {/if}
    <ul>
      {#each history?.commits ?? [] as c (c.full)}
        <li>
          <button class="row" onclick={() => compare(c.full)}>
            <span class="title">{c.summary}</span>
            <span class="meta">
              <em class="quiet">{c.id}</em>
              <em class="quiet">{when(c.when)}</em>
              <em>compare</em>
            </span>
          </button>
        </li>
      {/each}
    </ul>
    {#if history?.truncated}
      <p class="explain">
        Stopped after {history.scanned} commits. There may be older ones that touched this
        world.
      </p>
    {/if}

    <p class="label">What-ifs</p>
    <ul>
      {#each branches as b (b.name)}
        <li>
          <div class="branch" class:here={b.is_head}>
            <button class="title" onclick={() => (openRow.id = openRow.id === b.name ? null : b.name)}>
              {b.name}
            </button>
            <span class="meta">
              {#if b.is_head}<em class="good">here</em>{/if}
              {#if b.ahead}<em>{b.ahead} ahead</em>{/if}
              {#if b.behind}<em class="era">{b.behind} behind</em>{/if}
            </span>
          </div>

          {#if openRow.id === b.name}
            <div class="acts">
              <button disabled={busy || b.is_head} onclick={() => compare(b.name)}>compare</button>
              {#if !readOnly}
                <button
                  disabled={busy || b.is_head}
                  onclick={() =>
                    act(async () => {
                      onchanged(await api.versionSwitch(b.name));
                      return `now on ${b.name}`;
                    })}
                >
                  switch to it
                </button>
                <button
                  disabled={busy || b.is_head}
                  onclick={() => act(() => api.versionMerge(b.name))}
                >
                  fast-forward it to here
                </button>
                <button
                  class="danger"
                  disabled={busy || b.is_head}
                  onclick={() => {
                    if (confirmDelete !== b.name) {
                      confirmDelete = b.name;
                      return;
                    }
                    confirmDelete = null;
                    void act(async () => {
                      await api.versionDelete(b.name);
                      return `deleted ${b.name}`;
                    });
                  }}
                >
                  <!-- The number is the whole point of the two-step: deleting a what-if is
                       the normal end of one, and losing four save points by accident is not. -->
                  {confirmDelete === b.name
                    ? b.ahead
                      ? `really? ${b.ahead} save point${b.ahead === 1 ? "" : "s"} become unreachable`
                      : "really delete it?"
                    : "delete"}
                </button>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>

    {#if !readOnly}
      <div class="compose">
        <input
          bind:value={newBranch}
          placeholder="what-if/aldric-lived"
          spellcheck="false"
          aria-label="New branch name"
        />
        <button
          disabled={busy || !newBranch.trim()}
          onclick={() =>
            act(async () => {
              const name = newBranch.trim();
              onchanged(await api.versionBranch(name, true));
              newBranch = "";
              return `started ${name}, and switched to it`;
            })}
        >
          try it
        </button>
      </div>
      <p class="explain">
        A what-if is a real branch of your world folder. Change anything you like on it,
        compare it against canon in records rather than in lines, then keep it or throw it
        away — your files, your history, on your disk.
      </p>
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

  .back {
    align-self: start;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .back:hover {
    color: var(--accent);
  }

  header {
    display: grid;
    gap: 2px;
    padding-bottom: 4px;
  }

  h2 {
    font-size: 19px;
    font-weight: 600;
    line-height: 1.2;
  }

  .kind {
    font-family: var(--f-mono);
    font-size: 10.5px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .id {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--rule-strong);
  }

  .label {
    margin: 8px 0 0;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .explain {
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink-3);
  }

  .explain.good {
    color: var(--accent);
  }

  code {
    font-family: var(--f-mono);
    font-size: 11px;
    color: var(--ink-2);
    word-break: break-all;
  }

  .caution,
  .failure {
    padding: 9px 11px;
    background: var(--surface-2);
    border-left: 2px solid var(--warn);
    color: var(--warn);
    font-size: 12.5px;
    line-height: 1.5;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }

  ul.paths {
    gap: 1px;
    font-family: var(--f-mono);
    font-size: 11px;
    color: var(--ink-2);
    word-break: break-all;
  }

  ul.paths em {
    font-style: normal;
    color: var(--ink-3);
  }

  ul.paths em.new {
    color: var(--accent);
  }

  ul.paths em.deleted {
    color: var(--warn);
  }

  .row,
  .branch {
    width: 100%;
    text-align: left;
    display: grid;
    gap: 3px;
    padding: 8px 11px;
    background: var(--surface);
    border-left: 2px solid var(--rule-strong);
  }

  .row:hover {
    background: var(--surface-2);
    border-left-color: var(--accent);
  }

  .branch.here {
    border-left-color: var(--accent);
  }

  .title {
    font-size: 13px;
    color: var(--ink);
    text-align: left;
  }

  button.title:hover {
    color: var(--accent);
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  em {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    font-style: normal;
    color: var(--ink-3);
  }

  em.quiet {
    color: var(--rule-strong);
  }

  em.good {
    color: var(--accent);
  }

  em.era {
    color: var(--era);
  }

  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 6px 11px 8px;
    background: var(--surface-2);
  }

  .acts button,
  .compose button,
  .danger {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
    border: 1px solid var(--rule);
    padding: 4px 9px;
    white-space: nowrap;
  }

  .acts button:hover:not(:disabled),
  .compose button:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--rule-strong);
  }

  .danger {
    align-self: start;
    color: var(--warn);
    border-color: color-mix(in srgb, var(--warn) 35%, transparent);
  }

  .danger:hover:not(:disabled) {
    border-color: var(--warn);
  }

  button:disabled {
    opacity: 0.4;
  }

  .compose {
    display: flex;
    border: 1px solid var(--rule);
  }

  .compose input {
    flex: 1;
    min-width: 0;
    background: var(--surface);
    border: none;
    color: var(--ink);
    font-family: var(--f-mono);
    font-size: 11.5px;
    padding: 6px 9px;
  }

  .compose input::placeholder {
    color: var(--rule-strong);
  }

  .compose button {
    border: none;
    border-left: 1px solid var(--rule);
  }

  .effect {
    display: grid;
    grid-template-columns: 62px 1fr;
    gap: 8px;
    padding: 7px 10px;
    background: var(--surface);
    border-left: 2px solid var(--rule-strong);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .effect.good {
    border-left-color: var(--accent);
  }

  .effect.bad {
    border-left-color: var(--warn);
    color: var(--warn);
  }

  .tag {
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .file {
    display: grid;
    gap: 4px;
  }

  .path {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  pre {
    margin: 0;
    padding: 8px 10px;
    background: var(--surface);
    border: 1px solid var(--rule);
    overflow-x: auto;
    font-family: var(--f-mono);
    font-size: 11px;
    line-height: 1.45;
  }

  .add {
    color: var(--accent);
  }

  .del {
    color: var(--warn);
  }
</style>
