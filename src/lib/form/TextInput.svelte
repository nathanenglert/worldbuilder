<script lang="ts">
  /**
   * The header's jump box, extracted so every input in the app looks like the one that
   * was already here.
   *
   * `value` is bindable rather than a callback prop. The codebase rule is that *events*
   * are `onsomething` props, never a dispatcher — `bind:` is about values, and the one
   * input that existed before this already used it.
   */
  let {
    value = $bindable(""),
    placeholder = "",
    mono = false,
    readonly = false,
    list,
    warn = false,
    takeFocus = false,
    onblur,
    oninput,
  }: {
    value?: string;
    placeholder?: string;
    mono?: boolean;
    readonly?: boolean;
    /** A `<datalist>` id. The list itself belongs to whoever knows what is in it. */
    list?: string;
    /**
     * Reads as wrong without refusing anything — the orange `RefField` puts on an id
     * nothing answers to. Here because a fact value is *sometimes* an id.
     */
    warn?: boolean;
    /** Set by a caller that put this box on screen for the writer to type in. */
    takeFocus?: boolean;
    onblur?: () => void;
    /** Typing, specifically — not a programmatic write to `value`, and not a tab through. */
    oninput?: () => void;
  } = $props();

  let el = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (takeFocus) el?.focus();
  });
</script>

<input
  type="text"
  bind:this={el}
  bind:value
  {placeholder}
  {readonly}
  {list}
  class:mono
  class:readonly
  class:warn
  {onblur}
  oninput={() => oninput?.()}
/>

<style>
  input {
    width: 100%;
    padding: 6px 9px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--rule);
    font-family: var(--f-body);
    font-size: 13px;
  }

  input.mono {
    font-family: var(--f-mono);
    font-size: 11.5px;
  }

  input.readonly {
    color: var(--ink-3);
    background: var(--paper);
  }

  input.warn {
    color: var(--warn);
  }

  input::placeholder {
    color: var(--rule-strong);
  }

  input:focus {
    outline: none;
    border-color: var(--rule-strong);
  }
</style>
