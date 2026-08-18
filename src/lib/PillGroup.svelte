<script lang="ts">
  /**
   * One of N, as a row of mono pills.
   *
   * There are four of these — the map's terrain layers, the map/lineage view toggle, the
   * lineage chart's baton picker and the export scope — and they were four hand-rolled
   * rows of buttons with four sets of paddings and four ideas of what "on" looks like.
   * They are one control because they are one *question*, and because the app cannot use
   * a `<select>`: a `<select>` anywhere in the DOM breaks the automation that verifies
   * every screenshot in this project, so this is the substitute and it should be good.
   *
   * Out of `form/` because three of the four are not in a form. It is a control, not a
   * field: nothing here writes a draft.
   */
  let {
    options,
    value,
    onpick,
  }: {
    options: { value: string; label: string; title?: string }[];
    value: string;
    onpick: (value: string) => void;
  } = $props();
</script>

<div class="pills">
  {#each options as o (o.value)}
    <button
      type="button"
      title={o.title}
      aria-pressed={value === o.value}
      class:on={value === o.value}
      onclick={() => onpick(o.value)}
    >
      {o.label}
    </button>
  {/each}
</div>

<style>
  .pills {
    display: flex;
    gap: var(--s-2);
    flex-wrap: wrap;
  }

  button {
    /* Vertical padding past the type, so ten pixels of text is more than ten pixels of
       target. The row's own height is unchanged: the negative margin gives it back. */
    padding: 5px 7px;
    margin: -2px 0;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
    border: 1px solid transparent;
  }

  button:hover {
    color: var(--ink-2);
  }

  button.on {
    color: var(--accent);
    background: var(--accent-soft);
    border-color: var(--rule-strong);
  }
</style>
