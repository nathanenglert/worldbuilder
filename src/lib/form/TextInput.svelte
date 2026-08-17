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
    onblur,
    oninput,
  }: {
    value?: string;
    placeholder?: string;
    mono?: boolean;
    readonly?: boolean;
    onblur?: () => void;
    /** Typing, specifically — not a programmatic write to `value`, and not a tab through. */
    oninput?: () => void;
  } = $props();
</script>

<input
  type="text"
  bind:value
  {placeholder}
  {readonly}
  class:mono
  class:readonly
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

  input::placeholder {
    color: var(--rule-strong);
  }

  input:focus {
    outline: none;
    border-color: var(--rule-strong);
  }
</style>
