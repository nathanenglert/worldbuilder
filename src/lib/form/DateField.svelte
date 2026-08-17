<script lang="ts">
  /**
   * The most important control in the app, because vagueness is the data.
   *
   * `?`, `0812~`, `0810..0815`, `>0800`, `@evt_siege_of_marrow+2y` are all legitimate
   * answers, and precision is the failure mode — every unearned digit is a fact the
   * writer never wrote and will later have to discover is wrong. So an empty box is
   * fine, an unresolvable expression is *not* an error, and nothing here ever fills
   * anything in.
   *
   * Validation goes to the backend on purpose. `resolve_expr` is the world's own parser
   * with the world's own calendar and fuzz; a second parser in TypeScript would drift,
   * and would drift silently.
   */
  import { api } from "../api";
  import Field from "./Field.svelte";

  let {
    value = $bindable(""),
    label,
    hint = "",
    error = null,
    anchors = [],
    onjump,
    onsettled,
  }: {
    value?: string;
    label: string;
    hint?: string;
    error?: string | null;
    /**
     * The events a date can be pinned to, as the expressions they would be typed as.
     *
     * `@evt_siege_of_marrow+2y` is the most useful thing this box can hold — it is how a
     * date *moves* when the siege does — and it only works if the writer can produce the
     * id from memory. Offering them is the difference between a documented grammar and a
     * usable one.
     */
    anchors?: string[];
    onjump?: (day: number) => void;
    onsettled?: () => void;
  } = $props();

  let day = $state<number | null>(null);
  let reading = $state("");
  let complaint = $state<string | null>(null);

  // Same stale-response guard the header's date box uses: a slow answer for keystroke
  // three must never land on top of the answer for keystroke seven.
  let token = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const expr = value.trim();
    clearTimeout(timer);
    if (expr === "") {
      day = null;
      reading = "";
      complaint = null;
      return;
    }

    const mine = ++token;
    timer = setTimeout(async () => {
      try {
        const resolved = await api.resolveExpr(expr);
        if (mine !== token) return;
        complaint = null;
        day = resolved;
        reading = resolved === null ? "unplaced" : await api.formatDay(resolved);
        if (mine !== token) return;
      } catch (e) {
        if (mine !== token) return;
        day = null;
        reading = "";
        complaint = String(e).replace(/^Error:\s*/, "");
      }
    }, 250);

    return () => clearTimeout(timer);
  });
</script>

<Field {label} {hint} error={error ?? complaint}>
  <div class="row">
    <!-- The list is anchors only. A datalist filters as you type, so `@` narrows to
         exactly them, and a plain `0812-04` is untouched by having one attached. -->
    <input
      type="text"
      class="date"
      bind:value
      list={anchors.length ? "date-anchors" : undefined}
      placeholder="0812-04 · 812~ · @event+2y"
      onblur={onsettled}
    />
    {#if anchors.length}
      <datalist id="date-anchors">
        {#each anchors as a (a)}
          <option value={a}></option>
        {/each}
      </datalist>
    {/if}
    {#if day !== null && onjump}
      <button type="button" class="jump" title="Go to this date" onclick={() => onjump(day!)}>
        →
      </button>
    {/if}
  </div>
  {#if reading}
    <!-- `unplaced` is a reading, not a complaint: `?` and `>0800` resolve to nothing
         and are both perfectly good answers. -->
    <p class="reading" class:vague={day === null}>{reading}</p>
  {/if}
</Field>

<style>
  .row {
    display: flex;
  }

  /* `TextInput` with a list attached, spelled out rather than given a `list` prop: this
     is the only box in the app whose completions are a grammar rather than a vocabulary,
     and the datalist has to sit beside the input that names it. */
  .date {
    width: 100%;
    padding: 6px 9px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--rule);
    font-family: var(--f-mono);
    font-size: 11.5px;
  }

  .date::placeholder {
    color: var(--rule-strong);
  }

  .date:focus {
    outline: none;
    border-color: var(--rule-strong);
  }

  .jump {
    padding: 0 9px;
    border: 1px solid var(--rule);
    border-left: none;
    font-family: var(--f-mono);
    font-size: 12px;
    color: var(--ink-3);
  }

  .jump:hover {
    color: var(--accent);
  }

  .reading {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-2);
  }

  .reading.vague {
    color: var(--ink-3);
  }
</style>
