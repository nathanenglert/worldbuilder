<script lang="ts">
  /**
   * The wrapper every control sits in.
   *
   * It owns the 2px left border that carries state across this whole app — quiet by
   * default, accent when focused, warn when wrong — so that channel is implemented once
   * rather than in each control, and cannot drift between them.
   */
  import type { Snippet } from "svelte";
  import { offerCaption } from "./caption";

  let {
    label,
    hint = "",
    error = null,
    controlId,
    children,
  }: {
    label: string;
    hint?: string;
    error?: string | null;
    /**
     * For a control that renders its own `Field` and is therefore above it rather than
     * inside it, where context does not reach. `DateField` is the only one.
     */
    controlId?: string;
    children: Snippet;
  } = $props();

  /**
   * A real `for`, which means a real id, which means one nobody has to supply.
   *
   * Generated here, where uniqueness is free, and handed to the control through context —
   * see `caption.ts` for why it does not travel as a prop.
   */
  // Read once, at initialisation, because that is when context can be set — and because
  // an id that changed under a rendered `for` would be a label pointing at nothing.
  const generated = $props.id();
  // svelte-ignore state_referenced_locally
  // Capturing the initial value is the intent, not an oversight: the id is written into a
  // `for` and into the control, and the two must go on agreeing.
  const id = controlId ?? generated;
  offerCaption(id);
</script>

<div class="field" class:bad={!!error}>
  <label for={id}>{label}</label>
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
    gap: var(--s-3);
    padding-left: 9px;
    border-left: 2px solid transparent;
  }

  .field:focus-within {
    border-left-color: var(--accent);
  }

  .field.bad {
    border-left-color: var(--warn);
  }

  /* Deliberately an element selector and not `.label`: the panels' section headings are
     `.label` now, globally, and a form caption is a different thing at a different size. */
  label {
    font-family: var(--f-mono);
    font-size: var(--caps-sm);
    letter-spacing: var(--track-wide);
    text-transform: uppercase;
    color: var(--ink-3);
    /* The caption is a click target now, so it is worth being able to hit. */
    width: fit-content;
    cursor: pointer;
  }

  .msg {
    margin: 0;
    font-family: var(--f-mono);
    font-size: var(--caps);
    color: var(--ink-3);
  }

  .msg.bad {
    color: var(--warn);
  }
</style>
