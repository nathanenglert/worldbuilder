<script lang="ts">
  /**
   * The id, which is a commitment rather than a field.
   *
   * Ids appear in `@anchor` dates, in `participants`, in `location`, in `parents`, and
   * as fact values. Renaming one means rewriting every reference in the world — a real
   * feature, and not this one. So on an existing record it is shown and locked, with the
   * reason attached; on a new one it follows the name until the writer touches it.
   */
  import Field from "./Field.svelte";
  import TextInput from "./TextInput.svelte";

  let {
    value = $bindable(""),
    locked,
    taken,
    onedit,
  }: {
    value?: string;
    locked: boolean;
    taken: string[];
    /**
     * The writer has typed in here, so the id has stopped following the name.
     *
     * It has to be *typing*. The panel used to listen for focus on a hidden sibling —
     * which never fired, so the suggestion overwrote every keystroke and a custom id was
     * unreachable. Focus alone would be wrong the other way: tabbing past the box would
     * freeze the suggestion at whatever the name happened to be.
     */
    onedit?: () => void;
  } = $props();

  const clash = $derived(!locked && value.trim() !== "" && taken.includes(value.trim()));
</script>

<Field
  label="id"
  hint={locked
    ? "ids are referenced by anchors and fact values · renaming is a refactor, not an edit"
    : "follows the name until you change it"}
  error={clash ? `${value.trim()} already belongs to another record` : null}
>
  <TextInput bind:value mono readonly={locked} oninput={locked ? undefined : onedit} />
</Field>
