// Directory-watcher event listeners. Typed `on*` wrappers over the
// `tauri-specta` `events.directoryDiff` / `events.directoryDeleted` helpers.
// `directory-diff` carries coalesced add/remove/modify changes for a watched
// listing; `directory-deleted` fires when the watched directory itself is gone.

import { type UnlistenFn } from '@tauri-apps/api/event'
import { events } from '$lib/ipc/bindings'
import type { DirectoryDeletedEvent, DirectoryDiff } from '$lib/file-explorer/types'

/**
 * A batched set of add/remove/modify changes for one watched listing.
 *
 * The generated payload nests the typed `FileEntry`; we hand back the FE
 * `DirectoryDiff` (whose `entry` is the FE `FileEntry` with its extra
 * frontend-only fields and `?`-optionals) via a cast at this single boundary,
 * the same way `getFileRange` casts the generated entry to the FE shape.
 */
export function onDirectoryDiff(handler: (payload: DirectoryDiff) => void): Promise<UnlistenFn> {
  return events.directoryDiff.listen((event) => {
    handler(event.payload as DirectoryDiff)
  })
}

/** The watched directory itself was deleted. */
export function onDirectoryDeleted(handler: (payload: DirectoryDeletedEvent) => void): Promise<UnlistenFn> {
  return events.directoryDeleted.listen((event) => {
    handler(event.payload)
  })
}
