/**
 * The id a `Field`'s caption points at.
 *
 * A caption that is only a paragraph beside a box is decoration: clicking "value" does
 * nothing, and a screen reader announces an unlabelled text box next to some unrelated
 * words. A real `<label for>` needs a real id, and the id has to reach the control.
 *
 * Through context rather than through props, because the alternative was threading an id
 * from `Field` to fourteen call sites and then down through five wrappers — `SuggestField`
 * and `RefField` and `IdField` all being `TextInput` in a coat — which would have made
 * every field in the editor three lines longer to make one word clickable.
 *
 * The invariant this rests on: **one text control per field**. It holds by construction —
 * a field wraps one box, and the composite ones (`ValueField`, `DateField`) pair a single
 * box with buttons — and if it is ever broken the result is two elements sharing an id,
 * which is a thing a validator says out loud rather than a thing that silently misbehaves.
 */
import { getContext, setContext } from "svelte";

const KEY = Symbol("field-caption");

/** Offered by `Field` to whatever it contains. */
export function offerCaption(id: string) {
  setContext(KEY, id);
}

/**
 * Taken by a control that is the thing a caption would be about.
 *
 * `undefined` outside a `Field`, which is a real case — the header's jump box and the
 * opener's path box are labelled by `aria-label` instead.
 */
export function takeCaption(): string | undefined {
  return getContext<string | undefined>(KEY);
}
