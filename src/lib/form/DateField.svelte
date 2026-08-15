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
  import TextInput from "./TextInput.svelte";

  let {
    value = $bindable(""),
    label,
    hint = "",
    error = null,
    onjump,
    onsettled,
  }: {
    value?: string;
    label: string;
    hint?: string;
    error?: string | null;
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
    <TextInput bind:value mono placeholder="0812-04 · 812~ · @event+2y" onblur={onsettled} />
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
