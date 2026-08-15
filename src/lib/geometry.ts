/**
 * Screen to world, and the small edits a polygon needs.
 *
 * This lives in its own module because it is the one part of map authoring that can be
 * checked by reading rather than by clicking, and because getting it wrong fails in the
 * most expensive way available: a marker that lands correctly at 100% zoom and drifts
 * further off the more you zoom in. That survives a screenshot and a casual test, and
 * then writes a wrong coordinate into somebody's world file.
 */

export type Point = [number, number];

export interface View {
  /** Pan, in viewBox units. */
  tx: number;
  ty: number;
  scale: number;
  /** The viewBox's own extent. */
  W: number;
  H: number;
}

/**
 * A point in viewBox units to normalized world coordinates.
 *
 * The SVG's `getScreenCTM()` undoes the root `viewBox` and `preserveAspectRatio` — and
 * *only* those. Pan and zoom live on two inner `<g transform>` elements, so their inverse
 * has to be applied here as well. Omitting it is the drift described above.
 */
export function viewToWorld(vb: { x: number; y: number }, v: View): Point {
  return [(vb.x - v.tx) / v.scale / v.W, (vb.y - v.ty) / v.scale / v.H];
}

/** The reverse, for drawing something the writer has not saved yet. */
export function worldToView(p: Point, v: View): { x: number; y: number } {
  return { x: p[0] * v.W * v.scale + v.tx, y: p[1] * v.H * v.scale + v.ty };
}

/**
 * Keep a coordinate on the map.
 *
 * Not optional: `preserveAspectRatio="xMidYMid meet"` letterboxes, so a click in the
 * margin yields a viewBox coordinate outside the box and would otherwise be written
 * straight into a world file as an out-of-range marker.
 */
export function clamp01(p: Point): Point {
  const clamp = (n: number) => (n < 0 ? 0 : n > 1 ? 1 : n);
  return [clamp(p[0]), clamp(p[1])];
}

/**
 * Four decimal places, which is finer than the map image has pixels and coarse enough
 * to read. A click carries no more precision than that, and writing `0.2190476190476191`
 * into somebody's file claims a exactness the mouse never had.
 */
export function round4(p: Point): Point {
  return [Math.round(p[0] * 1e4) / 1e4, Math.round(p[1] * 1e4) / 1e4];
}

export function moveVertex(shape: Point[], i: number, to: Point): Point[] {
  const next = shape.slice();
  next[i] = to;
  return next;
}

/** Split the edge after `i`, which is how a coarse outline gets refined. */
export function insertVertex(shape: Point[], i: number, at: Point): Point[] {
  const next = shape.slice();
  next.splice(i + 1, 0, at);
  return next;
}

export function dropVertex(shape: Point[], i: number): Point[] {
  const next = shape.slice();
  next.splice(i, 1);
  return next;
}

/** The midpoint of the edge from `i` to the next vertex, wrapping at the end. */
export function midpoint(shape: Point[], i: number): Point {
  const a = shape[i];
  const b = shape[(i + 1) % shape.length];
  return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
}
