/**
 * Stable toast ids for the reveal flow. Split into its own module so the
 * toast components can import them without pulling `reveal.ts` (and its IPC
 * binding deps) into their module graph.
 */

/** Dedup id for the "Downloads is empty" INFO toast. */
export const REVEAL_EMPTY_TOAST_ID = 'downloads-reveal-empty'

/**
 * Dedup id for the "Cmdr needs Full Disk Access" INFO toast (also used for
 * the rarer `downloadsDirUnresolved` case — the user-facing story is the
 * same: we can't act on Downloads).
 */
export const REVEAL_FDA_TOAST_ID = 'downloads-reveal-fda'
