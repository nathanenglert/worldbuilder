<script lang="ts">
  /**
   * A free-text field that suggests what the world already uses.
   *
   * Not a `<select>`, for two reasons. The type vocabulary is genuinely open — an
   * undeclared type still loads, it is only *reported* — so a dropdown would make the
   * app's own UI stricter than its data model, and would need an escape hatch bolted on
   * to undo that. And the app has no dropdowns anywhere; it has one kind of text box,
   * and this is it with a list attached.
   */
  let {
    value = $bindable(""),
    options,
    listId,
    placeholder = "",
    mono = false,
    onsettled,
  }: {
    value?: string;
    options: string[];
    listId: string;
    placeholder?: string;
    /** For the machine-facing ones — an attribute key, not a name. Matches `TextInput`. */
    mono?: boolean;
    onsettled?: () => void;
  } = $props();
</script>

<input type="text" bind:value list={listId} {placeholder} class:mono onblur={onsettled} />
<datalist id={listId}>
  {#each options as o (o)}
    <option value={o}></option>
  {/each}
</datalist>

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

  input::placeholder {
    color: var(--rule-strong);
  }

  input:focus {
    outline: none;
    border-color: var(--rule-strong);
  }
</style>
