<script lang="ts">
  /**
   * The wrapper every control sits in.
   *
   * It owns the 2px left border that carries state across this whole app — quiet by
   * default, accent when focused, warn when wrong — so that channel is implemented once
   * rather than in each control, and cannot drift between them.
   */
  import type { Snippet } from "svelte";

  let {
    label,
    hint = "",
    error = null,
    children,
  }: {
    label: string;
    hint?: string;
    error?: string | null;
    children: Snippet;
  } = $props();
</script>

<div class="field" class:bad={!!error}>
  <p class="label">{label}</p>
  {@render children()}
  {#if error}
    <p class="msg bad">{error}</p>
  {:else if hint}
    <p class="msg">{hint}</p>
  {/if}
</div>

<style>
  .field {
    display: grid;
    gap: 4px;
    padding-left: 9px;
    border-left: 2px solid transparent;
  }

  .field:focus-within {
    border-left-color: var(--accent);
  }

  .field.bad {
    border-left-color: var(--warn);
  }

  .label {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 9.5px;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--ink-3);
  }

  .msg {
    margin: 0;
    font-family: var(--f-mono);
    font-size: 10.5px;
    color: var(--ink-3);
  }

  .msg.bad {
    color: var(--warn);
  }
</style>
