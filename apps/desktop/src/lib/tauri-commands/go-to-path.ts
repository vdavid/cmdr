// "Go to path" (⌘G) commands: resolving typed input against a pane's base dir,
// and the persisted recent-paths list the dialog renders.

import { commands, type RecentPathEntry } from '$lib/ipc/bindings'

/**
 * Resolves a typed input against the focused pane's `baseDir`. Serves the live
 * as-you-type warning, the actual jump, and the clipboard-prefill check.
 * Passthrough: the caller switches on the typed `kind` discriminator.
 */
export function resolveGoToPath(input: string, baseDir: string) {
  return commands.resolveGoToPath(input, baseDir)
}

/** Reads the persisted recent-path entries (newest first). */
export function getRecentPaths(): Promise<RecentPathEntry[]> {
  return commands.getRecentPaths()
}

/**
 * Adds a recent-path entry. Dedupes by resolved path, moves the match to the
 * top, and trims to the fixed cap. Passthrough: the caller checks the typed
 * `Result` before refreshing its mirror.
 */
export function addRecentPath(entry: RecentPathEntry) {
  return commands.addRecentPath(entry)
}

/** Removes a recent-path entry by id. No-op when the id isn't present. */
export function removeRecentPath(id: string) {
  return commands.removeRecentPath(id)
}
