// macOS Finder color-tag commands: toggling a tag across paths and enriching a
// cached listing's entries with fresh tag data.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { commands, events } from '#lib/ipc/bindings.js'
import type { TimedOut } from './ipc-types'

/**
 * Listens for `tags-skipped`: a toggle left `count` files alone because their
 * existing tags couldn't be read or decoded. Emitted by both the keyboard command
 * and the context-menu circles.
 */
export function onTagsSkipped(handler: (count: number) => void): Promise<UnlistenFn> {
  return events.tagsSkipped.listen((event) => {
    handler(event.payload.count)
  })
}

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
