<script lang="ts">
  /** The map's layer switcher, extracted — a row of mono pills, one of them on. */
  let {
    options,
    value,
    onpick,
  }: {
    options: { value: string; label: string }[];
    value: string;
    onpick: (value: string) => void;
  } = $props();
</script>

<div class="pills">
  {#each options as o (o.value)}
    <button type="button" class:on={value === o.value} onclick={() => onpick(o.value)}>
      {o.label}
    </button>
  {/each}
</div>

<style>
  .pills {
    display: flex;
    gap: 3px;
    flex-wrap: wrap;
  }

  button {
    padding: 3px 7px;
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
