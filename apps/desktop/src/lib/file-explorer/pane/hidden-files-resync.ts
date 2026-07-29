/**
 * Keeps a pane consistent after the hidden-files toggle changes how many rows
 * its listing has: republish the total, then put the cursor somewhere sensible.
 *
 * "Somewhere sensible" means the file the user was looking at, wherever it moved
 * to once the hidden entries appeared or vanished. Only when that file is gone
 * (it WAS the hidden one) do we fall back to clamping the cursor into range.
 */

import { findFileIndex, getTotalCount } from '$lib/tauri-commands'

export interface HiddenFilesResyncInput {
  listingId: string
  includeHidden: boolean
  /** The file under the cursor before the toggle, if any. */
  nameToFollow: string | undefined
  /** The cursor position before the toggle. */
  cursorIndex: number
  /** Read late: the `..` row's presence can change with the new total. */
  getHasParent: () => boolean
  setTotalCount: (count: number) => void
  setCursorIndex: (index: number) => Promise<void>
}

export async function resyncAfterHiddenFilesToggle(input: HiddenFilesResyncInput): Promise<void> {
  const count = await getTotalCount(input.listingId, input.includeHidden)
  input.setTotalCount(count)

  const hasParent = input.getHasParent()
  const total = hasParent ? count + 1 : count

  // Try to keep cursor on the same file
  if (input.nameToFollow) {
    const foundIndex = await findFileIndex(input.listingId, input.nameToFollow, input.includeHidden)
    if (foundIndex !== null) {
      await input.setCursorIndex(hasParent ? foundIndex + 1 : foundIndex)
      return
    }
  }

  // File not found (was hidden) or no file: clamp cursor
  if (input.cursorIndex >= total) {
    await input.setCursorIndex(Math.max(0, total - 1))
  }
}
