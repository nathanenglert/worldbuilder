<script lang="ts">
  /**
   * A reference to another record, with the ids to hand.
   *
   * Orphan references are one of the six things the consistency engine reports, and this
   * is where they get created — so the completion list is the cheapest place to prevent
   * them. It still accepts anything typed: a reference to something not yet written is a
   * legitimate way to work, and the check engine is the right place to mention it.
   */
  let {
    value = $bindable(""),
    ids,
    names = {},
    listId,
    takeFocus = false,
    onsettled,
  }: {
    value?: string;
    ids: string[];
    names?: Record<string, string>;
    listId: string;
    /** Set by a caller that put this box on screen for the writer to type in. */
    takeFocus?: boolean;
    onsettled?: () => void;
  } = $props();

  const known = $derived(ids.includes(value.trim()));

  let el = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (takeFocus) el?.focus();
  });
</script>

<div class="ref">
  <input
    type="text"
    bind:this={el}
    bind:value
    list={listId}
    placeholder="an id"
    onblur={onsettled}
    class:unknown={value.trim() !== "" && !known}
  />
  <datalist id={listId}>
    {#each ids as id (id)}
      <option value={id}>{names[id] ?? id}</option>
    {/each}
  </datalist>
</div>

<style>
  input {
    width: 100%;
    padding: 6px 9px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--rule);
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

  /* Nothing points here yet — worth showing, never worth blocking. */
  input.unknown {
    color: var(--warn);
  }
</style>
