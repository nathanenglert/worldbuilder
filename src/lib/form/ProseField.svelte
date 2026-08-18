<script lang="ts">
  /**
   * The app's first `<textarea>`, for a record's own prose.
   *
   * Worth being clear about which prose this is: the manuscript is never edited here and
   * never will be. This is the Markdown body of the record itself — the sentence or two
   * explaining what a place is — which the storage layer has always round-tripped
   * verbatim and which nothing parses.
   *
   * Four fixed rows was a guess about how much a writer has to say, and it was wrong in
   * both directions: a one-line stub sat in a box mostly empty, and Aldric's three
   * paragraphs were read four lines at a time through a slot. So the box takes the shape
   * of what is in it, up to the point where it would push the rest of the form off the
   * screen, and past that there is a button.
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

  /** Roughly ten lines. Beyond this the form's other fields start to disappear upward. */
  const CAP = 210;

  let el = $state<HTMLTextAreaElement | null>(null);
  let full = $state(false);
  let overflows = $state(false);

  $effect(() => {
    // Both are read so the box re-measures on a keystroke *and* on expanding.
    void value;
    const wide = full;
    const box = el;
    if (!box) return;
    // Shrink first: `scrollHeight` only ever reports the content, never that it got
    // smaller, so a box that had grown could not come back down.
    box.style.height = "auto";
    // `scrollHeight` is content plus padding, and the box is `border-box`, so the borders
    // have to be added back or an expanded field still scrolls by exactly two pixels.
    const wanted = box.scrollHeight + (box.offsetHeight - box.clientHeight);
    overflows = wanted > CAP;
    box.style.height = `${wide ? wanted : Math.min(wanted, CAP)}px`;
  });
</script>

<div class="prose">
  <textarea bind:this={el} bind:value {placeholder} rows="2" onblur={onsettled}></textarea>
  {#if overflows}
    <button type="button" onclick={() => (full = !full)}>
      {full ? "collapse" : "expand"}
    </button>
  {/if}
</div>

<style>
  .prose {
    display: grid;
    gap: 3px;
  }

  textarea {
    width: 100%;
    padding: 7px 9px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--rule);
    font-family: var(--f-body);
    font-size: 13px;
    line-height: 1.5;
    overflow-y: auto;
    /* The height is measured, so a hand-dragged one would be undone by the next
       keystroke. The button is the honest version of the same affordance. */
    resize: none;
  }

  textarea::placeholder {
    color: var(--rule-strong);
  }

  textarea:focus {
    outline: none;
    border-color: var(--rule-strong);
  }

  button {
    justify-self: end;
    font-family: var(--f-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--ink-3);
  }

  button:hover {
    color: var(--accent);
  }
</style>
