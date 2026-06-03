/**
 * Stable toast ids for the go-to-latest-download flow. Split into its own
 * module so the toast components can import them without pulling
 * `go-to-latest.ts` (and its IPC binding deps) into their module graph.
 */

/** Dedup id for the "Downloads is empty" INFO toast. */
export const LATEST_DOWNLOAD_EMPTY_TOAST_ID = 'downloads-go-to-latest-empty'

/**
 * Dedup id for the "Cmdr needs Full Disk Access" INFO toast (also used for
 * the rarer `downloadsDirUnresolved` case — the user-facing story is the
 * same: we can't act on Downloads).
 */
export const LATEST_DOWNLOAD_FDA_TOAST_ID = 'downloads-go-to-latest-fda'
