/**
 * Finding a record by what the writer calls it.
 *
 * Ids are how the world refers to itself and names are how the writer does, so both are
 * searched, and so are the aliases — a writer hunting for Aldric is at least as likely to
 * type "the duke", and the world already carries that string for the manuscript scanner.
 *
 * Ranked rather than filtered. A plain substring filter puts `pol_corrath` and
 * `place_corrath_city` in whatever order the world happened to load them in, and the one
 * the writer meant is the one whose *name* starts with what they typed. So where the match
 * landed is the score, and the tiers are few enough to keep in your head.
 *
 * Plain functions, no Svelte. Same reason [`./selection`] is: this is a decision about
 * ordering, it is worth reading on its own, and a `$derived` around it would hide it.
 */
import type { WorldRecord } from "./api";

export interface Hit {
  record: WorldRecord;
  /** Lower is better. The tier the best match fell in; see `scoreOf`. */
  score: number;
  /** Which string matched, when it was not the name — so the row can show why it is here. */
  via: string | null;
}

/**
 * Where a query landed in one string, best first, or `null` for no match at all.
 *
 * 0 is the whole string, 1 starts it, 2 starts a word inside it, 3 is anywhere. The gap
 * between 1 and 2 is what puts "Marrow" above "The Gate at Marrow" for `mar`, and the gap
 * between 2 and 3 is what keeps "Corrath" above "Vale of Corrath" for `corr` while still
 * finding the second.
 */
function scoreOf(haystack: string, needle: string): number | null {
  const at = haystack.indexOf(needle);
  if (at < 0) return null;
  if (at === 0) return haystack.length === needle.length ? 0 : 1;
  // A word boundary in an id is `_`; in a name it is a space or a hyphen.
  return /[\s\-_]/.test(haystack[at - 1]) ? 2 : 3;
}

/** Ids lose to names by one tier: the writer typed a word, and a name is made of words. */
const ID_PENALTY = 1;

export function rank(query: string, records: WorldRecord[]): Hit[] {
  const q = query.trim().toLowerCase();
  if (q === "") {
    // Nothing typed is not "no answer" — it is the world's index, which is worth
    // browsing. Entities first, then events, then scenes, each alphabetically.
    const order = { entity: 0, event: 1, scene: 2 };
    return [...records]
      .sort((a, b) => order[a.kind] - order[b.kind] || a.name.localeCompare(b.name))
      .map((record) => ({ record, score: 0, via: null }));
  }

  const hits: Hit[] = [];
  for (const record of records) {
    let best: number | null = scoreOf(record.name.toLowerCase(), q);
    let via: string | null = null;

    for (const alias of record.aka) {
      const s = scoreOf(alias.toLowerCase(), q);
      if (s !== null && (best === null || s < best)) {
        best = s;
        via = alias;
      }
    }

    const byId = scoreOf(record.id.toLowerCase(), q);
    if (byId !== null && (best === null || byId + ID_PENALTY < best)) {
      best = byId + ID_PENALTY;
      via = null; // The id is already on every row; saying "via the id" would be noise.
    }

    if (best !== null) hits.push({ record, score: best, via });
  }

  // Ties break on the shorter name, which is the closer match to the same query, and then
  // alphabetically so the list does not reshuffle between loads.
  return hits.sort(
    (a, b) =>
      a.score - b.score ||
      a.record.name.length - b.record.name.length ||
      a.record.name.localeCompare(b.record.name),
  );
}
