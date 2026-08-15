<script lang="ts">
  /**
   * The app's first `<textarea>`, for a record's own prose.
   *
   * Worth being clear about which prose this is: the manuscript is never edited here and
   * never will be. This is the Markdown body of the record itself — the sentence or two
   * explaining what a place is — which the storage layer has always round-tripped
   * verbatim and which nothing parses.
   */
  let {
    value = $bindable(""),
    placeholder = "",
    onsettled,
  }: {
    value?: string;
    placeholder?: string;
    onsettled?: () => void;
  } = $props();
</script>

<textarea bind:value {placeholder} rows="4" onblur={onsettled}></textarea>

<style>
  textarea {
    width: 100%;
    padding: 7px 9px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--rule);
    font-family: var(--f-body);
    font-size: 13px;
    line-height: 1.5;
    resize: vertical;
  }

  textarea::placeholder {
    color: var(--rule-strong);
  }

  textarea:focus {
    outline: none;
    border-color: var(--rule-strong);
  }
</style>
