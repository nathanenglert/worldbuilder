<script lang="ts">
  import type { WorldPlan } from "./api";
  import PillGroup from "./PillGroup.svelte";

  /**
   * Starting a world.
   *
   * Four questions, and two of them already have an answer. What a world *needs* is a
   * name and a calendar; everything else this app can do — a map, a manuscript, the
   * writer's own word for a duchy — is a line in `world.yaml`, and asking about any of it
   * here would be asking a novelist to configure a product before they have written a
   * sentence. The generated file carries those lines commented out instead, which is the
   * form this app can afford to be long-winded in.
   *
   * Owns its fields and nothing else. It is rendered in two places — the welcome screen,
   * where it is the whole point, and the header's opener, where it is the other half of
   * "which world" — so the work of actually making one belongs to whoever called it.
   */
  let {
    busy = false,
    oncreate,
  }: {
    busy?: boolean;
    oncreate: (plan: WorldPlan) => void;
  } = $props();

  let name = $state("");
  let path = $state("");
  /**
   * A calendar of your own, by default, in a tool whose example world counts in
   * Frostwane and Seedfall. The other arm is a real choice rather than a fallback:
   * historical and contemporary fiction should not have to invent time, and this is the
   * one decision here that is annoying to reverse once records carry dates.
   */
  let calendar = $state<"earth" | "own">("own");
  let track = $state(true);

  const ready = $derived(name.trim().length > 0 && path.trim().length > 0);
</script>

<form
  class="new"
  onsubmit={(e) => {
    e.preventDefault();
    if (!ready || busy) return;
    oncreate({ path: path.trim(), name: name.trim(), calendar, track });
  }}
>
  <label>
    <span>what it is called</span>
    <input
      bind:value={name}
      placeholder="The Vashen Reckoning"
      spellcheck="false"
      autocomplete="off"
    />
  </label>

  <label>
    <span>where it lives</span>
    <input bind:value={path} placeholder="~/worlds/vashen" spellcheck="false" autocomplete="off" />
  </label>
  <p class="note">Made if it is not there yet. A folder that already holds a world is left alone.</p>

  <div class="choice">
    <span>how it keeps time</span>
    <PillGroup
      options={[
        { value: "own", label: "a calendar of your own" },
        { value: "earth", label: "earth's calendar" },
      ]}
      value={calendar}
      onpick={(v: string) => (calendar = v as "earth" | "own")}
    />
  </div>
  <p class="note">
    {calendar === "own"
      ? "Twelve months of thirty days, called First through Twelfth — rename them, resize them, add or take some away."
      : "Months, weekdays and leap years as Earth keeps them."}
  </p>

  <div class="choice">
    <span>version control</span>
    <PillGroup
      options={[
        { value: "track", label: "track it with git" },
        { value: "plain", label: "just a folder" },
      ]}
      value={track ? "track" : "plain"}
      onpick={(v: string) => (track = v === "track")}
    />
  </div>
  <!-- Named as what it buys rather than as what it is. "Track it with git" is a sentence
       about a tool; save points and what-ifs are the two things this unlocks, and the
       second one is the reason this app stores a world as files at all. -->
  <p class="note">
    {track
      ? "Save points, what-ifs, and comparing two versions of the world. All of it on this machine — there is nowhere for it to go."
      : "You can start tracking it later; the version panel says how in one line."}
  </p>

  <button type="submit" disabled={!ready || busy}>create</button>
</form>

<style>
  .new {
    display: grid;
    gap: var(--s-4);
  }

  label {
    display: grid;
    gap: var(--s-2);
  }

  /* The question, in the same register as every other small caps label in the app. */
  label span,
  .choice span {
    font-family: var(--f-mono);
    font-size: var(--caps);
    letter-spacing: var(--track);
    text-transform: uppercase;
    color: var(--ink-3);
  }

  input {
    padding: 7px 9px;
    font-family: var(--f-mono);
    font-size: 12px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: 2px;
  }

  input:focus {
    outline: none;
    border-color: var(--rule-strong);
  }

  input::placeholder {
    color: var(--rule-strong);
  }

  .choice {
    display: grid;
    gap: var(--s-3);
    margin-top: var(--s-3);
  }

  /* What the choice above it means, in the teaching voice: this is the only place the
     app gets to explain a decision the writer is making about their own files. */
  .note {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--ink-3);
  }

  button {
    justify-self: start;
    margin-top: var(--s-5);
    padding: 6px 12px;
    font-family: var(--f-mono);
    font-size: 11px;
    letter-spacing: 0.06em;
    color: var(--accent);
    background: var(--accent-soft);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
  }

  button:hover:not(:disabled) {
    color: var(--paper);
    background: var(--accent);
  }

  button:disabled {
    color: var(--ink-3);
    background: none;
    cursor: default;
  }
</style>
