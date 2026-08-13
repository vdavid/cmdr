/**
 * Keeps stored search snapshots honest about files that no longer exist.
 *
 * A snapshot outlives the pane that opened it and the operation that emptied it:
 * the user can reopen `search-results://sr-3` in any window, hours later, and its
 * rows must not name files that were moved, deleted, trashed, or renamed away in
 * the meantime.
 *
 * **The input is outcome, not intent.** `write-source-item-done` fires once per
 * top-level source item, as it finishes, carrying whether that path is GONE. So a
 * skipped item emits nothing, an item a cancel never reached emits nothing, and a
 * cross-FS move's staging pass reports `sourceRemoved: false` until its
 * source-delete phase runs. Reading the operation's `sourcePaths` instead was
 * wrong in exactly those three ways.
 *
 * **It costs one event per top-level item and no state.** Putting the vanished
 * paths on the completion event was the obvious alternative and is not available:
 * a 500k-file move would ship 500k strings to every webview.
 *
 * **It needs no birth context**, which is why a window watching an operation it
 * never started (`file-explorer/pane/DETAILS.md` § "Birth context") keeps its
 * snapshots correct too. The stream reaches every webview, so each window purges
 * its own store.
 *
 * One subscription per window, started from the main page's init. Full contract:
 * `DETAILS.md` § "Snapshot store".
 */

import { onWriteSourceItemDone } from '$lib/tauri-commands'
import { removeEntryFromAllSnapshots } from './snapshot-store.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { UnlistenFn } from '@tauri-apps/api/event'

const log = getAppLogger('search')

let unlisten: UnlistenFn | null = null
/** In flight, so a double init can't leak a second listener. */
let subscribing: Promise<void> | null = null

/**
 * Subscribes this window's snapshot store to the write stream. Idempotent, and
 * safe to await concurrently. Never throws: a failed subscription leaves the
 * snapshots as stale as they were before, which is the status quo, not a crash.
 */
export async function initSnapshotPurge(): Promise<void> {
  if (unlisten) return
  subscribing ??= onWriteSourceItemDone((event) => {
    if (!event.sourceRemoved) return
    const mutated = removeEntryFromAllSnapshots(event.sourcePath)
    if (mutated.length > 0) {
      log.debug('Dropped {path} from {count} search snapshot(s)', {
        path: event.sourcePath,
        count: mutated.length,
      })
    }
  })
    .then((fn) => {
      // A teardown that landed while we were subscribing wins: drop the
      // listener we just got rather than parking a live one nobody will free.
      if (subscribing === null) {
        fn()
        return
      }
      unlisten = fn
    })
    .catch((err: unknown) => {
      log.warn('Failed to subscribe the search-snapshot purge: {error}', { error: err })
    })
    .finally(() => {
      subscribing = null
    })
  return subscribing
}

/** Drops the listener. Safe without an init, safe twice, and safe while an init
 *  is still in flight. */
export function destroySnapshotPurge(): void {
  unlisten?.()
  unlisten = null
  subscribing = null
}
