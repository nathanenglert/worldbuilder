<script lang="ts">
  /**
   * A fact's value, and — visibly — what kind of thing it is.
   *
   * This control exists because of one quiet failure: a population written as text sorts
   * and compares as text forever after, and nothing downstream ever complains. The kind
   * is inferred from what is typed, because that is right nearly always, and it stops
   * inferring the moment the writer picks one by hand.
   *
   * The second quiet failure is that half of these values are *references* — `owner` is
   * `pol_vashen`, `seat` is `place_marrow` — and the box offered no more help with that
   * than with a number, in a form where the box next to it has offered the ids all along.
   */
  import type { ValueKind } from "../api";
  import { danglingRef, inferKind, refPrefixes } from "../draft";
  import PillGroup from "../PillGroup.svelte";
  import TextInput from "./TextInput.svelte";

  let {
    value = $bindable(""),
    kind = $bindable<ValueKind>("text"),
    pinned = $bindable(false),
    ids = [],
    names = {},
    listId,
    onsettled,
  }: {
    value?: string;
    kind?: ValueKind;
    pinned?: boolean;
    /** Every record in the world. Offered, and never required — a value is usually a value. */
    ids?: string[];
    names?: Record<string, string>;
    listId: string;
    onsettled?: () => void;
  } = $props();

  const known = $derived(new Set(ids));
  const prefixes = $derived(refPrefixes(ids));

  /**
   * The value points at a record, and the record is not there.
   *
   * The kind matters: `9000` typed into a box the writer then pins to `int` is a number
   * whatever it looks like, and the engine reads references out of text values only.
   */
  const dangling = $derived(kind === "text" && danglingRef(value, known, prefixes));

  const KINDS = [
    { value: "text", label: "text" },
    { value: "int", label: "int" },
    { value: "float", label: "num" },
    { value: "bool", label: "bool" },
  ];

  $effect(() => {
    const seen = value;
    if (!pinned) kind = inferKind(seen);
  });
</script>

<div class="value">
  {#if kind === "bool"}
    <PillGroup
      options={[
        { value: "true", label: "true" },
        { value: "false", label: "false" },
      ]}
      value={value.trim().toLowerCase() === "true" ? "true" : "false"}
      onpick={(v) => {
        value = v;
        onsettled?.();
      }}
    />
  {:else}
    <!-- The ids are on the list, not on the rails: typing `iron_ore` here is an ordinary
         thing to do and nothing about it is questioned. -->
    <TextInput bind:value list={listId} warn={dangling} onblur={onsettled} />
    <datalist id={listId}>
      {#each ids as id (id)}
        <option value={id}>{names[id] ?? id}</option>
      {/each}
    </datalist>
  {/if}
  <PillGroup
    options={KINDS}
    value={kind}
    onpick={(k) => {
      kind = k as ValueKind;
      pinned = true;
      onsettled?.();
    }}
  />
</div>

<style>
  .value {
    display: grid;
    gap: 4px;
  }
</style>
