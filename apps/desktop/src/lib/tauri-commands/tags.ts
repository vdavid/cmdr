// macOS Finder color-tag commands: toggling a tag across paths and enriching a
// cached listing's entries with fresh tag data.

import { commands } from '$lib/ipc/bindings'
import type { TimedOut } from './ipc-types'

/**
 * Toggles a Finder color tag (`color` 1..=7) across `paths`, then patches the
 * resulting tags into the cached listing so the panes re-render immediately.
 * Off macOS this is a no-op.
 */
export function toggleTags(listingId: string, paths: string[], color: number): Promise<TimedOut<null>> {
  return commands.toggleTags(listingId, paths, color)
}

/**
 * Reads macOS Finder tags for the given paths and patches them into the cached
 * listing, emitting a coalesced `directory-diff` so the panes show the colored
 * dots.
 */
export function enrichTags(listingId: string, paths: string[]): Promise<TimedOut<null>> {
  return commands.enrichTags(listingId, paths)
}
