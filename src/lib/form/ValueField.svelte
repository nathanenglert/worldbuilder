<script lang="ts">
  /**
   * A fact's value, and — visibly — what kind of thing it is.
   *
   * This control exists because of one quiet failure: a population written as text sorts
   * and compares as text forever after, and nothing downstream ever complains. The kind
   * is inferred from what is typed, because that is right nearly always, and it stops
   * inferring the moment the writer picks one by hand.
   */
  import type { ValueKind } from "../api";
  import { inferKind } from "../draft";
  import PillGroup from "./PillGroup.svelte";
  import TextInput from "./TextInput.svelte";

  let {
    value = $bindable(""),
    kind = $bindable<ValueKind>("text"),
    pinned = $bindable(false),
    onsettled,
  }: {
    value?: string;
    kind?: ValueKind;
    pinned?: boolean;
    onsettled?: () => void;
  } = $props();

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
    <TextInput bind:value onblur={onsettled} />
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
