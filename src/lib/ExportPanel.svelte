<script lang="ts">
  /**
   * Publishing: one file, three scopes.
   *
   * The scope buttons are buttons rather than a `<select>` on purpose — a `<select>`
   * anywhere in this app's DOM breaks the automation that verifies it — and the preview
   * runs before every write because the interesting number is *what gets left out*.
   * A writer publishing a spoiler-free companion should see "omits 3" before they hand
   * the file to anybody, not after.
   */
  import { api, type ExportPreview, type ExportScope } from "./api";
  import DateField from "./form/DateField.svelte";
  import PillGroup from "./PillGroup.svelte";

  let {
    onclose,
    onjump,
    anchors,
  }: {
    onclose: () => void;
    onjump: (day: number) => void;
    /** Event expressions for the `as of` box. See `DateField`. */
    anchors: string[];
  } = $props();

  const SCOPES: { key: ExportScope; label: string; note: string }[] = [
    {
      key: "everything",
      label: "everything",
      note: "Every record, every fact, and the window each one held over.",
    },
    {
      key: "as-of",
      label: "as it stood",
      note: "Only what existed then, with the facts that held then and no ends that have not arrived. A gazetteer, in the voice of the year.",
    },
    {
      key: "on-the-page",
      label: "what the book names",
      note: "Only the records your manuscript actually names. Spoiler-free by exposure rather than by date — the same scan the iceberg reports.",
    },
  ];

  let scope = $state<ExportScope>("everything");
  let at = $state("");
  let path = $state("");
  let preview = $state<ExportPreview | null>(null);
  let failure = $state<string | null>(null);
  let wrote = $state<string | null>(null);
  let busy = $state(false);
  /** Set by a first press that hit an existing file; the second press replaces it. */
  let replacing = $state(false);

  const note = $derived(SCOPES.find((s) => s.key === scope)!.note);

  let token = 0;

  $effect(() => {
    // Re-runs whenever either input changes; `as it stood` with no date yet is simply
    // not measurable, and says so rather than erroring.
    const mine = ++token;
    const currentScope = scope;
    const currentAt = at.trim();
    failure = null;
    wrote = null;
    replacing = false;

    if (currentScope === "as-of" && currentAt === "") {
      preview = null;
      return;
    }

    void api
      .previewExport(currentScope, currentScope === "as-of" ? currentAt : null)
      .then((result) => {
        if (mine !== token) return;
        preview = result;
        if (!path.trim()) path = result.suggested;
      })
      .catch((e) => {
        if (mine !== token) return;
        preview = null;
        failure = String(e);
      });
  });

  async function write() {
    if (!preview) return;
    busy = true;
    failure = null;
    try {
      const result = await api.writeExport(
        scope,
        scope === "as-of" ? at.trim() : null,
        path,
        replacing,
      );
      wrote = `${result.path} · ${Math.round(result.bytes / 1024).toLocaleString()} KB`;
      replacing = false;
    } catch (e) {
      failure = String(e);
      // The one refusal worth arming the second press for.
      if (String(e).includes("already there")) replacing = true;
    } finally {
      busy = false;
    }
  }
</script>

<!-- Focusable so the keyboard has somewhere to land when a panel closes and the
     button that closed it goes with it. App.svelte does the placing. -->
<aside class="panel" tabindex="-1">
  <button class="back" onclick={onclose}>‹ back to the world</button>

  <header>
    <p class="kind">publish</p>
    <h2>A world bible</h2>
    <p class="id">one HTML file · no server, no network</p>
  </header>

  <p class="label">How much of it</p>
  <PillGroup
    options={SCOPES.map((s) => ({ value: s.key, label: s.label }))}
    value={scope}
    onpick={(v: string) => (scope = v as ExportScope)}
  />
  <p class="explain">{note}</p>

  {#if scope === "as-of"}
    <DateField
      bind:value={at}
      label="the day it is written"
      hint="0812-04 · @evt_siege_of_marrow"
      {anchors}
      {onjump}
    />
  {/if}

  {#if preview}
    <p class="label">What comes out</p>
    <dl>
      <div><dt>scope</dt><dd>{preview.caption}</dd></div>
      <div><dt>records</dt><dd>{preview.records}</dd></div>
      <!-- The number the writer is actually deciding about. A scope is a choice to leave
           things out, and a panel that only shows what is in reads as a failed export. -->
      <div><dt>left out</dt><dd class:quiet={preview.omitted === 0}>{preview.omitted}</dd></div>
      <div><dt>cross-links</dt><dd>{preview.links}</dd></div>
      <div><dt>size</dt><dd>{Math.round(preview.bytes / 1024).toLocaleString()} KB</dd></div>
    </dl>
  {:else if scope === "as-of"}
    <p class="explain">Give it a day to stand on.</p>
  {/if}

  <p class="label">Where it goes</p>
  <div class="compose">
    <input bind:value={path} spellcheck="false" aria-label="Where to write the file" />
    <button disabled={busy || !preview} onclick={write}>
      {busy ? "writing…" : replacing ? "replace it" : "write it"}
    </button>
  </div>

  {#if wrote}<p class="explain good">Written · {wrote}</p>{/if}
  {#if failure}<p class="caution bad">{failure}</p>{/if}

  <p class="explain">
    The map, the timeline, the type and every record travel inside the file. Open it on a
    machine that has never heard of this application, with no network, in ten years.
  </p>
  <p class="explain">
    Consistency findings and the review queue stay out of it: your open questions are
    working notes, and a mystery shipped to a reader as an erratum stops being a mystery.
  </p>
</aside>

<style>
  /* Only the part the global does not say: this one carries a filesystem path. */
  .explain.good {
    word-break: break-all;
  }






  dl {
    display: grid;
    gap: 1px;
    margin: 0;
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
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-2);
  }

  dd.quiet {
    color: var(--rule-strong);
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

  .compose button {
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
    border: none;
    border-left: 1px solid var(--rule);
    padding: 4px 11px;
    white-space: nowrap;
  }

  .compose button:hover:not(:disabled) {
    color: var(--accent);
  }

  .compose button:disabled {
    opacity: 0.4;
  }
</style>
