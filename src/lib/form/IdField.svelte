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
  }: {
    value?: string;
    locked: boolean;
    taken: string[];
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
  <TextInput bind:value mono readonly={locked} />
</Field>
