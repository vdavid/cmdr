/**
 * Calculate folder sizes on demand, Total Commander's ⌥⇧⏎ and Space on a folder.
 *
 * The walking is the backend's (`src-tauri/src/listing_index_sizes/count/`): it
 * sends each folder's running total and exact size as `listing-index-sizes-changed`,
 * the event the pane already applies for the drive index, so no row code here.
 * This module starts a count, remembers which listings have one running so Esc
 * can stop it (a Space while a count runs joins its queue and resolves when that
 * count ends, so the bookkeeping holds), and tells the user when the folder's
 * volume is gone or some folders couldn't be read.
 */

import { cancelFolderSizeCount, countFolderSizes, getFileAt } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { getAppLogger } from '$lib/logging/logger'
import type { FileEntry } from '../types'

const log = getAppLogger('fileExplorer')

/**
 * How many counts are in flight per listing. Not reactive: only Esc asks. A
 * count, not a set: a newer count of the same pane overlaps the older one's
 * ending, and the older one's exit must not unregister the newer.
 */
const running = new Map<string, number>()

export interface CountOutcome {
  counted: number
  unreadable: number
  cancelled: boolean
}

/** Dedup id of the "couldn't read" toast: a queued Space and the count it joined share one outcome. */
const UNREADABLE_TOAST_ID = 'folder-sizes-unreadable'

/**
 * Counts the folders a pane shows whose exact size isn't known yet, or just
 * `paths` (Space on a folder). Resolves when the count ends, or `null` when it
 * couldn't start.
 */
export async function countFoldersInPane(
  listingId: string,
  includeHidden: boolean,
  paths?: string[],
): Promise<CountOutcome | null> {
  if (listingId === '') return null
  running.set(listingId, (running.get(listingId) ?? 0) + 1)
  try {
    const outcome = await countFolderSizes(listingId, includeHidden, paths ?? null)
    // A folder it couldn't read wholly went back to what it showed, and one it read
    // in part shows a lower bound: say so, or either reads as a silent failure.
    if (outcome.unreadable > 0) {
      addToast(tString('fileExplorer.folderSizes.unreadable', { count: outcome.unreadable }), {
        level: 'info',
        id: UNREADABLE_TOAST_ID,
      })
    }
    return outcome
  } catch (e) {
    const reason = (e as { type?: string }).type
    log.warn("couldn't calculate folder sizes: {reason}", { reason: reason ?? String(e) })
    if (reason === 'notConnected') {
      addToast(tString('fileExplorer.folderSizes.notConnected'), { level: 'warn' })
    }
    return null
  } finally {
    const left = (running.get(listingId) ?? 1) - 1
    if (left > 0) running.set(listingId, left)
    else running.delete(listingId)
  }
}

/**
 * Stops the count running in `listingId`'s pane. Reports whether one was running
 * (Esc consumed). The bookkeeping stays with the count's own `finally`, so a newer
 * count of the same pane stays stoppable.
 */
export function cancelCountInPane(listingId: string): boolean {
  if (!running.has(listingId)) return false
  void cancelFolderSizeCount(listingId)
  return true
}

/**
 * Space on a folder: once Space has selected the row at `backendRow`, count its
 * size if it should be counted. Reads the row from the backend rather than from
 * the cursor's cached entry, which trails a fast ↓ then Space by a fetch.
 */
export async function countFolderOnSpace(args: {
  listingId: string
  backendRow: number
  includeHidden: boolean
  selected: boolean
  enabled: boolean
}): Promise<void> {
  const { listingId, backendRow, includeHidden, selected, enabled } = args
  if (!selected || !enabled || listingId === '' || backendRow < 0) return
  let entry: FileEntry | null
  try {
    entry = await getFileAt(listingId, backendRow, includeHidden)
  } catch {
    return // The pane moved on: nothing to count.
  }
  if (!shouldCountOnSpace(entry, selected, enabled) || !entry) return
  await countFoldersInPane(listingId, includeHidden, [entry.path])
}

/**
 * Whether Space on `entry` should also calculate its size: it just got selected,
 * the setting is on, it's a real folder (not `..`, not a link), and its exact
 * size isn't known yet (the drive index usually has it on a local disk).
 */
export function shouldCountOnSpace(entry: FileEntry | null | undefined, selected: boolean, enabled: boolean): boolean {
  if (!selected || !enabled || !entry) return false
  if (!entry.isDirectory || entry.isSymlink || entry.name === '..') return false
  return !(entry.recursiveSizeComplete === true && entry.recursiveSize != null)
}
