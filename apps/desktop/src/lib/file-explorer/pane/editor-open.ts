/**
 * F4: hand a file to the OS text editor, or say plainly why not.
 *
 * `open -t` takes a path the Mac's own filesystem knows. A row on a phone (MTP,
 * ADB), on a server (SFTP, WebDAV), inside an archive, or in the virtual `.git`
 * portal has none, and handing its `adb://` or archive-inner path over does
 * nothing at all: no editor, no message. So every F4 goes through here, and a row
 * with no real file behind it gets a toast pointing at F3, which can read it.
 * ❌ Editing those files in place (pull, edit, write back) isn't built.
 *
 * Two gates, in this order, the same pair the snapshot clipboard uses:
 *
 * 1. The PATH scheme (`isPlainFilesystemPath`). It holds with no volume to ask: a
 *    search-results pane's own volume is virtual, and an unplugged phone has left
 *    the volume list while its rows still read `adb://…`.
 * 2. The row's CAPABILITY (`rowIsOsVisible`). It knows what a path can't show: an
 *    archive's insides and the `.git` portal look like plain paths.
 */

import { openInEditor } from '$lib/tauri-commands'
import { addToast } from '$lib/ui/toast'
import { tString } from '$lib/intl/messages.svelte'
import { isPlainFilesystemPath } from '$lib/path/canonical'
import { rowIsOsVisible } from './volume-capabilities'

/** What an F4 press did: handed the file to the editor, or refused it on screen. */
export type EditorOpenOutcome = 'opened' | 'refusedNotOnThisMac'

/** Whether `rowPath`, living on `volumeId`, is a file the OS text editor can open. */
export function canOpenInEditor(volumeId: string, rowPath: string): boolean {
  return isPlainFilesystemPath(rowPath) && rowIsOsVisible(volumeId, rowPath)
}

/**
 * Opens the file in the OS text editor, or tells the user why it can't, and reports
 * which. `volumeId` is the volume the ROW lives on: a pane's own, or the one a
 * search-results snapshot covered.
 */
export async function openInEditorOrExplain(volumeId: string, rowPath: string): Promise<EditorOpenOutcome> {
  if (!canOpenInEditor(volumeId, rowPath)) {
    addToast(tString('fileExplorer.edit.notOnThisMac'), { level: 'info' })
    return 'refusedNotOnThisMac'
  }
  // The system default, and no question about other editors: nothing reads
  // `behavior.textEditorApp` or the hint's flag here yet.
  await openInEditor(rowPath, 'system', false)
  return 'opened'
}
