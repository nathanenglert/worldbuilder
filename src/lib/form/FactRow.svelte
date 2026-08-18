<script lang="ts">
  /**
   * One time-indexed assertion.
   *
   * The two date boxes are the reason a fact is not just a key and a value: *nothing is
   * ever silently overwritten — new truth closes an interval and opens another*. Marrow's
   * population going from nine thousand to three thousand is not one number replacing
   * another, it is one window ending where the next begins, and the form has to make
   * that the easy thing to express.
   */
  import type { DraftFact } from "../draft";
  import DateField from "./DateField.svelte";
  import Field from "./Field.svelte";
  import SuggestField from "./SuggestField.svelte";
  import ValueField from "./ValueField.svelte";

  let {
    fact = $bindable(),
    attrs = [],
    anchors = [],
    ids = [],
    names = {},
    sought = false,
    onremove,
    onsplit,
    onjump,
    onsettled,
  }: {
    fact: DraftFact;
    /**
     * This is the row the writer's attention is on — the fact they clicked in the
     * inspector, or the row they just added. Takes the caret and scrolls itself into
     * view: a record with a dozen facts is longer than the panel, and arriving at the top
     * of it after clicking one fact is arriving in the wrong place.
     */
    sought?: boolean;
    /**
     * Every record in the world, for the value box — half of these values are references.
     */
    ids?: string[];
    names?: Record<string, string>;
    /**
     * What this world's facts already call things.
     *
     * A world's vocabulary is the thing a fact most needs to agree with — `population`
     * and `populace` are two attributes to the consistency engine and one idea to the
     * writer, and a form that never showed the first is how the second gets typed.
     */
    attrs?: string[];
    /**
     * Event expressions for the two date boxes. A fact's window is the place they matter
     * most: "held until the siege" is a `to` of `@evt_siege_of_marrow`, and it stays
     * true when the siege moves.
     */
    anchors?: string[];
    onremove: () => void;
    onsplit?: () => void;
    onjump?: (day: number) => void;
    onsettled?: () => void;
  } = $props();

  let row = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (sought) row?.scrollIntoView({ block: "center" });
  });
</script>

<div class="fact" bind:this={row} class:sought>
  <div class="head">
    <div class="attr">
      <Field label="attribute">
        <SuggestField
          bind:value={fact.attr}
          options={attrs}
          listId="fact-attrs"
          mono
          takeFocus={sought}
          {onsettled}
        />
      </Field>
    </div>
    <div class="tools">
      {#if onsplit}
        <button
          type="button"
          title="Close this window and open a new one with the same attribute"
          onclick={onsplit}>close &amp; continue</button
        >
      {/if}
      <button type="button" class="drop" title="Remove this fact" onclick={onremove}>remove</button>
    </div>
  </div>

  <Field label="value">
    <ValueField
      bind:value={fact.value}
      bind:kind={fact.kind}
      bind:pinned={fact.pinned}
      {ids}
      {names}
      listId="fact-values"
      {onsettled}
    />
  </Field>

  <div class="window">
    <DateField bind:value={fact.from} label="from" {anchors} {onjump} {onsettled} />
    <DateField bind:value={fact.to} label="to" {anchors} {onjump} {onsettled} />
  </div>
</div>

<style>
  .fact {
    display: grid;
    gap: 8px;
    padding: 10px;
    background: var(--surface);
    border: 1px solid var(--rule);
  }

  /* Says "this is the one you asked for" for as long as the form is open on it. The
     caret alone is too quiet to answer that in a stack of identical rows. */
  .fact.sought {
    border-color: var(--rule-strong);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .head {
    display: flex;
    align-items: end;
    gap: 8px;
  }

  .attr {
    flex: 1;
  }

  .tools {
    display: flex;
    gap: 8px;
    padding-bottom: 6px;
  }

  .tools button {
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
  }

  .tools button:hover {
    color: var(--accent);
  }

  .tools .drop:hover {
    color: var(--warn);
  }

  .window {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
</style>
