/**
 * What ground on one volume is under a walker right now, and whether a given row
 * is affected by it.
 *
 * Two shapes, because there are two kinds of run. A whole-volume walk (a full
 * rebuild, and every SMB/MTP scan) puts every folder on the drive in flux for its
 * whole length and announces no branches. A phased first index covers the drive
 * branch by branch and says which branch, so only the ground it names is in flux
 * and the rest of the drive keeps its settled sizes.
 *
 * Pure and rune-free so the predicate is unit-tested on its own; the live state
 * that feeds it is `index-state.svelte.ts`.
 */

/** The ground under a walker on one volume. */
export interface WalkedGround {
  /** A walk is taking the volume whole, so every path on it is affected. */
  readonly wholeVolume: boolean
  /** The branch roots under the walker, absolute in the volume's path space. */
  readonly roots: readonly string[]
}

/** Nothing on this volume is being walked. */
export const NO_WALKED_GROUND: WalkedGround = { wholeVolume: false, roots: [] }

/** A run that takes the volume whole. */
export function wholeVolumeWalked(): WalkedGround {
  return { wholeVolume: true, roots: [] }
}

/** A phased run, with the branches currently under the walker. */
export function walkedBranches(roots: readonly string[]): WalkedGround {
  return { wholeVolume: false, roots }
}

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
  if (ground.wholeVolume) return true
  const row = normalize(path)
  return ground.roots.some((root) => {
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
