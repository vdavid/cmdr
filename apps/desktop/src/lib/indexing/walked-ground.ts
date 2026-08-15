/**
 * What ground on one volume is under a walker right now, and whether a given row
 * is affected by it.
 *
 * One shape for every kind of run: a list of absolute roots the backend says it
 * is walking. A run that takes the volume whole announces the volume root, so
 * `/` matches every row on the drive through the same predicate that matches
 * `~/Downloads` to the rows inside and above it. There is no second kind of run
 * to branch on here, and no sentinel: an empty list means nothing is moving.
 *
 * Pure and rune-free so the predicate is unit-tested on its own; the live state
 * that feeds it is `index-state.svelte.ts`.
 */

/** The roots under a walker on one volume, absolute in its own path space. */
export type WalkedGround = readonly string[]

/** Nothing on this volume is being walked. */
export const NO_WALKED_GROUND: WalkedGround = []

/**
 * Whether this row's folder size can move while the walk runs.
 *
 * The test is BIDIRECTIONAL, and that is the whole subtlety: the roll-up repairs
 * the ancestor chain upward, so walking `~/Downloads/big` changes the size shown
 * for `~/Downloads` and for `~` too. A downward-only test marks the ground being
 * walked and leaves every folder above it looking settled while its number is
 * about to change.
 *
 * Paths are compared by whole segments, so `~/Downloads2` is not inside
 * `~/Downloads`. Both sides come from the same backend, so no case folding is
 * applied: on a case-insensitive volume the walker and the listing agree on
 * spelling because they read the same rows.
 */
export function isPathAffectedByWalk(ground: WalkedGround, path: string): boolean {
  const row = normalize(path)
  return ground.some((root) => {
    const branch = normalize(root)
    return isAtOrUnder(row, branch) || isAtOrUnder(branch, row)
  })
}

/** Drop a trailing separator so `/a/b/` and `/a/b` are the same folder. The
 *  volume root itself (`/`) keeps its one character. */
function normalize(path: string): string {
  return path.length > 1 && path.endsWith('/') ? path.slice(0, -1) : path
}

/** Whether `path` IS `ancestor` or sits somewhere below it, by whole segments. */
function isAtOrUnder(path: string, ancestor: string): boolean {
  if (path === ancestor) return true
  const boundary = ancestor === '/' ? '/' : `${ancestor}/`
  return path.startsWith(boundary)
}
