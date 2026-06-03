/**
 * Stable dedup id for the go-to-path nearest-ancestor INFO toast. Split into
 * its own module so the toast component can import it without pulling
 * `go-to-path.ts` (and its IPC binding deps) into its module graph.
 */

/** Dedup id for the "jumped to nearest existing ancestor" INFO toast. */
export const GO_TO_PATH_ANCESTOR_TOAST_ID = 'go-to-path-ancestor'
